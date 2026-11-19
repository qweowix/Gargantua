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
    private fun player(id: String, sts: Int?, noAds: Boolean, acc: Boolean): kotlinx.serialization.json.JsonObject {
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
                    if (!acc) visitor()?.let { put("visitorData", it) }
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
        val b = Request.Builder()
            .url("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
            .post(body.toString().toRequestBody("application/json".toMediaType()))
            .header("User-Agent", TV_UA)
            .header("X-YouTube-Client-Name", "7")
            .header("X-YouTube-Client-Version", TV_VER)
            .header("Origin", ORIGIN)
            .header("X-Origin", ORIGIN)
        if (acc) {
            b.header("Authorization", authorization()).header("Cookie", jar.entries.joinToString("; ") { "${it.key}=${it.value}" })
        } else {
            visitor()?.let { b.header("X-Goog-Visitor-Id", it) }
        }
        return App.http.newCall(b.build()).execute().use { r ->
            if (acc) absorb(r.headers("Set-Cookie"))
            if (!r.isSuccessful) throw IOException("YouTube HTTP ${r.code}")
            Json.parseToJsonElement(r.body!!.string()).jsonObject.also { v ->
                // без аккаунта YouTube узнаёт нас по visitorData: один и тот же «гость» выглядит живым телевизором,
                // а не новым ботом на каждый трек
                if (!acc && visitor() == null) v["responseContext"]?.jsonObject?.get("visitorData")?.jsonPrimitive?.contentOrNull?.let { setVisitor(it) }
            }
        }
    }

    /** params=2AMB недокументирован. Сломался (ссылка не открывается или отдаёт только начало) — сутки берём
     *  ссылки по-старому, потом пробуем снова */
    fun noAdsOk() = System.currentTimeMillis() - prefs.getLong("no2amb", 0) > 24 * 3600_000L

    fun noAdsOff(why: String) {
        BhLog.line("YouTube: ссылка без рекламы сломалась ($why) — сутки работаю без 2AMB")
        prefs.edit().putLong("no2amb", System.currentTimeMillis()).apply()
    }

    private fun visitor(): String? = prefs.getString("visitor", null)?.takeIf { System.currentTimeMillis() - prefs.getLong("visitor_at", 0) < 7 * 24 * 3600_000L }

    private fun setVisitor(v: String) = prefs.edit().putString("visitor", v).putLong("visitor_at", System.currentTimeMillis()).apply()

    /** гостя забанили — следующий раз YouTube выдаст нового */
    fun dropVisitor() = prefs.edit().remove("visitor").apply()

    private fun playable(v: kotlinx.serialization.json.JsonObject) = v["playabilityStatus"]?.jsonObject?.get("status")?.jsonPrimitive?.contentOrNull == "OK"

    private fun reason(v: kotlinx.serialization.json.JsonObject): String {
        val ps = v["playabilityStatus"]?.jsonObject
        return ps?.get("reason")?.jsonPrimitive?.contentOrNull ?: ps?.get("status")?.jsonPrimitive?.contentOrNull ?: "?"
    }

    /** «Sign in to confirm you’re not a bot»: этот путь на время закрыт, а не трек плохой */
    private fun isBot(v: kotlinx.serialization.json.JsonObject): Boolean {
        if (v["playabilityStatus"] == null || playable(v)) return false
        return reason(v).contains("not a bot", ignoreCase = true)
    }

    /** id видео → прямая ссылка на лучший звук. acc — под аккаунтом (куки), иначе гостем-телевизором.
     *  Ошибки — исключением; «подтверди, что не бот» — [BotCheck] */
    fun audio(id: String, acc: Boolean): String = try {
        audioOnce(id, acc)
    } catch (e: org.schabi.newpipe.extractor.exceptions.ParsingException) {
        // YouTube сменил плеер, а у NewPipe в памяти старый — разбор подписи падает. Сбрасываем и берём свежий
        BhLog.line("YouTube: плеер устарел ($e) — беру свежий")
        YoutubeJavaScriptPlayerManager.clearAllCaches()
        audioOnce(id, acc)
    }

    private fun audioOnce(id: String, acc: Boolean): String {
        val sts = runCatching { YoutubeJavaScriptPlayerManager.getSignatureTimestamp(id) }.getOrNull() ?: YtSolver.sts()
        // на бан второй запрос без 2AMB не шлём — он только продлит бан
        val first = if (noAdsOk()) runCatching { player(id, sts, true, acc) }.getOrNull() else null
        if (first != null && isBot(first)) throw BotCheck(reason(first))
        val v = first?.takeIf(::playable) ?: player(id, sts, false, acc)
        val ps = v["playabilityStatus"]?.jsonObject
        val status = ps?.get("status")?.jsonPrimitive?.contentOrNull
        if (isBot(v)) throw BotCheck(reason(v))
        if (status != "OK") throw IOException("YouTube: ${reason(v)}")
        val best = v["streamingData"]?.jsonObject?.get("adaptiveFormats")?.jsonArray.orEmpty()
            .map { it.jsonObject }
            .filter { it["mimeType"]?.jsonPrimitive?.contentOrNull?.startsWith("audio/") == true }
            .maxByOrNull { it["bitrate"]?.jsonPrimitive?.intOrNull ?: 0 }
            ?: throw IOException("YouTube не дал звук")
        val raw = best["url"]?.jsonPrimitive?.contentOrNull ?: run {
            // зашифрованная подпись: расшифровывает плеер YouTube, его разбор есть в NewPipe
            val c = best["signatureCipher"]?.jsonPrimitive?.contentOrNull ?: throw IOException("нет ссылки")
            val q = c.split('&').associate { it.substringBefore('=') to URLDecoder.decode(it.substringAfter('='), "UTF-8") }
            val sig = deobfuscate(id, q["s"] ?: throw IOException("нет подписи"))
            "${q["url"]}&${q["sp"] ?: "signature"}=$sig"
        }
        // параметр n — без расшифровки googlevideo режет скорость до улитки
        return throttle(id, raw)
    }

    /** Когда NewPipe не смог разобрать плеер: пока он такой, сразу идём к решателю (раз в 6 ч пробуем NewPipe снова) */
    @Volatile
    private var npBrokenAt = 0L

    private fun npBroken() = System.currentTimeMillis() - npBrokenAt < 6 * 3600_000L

    private fun npFailed(e: Throwable) {
        if (!npBroken()) BhLog.line("YouTube: NewPipe не разобрал плеер (${e.message}) — расшифровываю как yt-dlp")
        npBrokenAt = System.currentTimeMillis()
    }

    private fun deobfuscate(id: String, s: String): String {
        if (!npBroken()) {
            try {
                return YoutubeJavaScriptPlayerManager.deobfuscateSignature(id, s)
            } catch (e: org.schabi.newpipe.extractor.exceptions.ParsingException) {
                npFailed(e)
            }
        }
        return YtSolver.sig(s)
    }

    private fun throttle(id: String, url: String): String {
        if (!npBroken()) {
            try {
                return YoutubeJavaScriptPlayerManager.getUrlWithThrottlingParameterDeobfuscated(id, url)
            } catch (e: org.schabi.newpipe.extractor.exceptions.ParsingException) {
                npFailed(e)
            }
        }
        return YtSolver.nUrl(url)
    }
}

/** YouTube просит подтвердить, что не бот: этот путь к звуку на время закрыт */
class BotCheck(why: String) : IOException("YouTube: $why")
