package app.blackhole.ui

import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material.icons.rounded.EditNote
import androidx.compose.material.icons.rounded.Bedtime
import android.media.AudioManager
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.offset
import androidx.compose.material.icons.automirrored.rounded.FormatListBulleted
import androidx.compose.material.icons.automirrored.rounded.VolumeMute
import androidx.compose.material.icons.automirrored.rounded.VolumeUp
import androidx.compose.material.icons.rounded.FastForward
import androidx.compose.material.icons.rounded.Devices
import androidx.compose.material.icons.rounded.FastRewind
import androidx.compose.material.icons.rounded.FormatQuote
import androidx.compose.material.icons.rounded.MoreHoriz
import androidx.compose.material.icons.rounded.Favorite
import androidx.compose.material.icons.rounded.FavoriteBorder
import androidx.compose.material.icons.automirrored.rounded.PlaylistAdd
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.launch
import kotlin.math.roundToInt
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Translate
import androidx.compose.material.icons.rounded.DragHandle
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.zIndex
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.graphics.graphicsLayer
import androidx.media3.common.MediaItem
import android.os.SystemClock
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import kotlinx.coroutines.delay
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.material.icons.automirrored.rounded.QueueMusic
import androidx.compose.material.icons.rounded.AddCircleOutline
import androidx.compose.material.icons.rounded.MoreVert
import androidx.compose.material.icons.rounded.Person
import androidx.compose.material.icons.rounded.Link
import androidx.compose.material.icons.rounded.Radio
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import app.blackhole.Screen
import app.blackhole.Track
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material.icons.rounded.Pause
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Repeat
import androidx.compose.material.icons.rounded.RepeatOne
import androidx.compose.material.icons.rounded.Shuffle
import androidx.compose.material.icons.rounded.SkipNext
import androidx.compose.material.icons.rounded.SkipPrevious
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import app.blackhole.AppState
import app.blackhole.LyrLine
import app.blackhole.fmtDur
import app.blackhole.tr

/** Мини-плеер — плавающая «таблетка» над нижним меню: обложка, название, ⏯ и ⏩ */
@UnstableApi
@Composable
fun MiniPlayer(vm: AppState) {
    val t = vm.current ?: return
    // свайп вбок: обложка с названием едут за пальцем, дальше порога — улетают и трек переключается,
    // новый въезжает с другой стороны
    val scope = rememberCoroutineScope()
    val shift = remember { Animatable(0f) }
    val gap = with(LocalDensity.current) { 70.dp.toPx() }
    var w by remember { mutableFloatStateOf(1f) }
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 10.dp)
            .shadow(16.dp, RoundedCornerShape(26.dp))
            .clip(RoundedCornerShape(26.dp))
            .background(Glass)
            .border(0.5.dp, Color(0x22FFFFFF), RoundedCornerShape(26.dp))
            .onSizeChanged { w = it.width.toFloat().coerceAtLeast(1f) }
            .pointerInput(Unit) {
                detectHorizontalDragGestures(
                    onDragEnd = {
                        scope.launch {
                            val d = shift.value
                            if (kotlin.math.abs(d) < gap) { shift.animateTo(0f, spring()); return@launch }
                            val dir = if (d < 0) -1f else 1f
                            shift.animateTo(dir * w, tween(140))
                            if (dir < 0) vm.next() else vm.prevItem()
                            shift.snapTo(-dir * w * 0.5f)
                            shift.animateTo(0f, spring(dampingRatio = 0.8f, stiffness = 400f))
                        }
                    },
                    onDragCancel = { scope.launch { shift.animateTo(0f, spring()) } },
                ) { ch, dx -> ch.consume(); scope.launch { shift.snapTo(shift.value + dx) } }
            }
            .clickable { vm.nowOpen = true }
            .padding(start = 8.dp, end = 4.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            Modifier.weight(1f).graphicsLayer {
                translationX = shift.value
                alpha = 1f - (kotlin.math.abs(shift.value) / w).coerceIn(0f, 1f)
            },
            verticalAlignment = Alignment.CenterVertically,
        ) {
        Cover(t.cover, 40.dp, 8.dp)
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(t.title, color = Pal.text, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (vm.remote != null) Text(tr("Играет на «${vm.remoteName}»", "Playing on “${vm.remoteName}”", "Грає на «${vm.remoteName}»", "Hrá na „${vm.remoteName}“"), color = LocalAccent.current.c1, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            else Text(t.artist, color = Pal.dim, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        }
        PlayButton(vm, 44)
        IconButton(onClick = { vm.next() }) { Icon(Icons.Rounded.FastForward, null, tint = Pal.text, modifier = Modifier.size(30.dp)) }
    }
}

/** ⏯ без подложки, как у Apple; грузится — крутилка */
@UnstableApi
@Composable
fun PlayButton(vm: AppState, size: Int) {
    Box(
        Modifier.size(size.dp).clip(CircleShape).clickable { vm.toggle() },
        contentAlignment = Alignment.Center,
    ) {
        if (vm.buffering && !vm.playing) {
            CircularProgressIndicator(Modifier.size((size / 2).dp), color = Color.White, strokeWidth = 2.dp)
        } else {
            Icon(if (vm.playing) Icons.Rounded.Pause else Icons.Rounded.PlayArrow, null, tint = Color.White, modifier = Modifier.size((size * 0.8).dp))
        }
    }
}

/** Экран «сейчас играет» в духе Apple Music. Три режима: плеер (большая обложка), текст, очередь —
 *  переключаются кнопками снизу, повторное нажатие возвращает к плееру. Сверху ручка: тянешь вниз — закрывается */
@UnstableApi
// затухание обложки в фон по плавной S-кривой: у линейного на концах излом, и на светлых обложках виден шов
private val coverFade = Array(17) { i ->
    val x = i / 16f
    val k = x * x * x * (x * (x * 6 - 15) + 10) // smootherstep
    (0.4f + 0.6f * x) to Color.Black.copy(alpha = 1 - k)
}

@Composable
fun NowPlaying(vm: AppState) {
    val t = vm.current ?: return
    var mode by remember { mutableIntStateOf(0) } // 0 плеер, 1 текст, 2 очередь
    val clock = rememberLyrClock(vm)
    // в тексте панель управления прячется после 2 с без касаний, любое касание её возвращает
    var touchAt by remember { mutableLongStateOf(0L) }
    var panel by remember { mutableStateOf(true) }
    LaunchedEffect(mode, touchAt) {
        panel = true
        if (mode == 1) {
            delay(2000)
            panel = false
        }
    }
    // смахивание вниз: экран едет за пальцем, дальше 120 dp — закрываем, иначе пружиной назад
    val drag = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val closeAt = with(density) { 120.dp.toPx() }
    val swipe = Modifier.pointerInput(Unit) {
        detectVerticalDragGestures(
            onDragEnd = {
                scope.launch {
                    if (drag.value > closeAt) {
                        vm.nowOpen = false
                        delay(400)
                        drag.snapTo(0f)
                    } else {
                        drag.animateTo(0f, spring())
                    }
                }
            },
            onDragCancel = { scope.launch { drag.animateTo(0f) } },
        ) { ch, dy ->
            ch.consume()
            scope.launch { drag.snapTo((drag.value + dy).coerceAtLeast(0f)) }
        }
    }
    // экран плеера лежит поверх приложения: без этого нажатия проваливались в список треков под ним
    Box(
        Modifier
            .fillMaxSize()
            .graphicsLayer {
                translationY = drag.value
                val k = 1 - (drag.value / (size.height * 4)).coerceIn(0f, 0.06f)
                scaleX = k
                scaleY = k
            }
            .clip(RoundedCornerShape(if (drag.value > 1) 28.dp else 0.dp))
            .background(Pal.bg)
            .clickable(remember { MutableInteractionSource() }, indication = null) {}
            // касания видим раньше детей и не забираем их — текст листается и тапается как обычно
            .pointerInput(Unit) {
                awaitPointerEventScope {
                    while (true) {
                        awaitPointerEvent(PointerEventPass.Initial)
                        touchAt = SystemClock.uptimeMillis()
                    }
                }
            },
    ) {
        LyrBackdrop(t.cover, clock)
        // в режиме плеера обложка на всю ширину сверху и плавно уходит в фон
        AnimatedVisibility(mode == 0, enter = fadeIn(tween(350)), exit = fadeOut(tween(250))) {
            CoverImage(
                t.cover, big = true,
                modifier = Modifier
                    .fillMaxWidth()
                    .aspectRatio(0.92f)
                    .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
                    .drawWithContent {
                        drawContent()
                        drawRect(Brush.verticalGradient(*coverFade), blendMode = BlendMode.DstIn)
                    }
                    // жесты по обложке: двойной тап — в любимые, свайп влево/вправо — следующий/прошлый трек
                    .pointerInput(t.key) { detectTapGestures(onDoubleTap = { vm.toggleLike(t) }) }
                    .pointerInput(Unit) {
                        var dx = 0f
                        detectHorizontalDragGestures(
                            onDragStart = { dx = 0f },
                            onDragEnd = { if (dx < -80.dp.toPx()) vm.next() else if (dx > 80.dp.toPx()) vm.prev() },
                        ) { ch, d ->
                            ch.consume()
                            dx += d
                        }
                    }
                    .then(swipe),
            )
        }
        Column(Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding()) {
            // ручка: за неё (и за шапку) экран смахивается вниз
            Box(Modifier.fillMaxWidth().height(28.dp).then(swipe), contentAlignment = Alignment.Center) {
                Box(Modifier.size(40.dp, 5.dp).clip(CircleShape).background(Color(0x66FFFFFF)))
            }
            if (mode != 0) {
                Row(Modifier.fillMaxWidth().then(swipe).padding(start = 24.dp, end = 12.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Cover(t.cover, 58.dp, 8.dp)
                    Spacer(Modifier.width(14.dp))
                    TitleBlock(vm, t, Modifier.weight(1f), 17.sp)
                    TrackActions(vm, t)
                }
            }
            Box(Modifier.weight(1f)) {
                Crossfade(mode, animationSpec = tween(250), label = "mode") { m ->
                    when (m) {
                        0 -> Box(Modifier.fillMaxSize().then(swipe))
                        1 -> Box(Modifier.fillMaxSize()) {
                            LyricsView(vm, clock)
                            // перевод — круглой кнопкой справа внизу, как кнопка караоке у Apple; прячется вместе с панелью
                            androidx.compose.animation.AnimatedVisibility(panel, Modifier.align(Alignment.BottomEnd), enter = fadeIn(), exit = fadeOut()) {
                            Box(
                                Modifier
                                    .padding(end = 20.dp, bottom = 12.dp)
                                    .size(46.dp)
                                    .clip(CircleShape)
                                    .background(if (vm.lyrTr) Color(0xCCFFFFFF) else Color(0x33FFFFFF))
                                    .clickable { vm.toggleLyrTr() },
                                contentAlignment = Alignment.Center,
                            ) {
                                Icon(Icons.Rounded.Translate, tr("Перевод текста", "Translate lyrics", "Переклад тексту", "Preklad textu"), tint = if (vm.lyrTr) Color.Black else Color.White, modifier = Modifier.size(22.dp))
                            }
                            }
                        }
                        else -> QueueView(vm)
                    }
                }
            }
            AnimatedVisibility(panel, enter = expandVertically() + fadeIn(), exit = shrinkVertically() + fadeOut()) {
                Column(Modifier.padding(horizontal = 26.dp)) {
                    if (mode == 0) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            TitleBlock(vm, t, Modifier.weight(1f), 22.sp)
                            TrackActions(vm, t)
                        }
                        Spacer(Modifier.height(10.dp))
                    }
                    Seek(vm)
                    Row(Modifier.fillMaxWidth().padding(vertical = 10.dp), horizontalArrangement = Arrangement.SpaceEvenly, verticalAlignment = Alignment.CenterVertically) {
                        IconButton(onClick = { vm.prev() }, Modifier.size(64.dp)) { Icon(Icons.Rounded.FastRewind, null, tint = Color.White, modifier = Modifier.size(48.dp)) }
                        PlayButton(vm, 72)
                        IconButton(onClick = { vm.next() }, Modifier.size(64.dp)) { Icon(Icons.Rounded.FastForward, null, tint = Color.White, modifier = Modifier.size(48.dp)) }
                    }
                    Volume(vm)
                    if (vm.remote != null) {
                        Text(
                            tr("Играет на «${vm.remoteName}»", "Playing on “${vm.remoteName}”", "Грає на «${vm.remoteName}»", "Hrá na „${vm.remoteName}“"),
                            color = LocalAccent.current.c1, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
                            modifier = Modifier.align(Alignment.CenterHorizontally).padding(top = 4.dp),
                        )
                    }
                    var devices by remember { mutableStateOf(false) }
                    if (devices) ConnDialog(vm) { devices = false }
                    // низ: текст, устройства, повтор, очередь (на очереди — значок, если включена перемешка)
                    Row(Modifier.fillMaxWidth().padding(top = 6.dp, bottom = 12.dp), horizontalArrangement = Arrangement.SpaceAround, verticalAlignment = Alignment.CenterVertically) {
                        ModeButton(Icons.Rounded.FormatQuote, mode == 1) { mode = if (mode == 1) 0 else 1 }
                        if (vm.connOn) ModeButton(Icons.Rounded.Devices, vm.remote != null) { devices = true }
                        ModeButton(if (vm.repeat == Player.REPEAT_MODE_ONE) Icons.Rounded.RepeatOne else Icons.Rounded.Repeat, vm.repeat != Player.REPEAT_MODE_OFF) { vm.cycleRepeat() }
                        Box {
                            ModeButton(Icons.AutoMirrored.Rounded.FormatListBulleted, mode == 2) { mode = if (mode == 2) 0 else 2 }
                            if (vm.shuffle) {
                                Box(
                                    Modifier.align(Alignment.TopEnd).offset(x = 4.dp, y = (-4).dp).size(20.dp).clip(CircleShape).background(Color(0xFF3A3A3C)),
                                    contentAlignment = Alignment.Center,
                                ) { Icon(if (vm.smartShuffle && vm.remote == null) SmartShuffle else Icons.Rounded.Shuffle, null, tint = Color.White, modifier = Modifier.size(13.dp)) }
                            }
                        }
                    }
                }
            }
        }
    }
}

@UnstableApi
@Composable
private fun TitleBlock(vm: AppState, t: Track, modifier: Modifier, size: androidx.compose.ui.unit.TextUnit) {
    Column(modifier) {
        Text(t.title, color = Color.White, fontSize = size, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(
            t.artist, color = Color.White.copy(alpha = 0.6f), fontSize = size * 0.9f, maxLines = 1, overflow = TextOverflow.Ellipsis,
            modifier = Modifier.clickable { vm.nowOpen = false; vm.openArtistOf(t) },
        )
    }
}

/** Кнопка нижнего ряда: включённая — в светлом круге */
@Composable
private fun ModeButton(icon: androidx.compose.ui.graphics.vector.ImageVector, on: Boolean, onClick: () -> Unit) {
    Box(
        Modifier.size(46.dp).clip(CircleShape).background(if (on) Color(0x40FFFFFF) else Color.Transparent).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Icon(icon, null, tint = if (on) Color.White else Color.White.copy(alpha = 0.6f), modifier = Modifier.size(26.dp)) }
}

/** Действия с играющим треком: ☆ — в плейлист, «•••» — следующим, радио, артист */
@UnstableApi
@Composable
private fun TrackActions(vm: AppState, t: Track) {
    var adding by remember { mutableStateOf(false) }
    var menu by remember { mutableStateOf(false) }
    var sleep by remember { mutableStateOf(false) }
    if (sleep) SleepDialog(vm) { sleep = false }
    var notes by remember { mutableStateOf(false) }
    if (notes) NotesDialog(vm, t) { notes = false }
    // ♥ — в любимые; в плейлист — через «•••»
    val liked = vm.isLiked(t)
    val beat by animateFloatAsState(if (liked) 1f else 0f, spring(dampingRatio = 0.4f, stiffness = 500f), label = "heart")
    IconButton(onClick = { vm.toggleLike(t) }) {
        Icon(
            if (liked) Icons.Rounded.Favorite else Icons.Rounded.FavoriteBorder,
            if (liked) tr("Убрать из любимых", "Remove from Liked", "Прибрати з улюблених", "Odobrať z obľúbených") else tr("В любимые", "Add to Liked", "В улюблені", "Do obľúbených"),
            tint = if (liked) LocalAccent.current.c1 else Color.White,
            modifier = Modifier.size(28.dp).graphicsLayer { val k = 1f + 0.18f * beat * (1f - beat) * 4f; scaleX = k; scaleY = k },
        )
    }
    Box {
        IconButton(onClick = { menu = true }) { Icon(Icons.Rounded.MoreHoriz, null, tint = Color.White, modifier = Modifier.size(28.dp)) }
        GlassMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(
                text = { Text(tr("Добавить в плейлист", "Add to playlist", "Додати в плейлист", "Pridať do playlistu")) },
                leadingIcon = { Icon(Icons.AutoMirrored.Rounded.PlaylistAdd, null) },
                onClick = { menu = false; adding = true },
            )
            DropdownMenuItem(
                text = { Text(tr("Играть следующим", "Play next", "Грати наступним", "Hrať ako ďalšiu")) },
                leadingIcon = { Icon(Icons.AutoMirrored.Rounded.QueueMusic, null) },
                onClick = { menu = false; vm.playNext(t) },
            )
            DropdownMenuItem(
                text = { Text(tr("Поделиться ссылкой", "Share link", "Поділитися посиланням", "Zdieľať odkaz")) },
                leadingIcon = { Icon(Icons.Rounded.Link, null) },
                onClick = { menu = false; vm.share(t) },
            )
            DropdownMenuItem(
                text = { Text(tr("Радио по треку", "Track radio", "Радіо за треком", "Rádio podľa skladby")) },
                leadingIcon = { Icon(Icons.Rounded.Radio, null) },
                onClick = { menu = false; vm.startRadio(t) },
            )
            DropdownMenuItem(
                text = { Text(tr("Эквалайзер и скорость", "Equalizer and speed", "Еквалайзер і швидкість", "Ekvalizér a rýchlosť")) },
                leadingIcon = { Icon(Icons.Rounded.Tune, null) },
                onClick = { menu = false; vm.nowOpen = false; vm.go(Screen.Eq) },
            )
            DropdownMenuItem(
                text = { Text(tr("Заметки к моментам", "Notes at moments", "Нотатки до моментів", "Poznámky k momentom")) },
                leadingIcon = { Icon(Icons.Rounded.EditNote, null) },
                onClick = { menu = false; notes = true },
            )
            DropdownMenuItem(
                text = { Text(tr("Таймер сна", "Sleep timer", "Таймер сну", "Časovač spánku")) },
                leadingIcon = { Icon(Icons.Rounded.Bedtime, null) },
                onClick = { menu = false; sleep = true },
            )
            DropdownMenuItem(
                text = { Text(tr("Артист", "Artist", "Артист", "Interpret")) },
                leadingIcon = { Icon(Icons.Rounded.Person, null) },
                onClick = { menu = false; vm.nowOpen = false; vm.openArtistOf(t) },
            )
        }
    }
    if (adding) AddToPlaylist(vm, t) { adding = false }
}

/** Полоса как у Apple: толстая скруглённая, без точки; пока тянешь — толще и ярче */
@Composable
private fun Bar(v: Float, grab: Float, modifier: Modifier, marks: List<Float> = emptyList()) {
    val dot = LocalAccent.current.c1
    Canvas(modifier.fillMaxWidth().height(28.dp)) {
        val h = (7 + 5 * grab).dp.toPx()
        val top = (size.height - h) / 2
        val r = CornerRadius(h / 2)
        // заметки к моментам — точками над полосой
        for (m in marks) drawCircle(dot, 2.5.dp.toPx(), Offset(size.width * m.coerceIn(0f, 1f), top - 5.dp.toPx()))
        drawRoundRect(Color(0x33FFFFFF), Offset(0f, top), Size(size.width, h), r)
        drawRoundRect(Color.White.copy(alpha = 0.55f + 0.4f * grab), Offset(0f, top), Size((size.width * v).coerceAtLeast(h), h), r)
    }
}

/** Перемотка: тап — сразу туда, тянуть — с отпусканием. Справа сколько осталось */
@UnstableApi
@Composable
private fun Seek(vm: AppState) {
    var drag by remember { mutableStateOf<Float?>(null) }
    val dur = vm.duration.coerceAtLeast(1)
    val v = drag ?: (vm.position.toFloat() / dur).coerceIn(0f, 1f)
    val grab by animateFloatAsState(if (drag != null) 1f else 0f, tween(150), label = "grab")
    Bar(
        v, grab,
        Modifier
            .pointerInput(dur) { detectTapGestures { vm.seek(((it.x / size.width).coerceIn(0f, 1f) * dur).toLong()) } }
            .pointerInput(dur) {
                detectHorizontalDragGestures(
                    onDragStart = { drag = (it.x / size.width).coerceIn(0f, 1f) },
                    onDragEnd = { drag?.let { vm.seek((it * dur).toLong()) }; drag = null },
                    onDragCancel = { drag = null },
                ) { ch, _ -> drag = (ch.position.x / size.width).coerceIn(0f, 1f) }
            },
        marks = vm.notes.map { (it.pos * 1000 / dur).toFloat() },
    )
    val at = (v * dur / 1000).toLong()
    Row(Modifier.fillMaxWidth()) {
        Text(fmtDur(at).ifEmpty { "0:00" }, color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
        Text("−" + fmtDur((vm.duration / 1000 - at).coerceAtLeast(0)).ifEmpty { "0:00" }, color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
    }
}

/** Громкость там, где звук: телефона (поток музыки, кнопки громкости тоже двигают полосу) или другого устройства */
@UnstableApi
@Composable
private fun Volume(vm: AppState) {
    val ctx = LocalContext.current
    val am = remember { ctx.getSystemService(AudioManager::class.java) }
    val max = remember { am.getStreamMaxVolume(AudioManager.STREAM_MUSIC).coerceAtLeast(1) }
    var drag by remember { mutableStateOf<Float?>(null) }
    var vol by remember { mutableFloatStateOf(am.getStreamVolume(AudioManager.STREAM_MUSIC) / max.toFloat()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(400)
            if (drag == null) vol = am.getStreamVolume(AudioManager.STREAM_MUSIC) / max.toFloat()
        }
    }
    val grab by animateFloatAsState(if (drag != null) 1f else 0f, tween(150), label = "vgrab")
    fun set(f: Float, end: Boolean = false) {
        drag = f
        if (vm.remote != null) vm.connVolume(f, end) else am.setStreamVolume(AudioManager.STREAM_MUSIC, (f * max).roundToInt(), 0)
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Icon(Icons.AutoMirrored.Rounded.VolumeMute, null, tint = Color.White.copy(alpha = 0.6f), modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(8.dp))
        Bar(
            drag ?: vm.remoteVol.takeIf { vm.remote != null } ?: vol, grab,
            Modifier
                .weight(1f)
                .pointerInput(Unit) { detectTapGestures { set((it.x / size.width).coerceIn(0f, 1f), true); drag = null } }
                .pointerInput(Unit) {
                    detectHorizontalDragGestures(
                        onDragStart = { set((it.x / size.width).coerceIn(0f, 1f)) },
                        onDragEnd = { drag?.let { vol = it; if (vm.remote != null) vm.connVolume(it, true) }; drag = null },
                        onDragCancel = { drag = null },
                    ) { ch, _ -> set((ch.position.x / size.width).coerceIn(0f, 1f)) }
                },
        )
        Spacer(Modifier.width(8.dp))
        Icon(Icons.AutoMirrored.Rounded.VolumeUp, null, tint = Color.White.copy(alpha = 0.6f), modifier = Modifier.size(22.dp))
    }
}

/** Очередь: спетые сверху приглушены, играющий подсвечен. Тап — играть, ✕ — убрать, за ручку справа —
 *  перетащить. Пока тянем, порядок живёт локально; в плеер уходит один перенос, когда отпустили */
@UnstableApi
@Composable
private fun QueueView(vm: AppState) {
    val a = LocalAccent.current
    val state = rememberLazyListState(initialFirstVisibleItemIndex = vm.queueIndex.coerceAtLeast(0))
    val rowH = 64.dp
    val rowPx = with(LocalDensity.current) { rowH.toPx() }
    var local by remember { mutableStateOf<List<MediaItem>?>(null) }
    var dragKey by remember { mutableStateOf<String?>(null) }
    var dragFrom by remember { mutableIntStateOf(-1) }
    var dy by remember { mutableFloatStateOf(0f) }
    val items = local ?: vm.queue
    // ключ строки: id трека + какой он по счёту (один трек может стоять в очереди дважды)
    val keys = remember(items) {
        val seen = HashMap<String, Int>()
        items.map { val n = seen.merge(it.mediaId, 1, Int::plus)!!; "${it.mediaId}#$n" }
    }
    // жест живёт дольше одной отрисовки строки — список, ключи и индекс он берёт всегда свежие
    val itemsNow by rememberUpdatedState(items)
    val keysNow by rememberUpdatedState(keys)
    // плеер принял перенос — локальный порядок больше не нужен
    LaunchedEffect(vm.queue.map { it.mediaId }) {
        if (dragKey == null && local?.map { it.mediaId } == vm.queue.map { it.mediaId }) local = null
    }
    Column {
    // как у Apple: «Далее» и переключатели перемешки и повтора над списком
    Row(Modifier.fillMaxWidth().padding(start = 24.dp, end = 16.dp, top = 8.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(tr("Далее", "Playing Next", "Далі", "Ďalej"), color = Color.White, fontSize = 18.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
        QueueToggle(if (vm.smartShuffle && vm.shuffle && vm.remote == null) SmartShuffle else Icons.Rounded.Shuffle, vm.shuffle) { vm.toggleShuffle() }
        Spacer(Modifier.width(10.dp))
        QueueToggle(if (vm.repeat == Player.REPEAT_MODE_ONE) Icons.Rounded.RepeatOne else Icons.Rounded.Repeat, vm.repeat != Player.REPEAT_MODE_OFF) { vm.cycleRepeat() }
    }
    LazyColumn(state = state, contentPadding = PaddingValues(vertical = 8.dp)) {
        itemsIndexed(items, key = { i, _ -> keys[i] }) { i, item ->
            val md = item.mediaMetadata
            val key = keys[i]
            val dragged = key == dragKey
            val cur = i == vm.queueIndex && local == null
            val iNow by rememberUpdatedState(i)
            Row(
                Modifier
                    .then(if (dragged) Modifier.zIndex(1f) else Modifier.animateItem())
                    .graphicsLayer { if (dragged) translationY = dy }
                    .fillMaxWidth()
                    .height(rowH)
                    .background(if (dragged) Pal.card else if (cur) a.c1.copy(alpha = 0.12f) else Color.Transparent)
                    .clickable { vm.jump(i) }
                    .padding(start = 20.dp, end = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val past = i < vm.queueIndex && local == null
                Cover(md.artworkUri?.toString() ?: "", 44.dp, 8.dp, Modifier.graphicsLayer { alpha = if (past) 0.45f else 1f })
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f).graphicsLayer { alpha = if (past) 0.45f else 1f }) {
                    Text(md.title?.toString() ?: "", color = if (cur) a.c1 else Pal.text, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(md.artist?.toString() ?: "", color = Pal.dim, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                if (cur) {
                    Icon(Icons.Rounded.GraphicEq, null, tint = a.c1, modifier = Modifier.padding(12.dp).size(22.dp))
                } else {
                    IconButton(onClick = { vm.removeFromQueue(iNow) }) { Icon(Icons.Rounded.Close, tr("Убрать", "Remove", "Прибрати", "Odstrániť"), tint = Pal.dim) }
                }
                Icon(
                    Icons.Rounded.DragHandle,
                    tr("Перетащить", "Drag", "Перетягнути", "Presunúť"),
                    tint = Pal.dim,
                    modifier = Modifier
                        .size(48.dp)
                        .padding(12.dp)
                        .pointerInput(key) {
                            detectDragGestures(
                                onDragStart = {
                                    local = itemsNow.toList()
                                    dragKey = key
                                    dragFrom = iNow
                                    dy = 0f
                                },
                                onDragEnd = {
                                    val to = keysNow.indexOf(dragKey)
                                    if (dragFrom >= 0 && to >= 0 && to != dragFrom) vm.moveInQueue(dragFrom, to) else local = null
                                    dragKey = null
                                    dy = 0f
                                },
                                onDragCancel = { local = null; dragKey = null; dy = 0f },
                            ) { ch, d ->
                                ch.consume()
                                dy += d.y
                                val l = local ?: return@detectDragGestures
                                val at = keysNow.indexOf(dragKey)
                                // перевалили за половину соседа — меняемся с ним местами
                                val step = when {
                                    dy > rowPx / 2 && at < l.size - 1 -> 1
                                    dy < -rowPx / 2 && at > 0 -> -1
                                    else -> 0
                                }
                                if (step != 0) {
                                    local = l.toMutableList().apply { add(at + step, removeAt(at)) }
                                    dy -= step * rowPx
                                }
                            }
                        },
                )
            }
        }
    }
    }
}

@Composable
private fun QueueToggle(icon: androidx.compose.ui.graphics.vector.ImageVector, on: Boolean, onClick: () -> Unit) {
    Box(
        Modifier.size(width = 52.dp, height = 34.dp).clip(RoundedCornerShape(17.dp)).background(if (on) Color(0xCCFFFFFF) else Color(0x26FFFFFF)).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Icon(icon, null, tint = if (on) Color.Black else Color.White, modifier = Modifier.size(20.dp)) }
}
