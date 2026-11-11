package app.blackhole

import android.app.Application
import android.content.ComponentName
import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.setValue
import androidx.core.content.ContextCompat
import androidx.core.content.edit
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import kotlin.math.roundToInt
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/** Экраны: стек для «назад» */
sealed interface Screen {
    data object Home : Screen
    data object Search : Screen
    data object Library : Screen
    data object Account : Screen
    data object Settings : Screen
    data object Stats : Screen
    data object Eq : Screen
    data class Pl(val name: String) : Screen
    data class ArtistPage(val id: String, val name: String) : Screen
    data class AlbumPage(val id: String) : Screen
    data class Tracks(val title: String, val sub: String, val cover: String, val tracks: List<Track>) : Screen
}

@UnstableApi
class AppState(app: Application) : AndroidViewModel(app) {
    private val prefs = app.getSharedPreferences("bh", Context.MODE_PRIVATE)

    // ── настройки ──
    var theme by mutableStateOf(prefs.getInt("theme", 0))
        private set
    var autoplay by mutableStateOf(prefs.getBoolean("autoplay", true))
        private set
    var spicyKey by mutableStateOf(prefs.getString("spicy", "") ?: "")
        private set

    fun setLang(l: String) {
        L.lang = l
        prefs.edit { putString("lang", l) }
        // перевод текста был на прошлый язык
        if (lyrTrMap.isNotEmpty()) {
            lyrTrMap = emptyMap()
            translateLyrics(false)
        }
    }

    fun setThemeId(i: Int) {
        theme = i
        prefs.edit { putInt("theme", i) }
    }

    fun setAutoplayOn(on: Boolean) {
        autoplay = on
        prefs.edit { putBoolean("autoplay", on) }
    }

    /** кроссфейд, сек; 0 — выключен (сервис читает ту же настройку) */
    var xfade by mutableStateOf(prefs.getInt("xfade", 0))
        private set

    fun setXfadeSecs(s: Int) {
        xfade = s
        prefs.edit { putInt("xfade", s) }
    }

    /** Таймер сна: минуты; 0 — выключить, -1 — после текущего трека */
    fun setSleep(min: Int) {
        App.sleepAt.value = when {
            min == 0 -> 0L
            min < 0 -> -1L
            else -> System.currentTimeMillis() + min * 60_000L
        }
    }

    /** Итоги месяца или года; null — ещё считаются */
    var recap by mutableStateOf<Recap?>(null)
    var recapYear by mutableStateOf(false)

    fun loadRecap(year: Boolean) = viewModelScope.launch {
        recapYear = year
        recap = runCatching { core("recap") { put("year", JsonPrimitive(year)) }.to<Recap>() }.getOrNull() ?: Recap()
    }

    /** История прослушиваний: свежие сверху. Открывается обычным списком треков — его можно сохранить плейлистом */
    fun openHistory() = viewModelScope.launch {
        val h = runCatching { core("history").to<List<Played>>() }.getOrDefault(emptyList())
        go(Screen.Tracks(
            tr("История", "History", "Історія", "História"),
            tr("Последние прослушивания: ${h.size}", "Recent plays: ${h.size}", "Останні прослуховування: ${h.size}", "Posledné prehratia: ${h.size}"),
            h.firstOrNull()?.track?.cover ?: "", h.map { it.track },
        ))
    }

    // ── заметки к моментам трека ──
    var notes by mutableStateOf<List<Note>>(emptyList())

    fun loadNotes(t: Track) = viewModelScope.launch {
        notes = runCatching { core("notes") { put("key", JsonPrimitive(t.key)) }.to<List<Note>>() }.getOrDefault(emptyList())
    }

    fun addNote(t: Track, text: String) = viewModelScope.launch {
        if (text.isBlank()) return@launch
        // звук на другом устройстве — время берём у него (position), а не у своего плеера, который стоит на старом месте
        val pos = (if (remote != null) position else controller?.currentPosition ?: position) / 1000.0
        notes = runCatching { core("note_add") { put("key", JsonPrimitive(t.key)); put("pos", JsonPrimitive(pos)); put("text", JsonPrimitive(text)) }.to<List<Note>>() }.getOrDefault(notes)
    }

    fun removeNote(t: Track, i: Int) = viewModelScope.launch {
        notes = runCatching { core("note_remove") { put("key", JsonPrimitive(t.key)); put("i", JsonPrimitive(i)) }.to<List<Note>>() }.getOrDefault(notes)
    }

    fun copyLog() = viewModelScope.launch {
        val text = BhLog.tail()
        val cm = getApplication<Application>().getSystemService(android.content.ClipboardManager::class.java)
        cm.setPrimaryClip(android.content.ClipData.newPlainText("Gargantua log", text))
        toast(tr("Журнал скопирован", "Log copied", "Журнал скопійовано", "Záznam skopírovaný"))
    }

    fun setSpicy(k: String) {
        spicyKey = k.trim()
        prefs.edit { putString("spicy", spicyKey) }
    }

    // ── навигация ──
    val stack = mutableStateListOf<Screen>(Screen.Home)
    val screen get() = stack.last()

    fun go(s: Screen) {
        if (s == Screen.Home || s == Screen.Search || s == Screen.Library || s == Screen.Account) {
            stack.clear() // вкладки снизу — корень
        }
        stack.add(s)
    }

    fun back(): Boolean {
        if (stack.size <= 1) return false
        stack.removeAt(stack.lastIndex)
        return true
    }

    fun toast(s: String) {
        App.toasts.tryEmit(s)
    }

    // ── аккаунт ──
    val cloudOn = App.cloudOn
    var email by mutableStateOf("")
    var signedIn by mutableStateOf(false)
    var verified by mutableStateOf(false)
    var accBusy by mutableStateOf(false)
    var accMsg by mutableStateOf("")
    var register by mutableStateOf(false)

    private val lang get() = L.lang

    fun refreshAccount() = viewModelScope.launch {
        runCatching { core("account").jsonObject }.getOrNull()?.let {
            email = it["email"]?.jsonPrimitive?.content ?: ""
            signedIn = it["signed_in"]?.jsonPrimitive?.boolean == true
            verified = it["verified"]?.jsonPrimitive?.boolean == true
        }
        if (!(signedIn && verified) && loaded) {
            loaded = false // вышли — плейлисты этого аккаунта больше не показываем
            playlists.clear()
        }
    }

    /** Запрос к аккаунту; Ok — что написать под кнопками */
    private fun accJob(f: suspend () -> String) = viewModelScope.launch {
        accBusy = true
        accMsg = ""
        accMsg = try {
            f()
        } catch (e: CoreError) {
            if (sessionDead(e.code)) runCatching { core("sign_out") }
            accErr(e.code)
        }
        accBusy = false
        refreshAccount().join()
        if (verified) sync(false)
    }

    fun signIn(e: String, p: String) = accJob {
        val ok = core("sign_in") { put("lang", JsonPrimitive(lang)); put("email", JsonPrimitive(e)); put("password", JsonPrimitive(p)) }.jsonPrimitive.boolean
        if (ok) "" else tr("Почта не подтверждена: открой ссылку из письма или отправь его ещё раз", "Email isn't confirmed: open the link from the email or send it again", "Пошту не підтверджено: відкрий посилання з листа або надішли його ще раз", "E-mail nie je potvrdený: otvor odkaz z e-mailu alebo ho pošli znova")
    }

    fun signUp(e: String, p: String) = accJob {
        core("sign_up") { put("lang", JsonPrimitive(lang)); put("email", JsonPrimitive(e)); put("password", JsonPrimitive(p)) }
        tr("Письмо ушло на $e. Открой ссылку в нём и вернись сюда", "Sent an email to $e. Open the link in it and come back", "Лист пішов на $e. Відкрий посилання в ньому і повертайся", "E-mail odišiel na $e. Otvor odkaz v ňom a vráť sa sem")
    }

    fun resetPassword(e: String) = accJob {
        core("send_reset") { put("lang", JsonPrimitive(lang)); put("email", JsonPrimitive(e.trim())) }
        tr("Письмо для смены пароля ушло на ${e.trim()}", "Sent a password reset email to ${e.trim()}", "Лист для зміни пароля пішов на ${e.trim()}", "E-mail na zmenu hesla odišiel na ${e.trim()}")
    }

    fun resendVerify() = accJob {
        core("send_verify") { put("lang", JsonPrimitive(lang)) }
        tr("Отправил ещё раз, глянь и в спаме", "Sent again, check spam too", "Надіслав ще раз, глянь і в спамі", "Poslal som znova, pozri aj do spamu")
    }

    fun checkVerified(manual: Boolean = true) = accJob {
        val ok = core("check_verified") { put("lang", JsonPrimitive(lang)) }.jsonPrimitive.boolean
        if (ok || !manual) "" else tr("Почта ещё не подтверждена", "Email isn't confirmed yet", "Пошту ще не підтверджено", "E-mail ešte nie je potvrdený")
    }

    fun signOut() = viewModelScope.launch {
        pushNow()
        runCatching { core("sign_out") }
        refreshAccount().join()
        accMsg = tr("Вышел. Плейлисты остались в аккаунте", "Signed out. Playlists stay in your account", "Вийшов. Плейлисти лишилися в акаунті", "Odhlásený. Playlisty zostali v účte")
        stack.clear()
        stack.add(Screen.Home)
    }

    // ── плейлисты: живут в аккаунте, на телефоне только в памяти ──
    val playlists = mutableStateListOf<Playlist>()
    var loaded by mutableStateOf(false)
    var syncing by mutableStateOf(false)
    var syncedAt by mutableStateOf("")
    private var pushJob: Job? = null

    /** Слить с облаком: первый раз — загрузить, дальше — трёхстороннее слияние в ядре (как на ПК) */
    fun sync(manual: Boolean) {
        if (!cloudOn || !signedIn || syncing || accBusy) return
        if (!verified) {
            checkVerified(manual)
            return
        }
        startConnect()
        syncing = true
        viewModelScope.launch {
            val snap = playlists.toList()
            val first = !loaded
            try {
                val res = if (first) core("cloud_load").to<List<Playlist>>() else core("cloud_sync") { put("playlists", snap) }.to<List<Playlist>>()
                if (!first && playlists.toList() != snap) {
                    // пока ждали, плейлисты поправили — результат устарел, сливаем заново
                    syncing = false
                    sync(manual)
                    return@launch
                }
                core("cloud_commit")
                if (res != snap) {
                    playlists.clear()
                    playlists.addAll(res)
                }
                ensureLiked()
                loaded = true
                syncedAt = SimpleDateFormat("HH:mm", Locale.ROOT).format(Date())
                if (manual) toast(tr("Плейлисты синхронизированы", "Playlists synced", "Плейлисти синхронізовано", "Playlisty synchronizované"))
            } catch (e: CoreError) {
                if (sessionDead(e.code)) {
                    runCatching { core("sign_out") }
                    refreshAccount()
                }
                if (manual || first) toast(accErr(e.code))
            }
            syncing = false
        }
    }

    /** Правка плейлиста: через 2 с на сервер */
    private fun edited() {
        pushJob?.cancel()
        pushJob = viewModelScope.launch {
            delay(2000)
            sync(false)
        }
    }

    /** Свернули приложение или выходим — неотправленное на сервер сразу */
    fun pushNow() = viewModelScope.launch {
        if (!loaded) return@launch
        val dirty = runCatching { core("cloud_dirty") { put("playlists", playlists.toList()) }.jsonPrimitive.boolean }.getOrDefault(false)
        if (dirty) sync(false)
        runCatching { core("stats_save") }
        syncStats()
    }

    private fun uniqueName(base: String): String {
        val b = base.trim().ifEmpty { tr("Плейлист ${playlists.size + 1}", "Playlist ${playlists.size + 1}", "Плейлист ${playlists.size + 1}", "Playlist ${playlists.size + 1}") }
        var name = b
        var n = 2
        while (playlists.any { it.name == name }) name = "$b ${n++}"
        return name
    }

    fun createPlaylist(name: String, tracks: List<Track> = emptyList(), source: Source? = null): String {
        val n = uniqueName(name)
        playlists.add(Playlist(name = n, tracks = tracks, source = source))
        edited()
        return n
    }

    fun renamePlaylist(old: String, name: String, desc: String) {
        val i = playlists.indexOfFirst { it.name == old }.takeIf { it >= 0 } ?: return
        val n = if (name.isNotBlank() && name.trim() != old) uniqueName(name) else old
        playlists[i] = playlists[i].copy(name = n, desc = desc.trim())
        if (n != old) stack.replaceAll { if (it == Screen.Pl(old)) Screen.Pl(n) else it }
        edited()
    }

    fun deletePlaylist(name: String) {
        playlists.removeAll { it.name == name }
        stack.removeAll { it == Screen.Pl(name) }
        if (stack.isEmpty()) stack.add(Screen.Library)
        toast(tr("«$name» улетел за горизонт событий", "“$name” fell past the event horizon", "«$name» полетів за горизонт подій", "„$name“ odletel za horizont udalostí"))
        edited()
    }

    /** «Любимые треки» есть всегда и стоят первыми */
    private fun ensureLiked() {
        val i = playlists.indexOfFirst { it.name == LIKED }
        when {
            i < 0 -> playlists.add(0, Playlist(name = LIKED))
            i > 0 -> playlists.add(0, playlists.removeAt(i))
        }
    }

    fun addTo(name: String, t: Track) {
        if (name == LIKED) return // в любимые — только сердечком
        val i = playlists.indexOfFirst { it.name == name }.takeIf { it >= 0 } ?: return
        if (playlists[i].tracks.any { it.key == t.key }) {
            toast(tr("Уже есть в «$name»", "Already in “$name”", "Вже є в «$name»", "Už je v „$name“"))
            return
        }
        playlists[i] = playlists[i].copy(tracks = playlists[i].tracks + t)
        toast(tr("Добавил в «$name»", "Added to “$name”", "Додав у «$name»", "Pridané do „$name“"))
        edited()
    }

    fun isLiked(t: Track?) = t != null && playlists.firstOrNull { it.name == LIKED }?.tracks?.any { it.key == t.key } == true

    /** ♥: в любимые (свежие сверху) или обратно. Плейлист любимых создаётся при первом сердечке */
    fun toggleLike(t: Track) {
        val i = playlists.indexOfFirst { it.name == LIKED }
        when {
            i < 0 -> playlists.add(0, Playlist(name = LIKED, tracks = listOf(t)))
            playlists[i].tracks.any { it.key == t.key } -> playlists[i] = playlists[i].copy(tracks = playlists[i].tracks.filter { it.key != t.key })
            else -> playlists[i] = playlists[i].copy(tracks = listOf(t) + playlists[i].tracks)
        }
        edited()
    }

    /** ♥ всем трекам плейлиста: новые — сверху любимых в том же порядке, уже лайкнутые остаются на месте */
    fun likeAll(tracks: List<Track>) {
        ensureLiked()
        val have = playlists[0].tracks.map { it.key }.toSet()
        val new = tracks.filter { it.key !in have }.distinctBy { it.key }
        if (new.isEmpty()) return toast(tr("Тут всё уже в любимых", "Everything here is already liked", "Тут усе вже в улюблених", "Všetko tu už je v obľúbených"))
        playlists[0] = playlists[0].copy(tracks = new + playlists[0].tracks)
        edited()
        toast(tr("В любимые: ${new.size} треков ♥", "Liked ${new.size} tracks ♥", "В улюблені: ${new.size} треків ♥", "Do obľúbených: ${new.size} skladieb ♥"))
    }

    fun removeFrom(name: String, t: Track) {
        val i = playlists.indexOfFirst { it.name == name }.takeIf { it >= 0 } ?: return
        playlists[i] = playlists[i].copy(tracks = playlists[i].tracks.filter { it.key != t.key })
        edited()
    }

    var importing by mutableStateOf(false)

    /** Картинки с QR-кодом плейлиста (галерея или камера) → новый плейлист. Формат QR как на ПК */
    fun importQr(paths: List<String>) {
        if (importing || paths.isEmpty()) return
        importing = true
        toast(tr("Читаю QR…", "Reading QR…", "Читаю QR…", "Čítam QR…"))
        viewModelScope.launch {
            try {
                val r = core("qr_decode") { put("paths", json.encodeToJsonElement(paths)) }.to<Imported>()
                val src = r.url.takeIf { it.isNotEmpty() }?.let { Source(url = it, keys = r.tracks.map { t -> t.key }) }
                go(Screen.Pl(createPlaylist(r.name, r.tracks, src)))
                toast(tr("Готово: ${r.tracks.size} треков", "Done: ${r.tracks.size} tracks", "Готово: ${r.tracks.size} треків", "Hotovo: ${r.tracks.size} skladieb"))
            } catch (e: CoreError) {
                toast(tr("QR не прочитался: ${e.code}", "Couldn't read the QR: ${e.code}", "QR не прочитався: ${e.code}", "QR sa neprečítal: ${e.code}"))
            }
            importing = false
        }
    }

    /** Плейлист → картинки QR (PNG в кэше), по одной на часть */
    suspend fun qrImages(p: Playlist): List<String> =
        runCatching { core("qr_encode") { put("playlist", json.encodeToJsonElement(p)) }.to<List<String>>() }.getOrDefault(emptyList())

    fun importUrl(url: String) {
        if (importing) return
        importing = true
        toast(tr("Импортирую…", "Importing…", "Імпортую…", "Importujem…"))
        viewModelScope.launch {
            try {
                val r = core("import") { put("url", JsonPrimitive(url.trim())) }.to<Imported>()
                if (r.tracks.isEmpty()) {
                    toast(tr("Пусто: в плейлисте нет треков", "Empty: no tracks in that playlist", "Порожньо: у плейлисті немає треків", "Prázdne: v playliste nie sú skladby"))
                } else {
                    val n = createPlaylist(r.name, r.tracks, Source(url = r.url, keys = r.tracks.map { it.key }))
                    go(Screen.Pl(n))
                    toast(
                        if (r.missing > 0) tr("Готово. Не нашлось на YT Music: ${r.missing}", "Done. Not found on YT Music: ${r.missing}", "Готово. Не знайшлося на YT Music: ${r.missing}", "Hotovo. Na YT Music sa nenašlo: ${r.missing}")
                        else tr("Готово: ${r.tracks.size} треков", "Done: ${r.tracks.size} tracks", "Готово: ${r.tracks.size} треків", "Hotovo: ${r.tracks.size} skladieb"),
                    )
                }
            } catch (e: CoreError) {
                toast(tr("Не вышло: ${e.code}", "Failed: ${e.code}", "Не вийшло: ${e.code}", "Nevyšlo to: ${e.code}"))
            }
            importing = false
        }
    }

    // ── поиск ──
    var query by mutableStateOf("")
    var filter by mutableStateOf(0) // 0 все, 1 YT Music, 2 SoundCloud, 3 Яндекс
    val results = mutableStateListOf<List<Track>>(emptyList(), emptyList(), emptyList())
    var cards by mutableStateOf(Cards())
    var searching by mutableStateOf(0)
    private var searchGen = 0

    fun search(q: String) {
        val text = q.trim()
        if (text.isEmpty()) return
        val gen = ++searchGen
        searching = 4
        // по строчке из песни — среди текстов, которые уже открывались (с диска, без сети)
        viewModelScope.launch {
            val h = runCatching { core("lyr_search") { put("q", JsonPrimitive(text)) }.to<List<LyrHit>>() }.getOrDefault(emptyList())
            if (gen == searchGen) lyrHits = h
        }
        listOf("yt", "sc", "ya").forEachIndexed { i, src ->
            viewModelScope.launch {
                val r = runCatching { core("search") { put("q", JsonPrimitive(text)); put("src", JsonPrimitive(src)) }.to<List<Track>>() }.getOrDefault(emptyList())
                if (gen == searchGen) {
                    results[i] = r
                    searching--
                }
            }
        }
        viewModelScope.launch {
            val c = runCatching { core("cards") { put("q", JsonPrimitive(text)) }.to<Cards>() }.getOrDefault(Cards())
            if (gen == searchGen) {
                cards = c
                searching--
            }
        }
    }

    /** Все сервисы вперемешку по кругу, чтобы сверху был каждый — как на ПК */
    var lyrHits by mutableStateOf<List<LyrHit>>(emptyList())

    val shownResults: List<Track>
        get() = if (filter == 4) lyrHits.map { it.track } else if (filter == 0) {
            val n = results.maxOfOrNull { it.size } ?: 0
            (0 until n).flatMap { i -> results.mapNotNull { it.getOrNull(i) } }
        } else results[filter - 1]

    // ── каталог ──
    suspend fun loadArtist(id: String, name: String): Artist? = runCatching { core("artist") { put("id", JsonPrimitive(id)); put("name", JsonPrimitive(name)) }.to<Artist>() }.getOrNull()
    suspend fun loadAlbum(id: String): Album? = runCatching { core("album") { put("id", JsonPrimitive(id)) }.to<Album>() }.getOrNull()

    fun openArtistOf(t: Track) = go(Screen.ArtistPage(if (t.src == "yt") t.artist_id else "", t.artist.split(",", "&", " feat").first().trim()))

    fun startRadio(t: Track) {
        toast(tr("Собираю радио по «${t.title}»…", "Building a radio from “${t.title}”…", "Збираю радіо за «${t.title}»…", "Skladám rádio podľa „${t.title}“…"))
        viewModelScope.launch {
            val r = runCatching { core("radio") { put("track", t) }.to<List<Track>>() }
            r.onSuccess { list ->
                val tracks = listOf(t) + list.filter { !(it.title == t.title && it.artist == t.artist) }
                val n = createPlaylist("📻 ${t.title}", tracks)
                go(Screen.Pl(n))
                play(tracks, 0)
            }.onFailure { toast(tr("Радио не собралось", "Couldn't build a radio", "Радіо не зібралося", "Rádio sa nepodarilo poskladať")) }
        }
    }

    // ── главная и статистика ──
    var home by mutableStateOf(Home())
    var homeLoading by mutableStateOf(false)
    var stats by mutableStateOf(StatsSummary())

    fun loadHome(force: Boolean = false) {
        if (homeLoading) return
        homeLoading = true
        viewModelScope.launch {
            home = runCatching {
                core("home") {
                    put("force", JsonPrimitive(force))
                    put("names", kotlinx.serialization.json.JsonArray(listOf(
                        tr("Твой микс", "Your mix", "Твій мікс", "Tvoj mix"), tr("Открытия", "Discover", "Відкриття", "Objavy"), tr("Микс: {}", "{} mix", "Мікс: {}", "Mix: {}"),
                        // умные подборки из статистики — идут после миксов, сохранить себе можно кнопкой «+» внутри
                        tr("Давно не слушал", "Long time no hear", "Давно не слухав", "Dlho nepočuté"),
                        tr("Заслушано в этом месяце", "On repeat this month", "Заслухано цього місяця", "Najhranejšie tento mesiac"),
                        tr("Лайкнул и забыл", "Liked and forgotten", "Лайкнув і забув", "Lajknuté a zabudnuté"),
                    ).map(::JsonPrimitive)))
                    put("liked", json.encodeToJsonElement(playlists.firstOrNull { it.name == LIKED }?.tracks ?: emptyList()))
                }.to<Home>()
            }.getOrDefault(home)
            homeLoading = false
        }
    }

    fun loadStats() = viewModelScope.launch {
        stats = runCatching { core("stats").to<StatsSummary>() }.getOrDefault(stats)
        // сначала то, что есть, потом — вместе со свежими долями других устройств
        syncStats()
        stats = runCatching { core("stats").to<StatsSummary>() }.getOrDefault(stats)
        val f = stats.fav ?: return@launch
        if (favPhotoFor != f.name) {
            favPhotoFor = f.name
            favPhoto = ""
            favPhoto = runCatching {
                core("artist_photo") { put("name", JsonPrimitive(f.name)); f.track?.let { put("track", json.encodeToJsonElement(it)) } }.to<String>()
            }.getOrDefault("")
        }
    }

    /** Фото любимого артиста для карточки статистики */
    var favPhoto by mutableStateOf("")
        private set
    private var favPhotoFor = ""

    /** Статистика общая на аккаунт: своя доля — в облако, доли других устройств (ПК) — к себе */
    // ── вход YouTube ──
    /** Куки YouTube с компа: там Настройки → Куки YouTube → «На телефон» */
    fun pullYtCookies() {
        if (!cloudOn || !signedIn) return toast(tr("Сначала войди в аккаунт Gargantua", "Sign in to Gargantua first", "Спершу увійди в акаунт Gargantua", "Najprv sa prihlás do Gargantua"))
        viewModelScope.launch {
            val r = runCatching { core("yt_cookies").to<String>() }
            val ck = r.getOrNull()
            when {
                r.isFailure -> toast(tr("Не вышло: ${r.exceptionOrNull()?.message}", "Failed: ${r.exceptionOrNull()?.message}", "Не вийшло: ${r.exceptionOrNull()?.message}", "Nevyšlo to: ${r.exceptionOrNull()?.message}"))
                ck.isNullOrEmpty() -> toast(tr(
                    "Комп ещё не отправил. На ПК: Настройки → Куки YouTube → «На телефон»",
                    "The PC hasn't sent it yet. On the PC: Settings → YouTube cookies → “To phone”",
                    "Комп ще не надіслав. На ПК: Налаштування → Куки YouTube → «На телефон»", "PC to ešte neposlal. Na PC: Nastavenia → Cookies YouTube → „Na telefón“",
                ))
                else -> {
                    YtAuth.setFromPc(ck)
                    ytSignedIn()
                }
            }
        }
    }

    fun ytSignedIn() {
        Streams.unblock()
        toast(tr("YouTube: вошёл, треки пойдут под аккаунтом", "YouTube: signed in, tracks will play via your account", "YouTube: увійшов, треки підуть під акаунтом", "YouTube: prihlásený, skladby pôjdu cez tvoj účet"))
    }

    private suspend fun syncStats() {
        if (!cloudOn || !signedIn || !verified) return
        runCatching { core("stats_sync") }
    }

    // ── плеер: MediaController к PlayerService ──
    var controller: MediaController? = null
        private set
    var current by mutableStateOf<Track?>(null)
    var playing by mutableStateOf(false)
    var buffering by mutableStateOf(false)
    var position by mutableStateOf(0L)
    var duration by mutableStateOf(0L)
    // перемешка помнится между запусками, пока её не выключат
    var shuffle by mutableStateOf(prefs.getBoolean("shuffle", false))
    var repeat by mutableStateOf(Player.REPEAT_MODE_OFF)
    val queue = mutableStateListOf<MediaItem>()
    var queueIndex by mutableStateOf(0)
    var nowOpen by mutableStateOf(false)

    private fun connect() {
        val token = SessionToken(getApplication(), ComponentName(getApplication(), PlayerService::class.java))
        val f = MediaController.Builder(getApplication(), token).buildAsync()
        f.addListener({
            val c = runCatching { f.get() }.getOrNull() ?: return@addListener
            controller = c
            c.addListener(object : Player.Listener {
                override fun onEvents(player: Player, events: Player.Events) = readPlayer()
            })
            // перемешка у нас своя (треки переставляются по-настоящему), встроенная только путала очередь
            c.shuffleModeEnabled = false
            readPlayer()
        }, ContextCompat.getMainExecutor(getApplication()))
    }

    private fun readPlayer() {
        if (remote != null) return // звук на другом устройстве — показываем его, а не свой плеер
        val c = controller ?: return
        val t = c.currentMediaItem?.track()
        // пока очередь правится (перемешка, «похожее»), плеер на миг отдаёт «нет трека» — это не смена трека,
        // иначе текст сбрасывался и искался заново
        if (t == null && c.mediaItemCount > 0) return
        if (t?.key != current?.key) {
            current = t
            loadLyrics(t)
            if (t != null) loadNotes(t) else notes = emptyList()
        }
        playing = c.isPlaying
        buffering = c.playbackState == Player.STATE_BUFFERING
        duration = c.duration.coerceAtLeast(0)
        position = c.currentPosition
        repeat = c.repeatMode
        queueIndex = c.currentMediaItemIndex
        val items = (0 until c.mediaItemCount).map { c.getMediaItemAt(it) }
        if (items.map { it.mediaId } != queue.map { it.mediaId }) {
            queue.clear()
            queue.addAll(items)
        }
    }

    /** Исходный порядок очереди до перемешки — по нему её и возвращаем */
    private var unshuffled: List<String> = emptyList()

    fun play(tracks: List<Track>, index: Int, shuffled: Boolean = false) {
        remote?.let { r ->
            // звук на другом устройстве — там и включаем
            val t = tracks.getOrNull(index) ?: return
            val sh = shuffled || r.shuffle
            val first = if (shuffled) tracks.first() else t
            val rest = tracks.filterIndexed { i, _ -> i != tracks.indexOf(first) }
            val upnext = if (sh) rest.shuffled() else tracks.drop(index + 1)
            return connLoad(if (sh) emptyList() else tracks.take(index), first, upnext, tracks, sh)
        }
        val c = controller ?: return
        unshuffled = tracks.map { it.key }
        val items = tracks.map { it.item() }
        shuffle = shuffled || (shuffle && tracks.size > 1)
        if (shuffle) {
            // нажатый трек — первым, остальные вразнобой следом
            val first = if (shuffled) -1 else index
            c.setMediaItems(listOfNotNull(items.getOrNull(first)) + items.filterIndexed { i, _ -> i != first }.shuffled(), 0, 0)
        } else {
            c.setMediaItems(items, index, 0)
        }
        c.prepare()
        c.play()
    }

    /** Ссылка на трек там, откуда он — в буфер обмена. Логика как Track::share_url на ПК */
    fun share(t: Track) {
        val url = when {
            t.src == "lc" -> ""
            t.url.startsWith("http") -> t.url
            t.src == "yt" && t.id.isNotEmpty() -> "https://music.youtube.com/watch?v=${t.id}"
            else -> ""
        }
        if (url.isEmpty()) return toast(tr("У этого трека нет ссылки", "This track has no link", "У цього треку немає посилання", "Táto skladba nemá odkaz"))
        val clip = getApplication<Application>().getSystemService(android.content.ClipboardManager::class.java)
        clip.setPrimaryClip(android.content.ClipData.newPlainText(t.title, url))
        toast(tr("Ссылка скопирована", "Link copied", "Посилання скопійовано", "Odkaz skopírovaný"))
    }

    fun playNext(t: Track) {
        remote?.let { r ->
            connQueue(r.played, listOf(t) + r.upnext, r.source, r.shuffle)
            return toast(tr("Следующим: ${t.title}", "Up next: ${t.title}", "Наступним: ${t.title}", "Ako ďalšia: ${t.title}"))
        }
        val c = controller ?: return
        if (c.mediaItemCount == 0) return play(listOf(t), 0)
        c.addMediaItem(c.currentMediaItemIndex + 1, t.item())
        toast(tr("Следующим: ${t.title}", "Up next: ${t.title}", "Наступним: ${t.title}", "Ako ďalšia: ${t.title}"))
    }

    fun toggle() {
        remote?.let { r ->
            val now = connNow()
            remote = hold(r, r.copy(pos = posNow(r), at = now, playing = !r.playing))
            playing = !r.playing
            return connSend("toggle", JsonNull)
        }
        controller?.run { if (isPlaying) pause() else { if (playbackState == Player.STATE_IDLE) prepare(); play() } }
    }
    fun next() { if (remote != null) connSend("next", JsonNull) else controller?.seekToNextMediaItem() }
    fun prev() { if (remote != null) connSend("prev", JsonNull) else controller?.seekToPrevious() }
    /** Строго прошлый трек, без «сначала в начало текущего» — для свайпа по мини-плееру */
    fun prevItem() {
        val r = remote ?: return run { controller?.seekToPreviousMediaItem() }
        val p = r.played.lastOrNull() ?: return
        connLoad(r.played.dropLast(1), p, listOfNotNull(r.track) + r.upnext, r.source, r.shuffle)
    }
    fun seek(ms: Long) {
        remote?.let { r ->
            remote = hold(r, r.copy(pos = ms / 1000.0, at = connNow()))
            position = ms
            return connSend("seek", JsonPrimitive(ms / 1000.0))
        }
        controller?.seekTo(ms)
    }
    fun jump(i: Int) {
        remote?.let { r ->
            val all = r.played + listOfNotNull(r.track) + r.upnext
            val t = all.getOrNull(i) ?: return
            return connLoad(all.take(i), t, all.drop(i + 1), r.source, r.shuffle)
        }
        controller?.run { seekTo(i, 0); play() }
    }
    /** Перемешка как в Spotify: треки после текущего переставляются по-настоящему, очередь показывает ровно то,
     *  что заиграет. Выключили — оставшиеся встают в исходный порядок (добавленные потом — в конце, как были) */
    /** умное перемешивание: раз в 2–3 трека сервис подмешивает похожий не из плейлиста */
    var smartShuffle by mutableStateOf(prefs.getBoolean("smart_shuffle", false))
        private set

    private fun setSmart(on: Boolean) {
        smartShuffle = on
        prefs.edit { putBoolean("smart_shuffle", on) }
        if (on) {
            App.smartKick.tryEmit(Unit)
            return
        }
        // подмешанное, но ещё не сыгранное, из очереди уходит
        controller?.let { c ->
            for (i in c.mediaItemCount - 1 downTo c.currentMediaItemIndex + 1) if (c.getMediaItemAt(i).mediaId in App.smartIds) c.removeMediaItem(i)
        }
        App.smartIds.clear()
    }

    /** Кнопка ходит по кругу: выкл → перемешка → умная перемешка → выкл */
    fun toggleShuffle() {
        if (remote == null && shuffle && !smartShuffle) {
            setSmart(true)
            return toast(tr("Умное перемешивание: раз в 2–3 трека — похожий не из плейлиста", "Smart shuffle: every 2–3 tracks, a similar one from outside the playlist", "Розумне перемішування: раз на 2–3 треки — схожий не з плейлиста", "Inteligentné miešanie: každé 2–3 skladby podobná mimo playlistu"))
        }
        remote?.let { r ->
            // звук на другом устройстве: круг проходит оно, а кнопка сразу показывает, что будет, и три секунды
            // не слушает базу — иначе, пока команда едет, значок прыгал туда-обратно
            val (on, smart) = when {
                !r.shuffle -> true to false
                !r.smart -> true to true
                else -> false to false
            }
            shuffle = on
            remote = r.copy(shuffle = on, smart = smart)
            heldSmart = smart
            shuffleHeldUntil = System.currentTimeMillis() + 3000
            return connSend("shuffle", JsonNull)
        }
        setSmart(false)
        val c = controller ?: return
        val cur = c.currentMediaItemIndex
        val rest = (cur + 1 until c.mediaItemCount).map { c.getMediaItemAt(it) }
        shuffle = !shuffle
        if (rest.size < 2) return
        val order = if (shuffle) {
            unshuffled = queue.map { it.mediaId }
            rest.shuffled()
        } else {
            val at = unshuffled.withIndex().associate { it.value to it.index }
            rest.sortedBy { at[it.mediaId] ?: Int.MAX_VALUE }
        }
        c.replaceMediaItems(cur + 1, c.mediaItemCount, order)
    }

    fun moveInQueue(from: Int, to: Int) {
        remote?.let { r ->
            // индексы — во всей очереди (прошлые + текущий + следующие); двигать можно только следующие
            val base = r.played.size + 1
            val up = r.upnext.toMutableList()
            if (from - base !in up.indices || to - base !in up.indices) return
            up.add(to - base, up.removeAt(from - base))
            return connQueue(r.played, up, r.source, r.shuffle)
        }
        controller?.moveMediaItem(from, to)
    }
    fun removeFromQueue(i: Int) {
        remote?.let { r ->
            val base = r.played.size + 1
            if (i - base !in r.upnext.indices) return
            return connQueue(r.played, r.upnext.filterIndexed { j, _ -> j != i - base }, r.source, r.shuffle)
        }
        controller?.removeMediaItem(i)
    }
    fun cycleRepeat() {
        remote?.let { r ->
            val next = (r.repeat + 1) % 3
            remote = r.copy(repeat = next)
            repeat = modeOf(next)
            repeatHeldUntil = System.currentTimeMillis() + 3000 // пока команда едет, из базы ещё приходит старое значение
            return connSend("repeat", JsonPrimitive(next))
        }
        cycleRepeatLocal()
    }
    private fun cycleRepeatLocal() = controller?.run {
        repeatMode = when (repeatMode) {
            Player.REPEAT_MODE_OFF -> Player.REPEAT_MODE_ALL
            Player.REPEAT_MODE_ALL -> Player.REPEAT_MODE_ONE
            else -> Player.REPEAT_MODE_OFF
        }
    }

    // ── Connect: одна сессия на все устройства аккаунта (ядро общее с ПК — connect.rs) ──
    // Звук идёт на одном устройстве. Если не тут — показываем ту сессию («зеркало»), а кнопки уходят туда
    // командами. Пока звук тут — пишем своё состояние в базу, остальные его показывают

    /** звук на другом устройстве: его сессия */
    var remote by mutableStateOf<ConnState?>(null)
    var remoteName by mutableStateOf("")
    var connDevs by mutableStateOf<List<ConnDev>>(emptyList())
    var connOn by mutableStateOf(false)
    private var connMe = ""
    private var connOffset = 0L // серверное время − наше
    private var connHost: String? = null // за кем записана сессия в базе
    private var adopted = ""
    private var pubSig = ""
    private var pubQ = 0
    private var pubPos = 0.0
    private var pubPlaying = false
    private var pubAt = 0L

    // нажали паузу/перемотку на зеркале: пока хозяин не записал новое состояние, события с прежним
    // (эхо нашей же команды) не откатывают кнопку и ползунок назад
    private var held: ConnState? = null
    private var heldBase = 0L
    private var heldUntil = 0L
    private fun hold(r: ConnState, next: ConnState): ConnState {
        if (held == null || System.currentTimeMillis() >= heldUntil) heldBase = r.at
        held = next
        heldUntil = System.currentTimeMillis() + 4000
        return next
    }

    private fun connNow() = System.currentTimeMillis() + connOffset
    private fun posNow(r: ConnState): Double {
        val p = if (r.playing && r.at > 0) r.pos + (connNow() - r.at).coerceAtLeast(0) / 1000.0 else r.pos
        val d = r.track?.dur ?: 0
        return if (d > 0) p.coerceAtMost(d.toDouble()) else p
    }
    private fun modeOf(r: Int) = when (r) { 1 -> Player.REPEAT_MODE_ALL; 2 -> Player.REPEAT_MODE_ONE; else -> Player.REPEAT_MODE_OFF }
    private fun repeatOf(m: Int) = when (m) { Player.REPEAT_MODE_ALL -> 1; Player.REPEAT_MODE_ONE -> 2; else -> 0 }

    private fun startConnect() {
        if (connOn || !cloudOn) return
        connOn = true
        viewModelScope.launch {
            val name = android.provider.Settings.Global.getString(getApplication<Application>().contentResolver, "device_name") ?: android.os.Build.MODEL
            runCatching { core("connect_start") { put("name", JsonPrimitive(name)) } }
            var ver = 0L
            while (true) {
                val r = runCatching { core("connect_wait") { put("since", JsonPrimitive(ver)); put("ms", JsonPrimitive(20_000)) }.to<ConnSnap>() }.getOrNull()
                if (r == null) {
                    delay(3000)
                    continue
                }
                ver = r.ver
                connOffset = r.now - System.currentTimeMillis()
                connMe = r.me
                onConn(r)
            }
        }
        viewModelScope.launch {
            while (true) {
                delay(500)
                connTick()
            }
        }
    }

    private fun onConn(r: ConnSnap) {
        r.cmds.forEach { connExec(it.op, it.a) }
        connHost = r.state?.host
        val st = r.state
        if (st != null && st.host == connMe && st.xfer.isNotEmpty() && st.xfer != adopted && r.now - st.at < 60_000) {
            adopted = st.xfer
            adopt(st, true)
        } else if (st != null && r.host != null && r.host != connMe) {
            mirror(st, r.devices.firstOrNull { it.id == r.host }?.name ?: "")
        } else if (remote != null) {
            // устройство со звуком пропало — его трек остаётся тут на паузе, play продолжит уже здесь
            remote?.let { adopt(it.copy(pos = posNow(it)), false) }
        }
        val active = r.host ?: connMe.takeIf { remote == null && playing }
        connDevs = r.devices.filter { it.online || it.id == connMe }.map { it.copy(online = it.id == active) }
    }

    /** Команда от другого устройства (звук тут) */
    private fun connExec(op: String, a: JsonElement) {
        val c = controller ?: return
        fun list(k: String) = runCatching { json.decodeFromJsonElement<List<Track>>(a.jsonObject[k] ?: JsonNull) }.getOrDefault(emptyList())
        when (op) {
            "toggle" -> toggle()
            "next" -> next()
            "prev" -> prev()
            "seek" -> seek(((a as? JsonPrimitive)?.doubleOrNull ?: 0.0).times(1000).toLong())
            "repeat" -> c.repeatMode = (a as? JsonPrimitive)?.let { p -> p.booleanOrNull?.let { if (it) Player.REPEAT_MODE_ALL else Player.REPEAT_MODE_OFF } ?: modeOf(p.intOrNull ?: 0) } ?: c.repeatMode
            "queue", "load" -> {
                val o = a.jsonObject
                val played = list("played")
                val upnext = list("upnext")
                unshuffled = list("source").map { it.key }
                shuffle = o["shuffle"]?.jsonPrimitive?.booleanOrNull ?: shuffle
                o["repeat"]?.jsonPrimitive?.let { p -> c.repeatMode = p.booleanOrNull?.let { if (it) Player.REPEAT_MODE_ALL else Player.REPEAT_MODE_OFF } ?: modeOf(p.intOrNull ?: 0) }
                val t = runCatching { json.decodeFromJsonElement<Track>(o["track"] ?: JsonNull) }.getOrNull()
                if (op == "load" && t != null) {
                    c.setMediaItems((played + t + upnext).map { it.item() }, played.size, 0)
                    c.prepare()
                    c.play()
                } else {
                    val cur = c.currentMediaItemIndex
                    c.replaceMediaItems(cur + 1, c.mediaItemCount, upnext.map { it.item() })
                    c.replaceMediaItems(0, cur, played.map { it.item() })
                }
            }
            "volume" -> audio.setStreamVolume(android.media.AudioManager.STREAM_MUSIC, (((a as? JsonPrimitive)?.doubleOrNull ?: return) * maxVol()).roundToInt(), 0)
            "shuffle" -> toggleShuffle()
            "transfer" -> connPick((a as? JsonPrimitive)?.contentOrNull ?: return)
        }
    }

    /** Звук на другом устройстве: показываем его сессию */
    private fun mirror(fresh: ConnState, name: String) {
        val h = held
        val st = if (h != null && fresh.at == heldBase && System.currentTimeMillis() < heldUntil) fresh.copy(pos = h.pos, at = h.at, playing = h.playing)
        else fresh.also { held = null }
        if (remote == null) controller?.let { if (it.isPlaying) it.pause() } // играли сами — звук теперь там
        remote = st
        remoteName = name
        if (System.currentTimeMillis() >= volHeldUntil) st.vol?.let { remoteVol = it.toFloat() }
        val t = st.track
        if (t?.key != current?.key) {
            current = t
            loadLyrics(t)
            if (t != null) loadNotes(t) else notes = emptyList()
        }
        playing = st.playing
        buffering = false
        duration = (t?.dur ?: 0) * 1000
        position = (posNow(st) * 1000).toLong()
        val nowMs = System.currentTimeMillis()
        var shown = st
        if (nowMs < repeatHeldUntil) shown = shown.copy(repeat = repeatOf(repeat)) else repeat = modeOf(st.repeat)
        if (nowMs < shuffleHeldUntil) shown = shown.copy(shuffle = shuffle, smart = heldSmart) else shuffle = st.shuffle
        remote = shown
        val items = (st.played + listOfNotNull(t) + st.upnext).map { it.item() }
        if (items.map { it.mediaId } != queue.map { it.mediaId }) {
            queue.clear()
            queue.addAll(items)
        }
        queueIndex = st.played.size
    }

    /** Сессия теперь тут: грузим её очередь и (если play) играем с того же места */
    private fun adopt(st: ConnState, play: Boolean) {
        remote = null
        remoteName = ""
        val c = controller ?: return
        val t = st.track ?: return
        unshuffled = st.source.map { it.key }
        shuffle = st.shuffle
        c.repeatMode = modeOf(st.repeat)
        c.setMediaItems((st.played + t + st.upnext).map { it.item() }, st.played.size, (posNow(st) * 1000).toLong())
        c.prepare()
        if (play && st.playing) c.play()
        pubSig = ""
        readPlayer()
    }

    private val audio by lazy { getApplication<Application>().getSystemService(android.media.AudioManager::class.java) }
    private fun maxVol() = audio.getStreamMaxVolume(android.media.AudioManager.STREAM_MUSIC).coerceAtLeast(1)

    /** Громкость там, где звук: ползунок показывает её, пока играет другое устройство */
    var remoteVol by mutableStateOf<Float?>(null)
        private set
    private var volHeldUntil = 0L
    private var repeatHeldUntil = 0L
    private var shuffleHeldUntil = 0L
    private var heldSmart = false

    /** показывать ли значок умного перемешивания: у устройства со звуком, а если звук тут — своя настройка */
    val smartShown get() = remote?.smart ?: (smartShuffle && shuffle)
    private var volSentAt = 0L

    /** Ползунок на зеркале: уходит хозяину. Пока тянут (end = false) — не чаще 5 раз в секунду, каждая команда — запись в базу */
    fun connVolume(f: Float, end: Boolean) {
        val now = System.currentTimeMillis()
        remoteVol = f
        volHeldUntil = now + 3000 // эхо из базы не дёргает ползунок назад
        if (!end && now - volSentAt < 200) return
        volSentAt = now
        connSend("volume", JsonPrimitive(f))
    }

    /** Кнопки громкости телефона, пока звук играет на другом устройстве: крутим громкость там. false — звук здесь */
    fun connVolumeStep(up: Boolean): Boolean {
        val r = remote ?: return false
        val cur = remoteVol ?: r.vol?.toFloat() ?: 0.5f
        connVolume((cur + if (up) 0.05f else -0.05f).coerceIn(0f, 1f), true)
        return true
    }

    /** Наша сессия как она есть */
    private fun localState(): ConnState? {
        val c = controller ?: return null
        val all = queue.mapNotNull { it.track() }
        val i = queueIndex
        val t = all.getOrNull(i) ?: return null
        val at = unshuffled.withIndex().associate { it.value to it.index }
        return ConnState(
            host = connMe, track = t,
            played = all.take(i).takeLast(50), upnext = all.drop(i + 1).take(500),
            source = if (shuffle) all.sortedBy { at[it.key] ?: Int.MAX_VALUE } else all,
            pos = c.currentPosition / 1000.0, playing = c.isPlaying || (c.playWhenReady && c.playbackState == Player.STATE_BUFFERING),
            shuffle = shuffle, smart = shuffle && smartShuffle, repeat = repeatOf(c.repeatMode),
            vol = audio.getStreamVolume(android.media.AudioManager.STREAM_MUSIC) / maxVol().toDouble(),
        )
    }

    /** Раз в полсекунды: зеркало двигает прогресс, а если звук тут — пишем состояние, когда что-то поменялось */
    private fun connTick() {
        remote?.let {
            position = (posNow(it) * 1000).toLong()
            return
        }
        if (connMe.isEmpty()) return
        val st = localState() ?: return
        if (!st.playing && connHost != connMe) return
        val sig = "${st.track?.key}|${st.playing}|${st.shuffle}${st.smart}|${st.repeat}|${st.vol}"
        val q = (st.played + st.upnext).map { it.key }.hashCode()
        val now = System.currentTimeMillis()
        val expected = pubPos + if (pubPlaying) (now - pubAt) / 1000.0 else 0.0
        val stale = now - pubAt > 30_000
        if (sig == pubSig && q == pubQ && kotlin.math.abs(st.pos - expected) < 2 && !stale) return
        val withQueue = q != pubQ || stale
        pubSig = sig; pubQ = q; pubPos = st.pos; pubPlaying = st.playing; pubAt = now
        connHost = connMe
        viewModelScope.launch { runCatching { core("connect_publish") { put("state", json.encodeToJsonElement(st)); put("queue", JsonPrimitive(withQueue)) } } }
    }

    private fun connSend(op: String, a: JsonElement) {
        val host = remote?.host ?: return
        viewModelScope.launch { runCatching { core("connect_send") { put("to", JsonPrimitive(host)); put("op", JsonPrimitive(op)); put("a", a) } } }
    }

    /** Очередь — устройству со звуком; connLoad — ещё и включить трек */
    private fun connQueue(played: List<Track>, upnext: List<Track>, source: List<Track>, shuffle: Boolean) =
        connSend("queue", buildJsonObject {
            put("played", json.encodeToJsonElement(played)); put("upnext", json.encodeToJsonElement(upnext))
            put("source", json.encodeToJsonElement(source)); put("shuffle", JsonPrimitive(shuffle))
        })

    private fun connLoad(played: List<Track>, t: Track, upnext: List<Track>, source: List<Track>, shuffle: Boolean) =
        connSend("load", buildJsonObject {
            put("played", json.encodeToJsonElement(played)); put("upnext", json.encodeToJsonElement(upnext))
            put("source", json.encodeToJsonElement(source)); put("shuffle", JsonPrimitive(shuffle)); put("track", t)
        })

    /** Выбрали устройство: сессия переезжает туда */
    fun connPick(id: String) {
        remote?.let { r ->
            if (r.host != id) connSend("transfer", JsonPrimitive(id))
            return
        }
        if (id == connMe) return
        val st = localState() ?: return toast(tr("Сначала включи что-нибудь", "Play something first", "Спершу увімкни щось", "Najprv niečo pusti"))
        val out = st.copy(host = id, playing = true, xfer = "$connMe-${connNow()}")
        connHost = id // сессия уже не наша: на паузе больше не пишем «хозяин я»
        viewModelScope.launch { runCatching { core("connect_publish") { put("state", json.encodeToJsonElement(out)); put("queue", JsonPrimitive(true)) } } }
        controller?.pause()
    }

    // ── текст песни ──
    var lyrics by mutableStateOf<Lyrics?>(null) // null — ищу
    private var lyrJob: Job? = null

    /** найденные тексты последних треков: вернулся к треку — текст сразу, без нового поиска */
    private val lyrCache = LinkedHashMap<String, Lyrics>()

    private fun loadLyrics(t: Track?) {
        lyrJob?.cancel()
        lyrics = null
        lyrTrMap = emptyMap()
        t ?: return
        // уже находили — сразу из памяти
        lyrCache[t.key]?.let {
            lyrics = it
            translateLyrics(false)
            return
        }
        lyrJob = viewModelScope.launch {
            val found = runCatching { core("lyrics") { put("track", t); put("spicy", JsonPrimitive(spicyKey)) }.to<Lyrics>() }.getOrDefault(Lyrics())
            if (found.kind != "none") {
                lyrCache[t.key] = found
                if (lyrCache.size > 30) lyrCache.remove(lyrCache.keys.first())
            }
            lyrics = found
            translateLyrics(false)
        }
    }

    // ── перевод текста на язык интерфейса: мелко под каждой строкой ──
    var lyrTr by mutableStateOf(prefs.getBoolean("lyr_tr", false))
        private set
    /** текст строки → перевод */
    var lyrTrMap by mutableStateOf<Map<String, String>>(emptyMap())
        private set
    private var trJob: Job? = null

    fun toggleLyrTr() {
        lyrTr = !lyrTr
        prefs.edit { putBoolean("lyr_tr", lyrTr) }
        if (lyrTr) translateLyrics(true)
    }

    /** Вся песня одним запросом. manual — нажали кнопку: тогда говорим, если перевести не вышло */
    private fun translateLyrics(manual: Boolean) {
        val l = lyrics ?: return
        if (!lyrTr || lyrTrMap.isNotEmpty() || l.kind != "synced") return
        val texts = l.lines.map { it.text.trim() }.filter { it.isNotEmpty() }.distinct()
        if (texts.isEmpty()) return
        trJob?.cancel()
        trJob = viewModelScope.launch {
            val r = runCatching { core("translate") { put("lines", json.encodeToJsonElement(texts)); put("lang", JsonPrimitive(L.lang)) } }
            if (lyrics !== l || !lyrTr) return@launch
            val out = r.getOrNull()?.let { runCatching { it.to<List<String>>() }.getOrNull() }
            when {
                out != null -> lyrTrMap = texts.zip(out).filter { (s, t) -> t.isNotEmpty() && s != t }.toMap()
                r.isSuccess && manual -> toast(tr("Текст и так на твоём языке", "Lyrics are already in your language", "Текст і так твоєю мовою", "Text už je v tvojom jazyku"))
                manual -> toast(tr("Не удалось перевести текст", "Couldn't translate the lyrics", "Не вдалося перекласти текст", "Text sa nepodarilo preložiť"))
            }
        }
    }

    init {
        L.lang = prefs.getString("lang", "en") ?: "en"
        connect()
        // новая версия: спрашиваем GitHub один раз за запуск
        viewModelScope.launch {
            delay(5000)
            Updater.check()
        }
        // уведомлению — что сейчас в любимых и включена ли перемешка; от него — нажатия его кнопок
        viewModelScope.launch {
            snapshotFlow { playlists.firstOrNull { it.name == LIKED }?.tracks?.map { it.key }?.toSet() ?: emptySet() }.collect { App.liked.value = it }
        }
        viewModelScope.launch {
            snapshotFlow { shuffle }.collect {
                App.shuffleOn.value = it
                prefs.edit { putBoolean("shuffle", it) }
            }
        }
        viewModelScope.launch {
            App.commands.collect { c ->
                when (c) {
                    "like" -> current?.let { toggleLike(it) }
                    "shuffle" -> toggleShuffle()
                }
            }
        }
        refreshAccount()
        viewModelScope.launch {
            refreshAccount().join()
            sync(false)
            syncStats()
            // раз в минуту — свежие плейлисты и статистика с других устройств; ждём подтверждения почты — проверяем его
            while (true) {
                delay(60_000)
                sync(false)
                syncStats()
            }
        }
        // позиция для полосы и текста: часто, пока играет
        viewModelScope.launch {
            while (true) {
                delay(if (playing && nowOpen) 100 else 500)
                // звук на другом устройстве — позиция его, свой плеер стоит и только сбивал бы её назад
                val r = remote
                if (r != null) position = (posNow(r) * 1000).toLong()
                else controller?.let { position = it.currentPosition; if (duration <= 0) duration = it.duration.coerceAtLeast(0) }
            }
        }
    }

    override fun onCleared() {
        controller?.release()
        super.onCleared()
    }
}
