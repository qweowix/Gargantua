package app.blackhole

import android.net.Uri
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSpec
import kotlinx.serialization.json.buildJsonObject
import okhttp3.OkHttpClient
import okhttp3.RequestBody.Companion.toRequestBody
import org.schabi.newpipe.extractor.NewPipe
import org.schabi.newpipe.extractor.downloader.Downloader
import org.schabi.newpipe.extractor.downloader.Request
import org.schabi.newpipe.extractor.downloader.Response
import org.schabi.newpipe.extractor.exceptions.ReCaptchaException
import org.schabi.newpipe.extractor.exceptions.SignInConfirmNotBotException
import org.schabi.newpipe.extractor.stream.DeliveryMethod
import org.schabi.newpipe.extractor.stream.StreamInfo
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import kotlin.concurrent.withLock

const val BROWSER_UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0"

/** NewPipe ходит в сеть через наш OkHttp */
class NpDownloader(private val client: OkHttpClient) : Downloader() {
    override fun execute(request: Request): Response {
        val body = request.dataToSend()?.toRequestBody()
        val b = okhttp3.Request.Builder().url(request.url()).method(request.httpMethod(), body).header("User-Agent", BROWSER_UA)
        for ((k, vs) in request.headers()) {
            b.removeHeader(k)
            vs.forEach { b.addHeader(k, it) }
        }
        client.newCall(b.build()).execute().use { r ->
            if (r.code == 429) throw ReCaptchaException("reCaptcha", request.url())
            return Response(r.code, r.message, r.headers.toMultimap(), r.body?.string(), r.request.url.toString())
        }
    }
}

/** Трек → прямая ссылка на звук. yt-dlp на телефоне нет — ссылку достаёт NewPipeExtractor */
@UnstableApi
object Streams {
    private const val TTL = 4 * 3600_000L // прямые ссылки YouTube живут ~6 ч
    private val cache = ConcurrentHashMap<String, Pair<String, Long>>()

    /** Треки очереди по ключу: плеер знает только bh://t/<ключ>, сам трек берём отсюда */
    val tracks = ConcurrentHashMap<String, Track>()

    /** YouTube «подтверди, что не бот»: IP на время забанен для анонимов. Каждый новый запрос продлевает бан,
     *  поэтому до этого времени предзагрузка молчит, а плеер не перебирает треки */
    @Volatile
    var blockedUntil = 0L
        private set

    fun blocked() = System.currentTimeMillis() < blockedUntil

    /** вошли в YouTube — бан анонимов нас больше не касается */
    fun unblock() {
        blockedUntil = 0
    }

    /** почему не вышло под аккаунтом YouTube — показываем это, а не «войди», если уже вошли */
    @Volatile
    var authError: String? = null

    fun isBotBlock(e: Throwable?) = generateSequence(e) { it.cause }.any { it is SignInConfirmNotBotException }

    fun forget(key: String) {
        cache.remove(key)
    }

    fun init(client: OkHttpClient) = NewPipe.init(NpDownloader(client))

    /** Для ExoPlayer: bh://t/<ключ> → настоящий URL с нужными заголовками (зовётся из потока загрузчика) */
    fun resolve(spec: DataSpec): DataSpec {
        if (spec.uri.scheme != "bh") return spec
        val key = spec.uri.lastPathSegment ?: throw IOException("нет трека")
        val url = urlFor(key)
        return spec.buildUpon().setUri(Uri.parse(url)).setHttpRequestHeaders(headersFor(url)).build()
    }

    /** Есть свежая ссылка — трек заиграет сразу */
    fun ready(key: String) = cache[key]?.let { System.currentTimeMillis() - it.second < TTL } == true

    // плеер и предзагрузка могут попросить один трек одновременно — второй ждёт первого, а не достаёт заново
    private val locks = ConcurrentHashMap<String, Any>()

    fun urlFor(key: String): String = synchronized(locks.getOrPut(key) { Any() }) { fetch(key) }

    private fun fetch(key: String): String {
        cache[key]?.takeIf { System.currentTimeMillis() - it.second < TTL }?.let { return it.first }
        val t = tracks[key] ?: throw IOException("трек потерялся: $key")
        val page = coreNow("resolve", buildJsonObject { put("track", t) }).to<String>()
        val url = audio(page) ?: run {
            // SoundCloud отдаёт многие треки только кусками (HLS) — тогда тот же трек с YT Music
            val yt = coreNow("yt_match", buildJsonObject { put("track", t) }).to<String>()
            audio(yt) ?: throw IOException("нет звука")
        }
        cache[key] = url to System.currentTimeMillis()
        return url
    }

    // NewPipe не рассчитан на разбор нескольких страниц сразу: плеер и предзагрузка параллельно
    // иногда валили друг друга, и трек «не играл». Ходим по одному, честная очередь
    private val extract = java.util.concurrent.locks.ReentrantLock(true)

    internal fun audio(page: String): String? {
        val url = page.replace("music.youtube.com", "www.youtube.com")
        // есть вход YouTube — сначала под аккаунтом, как ПК; не вышло — по-старому, анонимно
        val id = Regex("(?:[?&]v=|youtu\\.be/)([\\w-]{11})").find(url)?.groupValues?.get(1)
        if (id != null && YtAuth.has()) {
            runCatching { return YtAuth.audio(id).also { authError = null } }.onFailure {
                android.util.Log.w("bh", "yt auth: $it")
                authError = it.message ?: it.toString()
            }
        }
        // разовые сбои разбора (сеть, YouTube отдал не то) — одна повторная попытка
        // на бан не повторяем — только хуже
        val info = extract.withLock {
            runCatching { StreamInfo.getInfo(NewPipe.getServiceByUrl(url), url) }
                .recoverCatching { if (it is SignInConfirmNotBotException) throw it; Thread.sleep(400); StreamInfo.getInfo(NewPipe.getServiceByUrl(url), url) }
                .onFailure { if (isBotBlock(it)) blockedUntil = System.currentTimeMillis() + 15 * 60_000L }
                .getOrThrow()
        }
        return info.audioStreams
            .filter { it.deliveryMethod == DeliveryMethod.PROGRESSIVE_HTTP && it.isUrl && it.content.isNotEmpty() }
            .maxByOrNull { it.averageBitrate }?.content
    }

    /** googlevideo проверяет, что ссылку качает тот же «клиент», которым она получена */
    internal fun headersFor(url: String): Map<String, String> {
        val client = Regex("[?&]c=([^&]+)").find(url)?.groupValues?.get(1) ?: ""
        return when {
            client.startsWith("ANDROID") -> mapOf("User-Agent" to "com.google.android.youtube/20.10.38 (Linux; U; Android 14) gzip")
            client.startsWith("TVHTML5") -> mapOf("User-Agent" to YtAuth.TV_UA)
            client.startsWith("IOS") -> mapOf("User-Agent" to "com.google.ios.youtube/20.10.4 (iPhone16,2; U; CPU iOS 18_3_2 like Mac OS X;)")
            url.contains("googlevideo") -> mapOf("User-Agent" to BROWSER_UA, "Origin" to "https://www.youtube.com", "Referer" to "https://www.youtube.com/")
            else -> mapOf("User-Agent" to BROWSER_UA)
        }
    }
}
