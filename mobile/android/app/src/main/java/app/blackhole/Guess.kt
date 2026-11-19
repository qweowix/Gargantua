package app.blackhole

import android.net.Uri
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.viewModelScope
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonPrimitive

/** Длительность фрагмента на каждом уровне — как guess::LEVELS */
val GUESS_LEVELS = listOf(100L, 500L, 1000L, 2000L, 5000L)

@Serializable
data class DuelMeta(val host: String = "", val host_name: String = "", val guest: String = "", val guest_name: String = "", val source: String = "")

@Serializable
data class DuelRound(val n: Int = 0, val start: Double = 0.0, val at: Long = 0)

@Serializable
data class DuelState(
    val code: String = "", val meta: DuelMeta = DuelMeta(), val pool: List<Track> = emptyList(), val round: DuelRound? = null,
    val want: Map<String, Int> = emptyMap(), val score: Map<String, Int> = emptyMap(), val gone: Boolean = false,
) {
    fun opp(me: String) = if (meta.host == me) meta.guest else meta.host
    fun name(uid: String) = if (uid == meta.host) meta.host_name else meta.guest_name
}

@Serializable
data class DuelWinner(val uid: String = "", val level: Int = 0)

@Serializable
data class DuelSnap(val ver: Long = 0, val me: String = "", val duel: DuelState? = null, val level: Int? = null, val given_up: Boolean = false, val winner: DuelWinner? = null)

/** «Угадай песню»: соло и 1 на 1. Фрагменты играет свой плеер — не основной: ни статистики, ни уведомления с названием */
@UnstableApi
class GuessState(private val vm: AppState) {
    var stage by mutableStateOf(0) // 0 выбор, 1 раунд, 2 ответ, 3 дуэль: ждём соперника
    var kind by mutableStateOf(0) // 0 соло, 1 на 1
    var mode by mutableStateOf(0) // 0 плейлист, 1 артист
    var artists by mutableStateOf<List<Card>>(emptyList())
    var busy by mutableStateOf(false)
    var error by mutableStateOf("")
    var source by mutableStateOf("")
    var level by mutableStateOf(0)
    var ready by mutableStateOf(false)
    var results by mutableStateOf<List<Track>>(emptyList())
    var searching by mutableStateOf(false)
    var wrong by mutableStateOf("")
    var won by mutableStateOf(false)
    var gained by mutableStateOf(0)
    var cur by mutableStateOf<Track?>(null)
    var score by mutableStateOf(0)
    var streak by mutableStateOf(0)
    var best by mutableStateOf(0)
    var rounds by mutableStateOf(0)
    var hits by mutableStateOf(0)
    // 1 на 1
    var duelCode by mutableStateOf("")
    var host by mutableStateOf(false)
    var oppName by mutableStateOf("")
    var meScore by mutableStateOf(0)
    var oppScore by mutableStateOf(0)
    var meSkip by mutableStateOf(false)
    var oppSkip by mutableStateOf(false)
    var winner by mutableStateOf("")

    private var pool: List<Track> = emptyList()
    private var next = 0
    private var startMs = 0L
    private var player: ExoPlayer? = null
    private var searchJob: Job? = null
    private var artistJob: Job? = null
    private var duelLoop: Job? = null
    private var duelRound: Pair<Int, Long>? = null
    private var myWant = 0
    private var claimed = false
    private var revealed = false
    private var me = ""
    private var gen = 0

    private fun exo(): ExoPlayer = player ?: ExoPlayer.Builder(vm.getApplication())
        .setMediaSourceFactory(DefaultMediaSourceFactory(DefaultDataSource.Factory(vm.getApplication(), App.cacheSource())))
        .build().also { p ->
            p.addListener(object : Player.Listener {
                override fun onPlaybackStateChanged(state: Int) { if (state == Player.STATE_READY) ready = true }
            })
            player = p
        }

    /** Ушли с экрана — плеер игры не нужен */
    fun release() {
        player?.release()
        player = null
    }

    // ── выбор источника ──

    fun searchArtists(q: String) {
        artistJob?.cancel()
        if (q.trim().length < 2) { artists = emptyList(); return }
        artistJob = vm.viewModelScope.launch {
            delay(350)
            artists = runCatching { core("guess_artists") { put("q", JsonPrimitive(q)) }.to<List<Card>>() }.getOrDefault(emptyList())
        }
    }

    fun pickPlaylist(p: Playlist) = vm.viewModelScope.launch {
        error = ""
        val v = runCatching { core("guess_pool") { put("tracks", json.encodeToJsonElement(kotlinx.serialization.builtins.ListSerializer(Track.serializer()), p.tracks.filter { it.src != "lc" })) }.to<List<Track>>() }.getOrDefault(emptyList())
        if (v.size < 3) { error = tr("В плейлисте слишком мало песен", "Too few songs in this playlist", "У плейлисті замало пісень", "V playliste je primálo skladieb"); return@launch }
        begin(v, tr("Плейлист «${p.name}»", "Playlist “${p.name}”", "Плейлист «${p.name}»", "Playlist „${p.name}“"))
    }

    fun pickArtist(c: Card) = vm.viewModelScope.launch {
        busy = true; error = ""
        val r = runCatching { core("guess_pool_artist") { put("id", JsonPrimitive(c.id)); put("name", JsonPrimitive(c.title)) }.to<List<Track>>() }
        busy = false
        r.onSuccess { begin(it, c.title) }.onFailure { error = tr("Не вышло собрать песни артиста", "Couldn't get the artist's songs", "Не вийшло зібрати пісні артиста", "Nepodarilo sa získať skladby") }
    }

    private fun begin(v: List<Track>, src: String) {
        if (kind == 1) { createDuel(v, src); return }
        vm.controller?.pause()
        pool = v; next = 0; source = src
        round()
    }

    // ── раунд ──

    private fun round() {
        if (pool.isEmpty()) return
        if (next >= pool.size) { pool = pool.shuffled(); next = 0 }
        val t = pool[next++]
        vm.viewModelScope.launch {
            val start = runCatching { core("guess_start") { put("track", t) }.jsonPrimitive.content.toDouble() }.getOrDefault(30.0)
            prepare(t, start)
        }
    }

    private fun prepare(t: Track, start: Double) {
        gen++
        cur = t; level = 0; ready = false; wrong = ""; results = emptyList()
        myWant = 0; claimed = false; revealed = false; meSkip = false; oppSkip = false
        startMs = (start * 1000).toLong()
        stage = 1
        play()
        prefetch()
    }

    /** Следующие две песни — заранее: самое долгое — достать ссылку на поток, её и готовим */
    private fun prefetch() {
        val list = if (kind == 1) lastDuel?.let { d -> val n = d.round?.n ?: 0; (1..2).mapNotNull { d.pool.getOrNull((n + it) % maxOf(1, d.pool.size)) } } ?: emptyList()
        else pool.drop(next).take(2)
        for (t in list) {
            if (Offline.uri(t.key) != null) continue
            Streams.tracks[t.key] = t
            vm.viewModelScope.launch(kotlinx.coroutines.Dispatchers.IO) { runCatching { Streams.urlFor(t.key) } }
        }
    }

    /** Фрагмент текущего уровня: тот же трек, обрезанный с start на длину уровня (данные из кэша — мгновенно) */
    fun play() {
        val t = cur ?: return
        Streams.tracks[t.key] = t
        val uri = Offline.uri(t.key) ?: Uri.parse("bh://t/" + Uri.encode(t.key))
        val item = MediaItem.Builder().setUri(uri).setCustomCacheKey(t.key)
            .setClippingConfiguration(MediaItem.ClippingConfiguration.Builder().setStartPositionMs(startMs).setEndPositionMs(startMs + GUESS_LEVELS[level]).build())
            .build()
        val p = exo()
        p.setMediaItem(item)
        p.prepare()
        p.play()
    }

    fun skip() {
        if (stage != 1) return
        if (kind == 1) {
            if (myWant <= level && myWant < GUESS_LEVELS.size) {
                myWant = level + 1; meSkip = true
                val w = myWant
                vm.viewModelScope.launch { runCatching { core("duel_want") { put("times", JsonPrimitive(w)) } } }
            }
            return
        }
        if (level + 1 >= GUESS_LEVELS.size) return finish(false)
        level++
        play()
    }

    /** Поиск-ответ — только среди песен игры (плейлиста или артиста): мгновенно, без сети */
    fun search(q: String) {
        searchJob?.cancel()
        searching = false
        val n = q.trim().lowercase().replace('ё', 'е')
        if (n.isEmpty()) { results = emptyList(); return }
        val list = if (kind == 1) lastDuel?.pool ?: emptyList() else pool
        fun norm(s: String) = s.lowercase().replace('ё', 'е')
        results = list.withIndex()
            .mapNotNull { (i, t) ->
                val title = norm(t.title)
                when {
                    title.startsWith(n) -> 0 to i
                    title.contains(n) -> 1 to i
                    norm(t.artist).contains(n) -> 2 to i
                    else -> null
                }
            }
            .sortedWith(compareBy({ it.first }, { it.second }))
            .take(12).map { list[it.second] }
    }

    fun pick(g: Track) = vm.viewModelScope.launch {
        val a = cur ?: return@launch
        val ok = runCatching { core("guess_same") { put("answer", a); put("guess", g) }.jsonPrimitive.content == "true" }.getOrDefault(false)
        if (ok) finish(true) else wrongGuess(g.title)
    }

    fun typed(text: String) = vm.viewModelScope.launch {
        val a = cur ?: return@launch
        if (text.isBlank()) return@launch
        val ok = runCatching { core("guess_typed") { put("answer", a); put("text", JsonPrimitive(text)) }.jsonPrimitive.content == "true" }.getOrDefault(false)
        if (ok) finish(true) else wrongGuess(text)
    }

    private fun wrongGuess(what: String) {
        results = emptyList()
        val msg = tr("Не то: $what", "Not it: $what", "Не те: $what", "Nie je to: $what")
        if (kind == 1) { wrong = msg; return }
        skip()
        if (stage == 1) wrong = msg
    }

    private fun finish(ok: Boolean) {
        if (kind == 1) {
            if (ok && !claimed && !revealed) {
                claimed = true
                wrong = tr("Верно! Сверяю, кто был первым…", "Correct! Checking who was first…", "Правильно! Звіряю, хто був першим…", "Správne! Overujem, kto bol prvý…")
                val l = level
                vm.viewModelScope.launch { runCatching { core("duel_claim") { put("level", JsonPrimitive(l)) } } }
            }
            return
        }
        rounds++
        gained = if (ok) listOf(100, 60, 40, 25, 10).getOrElse(level) { 0 } else 0
        if (ok) { hits++; streak++; score += gained; best = maxOf(best, streak) } else streak = 0
        reveal(ok, "")
    }

    /** Раунд раскрыт: песня играет целиком с начала, пока не нажмёшь «Следующая» */
    private fun reveal(ok: Boolean, who: String) {
        won = ok; winner = who
        stage = 2
        level = GUESS_LEVELS.size - 1
        val t = cur ?: return
        Streams.tracks[t.key] = t
        val uri = Offline.uri(t.key) ?: Uri.parse("bh://t/" + Uri.encode(t.key))
        val p = exo()
        p.setMediaItem(MediaItem.Builder().setUri(uri).setCustomCacheKey(t.key).build())
        p.prepare()
        p.play()
    }

    fun nextRound() {
        if (kind == 0) return round()
        val d = lastDuel ?: return
        if (d.meta.host != me || d.pool.isEmpty()) return
        val n = ((d.round?.n ?: -1) + 1) % d.pool.size
        vm.viewModelScope.launch {
            val start = runCatching { core("guess_start") { put("track", d.pool[n]) }.jsonPrimitive.content.toDouble() }.getOrDefault(30.0)
            runCatching { core("duel_round") { put("n", JsonPrimitive(n)); put("start", JsonPrimitive(start)) } }
        }
    }

    fun change() {
        if (kind == 1 && duelCode.isNotEmpty()) leaveDuel()
        player?.stop()
        gen++
        stage = 0
    }

    // ── 1 на 1 ──

    private var lastDuel: DuelState? = null

    private fun createDuel(v: List<Track>, src: String) = vm.viewModelScope.launch {
        busy = true; error = ""
        val r = runCatching { core("duel_create") { put("source", JsonPrimitive(src)); put("pool", json.encodeToJsonElement(kotlinx.serialization.builtins.ListSerializer(Track.serializer()), v)) }.jsonPrimitive.content }
        busy = false
        r.onSuccess { enterDuel(it, true) }.onFailure { error = EcoText.err(it) }
    }

    fun joinDuel(code: String) = vm.viewModelScope.launch {
        if (code.isBlank()) return@launch
        busy = true; error = ""
        val r = runCatching { core("duel_join") { put("code", JsonPrimitive(code.trim().uppercase())) } }
        busy = false
        r.onSuccess { enterDuel(code.trim().uppercase(), false) }.onFailure {
            val m = it.message ?: ""
            error = when {
                "NO_DUEL" in m -> tr("Дуэли с таким кодом нет", "No duel with this code", "Дуелі з таким кодом немає", "Duel s týmto kódom neexistuje")
                "DUEL_FULL" in m -> tr("В этой дуэли уже двое", "This duel already has two players", "У цій дуелі вже двоє", "V tomto dueli sú už dvaja")
                else -> EcoText.err(it)
            }
        }
    }

    private fun enterDuel(code: String, isHost: Boolean) {
        vm.controller?.pause()
        duelCode = code; host = isHost; duelRound = null; stage = 3; rounds = 0; hits = 0
        duelLoop?.cancel()
        duelLoop = vm.viewModelScope.launch {
            var ver = 0L
            while (isActive) {
                val s = runCatching { core("duel_wait") { put("since", JsonPrimitive(ver)); put("ms", JsonPrimitive(20_000)) }.to<DuelSnap>() }.getOrNull()
                if (s == null) { delay(2000); continue }
                ver = s.ver
                me = s.me
                onDuel(s)
            }
        }
    }

    private fun leaveDuel() {
        duelLoop?.cancel()
        duelCode = ""
        vm.viewModelScope.launch { runCatching { core("duel_leave") } }
    }

    private fun onDuel(s: DuelSnap) {
        val d = s.duel ?: return
        if (d.meta.host.isEmpty() && lastDuel != null) {
            vm.toast(tr("Дуэль закрыта", "The duel was closed", "Дуель закрито", "Duel bol zatvorený"))
            leaveDuel(); stage = 0; return
        }
        lastDuel = d
        val opp = d.opp(me)
        source = d.meta.source
        host = d.meta.host == me
        oppName = if (opp.isEmpty()) "" else d.name(opp)
        meScore = d.score[me] ?: 0
        oppScore = d.score[opp] ?: 0
        if (d.meta.guest.isEmpty()) { stage = 3; return }
        val r = d.round
        if (r == null) {
            if (host && d.pool.isNotEmpty()) vm.viewModelScope.launch {
                val start = runCatching { core("guess_start") { put("track", d.pool[0]) }.jsonPrimitive.content.toDouble() }.getOrDefault(30.0)
                runCatching { core("duel_round") { put("n", JsonPrimitive(0)); put("start", JsonPrimitive(start)) } }
            }
            return
        }
        if (duelRound != r.n to r.at) {
            val t = d.pool.getOrNull(r.n) ?: return
            duelRound = r.n to r.at
            prepare(t, r.start)
        }
        if (revealed) return
        s.winner?.let { w ->
            revealed = true
            rounds++
            val mine = w.uid == me
            if (mine) {
                hits++
                val total = (d.score[me] ?: 0) + listOf(100, 60, 40, 25, 10).getOrElse(w.level) { 0 }
                vm.viewModelScope.launch { runCatching { core("duel_score") { put("total", JsonPrimitive(total)) } } }
            }
            reveal(mine, d.name(w.uid))
            return
        }
        if (s.given_up) { revealed = true; rounds++; reveal(false, ""); return }
        val lvl = (s.level ?: 0).coerceAtMost(GUESS_LEVELS.size - 1)
        oppSkip = (d.want[opp] ?: 0) > lvl
        meSkip = myWant > lvl
        if (lvl > level) { level = lvl; wrong = ""; play() }
    }
}
