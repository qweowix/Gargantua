package app.blackhole

import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import java.util.concurrent.Executors

/** Журнал ошибок: тот же файл, куда пишет ядро (cache/BlackHole/log.txt). Из настроек его можно скопировать */
object BhLog {
    private val bg = Executors.newSingleThreadExecutor()

    private fun write(msg: String) = runCatching { coreNow("log", buildJsonObject { put("msg", JsonPrimitive(msg)) }) }

    fun line(msg: String) {
        android.util.Log.w("bh", msg)
        bg.execute { write(msg) }
    }

    /** Падения тоже в журнал — иначе после вылета нечего показать */
    fun install() {
        val prev = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { t, e ->
            write("ПАДЕНИЕ в ${t.name}: ${android.util.Log.getStackTraceString(e).take(4000)}")
            prev?.uncaughtException(t, e)
        }
    }

    suspend fun tail(): String {
        val head = "Gargantua ${BuildConfig.VERSION_NAME} · Android ${Build.VERSION.RELEASE} · ${Build.MANUFACTURER} ${Build.MODEL}\n"
        return head + runCatching { core("log_tail").to<String>() }.getOrDefault("")
    }
}

/** Обновление с GitHub Releases: новее нашей версии и с APK — качаем и отдаём системному установщику.
 *  Работает, только пока релизы открыты всем: у закрытого репозитория GitHub отвечает 404, и мы молчим */
object Updater {
    private const val REPO = "qweowix/Gargantua"
    private const val ASSET = "Gargantua-android-arm64.apk"

    /** версия новее нашей (пусто — нет) и её APK */
    val version = MutableStateFlow("")
    val busy = MutableStateFlow(false)
    private var url = ""

    private fun nums(v: String) = v.trimStart('v').split('.').map { p -> p.takeWhile { it.isDigit() }.toIntOrNull() ?: 0 }

    fun newer(tag: String, than: String): Boolean {
        val (a, b) = nums(tag) to nums(than)
        for (i in 0 until maxOf(a.size, b.size)) {
            val (x, y) = (a.getOrNull(i) ?: 0) to (b.getOrNull(i) ?: 0)
            if (x != y) return x > y
        }
        return false
    }

    suspend fun check() = withContext(Dispatchers.IO) {
        runCatching {
            val req = okhttp3.Request.Builder().url("https://api.github.com/repos/$REPO/releases/latest").header("User-Agent", "Gargantua").build()
            val v = App.http.newCall(req).execute().use { r ->
                if (!r.isSuccessful) return@runCatching
                json.parseToJsonElement(r.body!!.string()).jsonObject
            }
            val tag = v["tag_name"]?.jsonPrimitive?.contentOrNull ?: return@runCatching
            if (!newer(tag, BuildConfig.VERSION_NAME)) return@runCatching
            val apk = v["assets"]?.jsonArray.orEmpty().map { it.jsonObject }.firstOrNull { it["name"]?.jsonPrimitive?.contentOrNull == ASSET } ?: return@runCatching
            url = apk["browser_download_url"]?.jsonPrimitive?.contentOrNull ?: return@runCatching
            version.value = tag.trimStart('v')
        }.onFailure { BhLog.line("проверка обновлений: $it") }
    }

    /** Скачать APK и открыть установщик (Android сам спросит разрешение на установку из этого приложения) */
    suspend fun install(ctx: Context) {
        if (url.isEmpty() || busy.value) return
        busy.value = true
        val r = withContext(Dispatchers.IO) {
            runCatching {
                val f = File(ctx.cacheDir, "update.apk")
                val client = App.http.newBuilder().readTimeout(120, java.util.concurrent.TimeUnit.SECONDS).build()
                client.newCall(okhttp3.Request.Builder().url(url).header("User-Agent", "Gargantua").build()).execute().use { resp ->
                    if (!resp.isSuccessful) throw java.io.IOException("HTTP ${resp.code}")
                    f.outputStream().use { out -> resp.body!!.byteStream().copyTo(out) }
                }
                f
            }
        }
        busy.value = false
        r.onSuccess { f ->
            val uri = FileProvider.getUriForFile(ctx, "${ctx.packageName}.files", f)
            ctx.startActivity(
                Intent(Intent.ACTION_VIEW).setDataAndType(uri, "application/vnd.android.package-archive")
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK),
            )
        }.onFailure {
            BhLog.line("обновление: $it")
            App.toasts.tryEmit(tr("Не обновилось: ${it.message}", "Update failed: ${it.message}", "Не оновилося: ${it.message}", "Aktualizácia zlyhala: ${it.message}"))
        }
    }
}
