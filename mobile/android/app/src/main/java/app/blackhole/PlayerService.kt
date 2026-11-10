package app.blackhole

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.Timeline
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.audio.DefaultAudioSink
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.DefaultMediaNotificationProvider
import androidx.media3.session.MediaSession
import androidx.media3.session.CommandButton
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionResult
import kotlinx.coroutines.flow.combine
import androidx.media3.session.MediaSessionService
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString

/** Трек → элемент очереди. Сам трек едет в extras: ссылку на звук сервис достанет, когда до него дойдёт */
fun Track.item(): MediaItem = MediaItem.Builder()
    .setMediaId(key)
    .setMediaMetadata(
        MediaMetadata.Builder()
            .setTitle(title)
            .setArtist(artist)
            .setArtworkUri(cover.takeIf { it.startsWith("http") }?.let(Uri::parse))
            .setExtras(Bundle().apply { putString("t", json.encodeToString(this@item)) })
            .build(),
    )
    .build()

fun MediaItem.track(): Track? = mediaMetadata.extras?.getString("t")?.let { runCatching { json.decodeFromString<Track>(it) }.getOrNull() }

private const val PREFETCH_AHEAD = 2
private const val PREFETCH_BEHIND = 2

@UnstableApi
class PlayerService : MediaSessionService() {
    companion object {
        /** живой сервис — для кнопок виджета (тот же процесс) */
        @Volatile
        var live: PlayerService? = null
    }

    private var session: MediaSession? = null
    private lateinit var player: ExoPlayer
    private val scope = MainScope()
    private var retries = 0
    private var listened = 0.0 // секунд текущего трека
    private var unsent = 0.0
    private var counted = false
    private var autoFor = ""
    private var curKey: String? = null
    private val prefs by lazy { getSharedPreferences("bh", Context.MODE_PRIVATE) }

    /** Плеер со своим эквалайзером в звуковом тракте. Скачанное (file://) читается с диска, остальное — кеш и сеть.
     *  focus: главный плеер сам ставит паузу при звонке и вынутых наушниках; второму (хвост кроссфейда) это не нужно */
    private fun newPlayer(focus: Boolean): ExoPlayer {
        val renderers = object : DefaultRenderersFactory(this) {
            override fun buildAudioSink(context: Context, enableFloatOutput: Boolean, enableAudioTrackPlaybackParams: Boolean): AudioSink =
                DefaultAudioSink.Builder(context).setAudioProcessors(arrayOf(EqProcessor())).build()
        }
        return ExoPlayer.Builder(this, renderers)
            .setMediaSourceFactory(DefaultMediaSourceFactory(DefaultDataSource.Factory(this, App.cacheSource())))
            .setAudioAttributes(AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(), focus)
            .setHandleAudioBecomingNoisy(focus)
            .setWakeMode(C.WAKE_MODE_NETWORK)
            .build()
    }

    override fun onCreate() {
        super.onCreate()
        player = newPlayer(true)
        player.addListener(listener)
        live = this
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        session = MediaSession.Builder(this, player).setCallback(callback).setSessionActivity(open).build()
        // свой значок в уведомлении, иначе там серая нота Media3 и плеер выглядит системным
        setMediaNotificationProvider(DefaultMediaNotificationProvider.Builder(this).build().apply { setSmallIcon(R.drawable.ic_notif) })
        scope.launch { tick() }
        scope.launch { xfadeLoop() }
        // сердечко и значок перемешки следуют за состоянием
        scope.launch { combine(App.liked, App.shuffleOn) { _, _ -> }.collect { refreshButtons() } }
    }

    override fun onGetSession(info: MediaSession.ControllerInfo) = session

    override fun onTaskRemoved(rootIntent: Intent?) {
        // смахнули приложение из недавних — музыка останавливается вместе с ним
        flushListen()
        pauseAllPlayersAndStopSelf()
    }

    override fun onDestroy() {
        // остальные устройства сразу видят, что звука тут больше нет
        Thread { runCatching { coreNow("connect_bye") } }.start()
        flushListen()
        live = null
        scope.cancel()
        ghost?.release()
        ghost = null
        Widget.update(this, null, false)
        session?.run {
            player.release()
            release()
        }
        session = null
        super.onDestroy()
    }

    // кнопки уведомления как у Spotify: перемешка слева, ♥ справа. В системную панель (Android 13+) Media3
    // передаёт только кнопки со слотом назад/вперёд или OVERFLOW — «вторые» слоты там молча выкидываются,
    // поэтому OVERFLOW обязателен, иначе кнопок на локскрине и в шторке нет
    private val likeCmd = SessionCommand("bh.like", Bundle.EMPTY)
    private val shuffleCmd = SessionCommand("bh.shuffle", Bundle.EMPTY)

    private fun buttons(): List<CommandButton> {
        val liked = player.currentMediaItem?.mediaId in App.liked.value
        return listOf(
            CommandButton.Builder(if (App.shuffleOn.value) CommandButton.ICON_SHUFFLE_ON else CommandButton.ICON_SHUFFLE_OFF)
                .setDisplayName(tr("Перемешать", "Shuffle", "Перемішати", "Zamiešať"))
                .setSessionCommand(shuffleCmd)
                .setSlots(CommandButton.SLOT_BACK_SECONDARY, CommandButton.SLOT_OVERFLOW)
                .build(),
            CommandButton.Builder(if (liked) CommandButton.ICON_HEART_FILLED else CommandButton.ICON_HEART_UNFILLED)
                .setDisplayName(if (liked) tr("Убрать из любимых", "Remove from Liked", "Прибрати з улюблених", "Odobrať z obľúbených") else tr("В любимые", "Add to Liked", "В улюблені", "Do obľúbených"))
                .setSessionCommand(likeCmd)
                .setSlots(CommandButton.SLOT_FORWARD_SECONDARY, CommandButton.SLOT_OVERFLOW)
                .build(),
        )
    }

    private fun refreshButtons() {
        session?.setMediaButtonPreferences(buttons())
    }

    private val callback = object : MediaSession.Callback {
        override fun onConnect(s: MediaSession, c: MediaSession.ControllerInfo): MediaSession.ConnectionResult {
            val cmds = MediaSession.ConnectionResult.DEFAULT_SESSION_COMMANDS.buildUpon().add(likeCmd).add(shuffleCmd).build()
            return MediaSession.ConnectionResult.AcceptedResultBuilder(s)
                .setAvailableSessionCommands(cmds)
                .setMediaButtonPreferences(buttons())
                .build()
        }

        // сами плейлисты и перемешка в AppState — передаём ему нажатие
        override fun onCustomCommand(s: MediaSession, c: MediaSession.ControllerInfo, cmd: SessionCommand, args: Bundle): ListenableFuture<SessionResult> {
            when (cmd.customAction) {
                likeCmd.customAction -> App.commands.tryEmit("like")
                shuffleCmd.customAction -> App.commands.tryEmit("shuffle")
            }
            return Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
        }

        // от контроллера элементы приходят без ссылки — ставим bh://t/<ключ>, Streams.resolve подменит на звук
        override fun onAddMediaItems(s: MediaSession, c: MediaSession.ControllerInfo, items: MutableList<MediaItem>): ListenableFuture<MutableList<MediaItem>> =
            Futures.immediateFuture(items.map { it.withUri() }.toMutableList())
    }

    private fun MediaItem.withUri(): MediaItem {
        track()?.let { Streams.tracks[it.key] = it }
        // скачанный трек играет прямо с диска — сеть и ссылка не нужны
        Offline.uri(mediaId)?.let { f ->
            return buildUpon().setUri(f).setMimeType(if (Offline.isHls(mediaId)) MimeTypes.APPLICATION_M3U8 else null).setCustomCacheKey(null).build()
        }
        return buildUpon().setUri(Uri.parse("bh://t/" + Uri.encode(mediaId))).setMimeType(null).setCustomCacheKey(mediaId).build()
    }

    private fun indexOf(key: String) = (0 until player.mediaItemCount).firstOrNull { player.getMediaItemAt(it).mediaId == key }

    private val listener = object : Player.Listener {
        override fun onMediaItemTransition(item: MediaItem?, reason: Int) {
            prefetch()
            refreshButtons()
            widget()
            // таймер сна «после трека»: трек доиграл сам — на этом всё
            if (App.sleepAt.value == -1L && reason == Player.MEDIA_ITEM_TRANSITION_REASON_AUTO) {
                App.sleepAt.value = 0
                player.pause()
            }
            // трек сменили руками посреди кроссфейда — хвост прошлого больше не нужен
            if (!ourSkip) endXfade()
            ourSkip = false
            // тот же трек: только подменили источник (HLS, повтор) — счётчики не трогаем
            if (item?.mediaId == curKey) return
            curKey = item?.mediaId
            flushListen()
            retries = 0
            listened = 0.0
            counted = false
            maybeAutoplay()
        }

        // очередь поменялась (добавили, перемешали, докинулось похожее) — соседи могли смениться
        override fun onTimelineChanged(timeline: Timeline, reason: Int) = prefetch()
        override fun onShuffleModeEnabledChanged(on: Boolean) = prefetch()
        override fun onRepeatModeChanged(mode: Int) = prefetch()

        // заиграло — прошлые сбои этого трека не считаем: иначе пара обрывов сети за трек его скипала
        override fun onPlaybackStateChanged(state: Int) {
            if (state == Player.STATE_READY) retries = 0
        }

        override fun onIsPlayingChanged(isPlaying: Boolean) = widget()

        override fun onPlayerError(error: PlaybackException) {
            // ошибка может быть и про следующий трек, поэтому индекс ищем по ключу из неё
            generateSequence<Throwable>(error) { it.cause }.filterIsInstance<HlsNeeded>().firstOrNull()?.let { h ->
                val i = indexOf(h.key) ?: return
                player.replaceMediaItem(i, player.getMediaItemAt(i).buildUpon().setUri(h.url).setMimeType(MimeTypes.APPLICATION_M3U8).setCustomCacheKey(null).build())
                player.prepare()
                player.play()
                return
            }
            val key = player.currentMediaItem?.mediaId ?: return
            BhLog.line("не играет ${player.currentMediaItem?.mediaMetadata?.title}: ${why(error)} · ${generateSequence<Throwable>(error) { it.cause }.joinToString(" ← ") { it.toString().take(160) }}")
            Streams.forget(key) // протухла ссылка или YouTube не отдал — следующая попытка достанет заново
            val title = player.currentMediaItem?.mediaMetadata?.title ?: ""
            // бан YouTube: повторы и перебор следующих треков его только продлевают — стоим и ждём,
            // ⏯ попробует снова
            if (Streams.isBotBlock(error)) {
                retries = 0
                player.pause()
                val why = Streams.authError
                App.toasts.tryEmit(if (YtAuth.has() && why != null) tr(
                    "YouTube не дал звук даже под аккаунтом: $why",
                    "YouTube gave no audio even via your account: $why",
                    "YouTube не дав звук навіть під акаунтом: $why",
                    "YouTube nedal zvuk ani cez tvoj účet: $why",
                ) else tr(
                    "YouTube временно не пускает с этого IP. Войди в YouTube (Аккаунт → Настройки → YouTube) или переключи Wi-Fi ↔ мобильный интернет",
                    "YouTube is temporarily blocking this IP. Sign in to YouTube (Account → Settings → YouTube) or switch Wi-Fi ↔ mobile data",
                    "YouTube тимчасово не пускає з цієї IP. Увійди в YouTube (Акаунт → Налаштування → YouTube) або перемкни Wi-Fi ↔ мобільний інтернет", "YouTube dočasne blokuje túto IP. Prihlás sa do YouTube (Účet → Nastavenia → YouTube) alebo prepni Wi-Fi ↔ mobilné dáta",
                ))
                return
            }
            // две попытки с паузой: часто это разовый сбой сети или выдачи
            if (retries++ < 2) {
                scope.launch {
                    delay(600L * retries)
                    if (player.currentMediaItem?.mediaId != key) return@launch
                    // протухшая ссылка HLS: возвращаем bh://, чтобы достать свежую
                    player.currentMediaItem?.let { if (it.localConfiguration?.uri?.scheme != "bh") player.replaceMediaItem(player.currentMediaItemIndex, it.withUri()) }
                    player.prepare()
                    player.play()
                }
                return
            }
            App.toasts.tryEmit(tr("Не играет: $title (${why(error)})", "Can't play: $title (${why(error)})", "Не грає: $title (${why(error)})", "Nehrá: $title (${why(error)})"))
            if (player.hasNextMediaItem()) {
                player.seekToNextMediaItem()
                player.prepare()
                player.play()
            }
        }
    }

    /** Короткая причина для тоста — чтобы было что рассказать, если опять не играет */
    private fun why(e: PlaybackException): String {
        var c: Throwable? = e
        while (c != null) {
            if (c is androidx.media3.datasource.HttpDataSource.InvalidResponseCodeException) return "HTTP ${c.responseCode}"
            if (c is java.net.UnknownHostException) return tr("нет сети", "no network", "немає мережі", "bez siete")
            c = c.cause
        }
        val root = generateSequence<Throwable>(e) { it.cause }.last()
        return root.message?.take(60)?.ifBlank { null } ?: e.errorCodeName
    }

    private var prefetchJob: Job? = null

    /** Как на ПК: пока играет текущий, заранее достаём прямые ссылки на 2 следующих и 2 прошлых (больше — YouTube банит IP) — скипать
     *  можно в обе стороны без ожидания. По одному в фоне, ближайший следующий первым; порядок — с учётом
     *  перемешки и повтора */
    private fun prefetch() {
        if (Streams.blocked()) return
        val tl = player.currentTimeline
        if (tl.isEmpty) return
        val cur = player.currentMediaItemIndex
        val order = ArrayList<Int>()
        fun walk(n: Int, step: (Int) -> Int) {
            var i = cur
            repeat(n) {
                i = step(i)
                if (i == C.INDEX_UNSET || i == cur) return
                order += i
            }
        }
        val rep = if (player.repeatMode == Player.REPEAT_MODE_ONE) Player.REPEAT_MODE_OFF else player.repeatMode
        walk(PREFETCH_AHEAD) { tl.getNextWindowIndex(it, rep, player.shuffleModeEnabled) }
        walk(PREFETCH_BEHIND) { tl.getPreviousWindowIndex(it, rep, player.shuffleModeEnabled) }
        val keys = order.distinct().map { player.getMediaItemAt(it).mediaId }.filter { !Streams.ready(it) && Streams.tracks.containsKey(it) }
        prefetchJob?.cancel()
        if (keys.isEmpty()) return
        prefetchJob = scope.launch(Dispatchers.IO) {
            for (k in keys) {
                if (!isActive || Streams.blocked()) break
                runCatching { Streams.urlFor(k) }
            }
        }
    }

    // ── кроссфейд: хвост уходящего трека доигрывает второй плеер и затихает, а главный уже начал следующий ──
    private var ghost: ExoPlayer? = null
    private var ghostFor = ""
    private var fade: Job? = null
    private var ourSkip = false
    private var xfK = 1f // громкость главного плеера от кроссфейда
    private var sleepK = 1f // и от затухания таймера сна

    private fun applyVolume() {
        player.volume = xfK * sleepK
    }

    private fun endXfade() {
        fade?.cancel()
        fade = null
        ghost?.release()
        ghost = null
        ghostFor = ""
        xfK = 1f
        applyVolume()
    }

    private suspend fun xfadeLoop() {
        while (scope.isActive) {
            delay(50)
            runCatching { xfadeStep() }.onFailure { BhLog.line("кроссфейд: $it"); endXfade() }
        }
    }

    private fun xfadeStep() {
        val x = prefs.getInt("xfade", 0) * 1000L
        // «сон после трека» — трек должен доиграть до конца сам, без перехода
        if (x <= 0 || fade?.isActive == true || App.sleepAt.value == -1L) return
        val dur = player.duration
        val cur = player.currentMediaItem
        // короткие треки, повтор одного и конец очереди идут без перехода
        if (!player.isPlaying || cur == null || dur == C.TIME_UNSET || dur < x * 2 + 5000 || player.repeatMode == Player.REPEAT_MODE_ONE || !player.hasNextMediaItem()) {
            if (ghost != null && fade == null && !player.isPlaying) return
            if (ghost != null && fade == null) endXfade()
            return
        }
        val left = dur - player.currentPosition
        if (left > x + 4000) {
            if (ghost != null) endXfade()
            return
        }
        // следующий трек должен стартовать сразу: скачан, в кеше или со свежей ссылкой — иначе обычный переход
        val next = player.getMediaItemAt(player.nextMediaItemIndex).mediaId
        if (Offline.uri(next) == null && !Streams.ready(next)) return
        if (ghostFor != cur.mediaId) {
            ghost?.release()
            // второй плеер заранее встаёт на точку начала перехода и ждёт
            ghost = newPlayer(false).apply {
                setMediaItem(cur, dur - x)
                volume = sleepK
                playWhenReady = false
                prepare()
            }
            ghostFor = cur.mediaId
        }
        val g = ghost ?: return
        if (left > x || g.playbackState != Player.STATE_READY) return
        // второй плеер не успел подготовиться к началу перехода — этот трек сменится как обычно
        if (left < x - 400) return
        g.play()
        xfK = 0f
        applyVolume()
        ourSkip = true
        player.seekToNextMediaItem()
        fade = scope.launch {
            var done = 0L
            while (done < x) {
                delay(40)
                // пауза посреди перехода — оба плеера стоят, время перехода тоже
                if (!player.playWhenReady) {
                    g.pause()
                    continue
                }
                if (!g.isPlaying && g.playbackState == Player.STATE_READY) g.play()
                done += 40
                val k = (done.toFloat() / x).coerceIn(0f, 1f)
                // равная мощность: суммарная громкость не проваливается в середине
                xfK = kotlin.math.sin(k * Math.PI / 2).toFloat()
                g.volume = kotlin.math.cos(k * Math.PI / 2).toFloat() * sleepK
                applyVolume()
                if (g.playbackState == Player.STATE_ENDED) break
            }
            fade = null
            endXfade()
        }
    }

    // ── виджет на рабочем столе ──
    private var widgetArt: Pair<String, android.graphics.Bitmap?>? = null

    private fun widget() {
        val t = player.currentMediaItem?.track()
        val playing = player.isPlaying
        Widget.update(this, t, playing, widgetArt?.takeIf { it.first == t?.cover }?.second)
        val cover = t?.cover?.takeIf { it.startsWith("http") } ?: return
        if (widgetArt?.first == cover) return
        widgetArt = cover to null
        scope.launch {
            val bmp = withContext(Dispatchers.IO) { runCatching { Widget.art(this@PlayerService, cover) }.getOrNull() }
            if (widgetArt?.first == cover) {
                widgetArt = cover to bmp
                Widget.update(this@PlayerService, player.currentMediaItem?.track(), player.isPlaying, bmp)
            }
        }
    }

    /** Кнопки виджета: сервис жив — управляем плеером напрямую */
    fun widgetCommand(cmd: String) {
        when (cmd) {
            "toggle" -> if (player.isPlaying) player.pause() else { player.prepare(); player.play() }
            "next" -> player.seekToNextMediaItem()
            "prev" -> player.seekToPrevious()
        }
    }

    /** Таймер сна: последние 10 с звук плавно уходит, потом пауза */
    private fun sleepStep() {
        val at = App.sleepAt.value
        if (at <= 0) {
            if (sleepK != 1f) {
                sleepK = 1f
                applyVolume()
            }
            return
        }
        val left = at - System.currentTimeMillis()
        if (left <= 0) {
            App.sleepAt.value = 0
            player.pause()
            sleepK = 1f
            endXfade()
            return
        }
        sleepK = (left / 10_000f).coerceIn(0f, 1f)
        applyVolume()
    }

    /** Раз в секунду: сколько дослушано — в статистику (засчитываем с 30 с или с половины короткого трека) */
    private suspend fun tick() {
        while (scope.isActive) {
            delay(1000)
            sleepStep()
            if (!player.isPlaying) continue
            listened += 1
            unsent += 1
            val dur = player.duration.takeIf { it > 0 }?.div(1000.0) ?: 0.0
            val need = if (dur > 0) minOf(dur * 0.5, 30.0) else 30.0
            if (!counted && listened >= need) {
                counted = true
                flushListen(count = true)
            } else if (unsent >= 15) {
                flushListen()
            }
        }
    }

    private fun flushListen(count: Boolean = false) {
        val t = player.currentMediaItem?.track() ?: return
        val secs = unsent
        unsent = 0.0
        if (secs <= 0 && !count) return
        scope.launch {
            runCatching {
                core("listen") {
                    put("track", t)
                    put("secs", kotlinx.serialization.json.JsonPrimitive(secs))
                    put("counted", kotlinx.serialization.json.JsonPrimitive(count))
                }
            }
        }
    }

    /** Очередь кончается — докидываем похожие по играющему (как «дальше похожее» на ПК) */
    private fun maybeAutoplay() {
        if (!prefs.getBoolean("autoplay", true)) return
        if (player.repeatMode != Player.REPEAT_MODE_OFF || player.mediaItemCount - player.currentMediaItemIndex > 2) return
        val seed = player.currentMediaItem?.track() ?: return
        if (autoFor == seed.key) return
        autoFor = seed.key
        scope.launch {
            val tracks = runCatching { withContext(Dispatchers.IO) { core("radio") { put("track", seed) }.to<List<Track>>() } }.getOrNull() ?: return@launch
            val have = (0 until player.mediaItemCount).map { player.getMediaItemAt(it).mediaId }.toSet()
            val add = tracks.filter { it.key !in have && !(it.title == seed.title && it.artist == seed.artist) }.take(20)
            player.addMediaItems(add.map { it.item().withUri() })
        }
    }
}
