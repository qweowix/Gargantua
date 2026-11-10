package app.blackhole

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.media3.common.C
import androidx.media3.common.audio.AudioProcessor
import androidx.media3.common.audio.BaseAudioProcessor
import androidx.media3.common.util.UnstableApi
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.encodeToJsonElement
import java.nio.ByteBuffer
import java.util.concurrent.Executors
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sinh

@Serializable
data class EqPreset(val ru: String = "", val en: String = "", val gains: List<Float> = emptyList())

/** Расчёт из ядра (fx.rs, тот же, что на ПК): подогнанные усиления полос, запас громкости и нужен ли лимитер */
@Serializable
data class EqPlan(val on: Boolean = false, val freqs: List<Int> = emptyList(), val set: List<Double> = emptyList(), val pre: Double = 0.0, val limit: Boolean = false)

/** Эквалайзер как на ПК: те же 10 полос, пресеты и формулы. Ползунки → ядро считает план → обработчик звука его играет */
object Eq {
    val FREQS = listOf(31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000)
    const val MAX_DB = 12f

    var gains by mutableStateOf(List(10) { 0f })
        private set
    var presets by mutableStateOf<List<EqPreset>>(emptyList())
        private set

    @Volatile
    var plan: EqPlan? = null
        private set

    /** растёт при каждой смене плана — обработчик по нему замечает, что пора пересчитать коэффициенты */
    @Volatile
    var ver = 0
        private set

    /** скорость и «сохранять тон» — как на ПК: без сохранения тона замедление опускает голос (slowed) */
    var speed by mutableStateOf(1f)
        private set
    var keepPitch by mutableStateOf(true)
        private set

    /** то же для сервиса плеера: он подписан и ставит параметры на лету */
    val tempo = kotlinx.coroutines.flow.MutableStateFlow(1f to true)

    fun setSpeed(v: Float, keep: Boolean = keepPitch) {
        speed = (Math.round(v * 20) / 20f).coerceIn(0.5f, 2f)
        keepPitch = keep
        prefs.edit().putFloat("speed", speed).putBoolean("keep_pitch", keep).apply()
        tempo.value = speed to keep
    }

    private lateinit var prefs: android.content.SharedPreferences
    private val bg = Executors.newSingleThreadExecutor()

    fun init(ctx: Context) {
        prefs = ctx.getSharedPreferences("bh", Context.MODE_PRIVATE)
        gains = runCatching { json.decodeFromString<List<Float>>(prefs.getString("eq", "") ?: "") }.getOrNull()?.takeIf { it.size == 10 } ?: List(10) { 0f }
        speed = prefs.getFloat("speed", 1f)
        keepPitch = prefs.getBoolean("keep_pitch", true)
        tempo.value = speed to keepPitch
        bg.execute { presets = runCatching { coreNow("eq_presets").to<List<EqPreset>>() }.getOrDefault(emptyList()) }
        apply()
    }

    fun set(g: List<Float>) {
        gains = g.map { it.coerceIn(-MAX_DB, MAX_DB) }
        prefs.edit().putString("eq", json.encodeToString(gains)).apply()
        apply()
    }

    /** какой пресет совпадает с ползунками (-1 — свои настройки) */
    fun presetIndex() = presets.indexOfFirst { p -> p.gains.size == gains.size && p.gains.zip(gains).all { (a, b) -> abs(a - b) < 0.05f } }

    private fun apply() {
        val g = gains
        bg.execute {
            plan = runCatching { coreNow("eq_plan", buildJsonObject { put("gains", json.encodeToJsonElement(g)) }).to<EqPlan>() }.getOrNull()?.takeIf { it.on }
            ver++
        }
    }
}

/** Колокола RBJ шириной в октаву (как фильтр equalizer у ffmpeg на ПК), запас громкости и лимитер 0.97.
 *  Работает с 16-битным PCM; ползунки в нуле — звук проходит нетронутым */
@UnstableApi
class EqProcessor : BaseAudioProcessor() {
    private var rate = 44100
    private var ch = 2
    private var seen = -1
    private var b = DoubleArray(0) // по 5 коэффициентов на полосу: b0 b1 b2 a1 a2
    private var z = DoubleArray(0) // состояние: канал × полоса × 2
    private var bands = 0
    private var pre = 1.0
    private var limit = false
    private var gain = 1.0
    private var release = 0.0

    override fun onConfigure(inputAudioFormat: AudioProcessor.AudioFormat): AudioProcessor.AudioFormat {
        if (inputAudioFormat.encoding != C.ENCODING_PCM_16BIT) throw AudioProcessor.UnhandledAudioFormatException(inputAudioFormat)
        rate = inputAudioFormat.sampleRate
        ch = inputAudioFormat.channelCount
        seen = -1
        return inputAudioFormat
    }

    private fun rebuild() {
        seen = Eq.ver
        val p = Eq.plan
        val use = p?.freqs.orEmpty().zip(p?.set.orEmpty()).filter { (f, g) -> abs(g) >= 0.05 && f < rate / 2 }
        bands = use.size
        b = DoubleArray(bands * 5)
        z = DoubleArray(ch * bands * 2)
        use.forEachIndexed { k, (f, g) ->
            val a = 10.0.pow(g / 40)
            val w0 = 2 * PI * f / rate
            val alpha = sin(w0) * sinh(ln(2.0) / 2 * w0 / sin(w0))
            val a0 = 1 + alpha / a
            b[k * 5] = (1 + alpha * a) / a0
            b[k * 5 + 1] = -2 * cos(w0) / a0
            b[k * 5 + 2] = (1 - alpha * a) / a0
            b[k * 5 + 3] = -2 * cos(w0) / a0
            b[k * 5 + 4] = (1 - alpha / a) / a0
        }
        limit = p?.limit == true
        pre = if (limit) 10.0.pow((p?.pre ?: 0.0) / 20) else 1.0
        gain = 1.0
        release = 1 - exp(-1.0 / (0.060 * rate))
    }

    override fun queueInput(inputBuffer: ByteBuffer) {
        if (seen != Eq.ver) rebuild()
        val out = replaceOutputBuffer(inputBuffer.remaining())
        if (bands == 0) {
            out.put(inputBuffer)
            out.flip()
            return
        }
        val frame = DoubleArray(ch)
        val top = 0.97 * 32767
        while (inputBuffer.remaining() >= 2 * ch) {
            var peak = 0.0
            for (c in 0 until ch) {
                var x = inputBuffer.short * pre
                for (k in 0 until bands) {
                    val i = k * 5
                    val s = (c * bands + k) * 2
                    val y = b[i] * x + z[s]
                    z[s] = b[i + 1] * x - b[i + 3] * y + z[s + 1]
                    z[s + 1] = b[i + 2] * x - b[i + 4] * y
                    x = y
                }
                frame[c] = x
                if (abs(x) > peak) peak = abs(x)
            }
            // лимитер: пик выше потолка — усиление падает сразу, потом возвращается за ~60 мс
            if (limit) {
                if (peak * gain > top) gain = top / peak else gain += (1 - gain) * release
            }
            for (c in 0 until ch) out.putShort((frame[c] * gain).coerceIn(-32768.0, 32767.0).toInt().toShort())
        }
        out.flip()
    }

    override fun onFlush() {
        z.fill(0.0)
        gain = 1.0
    }

    override fun onReset() {
        seen = -1
    }
}
