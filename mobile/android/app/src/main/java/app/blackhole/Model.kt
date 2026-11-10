package app.blackhole

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.serialization.Serializable

// Те же структуры, что в src/model.rs: плейлисты в облаке общие с плеером на ПК

@Serializable
data class Track(
    val src: String = "yt",
    val id: String = "",
    val title: String = "",
    val artist: String = "",
    val dur: Long = 0,
    val cover: String = "",
    val url: String = "",
    val snip: Boolean = false,
    val artist_id: String = "",
) {
    val key get() = "$src:$id"
}

@Serializable
data class Source(val url: String = "", val keys: List<String> = emptyList())

@Serializable
data class Playlist(
    val name: String = "",
    val tracks: List<Track> = emptyList(),
    val source: Source? = null,
    val desc: String = "",
    val cover: String = "",
)

@Serializable
data class Card(val id: String = "", val title: String = "", val sub: String = "", val cover: String = "")

@Serializable
data class Cards(val artists: List<Card> = emptyList(), val albums: List<Card> = emptyList())

@Serializable
data class Artist(
    val id: String = "",
    val name: String = "",
    val desc: String = "",
    val photo: String = "",
    val stats: String = "",
    val top: List<Track> = emptyList(),
    val songs_id: String? = null,
    val albums: List<Card> = emptyList(),
    val singles: List<Card> = emptyList(),
    val similar: List<Card> = emptyList(),
)

@Serializable
data class Album(val title: String = "", val sub: String = "", val cover: String = "", val tracks: List<Track> = emptyList())

@Serializable
data class Mix(val title: String = "", val sub: String = "", val cover: String = "", val tracks: List<Track> = emptyList())

@Serializable
data class Home(val mixes: List<Mix> = emptyList(), val similar_to: String = "", val similar: List<Card> = emptyList())

@Serializable
data class Word(val s: Double = 0.0, val e: Double = 0.0, val a: Int = 0, val b: Int = 0)

/** Строка как на ПК: end — когда перестаёт быть активной, fill — докуда заливается без таймингов слов,
 *  dots — точки в проигрыше, right — второй исполнитель дуэта, слова — отрезки [a, b) текста */
@Serializable
data class LyrLine(
    val start: Double = 0.0,
    val end: Double = 0.0,
    val fill: Double = 0.0,
    val text: String = "",
    val bg: Boolean = false,
    val right: Boolean = false,
    val dots: Boolean = false,
    val words: List<Word> = emptyList(),
)

@Serializable
data class Lyrics(
    val kind: String = "none",
    val credit: String = "",
    val lines: List<LyrLine> = emptyList(),
    val plain: List<String> = emptyList(),
)

@Serializable
data class TopTrack(val track: Track = Track(), val plays: Long = 0, val ms: Long = 0)

@Serializable
data class TopArtist(val name: String = "", val ms: Long = 0)

@Serializable
data class StatsSummary(
    val today: Long = 0,
    val week: Long = 0,
    val month: Long = 0,
    val total: Long = 0,
    val heat: List<Int> = emptyList(),
    val top: List<TopTrack> = emptyList(),
    val artists: List<TopArtist> = emptyList(),
    val fav: FavArtist? = null,
)

@Serializable
data class RecapTrack(val track: Track = Track(), val ms: Long = 0, val plays: Int = 0)

/** Итоги месяца (или года): время и дни точные, топы копятся с версии 0.3 */
@Serializable
data class Recap(
    val year: Int = 0,
    val month: Int = 0,
    val ms: Long = 0,
    val active: Int = 0,
    val count: Int = 0,
    val tracks: List<RecapTrack> = emptyList(),
    val artists: List<TopArtist> = emptyList(),
)

/** Любимый артист и его трек, который включали чаще всех */
@Serializable
data class FavArtist(val name: String = "", val ms: Long = 0, val track: Track? = null, val plays: Int = 0)

@Serializable
data class Imported(val name: String = "", val tracks: List<Track> = emptyList(), val missing: Int = 0, val url: String = "")

// ── язык ──

object L {
    var lang by mutableStateOf("en")
}

/** Как tr! на ПК: строка на языке интерфейса (Compose перерисуется при смене языка) */
fun tr(ru: String, en: String, uk: String, sk: String) = when (L.lang) {
    "ru" -> ru
    "uk" -> uk
    "sk" -> sk
    else -> en
}

fun fmtDur(s: Long): String = when {
    s <= 0 -> ""
    s >= 3600 -> "%d:%02d:%02d".format(s / 3600, s / 60 % 60, s % 60)
    else -> "%d:%02d".format(s / 60, s % 60)
}

fun fmtLong(ms: Long): String {
    val m = ms / 60000
    return when {
        m < 1 -> tr("меньше минуты", "under a minute", "менше хвилини", "menej ako minúta")
        m < 60 -> tr("$m мин", "$m min", "$m хв", "$m min")
        m % 60 == 0L -> tr("${m / 60} ч", "${m / 60} h", "${m / 60} год", "${m / 60} h")
        else -> tr("${m / 60} ч ${m % 60} мин", "${m / 60} h ${m % 60} min", "${m / 60} год ${m % 60} хв", "${m / 60} h ${m % 60} min")
    }
}

fun srcName(src: String) = when (src) {
    "yt" -> "YT"
    "sc" -> "SC"
    "sp" -> "SP"
    "ya" -> "YA"
    else -> "♪"
}

/** Код ошибки Firebase → человеческий текст (как acc_err на ПК) */
fun accErr(code: String): String {
    val c = code.split(' ', ':').firstOrNull() ?: ""
    return when (c) {
        "EMAIL_EXISTS" -> tr("На эту почту уже есть аккаунт — просто войди", "This email already has an account — just sign in", "На цю пошту вже є акаунт — просто увійди", "Na tento e-mail už účet existuje — stačí sa prihlásiť")
        "INVALID_LOGIN_CREDENTIALS", "INVALID_PASSWORD", "EMAIL_NOT_FOUND" -> tr("Неверная почта или пароль", "Wrong email or password", "Невірна пошта або пароль", "Nesprávny e-mail alebo heslo")
        "WEAK_PASSWORD" -> tr("Пароль слишком короткий: нужно от 6 символов", "Password is too short: at least 6 characters", "Пароль закороткий: потрібно від 6 символів", "Heslo je príliš krátke: aspoň 6 znakov")
        "MISSING_EMAIL" -> tr("Введи почту", "Enter your email", "Введи пошту", "Zadaj e-mail")
        "INVALID_EMAIL" -> tr("Это не похоже на почту", "That doesn't look like an email", "Це не схоже на пошту", "Toto nevyzerá ako e-mail")
        "MISSING_PASSWORD" -> tr("Введи пароль", "Enter a password", "Введи пароль", "Zadaj heslo")
        "TOO_MANY_ATTEMPTS_TRY_LATER" -> tr("Слишком много попыток, подожди немного", "Too many attempts, wait a bit", "Забагато спроб, зачекай трохи", "Príliš veľa pokusov, chvíľu počkaj")
        "NETWORK" -> tr("Нет связи с сервером", "Can't reach the server", "Немає зв'язку з сервером", "Server je nedostupný")
        "TOKEN_EXPIRED", "INVALID_REFRESH_TOKEN", "USER_NOT_FOUND", "USER_DISABLED", "NOT_SIGNED_IN" ->
            tr("Сессия кончилась, войди заново", "Session expired, sign in again", "Сесія скінчилася, увійди знову", "Relácia vypršala, prihlás sa znova")
        else -> tr("Не вышло: $code", "Failed: $code", "Не вийшло: $code", "Nevyšlo to: $code")
    }
}

fun sessionDead(code: String) = code.split(' ', ':').firstOrNull() in
    setOf("TOKEN_EXPIRED", "INVALID_REFRESH_TOKEN", "USER_NOT_FOUND", "USER_DISABLED", "NOT_SIGNED_IN")

/** Любимые — обычный плейлист с постоянным именем: так он сам синхронизируется между устройствами
 *  (то же имя на ПК). Показываем его как «♥ Любимые» на языке интерфейса */
const val LIKED = "♥"

fun plName(name: String) = if (name == LIKED) tr("Любимые треки", "Liked Songs", "Улюблені треки", "Obľúbené skladby") else name

/** «Обложка» любимых: вместо картинки рисуется сердечко на градиенте */
const val LIKED_ART = "bh:liked"

// ── Connect: одна сессия на все устройства аккаунта (ядро — connect.rs) ──
@Serializable
data class ConnDev(val id: String = "", val name: String = "", val kind: String = "pc", val online: Boolean = false)

@Serializable
data class ConnState(
    val host: String = "",
    val track: Track? = null,
    val played: List<Track> = emptyList(),
    val upnext: List<Track> = emptyList(),
    val source: List<Track> = emptyList(),
    val pos: Double = 0.0,
    val at: Long = 0,
    val playing: Boolean = false,
    val shuffle: Boolean = false,
    val repeat: Int = 0,
    val vol: Double? = null,
    val xfer: String = "",
)

@Serializable
data class ConnCmd(val op: String = "", val a: kotlinx.serialization.json.JsonElement = kotlinx.serialization.json.JsonNull)

@Serializable
data class ConnSnap(
    val ver: Long = 0,
    val now: Long = 0,
    val me: String = "",
    val host: String? = null,
    val devices: List<ConnDev> = emptyList(),
    val state: ConnState? = null,
    val cmds: List<ConnCmd> = emptyList(),
)
