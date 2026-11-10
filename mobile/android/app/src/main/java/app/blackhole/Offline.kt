package app.blackhole

import android.content.Context
import android.net.Uri
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSpec
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.io.File
import java.io.IOException

/** Скачанное насовсем: в отличие от кеша не вытесняется и играет без сети, включая SoundCloud (его HLS
 *  сохраняется плейлистом с кусками). Лежит в files/offline: «<ключ>.a» — файл звука, «<ключ>.hls/» — HLS */
@UnstableApi
object Offline {
    private lateinit var dir: File
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var job: Job? = null
    private val queue = ArrayDeque<Track>()

    /** ключи скачанных треков */
    val have = MutableStateFlow<Set<String>>(emptySet())

    /** (готово, всего) в текущей пачке; null — ничего не качается */
    val progress = MutableStateFlow<Pair<Int, Int>?>(null)

    fun init(ctx: Context) {
        dir = File(ctx.filesDir, "offline").apply { mkdirs() }
        dir.listFiles { f -> f.name.endsWith(".part") }?.forEach { it.deleteRecursively() }
        scan()
    }

    private fun scan() {
        have.value = dir.listFiles().orEmpty().mapNotNull { f ->
            when {
                f.isFile && f.name.endsWith(".a") -> Uri.decode(f.name.removeSuffix(".a"))
                f.isDirectory && f.name.endsWith(".hls") && File(f, "index.m3u8").isFile -> Uri.decode(f.name.removeSuffix(".hls"))
                else -> null
            }
        }.toSet()
    }

    private fun audio(key: String) = File(dir, Uri.encode(key) + ".a")
    private fun hls(key: String) = File(dir, Uri.encode(key) + ".hls")

    /** Файл скачанного трека для плеера; null — не скачан */
    fun uri(key: String): Uri? = when {
        key !in have.value -> null
        audio(key).isFile -> Uri.fromFile(audio(key))
        else -> File(hls(key), "index.m3u8").takeIf { it.isFile }?.let(Uri::fromFile)
    }

    fun isHls(key: String) = hls(key).isDirectory

    fun size(): Long = dir.walkTopDown().filter { it.isFile }.sumOf { it.length() }

    fun clear() {
        stop()
        dir.listFiles()?.forEach { it.deleteRecursively() }
        scan()
    }

    fun stop() {
        synchronized(queue) { queue.clear() }
        job?.cancel()
        progress.value = null
    }

    /** Поставить треки в очередь на скачивание. Качаем по одному с паузой — YouTube не любит, когда его дёргают пачкой */
    fun download(tracks: List<Track>) {
        val add = tracks.filter { it.key !in have.value }
        if (add.isEmpty()) return
        synchronized(queue) { queue.addAll(add.filter { t -> queue.none { it.key == t.key } }) }
        if (job?.isActive == true) {
            progress.value = progress.value?.let { it.first to it.first + synchronized(queue) { queue.size } + 1 }
            return
        }
        job = scope.launch {
            var done = 0
            var failed = 0
            while (isActive) {
                val t = synchronized(queue) { queue.removeFirstOrNull() } ?: break
                progress.value = done to done + synchronized(queue) { queue.size } + 1
                if (Streams.blocked()) {
                    App.toasts.tryEmit(tr("YouTube временно не пускает — скачивание остановлено", "YouTube is blocking for now — download stopped", "YouTube тимчасово не пускає — завантаження зупинено", "YouTube dočasne blokuje — sťahovanie zastavené"))
                    synchronized(queue) { queue.clear() }
                    break
                }
                runCatching { one(t) }.onFailure {
                    failed++
                    BhLog.line("скачивание ${t.title}: $it")
                }
                done++
                scan()
                delay(700)
            }
            progress.value = null
            App.toasts.tryEmit(
                if (failed == 0) tr("Скачано: $done", "Downloaded: $done", "Завантажено: $done", "Stiahnuté: $done")
                else tr("Скачано: ${done - failed}, не вышло: $failed", "Downloaded: ${done - failed}, failed: $failed", "Завантажено: ${done - failed}, не вийшло: $failed", "Stiahnuté: ${done - failed}, zlyhalo: $failed"),
            )
        }
    }

    private fun one(t: Track) {
        Streams.tracks[t.key] = t
        val url = Streams.urlFor(t.key)
        if (Streams.isHls(url)) return saveHls(t.key, url)
        // читаем тем же путём, что и плеер: что уже лежит в кеше — берётся с диска, остальное — из сети
        val tmp = File(dir, Uri.encode(t.key) + ".part")
        val ds = App.cacheSource().createDataSource()
        try {
            ds.open(DataSpec.Builder().setUri(Uri.parse("bh://t/" + Uri.encode(t.key))).setKey(t.key).build())
            tmp.outputStream().use { out ->
                val buf = ByteArray(64 * 1024)
                while (true) {
                    val n = ds.read(buf, 0, buf.size)
                    if (n < 0) break
                    out.write(buf, 0, n)
                }
            }
        } finally {
            runCatching { ds.close() }
        }
        if (tmp.length() == 0L) {
            tmp.delete()
            throw IOException("пустой файл")
        }
        if (!tmp.renameTo(audio(t.key))) throw IOException("не переименовался")
        // в кеше вторая копия не нужна
        runCatching { App.cache.removeResource(t.key) }
    }

    /** HLS: плейлист и все куски — в папку, ссылки в плейлисте — на локальные файлы */
    private fun saveHls(key: String, url: String) {
        val tmp = File(dir, Uri.encode(key) + ".part").apply { deleteRecursively(); mkdirs() }
        fun get(u: String): ByteArray = App.http.newCall(okhttp3.Request.Builder().url(u).header("User-Agent", BROWSER_UA).build()).execute().use { r ->
            if (!r.isSuccessful) throw IOException("HTTP ${r.code}")
            r.body!!.bytes()
        }
        fun abs(u: String) = java.net.URI(url).resolve(u).toString()
        var n = 0
        var extra = 0
        val out = StringBuilder()
        for (line in String(get(url)).lines()) {
            val ref = Regex("URI=\"([^\"]+)\"").find(line)
            when {
                // заголовок fMP4 (EXT-X-MAP) и ключ (EXT-X-KEY) тоже нужны локально
                line.startsWith("#") && ref != null -> {
                    val name = "x${extra++}"
                    File(tmp, name).writeBytes(get(abs(ref.groupValues[1])))
                    out.append(line.replace(ref.groupValues[1], name)).append('\n')
                }
                line.isBlank() || line.startsWith("#") -> out.append(line).append('\n')
                else -> {
                    val name = "s${n++}"
                    File(tmp, name).writeBytes(get(abs(line.trim())))
                    out.append(name).append('\n')
                }
            }
        }
        if (n == 0) throw IOException("в плейлисте нет кусков")
        File(tmp, "index.m3u8").writeText(out.toString())
        hls(key).deleteRecursively()
        if (!tmp.renameTo(hls(key))) throw IOException("не переименовалось")
    }
}
