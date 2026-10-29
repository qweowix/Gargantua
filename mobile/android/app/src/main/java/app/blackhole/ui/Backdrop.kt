package app.blackhole.ui

import android.content.Context
import android.graphics.Bitmap
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import coil3.SingletonImageLoader
import coil3.request.ImageRequest
import coil3.request.SuccessResult
import coil3.request.allowHardware
import coil3.toBitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlin.math.ceil
import kotlin.math.exp
import kotlin.math.max
import kotlin.math.pow
import kotlin.math.sqrt

/** Слои фона экрана текста — те же, что делает covers::big на ПК */
class Backdrop(val bg: ImageBitmap, val blob: ImageBitmap)

/** Пиксели как float RGBA 0..255 — чтобы размытие и подъём яркости не теряли точность */
private class Px(val w: Int, val h: Int, val p: FloatArray)

private fun Bitmap.px(): Px {
    val a = IntArray(width * height)
    getPixels(a, 0, width, 0, 0, width, height)
    val p = FloatArray(a.size * 4)
    for (i in a.indices) {
        val c = a[i]
        p[i * 4] = (c shr 16 and 0xFF).toFloat()
        p[i * 4 + 1] = (c shr 8 and 0xFF).toFloat()
        p[i * 4 + 2] = (c and 0xFF).toFloat()
        p[i * 4 + 3] = (c ushr 24).toFloat()
    }
    return Px(width, height, p)
}

private fun Px.bitmap(): ImageBitmap {
    val a = IntArray(w * h) { i ->
        val c = { k: Int -> p[i * 4 + k].toInt().coerceIn(0, 255) }
        (c(3) shl 24) or (c(0) shl 16) or (c(1) shl 8) or c(2)
    }
    return Bitmap.createBitmap(a, w, h, Bitmap.Config.ARGB_8888).asImageBitmap()
}

/** Гауссово размытие с краями «как есть» (как imageops::blur) */
private fun Px.blur(sigma: Float): Px {
    val r = ceil(sigma * 3).toInt()
    val k = FloatArray(r * 2 + 1) { exp(-((it - r) * (it - r)) / (2 * sigma * sigma)) }
    val sum = k.sum()
    for (i in k.indices) k[i] /= sum
    fun pass(src: FloatArray, horiz: Boolean): FloatArray {
        val out = FloatArray(src.size)
        for (y in 0 until h) for (x in 0 until w) for (c in 0..3) {
            var acc = 0f
            for (j in -r..r) {
                val sx = if (horiz) (x + j).coerceIn(0, w - 1) else x
                val sy = if (horiz) y else (y + j).coerceIn(0, h - 1)
                acc += src[(sy * w + sx) * 4 + c] * k[j + r]
            }
            out[(y * w + x) * 4 + c] = acc
        }
        return out
    }
    return Px(w, h, pass(pass(p, true), false))
}

/** Во сколько раз поднять яркость: светлые 10% пикселей должны дотягивать до ~60% */
private fun Px.gain(): Float {
    val v = (0 until w * h).map { maxOf(p[it * 4], p[it * 4 + 1], p[it * 4 + 2]) }.sorted()
    return (0.6f / max(v[v.size * 9 / 10] / 255f, 0.02f)).coerceIn(1f, 6f)
}

/** Тёмная обложка после размытия становилась почти чёрной: ярче в gain раз и насыщеннее, оттенок тот же */
private fun Px.lift(gain: Float): Px {
    if (gain <= 1f) return this
    val sat = 1 + 0.3f * minOf(gain - 1, 1f)
    for (i in 0 until w * h) {
        val r = p[i * 4] * gain
        val g = p[i * 4 + 1] * gain
        val b = p[i * 4 + 2] * gain
        val m = (r + g + b) / 3
        p[i * 4] = (m + (r - m) * sat).coerceIn(0f, 255f)
        p[i * 4 + 1] = (m + (g - m) * sat).coerceIn(0f, 255f)
        p[i * 4 + 2] = (m + (b - m) * sat).coerceIn(0f, 255f)
    }
    return this
}

suspend fun backdrop(ctx: Context, url: String): Backdrop? = withContext(Dispatchers.Default) {
    val req = ImageRequest.Builder(ctx).data(url).size(128).allowHardware(false).build()
    val img = (SingletonImageLoader.get(ctx).execute(req) as? SuccessResult)?.image?.toBitmap() ?: return@withContext null
    // фон: крошечная размытая копия, растянутая на весь экран — сама превращается в мягкие пятна
    val bg = Bitmap.createScaledBitmap(img, 20, 20, true).px().blur(1.2f)
    // пятно с мягкими прозрачными краями, его таскаем поверх фона
    val blob = Bitmap.createScaledBitmap(img, 48, 48, true).px().blur(2.5f)
    for (y in 0 until 48) for (x in 0 until 48) {
        val d = sqrt((x - 23.5f).pow(2) + (y - 23.5f).pow(2)) / 24f
        blob.p[(y * 48 + x) * 4 + 3] = (1 - d).coerceIn(0f, 1f).pow(1.4f) * 255
    }
    val gain = bg.gain()
    Backdrop(bg.lift(gain).bitmap(), blob.lift(gain).bitmap())
}
