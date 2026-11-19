package app.blackhole

import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder

// ── то, что отдаёт ядро (mobile/core/src/eco.rs) — те же структуры, что на ПК ──

@Serializable
data class UStar(val i: Int = 0, val node: Int = 0, val n: String = "", val x: Float = 0f, val y: Float = 0f, val r: Float = 4f, val c: List<Int> = listOf(196, 181, 253), val l: Long = 0, val top: List<Int> = emptyList())

@Serializable
data class UNeb(val n: String = "", val x: Float = 0f, val y: Float = 0f, val r: Float = 60f, val c: List<Int> = listOf(200, 200, 200))

@Serializable
data class UniData(
    val stars: List<UStar> = emptyList(), val links: List<List<Double>> = emptyList(), val nebulas: List<UNeb> = emptyList(),
    val bounds: List<Float> = listOf(0f, 0f, 400f, 400f), val dna: Int = 0, val sparse: Boolean = false,
)

@Serializable
data class Reason(val rel: String = "", val note: String = "")

@Serializable
data class ULink(val star: Int = 0, val name: String = "", val origin: String = "Derived", val w: Float = 0f, val why: List<Reason> = emptyList())

@Serializable
data class UDnaBrief(val bpm: Float? = null, val key: String? = null, val n: Int = 0)

@Serializable
data class UStarInfo(
    val node: Int = 0, val name: String = "", val listened: Long = 0, val plays: Int = 0, val dna: UDnaBrief? = null,
    val links: List<ULink> = emptyList(), val tracks: List<Track> = emptyList(),
    val genres: List<String> = emptyList(), val albums: List<String> = emptyList(), val people: List<String> = emptyList(),
)

@Serializable
data class UFound(val node: Int = 0, val name: String = "", val kind: Int = 0, val sub: String? = null, val star: Int? = null, val nebula: Int? = null)

@Serializable
data class UStep(val node: Int = 0, val name: String = "", val kind: String = "", val rel: String? = null, val note: String = "", val origin: String? = null)

@Serializable
data class UBridge(val steps: List<UStep> = emptyList(), val facts: Int = 0, val stars: List<Int> = emptyList())

@Serializable
data class Sim(val rhythm: Float = 0f, val harmony: Float = 0f, val timbre: Float = 0f, val energy: Float = 0f, val spectrum: Float = 0f, val total: Float = 0f)

@Serializable
data class Dna(
    val secs: Float = 0f, val bpm: Float? = null, val pulse: Float = 0f, val onsets: Float = 0f, val key: Int? = null, val minor: Boolean = false,
    val key_conf: Float = 0f, val rms_db: Float = 0f, val peak_db: Float = 0f, val dyn_db: Float = 0f, val energy: Float = 0f,
    val centroid: Float = 0f, val rolloff: Float = 0f, val flatness: Float = 0f, val bands: List<Float> = listOf(0f, 0f, 0f), val sections: List<Float> = emptyList(),
) {
    val keyName get() = key?.let { listOf("C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B")[it % 12] + if (minor) "m" else "" }
}

@Serializable
data class DnaPick(val id: Int = 0, val name: String = "", val sub: String? = null, val sim: Sim = Sim(), val why: List<Int> = emptyList())

@Serializable
data class DnaCmp(val name: String = "", val radar: List<Float> = emptyList(), val sim: Sim = Sim())

@Serializable
data class DnaInfo(val dna: Dna = Dna(), val n: Int = 0, val total: Int = 0, val radar: List<Float> = emptyList(), val cmp: DnaCmp? = null, val similar: List<DnaPick> = emptyList(), val far: List<DnaPick> = emptyList())

@Serializable
data class Share(val name: String = "", val w: Float = 0f, val ms: Long = 0)

@Serializable
data class Combo(val genres: List<String> = emptyList(), val strength: Float = 0f, val rarity: Float = 0f, val together: Int = 0, val of: Int = 0)

@Serializable
data class Cluster(val name: String = "", val artists: List<String> = emptyList(), val w: Float = 0f)

@Serializable
data class SoundPref(
    val tracks: Int = 0, val coverage: Float = 0f, val tempo: List<Float>? = null, val minor: Float? = null, val keys: List<Share> = emptyList(),
    val energy: Float = 0f, val pulse: Float = 0f, val brightness: Float = 0f, val bass: Float = 0f, val dynamics: Float = 0f, val mean: Dna? = null,
)

@Serializable
data class Dynamics(
    val rising: List<Share> = emptyList(), val falling: List<Share> = emptyList(), val new_artists: List<String> = emptyList(),
    val stable: List<String> = emptyList(), val diversity: List<List<Double>> = emptyList(), val discovered: List<String> = emptyList(),
)

@Serializable
data class Fingerprint(
    val total_ms: Long = 0, val tracks: Int = 0, val artists: List<Share> = emptyList(), val genres: List<Share> = emptyList(), val genre_coverage: Float = 0f,
    val combos: List<Combo> = emptyList(), val clusters: List<Cluster> = emptyList(), val sound: SoundPref = SoundPref(),
    val artist_diversity: Float = 0f, val genre_diversity: Float = 0f, val repeat: Float = 0f, val loyal: Float = 0f, val skip_rate: Float? = null,
    val hours: List<Float> = emptyList(), val dynamics: Dynamics = Dynamics(),
)

@Serializable
data class Star(val x: Float = 0f, val y: Float = 0f, val r: Float = 4f, val c: List<Int> = listOf(200, 200, 200), val label: String = "", val kind: Int = 1, val lab: Boolean = false)

@Serializable
data class TasteData(val fp: Fingerprint = Fingerprint(), val stars: List<Star> = emptyList(), val lines: String = "", val arcs: String = "", val code: String = "", val shows: List<Boolean> = List(5) { true }, val published: Boolean = false)

@Serializable
data class Discover(val track: Track = Track(), val score: Int = 0, val why: String = "", val arg: String = "", val arg2: String = "")

@Serializable
data class Compat(
    val shared: List<JsonArray> = emptyList(), val only_me: List<String> = emptyList(), val only_them: List<String> = emptyList(),
    val shared_artists: List<String> = emptyList(), val surprises: List<String> = emptyList(), val sound: Sim? = null,
    val genre_overlap: Float = 0f, val artist_overlap: Float = 0f,
)

@Serializable
data class CompatData(val name: String = "", val compat: Compat = Compat(), val discover: List<Discover> = emptyList())

@Serializable
data class RMeta(val owner: String = "", val owner_name: String = "", val name: String = "", val public: Boolean = false, val queue: String = "all", val control: String = "owner")

@Serializable
data class RMember(val uid: String = "", val name: String = "", val role: String = "", val seen: Long = 0, val pos: Double = 0.0, val playing: Boolean = false, val key: String = "")

@Serializable
data class RState(val track: Track? = null, val pos: Double = 0.0, val at: Long = 0, val playing: Boolean = false, val by: String = "", val seq: Long = 0, val upnext: List<Track> = emptyList())

@Serializable
data class RQ(val id: String = "", val track: Track = Track(), val by: String = "", val by_name: String = "", val ord: Double = 0.0, val votes: Int = 0, val mine: Boolean = false)

@Serializable
data class RMsg(val id: String = "", val uid: String = "", val name: String = "", val text: String = "", val emoji: String = "", val key: String = "")

@Serializable
data class Room(val code: String = "", val meta: RMeta = RMeta(), val members: List<RMember> = emptyList(), val state: RState? = null, val queue: List<RQ> = emptyList(), val chat: List<RMsg> = emptyList()) {
    fun owner(me: String) = meta.owner == me
    fun control(me: String) = owner(me) || meta.control == "all"
    fun canQueue(me: String) = owner(me) || meta.queue == "all"
    /** следующим — больше голосов, при равенстве — раньше в очереди */
    fun next(): RQ? = queue.withIndex().maxWithOrNull(compareBy({ it.value.votes }, { -it.index }))?.value
}

@Serializable
data class RoomSnap(val ver: Long = 0, val now: Long = 0, val me: String = "", val room: Room? = null)

@Serializable
data class PublicRoom(val code: String = "", val name: String = "", val owner: String = "")

/** Подписи связей, видов узлов и происхождения — на языке интерфейса */
object EcoText {
    fun rel(r: String?) = when (r) {
        "Performs" -> tr("исполнитель", "performer", "виконавець", "interpret")
        "OnAlbum" -> tr("трек альбома", "album track", "трек альбому", "skladba albumu")
        "AlbumBy" -> tr("альбом артиста", "artist's album", "альбом артиста", "album interpreta")
        "Feat" -> tr("вместе в треке", "together on a track", "разом у треку", "spolu v skladbe")
        "Member" -> tr("участник группы", "band member", "учасник гурту", "člen skupiny")
        "Collab" -> tr("совместный проект", "collaboration", "спільний проєкт", "spolupráca")
        "Producer" -> tr("продюсер", "producer", "продюсер", "producent")
        "Engineer" -> tr("звукорежиссёр", "engineer", "звукорежисер", "zvukový majster")
        "Musician" -> tr("играл на записи", "played on the record", "грав на записі", "hral na nahrávke")
        "Label" -> tr("лейбл", "label", "лейбл", "vydavateľstvo")
        "Genre" -> tr("жанр", "genre", "жанр", "žáner")
        "YtSimilar" -> tr("похожие артисты", "similar artists", "схожі артисти", "podobní interpreti")
        "CoPlaylist" -> tr("вместе в твоих плейлистах", "together in your playlists", "разом у твоїх плейлистах", "spolu v tvojich playlistoch")
        "Sound" -> tr("похожи по звуку", "sound alike", "схожі за звуком", "znejú podobne")
        else -> ""
    }

    fun src(r: String?) = when (r) {
        "Performs", "Feat" -> tr("метаданные трека", "track metadata", "метадані треку", "metadáta skladby")
        "YtSimilar" -> "YouTube Music"
        "CoPlaylist" -> tr("твои плейлисты", "your playlists", "твої плейлисти", "tvoje playlisty")
        "Sound" -> "Music DNA"
        else -> "MusicBrainz"
    }

    fun origin(o: String?) = when (o) {
        "Confirmed" -> tr("факт", "fact", "факт", "fakt")
        "External" -> tr("алгоритм", "algorithm", "алгоритм", "algoritmus")
        else -> tr("вычислено", "computed", "обчислено", "vypočítané")
    }

    fun kind(k: Int) = when (k) {
        0 -> tr("ТРЕК", "TRACK", "ТРЕК", "SKLADBA")
        1 -> tr("АРТИСТ", "ARTIST", "АРТИСТ", "INTERPRET")
        2 -> tr("АЛЬБОМ", "ALBUM", "АЛЬБОМ", "ALBUM")
        3 -> tr("ЖАНР", "GENRE", "ЖАНР", "ŽÁNER")
        4 -> tr("ЧЕЛОВЕК ЗА ЗАПИСЬЮ", "PERSON BEHIND THE RECORD", "ЛЮДИНА ЗА ЗАПИСОМ", "ČLOVEK ZA NAHRÁVKOU")
        else -> tr("ЛЕЙБЛ", "LABEL", "ЛЕЙБЛ", "VYDAVATEĽSTVO")
    }

    val parts get() = listOf(tr("ритм", "rhythm", "ритм", "rytmus"), tr("гармония", "harmony", "гармонія", "harmónia"), tr("тембр", "timbre", "тембр", "farba"), tr("энергия", "energy", "енергія", "energia"), tr("спектр", "spectrum", "спектр", "spektrum"))

    fun sim(s: Sim, i: Int) = listOf(s.rhythm, s.harmony, s.timbre, s.energy, s.spectrum).getOrElse(i) { 0f }

    fun why(p: DnaPick) = p.why.joinToString(", ") { "${parts.getOrElse(it) { "" }} ${(sim(p.sim, it) * 100).toInt()} %" }

    fun discover(d: Discover) = when (d.why) {
        "artist" -> tr("ты слушаешь ${d.arg}, а этот трек — нет", "you listen to ${d.arg}, but not this track", "ти слухаєш ${d.arg}, а цей трек — ні", "počúvaš ${d.arg}, ale túto skladbu nie")
        "similar" -> tr("YouTube Music считает ${d.arg} похожим на твоего ${d.arg2}", "YouTube Music finds ${d.arg} similar to your ${d.arg2}", "YouTube Music вважає ${d.arg} схожим на твого ${d.arg2}", "YouTube Music považuje ${d.arg} za podobného tvojmu ${d.arg2}")
        else -> tr("в твоём жанре ${d.arg}", "in your genre ${d.arg}", "у твоєму жанрі ${d.arg}", "v tvojom žánri ${d.arg}")
    }

    fun err(e: Throwable): String {
        val m = e.message ?: ""
        return when {
            "PERMISSION" in m -> tr("База не пускает: правила для комнат не опубликованы (см. docs/firebase.md)", "The database refused: room rules aren't published (see docs/firebase.md)", "База не пускає: правила для кімнат не опубліковано (див. docs/firebase.md)", "Databáza odmietla: pravidlá pre miestnosti nie sú zverejnené (pozri docs/firebase.md)")
            "NO_ROOM" in m -> tr("Комнаты с таким кодом нет", "No room with this code", "Кімнати з таким кодом немає", "Miestnosť s takým kódom neexistuje")
            "NOT_VERIFIED" in m -> tr("Нужен вход с подтверждённой почтой", "Sign in with a verified email first", "Потрібен вхід з підтвердженою поштою", "Najprv sa prihlás s overeným e-mailom")
            "LOCAL" in m -> tr("Файл с телефона в комнату не добавить — у других его нет", "A local file can't go into a room — others don't have it", "Файл з телефона в кімнату не додати — в інших його немає", "Lokálny súbor do miestnosti nepridáš — ostatní ho nemajú")
            "404" in m -> tr("Карточки с таким кодом нет", "No card with this code", "Картки з таким кодом немає", "Karta s týmto kódom neexistuje")
            "NETWORK" in m -> tr("Нет связи", "No connection", "Немає зв'язку", "Bez pripojenia")
            else -> m
        }
    }
}

/** Вселенная, вкус, комнаты и Music DNA на телефоне. Состояние экранов + синхронизация плеера с комнатой */
@UnstableApi
class EcoState(private val vm: AppState) {
    // ── вселенная: звёзды — артисты, номера звёзд; мосты и DNA — узлы графа ──
    var uni by mutableStateOf<UniData?>(null)
    var uniBusy by mutableStateOf(false)
    var fetching by mutableStateOf(false)
    var sel by mutableStateOf<Int?>(null)
    var info by mutableStateOf<UStarInfo?>(null)
    var found by mutableStateOf<List<UFound>>(emptyList())
    var brA by mutableStateOf<Int?>(null)
    var brB by mutableStateOf<Int?>(null)
    var bridges by mutableStateOf<List<UBridge>>(emptyList())
    var brShown by mutableStateOf<Int?>(null)
    var brSearched by mutableStateOf(false)
    var dna by mutableStateOf<DnaInfo?>(null)
    var dnaNode by mutableStateOf<Int?>(null)
    var cmpPick by mutableStateOf(false)
    /** куда лететь камере: звезда или туманность (экран забирает и обнуляет) */
    var flyTo by mutableStateOf<Int?>(null)
    var flyNeb by mutableStateOf<Int?>(null)

    private fun pls() = json.encodeToJsonElement(kotlinx.serialization.builtins.ListSerializer(Playlist.serializer()), vm.playlists.toList())

    fun buildUniverse() = vm.viewModelScope.launch {
        if (uniBusy) return@launch
        uniBusy = true
        runCatching { core("dna_sync") }
        val r = runCatching { core("uni_build") { put("playlists", pls()) }.to<UniData>() }
        uniBusy = false
        r.onSuccess { uni = it; sel?.let { s -> if (s >= it.stars.size) { sel = null; info = null } else pick(s) } }.onFailure { vm.toast(EcoText.err(it)) }
        scanDna()
    }

    fun pick(star: Int) = vm.viewModelScope.launch {
        val node = uni?.stars?.getOrNull(star)?.node ?: return@launch
        if (cmpPick) {
            cmpPick = false
            dnaNode?.let { openDna(it, node) }
            return@launch
        }
        // связи — до смены выбора: выбор и его линии меняются разом (иначе линии росли бы к соседям прошлой звезды)
        val inf = runCatching { core("uni_star") { put("i", JsonPrimitive(star)) }.to<UStarInfo>() }.getOrNull()
        sel = star
        brShown = null
        info = inf
    }

    fun clear() { sel = null; info = null; brShown = null }

    fun search(q: String) = vm.viewModelScope.launch {
        found = if (q.isBlank()) emptyList() else runCatching { core("uni_search") { put("q", JsonPrimitive(q)) }.to<List<UFound>>() }.getOrDefault(emptyList())
    }

    /** Найденное: артист, трек — к звезде; жанр — к туманности */
    fun go(f: UFound) {
        f.star?.let { flyTo = it } ?: f.nebula?.let { flyNeb = it }
    }

    /** Узел графа (из DNA) → его звезда */
    fun goNode(node: Int) = vm.viewModelScope.launch {
        runCatching { core("uni_star_of") { put("node", JsonPrimitive(node)) }.to<Int?>() }.getOrNull()?.let { flyTo = it }
    }

    fun fetch() = vm.viewModelScope.launch {
        if (fetching) return@launch
        fetching = true
        vm.toast(tr("Спрашиваю YouTube Music и MusicBrainz — уходят только имена и названия. Это небыстро", "Asking YouTube Music and MusicBrainz — only names and titles are sent. It takes a while", "Питаю YouTube Music і MusicBrainz — ідуть лише імена й назви. Це неквапливо", "Pýtam sa YouTube Music a MusicBrainz — odchádzajú len mená a názvy. Chvíľu to potrvá"))
        runCatching { core("uni_fetch") }.onFailure { vm.toast(EcoText.err(it)) }
        fetching = false
        buildUniverse()
    }

    /** Узел по имени (из комнаты: «смотрите во Вселенной») — камера летит к нему */
    fun focusName(name: String) = vm.viewModelScope.launch {
        if (uni == null) buildUniverse().join()
        runCatching { core("uni_search") { put("q", JsonPrimitive(name)) }.to<List<UFound>>() }.getOrNull()?.firstOrNull()?.let { go(it) }
    }

    fun bridgeEnd(node: Int, from: Boolean) {
        if (from) brA = node else brB = node
        if (brA != null && brB != null) runBridge(5, listOf(true, true, true, true))
    }

    fun runBridge(depth: Int, groups: List<Boolean>) = vm.viewModelScope.launch {
        val a = brA ?: return@launch
        val b = brB ?: return@launch
        bridges = runCatching {
            core("uni_bridge") {
                put("a", JsonPrimitive(a)); put("b", JsonPrimitive(b)); put("depth", JsonPrimitive(depth))
                put("groups", JsonArray(groups.map { JsonPrimitive(it) }))
            }.to<List<UBridge>>()
        }.getOrDefault(emptyList())
        brSearched = true
        brShown = if (bridges.isEmpty()) null else 0
    }

    fun openDna(node: Int, cmp: Int? = null) = vm.viewModelScope.launch {
        dnaNode = node
        dna = runCatching { core("uni_dna") { put("i", JsonPrimitive(node)); if (cmp != null) put("cmp", JsonPrimitive(cmp)) }.to<DnaInfo>() }
            .onFailure { vm.toast(tr("Тут ещё нет Music DNA", "No Music DNA here yet", "Тут ще немає Music DNA", "Tu ešte nie je Music DNA")) }.getOrNull()
    }

    // ── Music DNA на телефоне: скачанные файлы (офлайн) раскодируются и разбираются по одному в фоне ──

    private var scanning: Job? = null

    fun scanDna() {
        if (scanning?.isActive == true) return
        scanning = vm.viewModelScope.launch(Dispatchers.Default) {
            val keys = Offline.have.value.toList()
            var done = 0
            for (k in keys) {
                if (!isActive) break
                if (Offline.isHls(k)) continue
                if (runCatching { coreNow("dna_has", kotlinx.serialization.json.buildJsonObject { put("key", JsonPrimitive(k)) }).jsonPrimitive.content == "true" }.getOrDefault(true)) continue
                val f = Offline.uri(k)?.path?.let(::File) ?: continue
                val pcm = File(vm.getApplication<android.app.Application>().cacheDir, "dna.f32")
                if (runCatching { decodeMono(f, pcm) }.getOrDefault(false)) {
                    runCatching { coreNow("dna_raw", kotlinx.serialization.json.buildJsonObject { put("key", JsonPrimitive(k)); put("path", JsonPrimitive(pcm.path)) }) }
                        .onSuccess { done++ }
                        .onFailure { BhLog.line("DNA $k: ${it.message}") }
                }
                pcm.delete()
            }
            if (done > 0) BhLog.line("Music DNA: разобрано $done треков на телефоне")
        }
    }

    // ── вкус ──
    var taste by mutableStateOf<TasteData?>(null)
    var tasteBusy by mutableStateOf(false)
    var period by mutableStateOf(0)
    var compat by mutableStateOf<CompatData?>(null)
    var compatErr by mutableStateOf("")
    var shows by mutableStateOf(List(5) { true })

    fun loadTaste(p: Int = period) = vm.viewModelScope.launch {
        period = p
        tasteBusy = true
        val r = runCatching { core("taste") { put("period", JsonPrimitive(p)); put("playlists", pls()) }.to<TasteData>() }
        tasteBusy = false
        r.onSuccess { taste = it; shows = it.shows }.onFailure { vm.toast(EcoText.err(it)) }
    }

    fun publish() = vm.viewModelScope.launch {
        runCatching { core("taste_publish") { put("shows", JsonArray(shows.map { JsonPrimitive(it) })); put("playlists", pls()) } }
            .onSuccess { c -> taste = taste?.copy(published = true, code = c.jsonPrimitive.content); vm.toast(tr("Карточка вкуса опубликована: код ${c.jsonPrimitive.content}", "Taste card published: code ${c.jsonPrimitive.content}", "Картку смаку опубліковано: код ${c.jsonPrimitive.content}", "Karta vkusu zverejnená: kód ${c.jsonPrimitive.content}")) }
            .onFailure { vm.toast(EcoText.err(it)) }
    }

    fun unpublish() = vm.viewModelScope.launch {
        runCatching { core("taste_unpublish") }.onSuccess { taste = taste?.copy(published = false) }.onFailure { vm.toast(EcoText.err(it)) }
    }

    fun compare(code: String) = vm.viewModelScope.launch {
        compatErr = ""
        runCatching { core("taste_compare") { put("code", JsonPrimitive(code)); put("playlists", pls()) }.to<CompatData>() }
            .onSuccess { compat = it }.onFailure { compatErr = EcoText.err(it) }
    }

    // ── комнаты ──
    var room by mutableStateOf<Room?>(null)
    var me by mutableStateOf("")
    var publics by mutableStateOf<List<PublicRoom>>(emptyList())
    var roomErr by mutableStateOf("")
    var roomBusy by mutableStateOf(false)
    var drift by mutableStateOf(0)
    val inRoom get() = room != null

    private var loop: Job? = null
    private var tick: Job? = null
    private var serverOffset = 0L
    private var pubSig = ""
    private var pubPos = 0.0
    private var pubAt = 0L
    private var seq = 0L
    private var quietUntil = 0L
    private var nudgeUntil = 0L
    private var beatAt = 0L
    private var pendingSeek: Double? = null
    private var pendingPause = false
    /** предложения, которые уже поставили в очередь */
    private val taken = HashSet<String>()

    private fun now() = System.currentTimeMillis()
    private fun serverNow() = now() + serverOffset

    fun loadPublics() = vm.viewModelScope.launch {
        publics = runCatching { core("rooms_public").to<List<PublicRoom>>() }.getOrDefault(emptyList())
    }

    fun create(name: String, public: Boolean, queueAll: Boolean, controlAll: Boolean) = vm.viewModelScope.launch {
        roomBusy = true
        roomErr = ""
        runCatching {
            core("rooms_create") { put("name", JsonPrimitive(name.ifBlank { tr("Комната ${vm.email.substringBefore('@')}", "${vm.email.substringBefore('@')}'s room", "Кімната ${vm.email.substringBefore('@')}", "Miestnosť ${vm.email.substringBefore('@')}") })); put("public", JsonPrimitive(public)); put("queue_all", JsonPrimitive(queueAll)); put("control_all", JsonPrimitive(controlAll)) }.jsonPrimitive.content
        }.onSuccess { join(it) }.onFailure { roomErr = EcoText.err(it); roomBusy = false }
    }

    fun join(code: String) = vm.viewModelScope.launch {
        roomBusy = true
        roomErr = ""
        val r = runCatching { core("rooms_join") { put("code", JsonPrimitive(code.trim().uppercase())) } }
        roomBusy = false
        r.onFailure { roomErr = EcoText.err(it); loadPublics(); return@launch } // комнаты нет — из списка она уже убрана
        pubSig = ""; seq = 0
        loop?.cancel()
        loop = vm.viewModelScope.launch {
            var ver = 0L
            while (isActive) {
                val s = runCatching { core("rooms_wait") { put("since", JsonPrimitive(ver)); put("ms", JsonPrimitive(20_000)) }.to<RoomSnap>() }.getOrNull()
                if (s == null) { delay(2000); continue }
                ver = s.ver
                serverOffset = s.now - now()
                me = s.me
                val r2 = s.room
                // комнату забыли из-под нас (сменили аккаунт) — выходим и тут
                if (r2 == null) { tick?.cancel(); room = null; Eq.nudge.value = 1f; break }
                if (r2.meta.owner.isEmpty() && room != null) {
                    vm.toast(tr("Комнату закрыли", "The room was closed", "Кімнату закрили", "Miestnosť bola zatvorená"))
                    leave()
                    break
                }
                room = r2
                follow(r2)
            }
        }
        tick?.cancel()
        tick = vm.viewModelScope.launch { while (isActive) { delay(500); tick() } }
    }

    fun leave() {
        loop?.cancel(); tick?.cancel()
        room = null
        Eq.nudge.value = 1f
        vm.viewModelScope.launch { runCatching { core("rooms_leave") } }
    }

    private fun bg(m: String, build: kotlinx.serialization.json.JsonObjectBuilder.() -> Unit) = vm.viewModelScope.launch {
        runCatching { core(m, build) }.onFailure { vm.toast(EcoText.err(it)) }
    }

    fun add(t: Track) = bg("rooms_enqueue") { put("track", t) }.also { vm.toast(tr("«${t.title}» — в очереди комнаты", "“${t.title}” added to the room queue", "«${t.title}» — у черзі кімнати", "„${t.title}“ je vo fronte miestnosti")) }
    fun vote(id: String, on: Boolean) = bg("rooms_vote") { put("id", JsonPrimitive(id)); put("on", JsonPrimitive(on)) }
    fun unqueue(id: String) = bg("rooms_unqueue") { put("id", JsonPrimitive(id)) }
    fun requeue(id: String, to: Int) = bg("rooms_requeue") { put("id", JsonPrimitive(id)); put("to", JsonPrimitive(to.coerceAtLeast(0))) }
    fun say(text: String, emoji: String = "") = bg("rooms_say") { put("text", JsonPrimitive(text)); put("emoji", JsonPrimitive(emoji)); put("key", JsonPrimitive(vm.current?.key ?: "")) }
    fun meta(public: Boolean, queueAll: Boolean, controlAll: Boolean) = bg("rooms_meta") { put("public", JsonPrimitive(public)); put("queue_all", JsonPrimitive(queueAll)); put("control_all", JsonPrimitive(controlAll)) }
    fun kick(uid: String) = bg("rooms_kick") { put("uid", JsonPrimitive(uid)) }
    fun shareTaste() = vm.viewModelScope.launch {
        runCatching { core("taste_line") { put("playlists", pls()) }.jsonPrimitive.content }.onSuccess { say(it) }.onFailure { vm.toast(EcoText.err(it)) }
    }

    private fun localPos(): Double = (vm.controller?.currentPosition ?: 0L) / 1000.0

    /** Подстроиться под комнату, если состояние записал не я */
    /** Очередь в плеере после текущего трека */
    private fun upcoming(): List<Track> {
        val c = vm.controller ?: return emptyList()
        return (c.currentMediaItemIndex + 1 until c.mediaItemCount).mapNotNull { c.getMediaItemAt(it).track() }
    }

    private fun qsig(q: List<Track>) = q.take(30).joinToString(",") { it.key }

    /** Подстроиться под комнату, если состояние записал не я. Очередь управляющего встаёт в плеер целиком:
     *  ExoPlayer заранее грузит следующий трек и сам включает его, когда текущий кончится — без ожидания */
    private fun follow(r: Room) {
        val st = r.state ?: return
        if (st.by == me || vm.remote != null) return
        val t = st.track ?: return
        val target = if (st.playing) st.pos + (serverNow() - st.at).coerceAtLeast(0) / 1000.0 else st.pos
        pubSig = "${t.key}|${qsig(st.upnext)}|${st.playing}"; pubPos = target; pubAt = now(); seq = maxOf(seq, st.seq)
        quietUntil = now() + 2500
        val c = vm.controller ?: return
        if (vm.current?.key != t.key) {
            val items = (listOf(t) + st.upnext.filter { it.src != "lc" }).map { it.item() }
            c.setMediaItems(items, 0, (target * 1000).toLong().coerceAtLeast(0))
            c.prepare()
            if (st.playing) c.play() else c.pause()
            return
        }
        // тот же трек: подменяем только то, что после него, если очередь управляющего изменилась
        if (qsig(upcoming()) != qsig(st.upnext)) {
            val from = c.currentMediaItemIndex + 1
            if (from < c.mediaItemCount) c.removeMediaItems(from, c.mediaItemCount)
            c.addMediaItems(st.upnext.filter { it.src != "lc" }.map { it.item() })
        }
        if (c.isPlaying != st.playing && c.playbackState != Player.STATE_BUFFERING) { if (st.playing) c.play() else c.pause() }
        if (!st.playing && kotlin.math.abs(localPos() - target) > 1.0) c.seekTo((target * 1000).toLong())
        if (st.playing) correct(target)
    }

    /** Расхождение до 0.15 с не трогаем, до 2 с — скорость ±3 %, больше — перемотка */
    private fun correct(target: Double) {
        val c = vm.controller ?: return
        if (c.playbackState == Player.STATE_BUFFERING) return
        val d = localPos() - target
        drift = (d * 1000).toInt()
        val a = kotlin.math.abs(d)
        when {
            a > 2.0 -> { c.seekTo((target * 1000).toLong()); Eq.nudge.value = 1f }
            a >= 0.15 -> { Eq.nudge.value = if (d > 0) 0.97f else 1.03f; nudgeUntil = now() + (a / 0.03 * 1000).toLong().coerceAtMost(30_000) }
            else -> Eq.nudge.value = 1f
        }
    }

    private suspend fun tick() {
        val r = room ?: return
        val c = vm.controller ?: return
        if (nudgeUntil in 1 until now()) { Eq.nudge.value = 1f; nudgeUntil = 0 }
        // трек комнаты встал на место — перемотать и, если в комнате пауза, остановить
        if (pendingSeek != null || pendingPause) {
            if (c.playbackState == Player.STATE_READY) {
                pendingSeek?.let { c.seekTo((it * 1000).toLong()) }
                if (pendingPause) c.pause()
                pendingSeek = null; pendingPause = false
            }
            return
        }
        val playing = c.isPlaying
        val pos = localPos()
        val key = vm.current?.key ?: ""
        if (now() - beatAt > 10_000) {
            beatAt = now()
            bg("rooms_beat") { put("pos", JsonPrimitive(pos)); put("playing", JsonPrimitive(playing)); put("key", JsonPrimitive(key)) }
        }
        val st = r.state
        if (st != null && st.by != me && st.playing && st.track?.key == key && now() > quietUntil + 500) {
            correct(st.pos + (serverNow() - st.at).coerceAtLeast(0) / 1000.0)
        }
        if (!r.control(me) || vm.remote != null || now() < quietUntil) return
        // самое популярное предложение — следующим за полминуты до конца трека (или сразу, если дальше пусто)
        r.next()?.takeIf { it.id !in taken }?.let { q ->
            if (c.currentMediaItemIndex + 1 >= c.mediaItemCount || (c.duration > 0 && c.duration - c.currentPosition < 30_000)) {
                taken += q.id
                c.addMediaItem(c.currentMediaItemIndex + 1, q.track.item())
                unqueue(q.id)
            }
        }
        val t = vm.current ?: return
        if (c.playbackState == Player.STATE_BUFFERING) return
        val up = upcoming().filter { it.src != "lc" }.take(30)
        val sig = "${t.key}|${qsig(up)}|$playing"
        val expected = pubPos + if (pubSig.endsWith("|true")) (now() - pubAt) / 1000.0 else 0.0
        val refresh = r.owner(me) && now() - pubAt > 20_000
        if (sig == pubSig && kotlin.math.abs(pos - expected) < 2.5 && !refresh) return
        if (t.src == "lc") return
        pubSig = sig; pubPos = pos; pubAt = now(); seq++
        val s = seq
        val upJson = json.encodeToJsonElement(kotlinx.serialization.builtins.ListSerializer(Track.serializer()), up)
        withContext(Dispatchers.IO) { runCatching { coreNow("rooms_state", kotlinx.serialization.json.buildJsonObject { put("track", t); put("pos", JsonPrimitive(pos)); put("playing", JsonPrimitive(playing)); put("seq", JsonPrimitive(s)); put("upnext", upJson) }) } }
    }
}

/** Раскодировать файл в моно float 22 050 Гц (как ffmpeg на ПК), не длиннее 8 минут. false — не вышло */
fun decodeMono(src: File, out: File): Boolean {
    val ex = MediaExtractor()
    ex.setDataSource(src.path)
    val ti = (0 until ex.trackCount).firstOrNull { ex.getTrackFormat(it).getString(MediaFormat.KEY_MIME)?.startsWith("audio/") == true } ?: return false
    ex.selectTrack(ti)
    val fmt = ex.getTrackFormat(ti)
    val codec = MediaCodec.createDecoderByType(fmt.getString(MediaFormat.KEY_MIME)!!)
    codec.configure(fmt, null, null, 0)
    codec.start()
    var rate = fmt.getInteger(MediaFormat.KEY_SAMPLE_RATE)
    var ch = fmt.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
    // декодер обычно отдаёт 16-битный PCM, но некоторые — float
    var floats = false
    val target = 22_050
    val maxOut = target * 480
    val buf = ByteBuffer.allocate(4 * 8192).order(ByteOrder.LITTLE_ENDIAN)
    var written = 0
    var acc = 0.0 // позиция в исходных отсчётах для линейной передискретизации
    var prev = 0f
    var srcIdx = 0L
    val info = MediaCodec.BufferInfo()
    var inDone = false
    var outDone = false
    out.outputStream().buffered().use { os ->
        fun emit(s: Float) {
            // ресэмплинг: на каждый исходный отсчёт выдаём столько, сколько выпадает целевых между prev и s
            val step = rate.toDouble() / target
            while (acc <= srcIdx && written < maxOut) {
                val frac = (acc - (srcIdx - 1)).toFloat().coerceIn(0f, 1f)
                buf.putFloat(prev + (s - prev) * frac)
                written++
                if (buf.position() == buf.capacity()) { os.write(buf.array(), 0, buf.position()); buf.clear() }
                acc += step
            }
            prev = s
            srcIdx++
        }
        while (!outDone && written < maxOut) {
            if (!inDone) {
                val i = codec.dequeueInputBuffer(10_000)
                if (i >= 0) {
                    val ib = codec.getInputBuffer(i)!!
                    val n = ex.readSampleData(ib, 0)
                    if (n < 0) { codec.queueInputBuffer(i, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM); inDone = true } else { codec.queueInputBuffer(i, 0, n, ex.sampleTime, 0); ex.advance() }
                }
            }
            val o = codec.dequeueOutputBuffer(info, 10_000)
            when {
                o == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                    codec.outputFormat.let {
                        rate = it.getInteger(MediaFormat.KEY_SAMPLE_RATE)
                        ch = it.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
                        floats = it.containsKey(MediaFormat.KEY_PCM_ENCODING) && it.getInteger(MediaFormat.KEY_PCM_ENCODING) == android.media.AudioFormat.ENCODING_PCM_FLOAT
                    }
                }
                o >= 0 -> {
                    val ob = codec.getOutputBuffer(o)!!.order(ByteOrder.LITTLE_ENDIAN)
                    ob.position(info.offset); ob.limit(info.offset + info.size)
                    if (floats) {
                        val fb = ob.asFloatBuffer()
                        repeat(fb.remaining() / ch) {
                            var sum = 0f
                            repeat(ch) { sum += fb.get() }
                            emit(sum / ch)
                        }
                    } else {
                        val sh = ob.asShortBuffer()
                        repeat(sh.remaining() / ch) {
                            var sum = 0f
                            repeat(ch) { sum += sh.get() }
                            emit(sum / ch / 32768f)
                        }
                    }
                    codec.releaseOutputBuffer(o, false)
                    if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) outDone = true
                }
            }
        }
        if (buf.position() > 0) os.write(buf.array(), 0, buf.position())
    }
    codec.stop(); codec.release(); ex.release()
    return written > target * 10
}

