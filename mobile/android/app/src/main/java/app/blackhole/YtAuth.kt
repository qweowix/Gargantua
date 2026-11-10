package app.blackhole

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.schabi.newpipe.extractor.services.youtube.YoutubeJavaScriptPlayerManager
import java.io.IOException
import java.net.URLDecoder
import java.security.MessageDigest

/** Вход в YouTube: куки аккаунта (перенесённые с компа или из окна входа). С ними звук берём как
 *  yt-dlp на ПК — TV-клиентом под аккаунтом, и анонимный бан по IP («подтверди, что не бот») не мешает */
object YtAuth {
    // клиент «телевизор Samsung», как у свежего yt-dlp: обычный TV-клиент YouTube сломал («page needs to be reloaded»)
    const val TV_UA = "Mozilla/5.0 (SMART-TV; Linux; Tizen 2.4.0) AppleWebKit/538.1 (KHTML, like Gecko) Version/2.4.0 TV Safari/538.1"
    private const val TV_VER = "5.20260707"
    private const val ORIGIN = "https://www.youtube.com"

    private lateinit var prefs: android.content.SharedPreferences
    private var jar: Map<String, String> = emptyMap()

    /** вошли ли — для экрана настроек */
    val on = MutableStateFlow(false)

    fun init(ctx: Context) {
        prefs = ctx.getSharedPreferences("yt", Context.MODE_PRIVATE)
        jar = runCatching { Json.decodeFromString<Map<String, String>>(prefs.getString("cookies", "") ?: "") }.getOrDefault(emptyMap())
        on.value = has()
    }

    fun has() = jar.containsKey("SAPISID") || jar.containsKey("__Secure-3PAPISID")

    /** [[имя, значение], …] — как отдаёт комп */
    fun setFromPc(pairs: String) {
        val m = Json.parseToJsonElement(pairs).jsonArray.associate { it.jsonArray[0].jsonPrimitive.content to it.jsonArray[1].jsonPrimitive.content }
        save(m)
        prefs.edit().putBoolean("from_pc", true).apply()
    }

    @Volatile
    private var pulledAt = 0L

    /** Вход взят с компа — подтянуть свежий (комп сам обновляет его в аккаунте раз в 20 минут). Блокирующе,
     *  не чаще раза в minGap. true — куки обновились */
    fun refreshFromPc(minGap: Long): Boolean {
        // установки до этой версии флага не знают: считаем, что вход с компа, — если комп ничего не отправлял, в аккаунте пусто и мы ничего не тронем
        if (!prefs.getBoolean("from_pc", true) || System.currentTimeMillis() - pulledAt < minGap) return false
        pulledAt = System.currentTimeMillis()
        val ck = runCatching { coreNow("yt_cookies").to<String>() }.onFailure { BhLog.line("вход YouTube с компа: $it") }.getOrNull()
        if (ck.isNullOrEmpty()) return false
        val before = jar
        runCatching { setFromPc(ck) }
        return jar != before
    }

    /** "a=b; c=d" — как отдаёт CookieManager после входа */
    fun setFromHeader(h: String) {
        save(parse(h))
        prefs.edit().putBoolean("from_pc", false).apply()
    }

    fun clear() {
        save(emptyMap())
        prefs.edit().putBoolean("from_pc", false).apply()
    }

    fun parse(h: String) = h.split(';').mapNotNull { p -> p.trim().split('=', limit = 2).takeIf { it.size == 2 && it[0].isNotEmpty() }?.let { it[0] to it[1] } }.toMap()

    @Synchronized
    private fun save(m: Map<String, String>) {
        jar = m
        prefs.edit().putString("cookies", Json.encodeToString(m)).apply()
        on.value = has()
    }

    /** Google ротирует часть кук — свежие из ответа сохраняем, иначе вход быстро протухнет */
    private fun absorb(setCookies: List<String>) {
        if (setCookies.isEmpty()) return
        val upd = setCookies.mapNotNull { it.substringBefore(';').split('=', limit = 2).takeIf { p -> p.size == 2 && p[1].isNotEmpty() }?.let { p -> p[0] to p[1] } }
        if (upd.isNotEmpty()) save(jar + upd)
    }

    private fun sha1(s: String) = MessageDigest.getInstance("SHA-1").digest(s.toByteArray()).joinToString("") { "%02x".format(it) }

    /** Заголовок Authorization как у yt-dlp: SAPISIDHASH + 1P/3P-варианты */
    private fun authorization(): String {
        val ts = (System.currentTimeMillis() / 1000).toString()
        return listOf("SAPISIDHASH" to (jar["SAPISID"] ?: jar["__Secure-3PAPISID"]), "SAPISID1PHASH" to jar["__Secure-1PAPISID"], "SAPISID3PHASH" to jar["__Secure-3PAPISID"])
            .mapNotNull { (scheme, sid) -> sid?.let { "$scheme ${ts}_${sha1("$ts $it $ORIGIN")}" } }
            .joinToString(" ")
    }

    /** Ответ плеера YouTube. params=2AMB: без рекламы, и ссылка открыта сразу, а не через ~4 с (замер на ПК).
     *  Параметр недокументирован: если с ним не вышло, спросим ещё раз без него */
    private fun player(id: String, sts: Int?, noAds: Boolean): kotlinx.serialization.json.JsonObject {
        val body = buildJsonObject {
            putJsonObject("context") {
                putJsonObject("client") {
                    put("clientName", "TVHTML5")
                    put("clientVersion", TV_VER)
                    put("userAgent", TV_UA)
                    put("deviceMake", "Samsung")
                    put("deviceModel", "UKS9800")
                    put("osName", "Tizen")
                    put("osVersion", "2.4.0")
                    put("hl", "en")
                }
            }
            put("videoId", id)
            if (noAds) put("params", "2AMB")
            putJsonObject("playbackContext") {
                putJsonObject("contentPlaybackContext") {
                    put("html5Preference", "HTML5_PREF_WANTS")
                    if (sts != null) put("signatureTimestamp", sts)
                }
            }
            put("contentCheckOk", true)
            put("racyCheckOk", true)
        }
        val req = Request.Builder()
            .url("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
            .post(body.toString().toRequestBody("application/json".toMediaType()))
            .header("User-Agent", TV_UA)
            .header("X-YouTube-Client-Name", "7")
            .header("X-YouTube-Client-Version", TV_VER)
            .header("Origin", ORIGIN)
            .header("X-Origin", ORIGIN)
            .header("Authorization", authorization())
            .header("Cookie", jar.entries.joinToString("; ") { "${it.key}=${it.value}" })
            .build()
        return App.http.newCall(req).execute().use { r ->
            absorb(r.headers("Set-Cookie"))
            if (!r.isSuccessful) throw IOException("YouTube HTTP ${r.code}")
            Json.parseToJsonElement(r.body!!.string()).jsonObject
        }
    }

    /** params=2AMB недокументирован. Сломался (ссылка не открывается или отдаёт только начало) — сутки берём
     *  ссылки по-старому, потом пробуем снова */
    fun noAdsOk() = System.currentTimeMillis() - prefs.getLong("no2amb", 0) > 24 * 3600_000L

    fun noAdsOff(why: String) {
        BhLog.line("YouTube: ссылка без рекламы сломалась ($why) — сутки работаю без 2AMB")
        prefs.edit().putLong("no2amb", System.currentTimeMillis()).apply()
    }

    private fun playable(v: kotlinx.serialization.json.JsonObject) = v["playabilityStatus"]?.jsonObject?.get("status")?.jsonPrimitive?.contentOrNull == "OK"

    /** id видео → прямая ссылка на лучший звук. Ошибки — исключением (тогда пробуем анонимно через NewPipe) */
    fun audio(id: String): String {
        val sts = runCatching { YoutubeJavaScriptPlayerManager.getSignatureTimestamp(id) }.getOrNull()
        val v = (if (noAdsOk()) runCatching { player(id, sts, true) }.getOrNull()?.takeIf(::playable) else null) ?: player(id, sts, false)
        val ps = v["playabilityStatus"]?.jsonObject
        val status = ps?.get("status")?.jsonPrimitive?.contentOrNull
        if (status != "OK") throw IOException("YouTube: ${ps?.get("reason")?.jsonPrimitive?.contentOrNull ?: status}")
        val best = v["streamingData"]?.jsonObject?.get("adaptiveFormats")?.jsonArray.orEmpty()
            .map { it.jsonObject }
            .filter { it["mimeType"]?.jsonPrimitive?.contentOrNull?.startsWith("audio/") == true }
            .maxByOrNull { it["bitrate"]?.jsonPrimitive?.intOrNull ?: 0 }
            ?: throw IOException("YouTube не дал звук")
        val raw = best["url"]?.jsonPrimitive?.contentOrNull ?: run {
            // зашифрованная подпись: расшифровывает плеер YouTube, его разбор есть в NewPipe
            val c = best["signatureCipher"]?.jsonPrimitive?.contentOrNull ?: throw IOException("нет ссылки")
            val q = c.split('&').associate { it.substringBefore('=') to URLDecoder.decode(it.substringAfter('='), "UTF-8") }
            val sig = YoutubeJavaScriptPlayerManager.deobfuscateSignature(id, q["s"] ?: throw IOException("нет подписи"))
            "${q["url"]}&${q["sp"] ?: "signature"}=$sig"
        }
        // параметр n — без расшифровки googlevideo режет скорость до улитки
        return YoutubeJavaScriptPlayerManager.getUrlWithThrottlingParameterDeobfuscated(id, raw)
    }
}
