package app.blackhole

import okhttp3.Request
import org.mozilla.javascript.Context
import org.mozilla.javascript.Function
import org.mozilla.javascript.NativeArray
import org.mozilla.javascript.ScriptableObject
import java.io.File
import java.io.IOException

/**
 * Подпись и параметр n для свежих плееров YouTube. С ноября 2026 расшифровка в плеере размазана по функциям-диспетчерам
 * (`Vx(49,8822,…)`), и NewPipe, который выдирает одну функцию регэкспами, её не находит. Делаем как yt-dlp (EJS):
 * берём весь плеер, выкидываем из тела вызовы с побочными эффектами (остаются объявления и присваивания),
 * находим функцию, где отдельным оператором стоит `x.set("alr","yes")`, — она сама собирает ссылку и через класс URL
 * плеера расшифровывает и подпись, и n. Исполняем в Rhino (он уже едет с NewPipe), в безопасной области — без доступа к Java.
 * Плеер на диске и разобранный в памяти — по одному на хеш; разбор раз на версию плеера, дальше каждый трек мгновенно
 */
object YtSolver {
    private var dir: File? = null
    private var hash: String? = null
    private var checkedAt = 0L
    private var scope: ScriptableObject? = null
    private var run: Function? = null
    private var stsValue: Int? = null

    fun init(cache: File) {
        dir = cache
    }

    /** Подпись из signatureCipher (уже раскодированная) → настоящая */
    fun sig(s: String): String = solve(s, null).first ?: throw IOException("решатель YouTube: пустая подпись")

    /** Ссылка с сырым n → с расшифрованным (без него googlevideo режет скорость или отвечает 403) */
    fun nUrl(url: String): String {
        val m = Regex("([?&])n=([^&]+)").find(url) ?: return url
        val raw = java.net.URLDecoder.decode(m.groupValues[2], "UTF-8")
        val out = solve(null, raw).second ?: throw IOException("решатель YouTube: пустой n")
        if (out == raw || out.contains("_w8_")) throw IOException("решатель YouTube: n не расшифрован")
        return url.replaceRange(m.range, m.groupValues[1] + "n=" + java.net.URLEncoder.encode(out, "UTF-8"))
    }

    /** signatureTimestamp того же плеера — его надо слать в запросе, иначе подпись не сойдётся */
    fun sts(): Int? = runCatching { ensure(); stsValue }.getOrNull()

    @Synchronized
    private fun solve(sig: String?, n: String?): Pair<String?, String?> {
        ensure()
        try {
            return call(scope!!, run!!, sig, n)
        } catch (e: Exception) {
            // плеер мог смениться — следующий раз возьмём свежий
            checkedAt = 0
            throw IOException("решатель YouTube: ${e.message}", e)
        }
    }

    private fun enter(): Context = Context.enter().apply {
        @Suppress("DEPRECATION")
        setOptimizationLevel(-1) // Android не грузит сгенерированный байткод — только интерпретатор
        languageVersion = Context.VERSION_ES6
    }

    /** Актуальный плеер разобран и готов. Хеш сверяем не чаще раза в 6 часов */
    @Synchronized
    private fun ensure() {
        if (run != null && System.currentTimeMillis() - checkedAt < 6 * 3600_000L) return
        val h = currentHash()
        checkedAt = System.currentTimeMillis()
        if (h == hash && run != null) return
        val js = playerCode(h)
        val t0 = System.currentTimeMillis()
        val code = prepare(js)
        stsValue = Regex("signatureTimestamp[=:](\\d+)").find(js)?.groupValues?.get(1)?.toIntOrNull()
        val (sc, fn) = try {
            load(code, "player-$h")
        } catch (e: Throwable) {
            throw IOException("решатель YouTube не разобрал плеер $h: ${e.message}", e)
        }
        scope = sc
        run = fn
        hash = h
        BhLog.line("YouTube: плеер $h разобран решателем за ${System.currentTimeMillis() - t0} мс")
    }

    /** Исполнить урезанный плеер: область и функция-решатель. Разбор трёх мегабайт рекурсивный — на обычном стеке
     *  Rhino может упасть, поэтому отдельный поток с большим стеком */
    internal fun load(code: String, name: String): Pair<ScriptableObject, Function> {
        var err: Throwable? = null
        var out: Pair<ScriptableObject, Function>? = null
        val th = Thread(null, {
            val cx = enter()
            try {
                val s = cx.initSafeStandardObjects()
                cx.evaluateString(s, code, name, 1, null)
                val res = ScriptableObject.getProperty(s, "_result") as ScriptableObject
                out = s to (ScriptableObject.getProperty(res, "run") as Function)
            } catch (e: Throwable) {
                err = e
            } finally {
                Context.exit()
            }
        }, "yt-solver", 64L shl 20)
        th.start()
        th.join()
        err?.let { throw it }
        return out!!
    }

    /** Вызвать решатель: (подпись, n) → (подпись, n); пустые — null */
    internal fun call(sc: ScriptableObject, fn: Function, sig: String?, n: String?): Pair<String?, String?> {
        val cx = enter()
        try {
            val out = fn.call(cx, sc, sc, arrayOf<Any?>(sig, n)) as NativeArray
            return (out[0] as? CharSequence)?.toString()?.takeIf { it.isNotEmpty() } to (out[1] as? CharSequence)?.toString()?.takeIf { it.isNotEmpty() }
        } finally {
            Context.exit()
        }
    }

    private fun get(url: String): String {
        val r = App.http.newCall(Request.Builder().url(url).header("User-Agent", BROWSER_UA).build()).execute()
        return r.use {
            if (!it.isSuccessful) throw IOException("YouTube HTTP ${it.code} ($url)")
            it.body!!.string()
        }
    }

    private fun currentHash(): String {
        val iframe = get("https://www.youtube.com/iframe_api")
        return Regex("player\\\\?/([a-z0-9]{8})\\\\?/").find(iframe)?.groupValues?.get(1) ?: throw IOException("YouTube: не нашёл плеер")
    }

    private fun playerCode(h: String): String {
        val d = dir ?: throw IOException("решатель не готов")
        val f = File(d, "ytplayer-$h.js")
        if (f.isFile && f.length() > 100_000) return f.readText()
        val js = get("https://www.youtube.com/s/player/$h/player_ias.vflset/en_GB/base.js")
        d.listFiles { x -> x.name.startsWith("ytplayer-") }?.forEach { it.delete() }
        f.writeText(js)
        return js
    }

    private const val SETUP = "var globalThis=this;var _result={};" +
        "if(typeof XMLHttpRequest===\"undefined\"){XMLHttpRequest={prototype:{}};}" +
        "var location={hash:\"\",host:\"www.youtube.com\",hostname:\"www.youtube.com\",href:\"https://www.youtube.com/watch?v=yt-dlp-wins\"," +
        "origin:\"https://www.youtube.com\",password:\"\",pathname:\"/watch\",port:\"\",protocol:\"https:\",search:\"?v=yt-dlp-wins\",username:\"\"};" +
        "if(typeof document===\"undefined\"){var document=Object.create(null);}" +
        "if(typeof navigator===\"undefined\"){var navigator=Object.create(null);}" +
        "var self=this;var window=this;\n"

    // как solver у yt-dlp: ссылку с подписью собирает сам плеер, n меняет первый «свой» метод класса URL
    private const val RUN = "\n_result.run=function(sig,n){if(n==null)n=undefined;var res=null,err=null;" +
        "for(var i=0;i<_result.cands.length;i++){try{var url=_result.cands[i](\"https://youtube.com/watch?v=yt-dlp-wins\",\"s\",sig?encodeURIComponent(sig):undefined);" +
        "url.set(\"n\",n);var proto=Object.getPrototypeOf(url);var keys=Object.keys(proto).concat(Object.getOwnPropertyNames(proto));" +
        "for(var k=0;k<keys.length;k++){var key=keys[k];if([\"constructor\",\"set\",\"get\",\"clone\"].indexOf(key)<0){url[key]();break;}}" +
        "var s=url.get(\"s\");var r=[s?decodeURIComponent(s):\"\",n?String(url.get(\"n\")||\"\"):\"\"];" +
        "if(res&&(res[0]!==r[0]||res[1]!==r[1]))throw \"разные ответы\";res=r;}catch(e){err=e;}}" +
        "if(!res)throw \"нет решения: \"+err;return res;};"

    /** Плеер → урезанный скрипт: определения и присваивания тела, кандидаты в _result.cands, решатель _result.run */
    fun prepare(js: String): String {
        val head = js.indexOf("(function(g){")
        if (head < 0) throw IOException("плеер YouTube: незнакомая обёртка")
        val start = head + "(function(g){".length
        val end = js.lastIndexOf("})(_yt_player)")
        if (end <= start) throw IOException("плеер YouTube: незнакомая обёртка")
        val kept = StringBuilder()
        val cands = mutableListOf<String>()
        for ((b, e) in statements(js, start, end)) {
            val s = js.substring(b, e)
            if (s.startsWith("var window=this")) continue
            if (!keep(s)) continue
            kept.append(s).append(";\n")
            alrName(s)?.let { cands += it }
        }
        if (cands.isEmpty()) throw IOException("плеер YouTube: не нашёл функцию ссылки (alr)")
        return SETUP + "var _yt_player={};(function(g){" + kept + "_result.cands=[" + cands.joinToString(",") + "];})(_yt_player);" + RUN
    }

    private val WORD = Regex("^[A-Za-z_$][\\w$]*")
    private val LABEL = Regex("^[A-Za-z_$][\\w$]*\\s*:(?!:)")
    private val STMT_WORDS = setOf("var", "let", "const", "function", "if", "for", "while", "do", "try", "switch", "class", "return", "throw", "break", "continue")

    /** Как у EJS: выкидываем только операторы-выражения, кроме одиночного присваивания и строки ('use strict') */
    internal fun keep(s: String): Boolean {
        val w = WORD.find(s)?.value
        if (w != null && w in STMT_WORDS) return true
        if (s[0] == '{' || s[0] == ';' || s[0] == '\'' || s[0] == '"') return true
        if (LABEL.containsMatchIn(s)) return true
        return isAssignment(s)
    }

    private val FUNC = Regex("^([\\w$.]+)=function\\([^)]*\\)\\{")
    private val ALR = Regex("^[A-Za-z_$][\\w$]*\\.set\\(\"alr\",\"yes\"\\)$")

    /** `имя=function(…){…}`, где отдельным оператором тела стоит `x.set("alr","yes")` → имя */
    internal fun alrName(s: String): String? {
        if (!s.contains("\"alr\",\"yes\"")) return null
        val m = FUNC.find(s) ?: return null
        val close = s.trimEnd(';').lastIndexOf('}')
        if (close <= m.range.last) return null
        return m.groupValues[1].takeIf { statements(s, m.range.last + 1, close).any { (b, e) -> ALR.matches(s.substring(b, e).trimEnd(';')) } }
    }

    private val BLOCK_STARTS = setOf("function", "if", "for", "while", "try", "switch", "{", "class", "do", "label")
    private val REGEX_AFTER_WORD = setOf("return", "typeof", "case", "do", "else", "in", "of", "new", "delete", "void", "throw", "instanceof", "yield", "await")
    private const val REGEX_AFTER_PUNCT = "(,=:[!&|?{};+-*%<>~^"

    /** Режет src[start, end) на операторы верхнего уровня: строки, шаблоны, комментарии и регэкспы не путают глубину */
    internal fun statements(src: String, start: Int, end: Int): List<Pair<Int, Int>> {
        val out = ArrayList<Pair<Int, Int>>()
        var i = start
        var depth = 0
        var begin = -1
        var firstKw: String? = null
        var last = "" // последний значимый токен: «/» после него — деление или регэксп
        fun finish(j: Int) {
            if (begin >= 0) out += begin to j
            begin = -1
            firstKw = null
        }
        while (i < end) {
            val c = src[i]
            if (c == ' ' || c == '\t' || c == '\r' || c == '\n') { i++; continue }
            if (c == '/' && i + 1 < end && src[i + 1] == '/') { val j = src.indexOf('\n', i); i = if (j < 0 || j > end) end else j; continue }
            if (c == '/' && i + 1 < end && src[i + 1] == '*') { val j = src.indexOf("*/", i + 2); i = if (j < 0 || j > end) end else j + 2; continue }
            if (begin < 0) {
                begin = i
                val w = WORD.find(src.subSequence(i, minOf(end, i + 40)))?.value
                firstKw = when {
                    w != null && LABEL.containsMatchIn(src.subSequence(i, minOf(end, i + 40))) && w !in STMT_WORDS -> "label"
                    w != null -> w
                    else -> c.toString()
                }
            }
            when {
                c == '\'' || c == '"' -> { i = skipString(src, i, end); last = "str" }
                c == '`' -> { i = skipTemplate(src, i + 1, end); last = "str" }
                c == '/' -> {
                    val regex = last.isEmpty() || last in REGEX_AFTER_WORD || (last.length == 1 && last[0] in REGEX_AFTER_PUNCT)
                    if (regex) {
                        var j = i + 1
                        var cls = false
                        while (j < end) {
                            val ch = src[j]
                            if (ch == '\\') { j += 2; continue }
                            if (ch == '[') cls = true else if (ch == ']') cls = false else if (ch == '/' && !cls) break
                            j++
                        }
                        j++
                        while (j < end && (src[j].isLetterOrDigit() || src[j] == '_' || src[j] == '$')) j++
                        i = j
                        last = "re"
                    } else { i++; last = "/" }
                }
                c == '(' || c == '[' || c == '{' -> { depth++; i++; last = c.toString() }
                c == ')' || c == ']' || c == '}' -> {
                    depth--
                    i++
                    last = c.toString()
                    if (c == '}' && depth == 0 && firstKw in BLOCK_STARTS) {
                        // блок верхнего уровня закрылся: оператор кончился, если дальше не else/catch/finally/while у do
                        var k = i
                        while (k < end && src[k].isWhitespace()) k++
                        val nxt = WORD.find(src.subSequence(k, minOf(end, k + 10)))?.value ?: (if (k < end) src[k].toString() else "")
                        if (nxt != "else" && nxt != "catch" && nxt != "finally" && nxt != ";" && !(firstKw == "do" && nxt == "while")) finish(i)
                    }
                }
                c == ';' && depth == 0 -> { finish(i + 1); i++; last = ";" }
                else -> {
                    val tok = token(src, i, end)
                    i += tok.length
                    last = tok
                }
            }
        }
        finish(end)
        return out
    }

    private val TOKEN = Regex("[A-Za-z_$][\\w$]*|\\d[\\w.]*|===|!==|==|!=|<=|>=|=>|&&|\\|\\||\\?\\?|\\+\\+|--|\\*\\*|<<|>>>|>>|.", RegexOption.DOT_MATCHES_ALL)

    private fun token(src: String, i: Int, end: Int): String = TOKEN.find(src.subSequence(i, minOf(end, i + 64)))?.value ?: src[i].toString()

    private fun skipString(src: String, i: Int, end: Int): Int {
        val q = src[i]
        var j = i + 1
        while (j < end && src[j] != q) j += if (src[j] == '\\') 2 else 1
        return j + 1
    }

    private fun skipTemplate(src: String, from: Int, end: Int): Int {
        var i = from
        while (i < end) {
            val ch = src[i]
            if (ch == '\\') { i += 2; continue }
            if (ch == '`') return i + 1
            if (ch == '$' && i + 1 < end && src[i + 1] == '{') {
                var d = 1
                i += 2
                while (i < end && d > 0) {
                    when (src[i]) {
                        '\'', '"' -> { i = skipString(src, i, end); continue }
                        '`' -> { i = skipTemplate(src, i + 1, end); continue }
                        '{' -> d++
                        '}' -> d--
                    }
                    i++
                }
                continue
            }
            i++
        }
        return end
    }

    private val ASSIGN_OPS = listOf(">>>=", "<<=", ">>=", "**=", "&&=", "||=", "??=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "=")

    /** Оператор-выражение — ровно одно присваивание верхнего уровня (последовательность через запятую — нет: так и у EJS) */
    internal fun isAssignment(s: String): Boolean {
        var depth = 0
        var i = 0
        var seen = false
        val n = s.length
        while (i < n) {
            val c = s[i]
            when {
                c == '\'' || c == '"' -> { i = skipString(s, i, n); continue }
                c == '`' -> { i = skipTemplate(s, i + 1, n); continue }
                c == '(' || c == '[' || c == '{' -> depth++
                c == ')' || c == ']' || c == '}' -> depth--
                depth == 0 && c == ',' -> return false
                depth == 0 && !seen && c == '?' -> return false
                depth == 0 && !seen && (s.startsWith("==", i) || s.startsWith("!=", i) || s.startsWith("=>", i) || s.startsWith("<=", i) || s.startsWith(">=", i)) -> {
                    if (s.startsWith("==", i) || s.startsWith("!=", i)) return false
                    i += 2
                    continue
                }
                depth == 0 && !seen -> {
                    val op = ASSIGN_OPS.firstOrNull { s.startsWith(it, i) }
                    if (op != null && !(op == "=" && i + 1 < n && (s[i + 1] == '=' || s[i + 1] == '>'))) {
                        seen = true
                        i += op.length
                        continue
                    }
                }
            }
            i++
        }
        return seen
    }
}
