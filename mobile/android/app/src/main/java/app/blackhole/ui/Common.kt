package app.blackhole.ui

import androidx.compose.foundation.layout.heightIn
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Favorite
import androidx.compose.material.icons.rounded.FavoriteBorder
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.ui.graphics.Brush
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.roundToInt
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.PlaylistAdd
import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material.icons.rounded.MoreHoriz
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.MusicNote
import androidx.compose.material.icons.rounded.Person
import androidx.compose.material.icons.automirrored.rounded.QueueMusic
import androidx.compose.material.icons.rounded.Radio
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.media3.common.util.UnstableApi
import app.blackhole.AppState
import app.blackhole.Card
import app.blackhole.Track
import app.blackhole.fmtDur
import app.blackhole.srcName
import app.blackhole.tr
import app.blackhole.plName
import app.blackhole.LIKED
import app.blackhole.LIKED_ART
import androidx.compose.ui.unit.min
import coil3.compose.AsyncImage

@Composable
fun Cover(url: String, size: Dp, radius: Dp = 10.dp, modifier: Modifier = Modifier) {
    if (url == LIKED_ART) return LikedArt(modifier.size(size).clip(RoundedCornerShape(radius)))
    Box(
        modifier.size(size).clip(RoundedCornerShape(radius)).background(Pal.card),
        contentAlignment = Alignment.Center,
    ) {
        Icon(Icons.Rounded.MusicNote, null, tint = Pal.faint, modifier = Modifier.size(size / 2.5f))
        if (url.isNotEmpty()) {
            AsyncImage(model = url, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.matchParentSize())
        }
    }
}

@Composable
fun Title(text: String, modifier: Modifier = Modifier) {
    Text(text, color = Pal.text, fontSize = 26.sp, fontWeight = FontWeight.ExtraBold, modifier = modifier.padding(horizontal = 16.dp, vertical = 8.dp))
}

@Composable
fun Section(text: String) {
    Text(
        text.uppercase(),
        color = Pal.faint,
        fontSize = 11.sp,
        fontWeight = FontWeight.Bold,
        letterSpacing = 1.5.sp,
        modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 18.dp, bottom = 6.dp),
    )
}

/** Кнопка-пилюля как у Apple: основная — белая с чёрным текстом, остальные — стекло; опасная — красный текст */
@Composable
fun Pill(text: String, icon: ImageVector? = null, primary: Boolean = true, enabled: Boolean = true, danger: Boolean = false, onClick: () -> Unit) {
    val solid = primary && !danger
    val fg = when {
        danger -> Color(0xFFFF5A6E)
        solid -> Color.Black
        else -> Color.White
    }.copy(alpha = if (enabled) 1f else 0.45f)
    Row(
        Modifier
            .height(44.dp)
            .then(if (solid) Modifier.clip(RoundedCornerShape(22.dp)).background(Color.White.copy(alpha = if (enabled) 1f else 0.5f)) else Modifier.glass(22))
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 20.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        icon?.let { Icon(it, null, tint = fg, modifier = Modifier.size(18.dp)) }
        Text(text, color = fg, fontWeight = FontWeight.Bold, fontSize = 15.sp)
    }
}

@UnstableApi
@Composable
fun TrackRow(
    vm: AppState,
    t: Track,
    onClick: () -> Unit,
    inPlaylist: String? = null,
    trailing: String? = null,
    number: Int? = null, // альбом: номер вместо обложки, как у Apple
) {
    val playing = vm.current?.key == t.key
    val a = LocalAccent.current
    var menu by remember { mutableStateOf(false) }
    var adding by remember { mutableStateOf(false) }
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(start = 16.dp, end = 4.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        when {
            number == null -> Cover(t.cover, 48.dp)
            playing -> Icon(Icons.Rounded.GraphicEq, null, tint = a.c1, modifier = Modifier.width(30.dp).size(20.dp))
            else -> Text("$number", color = Pal.dim, fontSize = 16.sp, modifier = Modifier.width(30.dp))
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(t.title, color = if (playing) a.c1 else Pal.text, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    srcName(t.src),
                    color = a.c2,
                    fontSize = 9.sp,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.clip(RoundedCornerShape(4.dp)).background(Color(0x14FFFFFF)).padding(horizontal = 4.dp, vertical = 1.dp),
                )
                Spacer(Modifier.width(6.dp))
                Text(
                    listOfNotNull(t.artist, trailing ?: fmtDur(t.dur).ifEmpty { null }).joinToString("  ·  "),
                    color = Pal.dim,
                    fontSize = 13.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Box {
            IconButton(onClick = { menu = true }) { Icon(Icons.Rounded.MoreHoriz, null, tint = Pal.dim) }
            GlassMenu(expanded = menu, onDismissRequest = { menu = false }) {
                DropdownMenuItem(
                    text = { Text(tr("В плейлист", "Add to playlist", "У плейлист", "Do playlistu")) },
                    leadingIcon = { Icon(Icons.AutoMirrored.Rounded.PlaylistAdd, null) },
                    onClick = { menu = false; adding = true },
                )
                val liked = vm.isLiked(t)
                DropdownMenuItem(
                    text = { Text(if (liked) tr("Убрать из любимых", "Remove from Liked", "Прибрати з улюблених", "Odobrať z obľúbených") else tr("В любимые", "Add to Liked", "В улюблені", "Do obľúbených")) },
                    leadingIcon = { Icon(if (liked) Icons.Rounded.Favorite else Icons.Rounded.FavoriteBorder, null) },
                    onClick = { menu = false; vm.toggleLike(t) },
                )
                DropdownMenuItem(
                    text = { Text(tr("Играть следующим", "Play next", "Грати наступним", "Hrať ako ďalšiu")) },
                    leadingIcon = { Icon(Icons.AutoMirrored.Rounded.QueueMusic, null) },
                    onClick = { menu = false; vm.playNext(t) },
                )
                DropdownMenuItem(
                    text = { Text(tr("Радио по треку", "Track radio", "Радіо за треком", "Rádio podľa skladby")) },
                    leadingIcon = { Icon(Icons.Rounded.Radio, null) },
                    onClick = { menu = false; vm.startRadio(t) },
                )
                DropdownMenuItem(
                    text = { Text(tr("Артист", "Artist", "Артист", "Interpret")) },
                    leadingIcon = { Icon(Icons.Rounded.Person, null) },
                    onClick = { menu = false; vm.nowOpen = false; vm.openArtistOf(t) },
                )
                if (inPlaylist != null) {
                    DropdownMenuItem(
                        text = { Text(tr("Убрать из плейлиста", "Remove from playlist", "Прибрати з плейлиста", "Odstrániť z playlistu")) },
                        leadingIcon = { Icon(Icons.Rounded.Delete, null) },
                        onClick = { menu = false; vm.removeFrom(inPlaylist, t) },
                    )
                }
            }
        }
    }
    if (adding) AddToPlaylist(vm, t) { adding = false }
}

/** «В плейлист»: стеклянная карточка со списком плейлистов (обложка, название, сколько треков) */
@UnstableApi
@Composable
fun AddToPlaylist(vm: AppState, t: Track, close: () -> Unit) {
    var creating by remember { mutableStateOf(false) }
    if (creating) {
        NameDialog(tr("Новый плейлист", "New playlist", "Новий плейлист", "Nový playlist"), "") { name ->
            if (name != null) vm.createPlaylist(name, listOf(t))
            close()
        }
        return
    }
    GlassDialog(close, tr("В плейлист", "Add to playlist", "У плейлист", "Do playlistu"), buttons = {
        Pill(tr("Отмена", "Cancel", "Скасувати", "Zrušiť"), primary = false, onClick = close)
        Pill(tr("Новый", "New", "Новий", "Nový"), Icons.Rounded.Add) { creating = true }
    }) {
        LazyColumn(Modifier.heightIn(max = 360.dp)) {
            // любимые — только сердечком, сюда их не выводим
            items(vm.playlists.filter { it.name != LIKED }, key = { it.name }) { p ->
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).clickable { vm.addTo(p.name, t); close() }.padding(vertical = 6.dp, horizontal = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Cover(p.cover.ifEmpty { p.tracks.firstOrNull()?.cover ?: "" }, 44.dp, 10.dp)
                    Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f)) {
                        Text(plName(p.name), color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(tr("${p.tracks.size} треков", "${p.tracks.size} tracks", "${p.tracks.size} треків", "${p.tracks.size} skladieb"), color = Pal.dim, fontSize = 12.sp)
                    }
                }
            }
        }
    }
}

/** Диалог с одним полем: null — отменили */
@Composable
fun NameDialog(title: String, initial: String, label: String = tr("Название", "Name", "Назва", "Názov"), done: (String?) -> Unit) {
    var text by remember { mutableStateOf(initial) }
    GlassDialog({ done(null) }, title, buttons = {
        Pill(tr("Отмена", "Cancel", "Скасувати", "Zrušiť"), primary = false) { done(null) }
        Pill("OK", enabled = text.isNotBlank()) { done(text) }
    }) {
        GlassField(text, { text = it }, Modifier.fillMaxWidth(), label = { Text(label) }, singleLine = true)
    }
}

/** Карусель карточек (артисты кругом, альбомы квадратом) */
@Composable
fun CardRow(cards: List<Card>, round: Boolean, onClick: (Card) -> Unit) {
    LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        items(cards, key = { it.id + it.title }) { c ->
            Column(Modifier.width(120.dp).clickable { onClick(c) }) {
                if (round) {
                    Box(Modifier.size(120.dp).clip(CircleShape)) { Cover(c.cover, 120.dp, 60.dp) }
                } else {
                    Cover(c.cover, 120.dp, 12.dp)
                }
                Spacer(Modifier.height(6.dp))
                Text(c.title, color = Pal.text, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (c.sub.isNotEmpty()) Text(c.sub, color = Pal.dim, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

@Composable
fun Dim(text: String, modifier: Modifier = Modifier) {
    Text(text, color = Pal.dim, fontSize = 13.sp, modifier = modifier.padding(horizontal = 16.dp, vertical = 4.dp))
}

/** Быстрая прокрутка длинного списка: ползунок справа виден, пока листаешь (и ещё 1.5 с после), за него
 *  можно тянуть — список едет пропорционально. Спрятан — касаний не ловит, свайпы у края не крадёт */
@Composable
fun BoxScope.FastScroll(state: LazyListState) {
    var dragging by remember { mutableStateOf(false) }
    var visible by remember { mutableStateOf(false) }
    val scrolling = state.isScrollInProgress
    LaunchedEffect(scrolling, dragging) {
        if (scrolling || dragging) visible = true else {
            delay(1500)
            visible = false
        }
    }
    val long by remember { derivedStateOf { state.layoutInfo.totalItemsCount > 30 } }
    if (!long) return
    val alpha by animateFloatAsState(if (visible) 1f else 0f, tween(250), label = "fs")
    val frac by remember {
        derivedStateOf {
            val info = state.layoutInfo
            val room = info.totalItemsCount - info.visibleItemsInfo.size
            if (room <= 0) 0f else (state.firstVisibleItemIndex.toFloat() / room).coerceIn(0f, 1f)
        }
    }
    val scope = rememberCoroutineScope()
    val a = LocalAccent.current
    BoxWithConstraints(Modifier.align(Alignment.CenterEnd).fillMaxHeight().width(32.dp).padding(vertical = 8.dp)) {
        val thumbH = 52.dp
        val travel = maxHeight - thumbH
        fun go(y: Float, h: Float, t: Float) {
            val f = ((y - t / 2) / (h - t)).coerceIn(0f, 1f)
            val info = state.layoutInfo
            val room = (info.totalItemsCount - info.visibleItemsInfo.size).coerceAtLeast(0)
            scope.launch { state.scrollToItem((f * room).roundToInt()) }
        }
        Box(
            Modifier
                .fillMaxSize()
                .then(
                    if (alpha > 0.05f) Modifier.pointerInput(Unit) {
                        val t = thumbH.toPx()
                        detectVerticalDragGestures(
                            onDragStart = { dragging = true; go(it.y, size.height.toFloat(), t) },
                            onDragEnd = { dragging = false },
                            onDragCancel = { dragging = false },
                        ) { ch, _ ->
                            ch.consume()
                            go(ch.position.y, size.height.toFloat(), t)
                        }
                    } else Modifier,
                ),
        ) {
            Box(
                Modifier
                    .align(Alignment.TopEnd)
                    .offset(y = travel * frac)
                    .padding(end = 4.dp)
                    .graphicsLayer { this.alpha = alpha }
                    .width(if (dragging) 8.dp else 6.dp)
                    .height(thumbH)
                    .clip(RoundedCornerShape(4.dp))
                    .background(if (dragging) a.c1 else Color.White.copy(alpha = 0.55f)),
            )
        }
    }
}

/** Круглая кнопка на «стекле» — по бокам от «Слушать», «назад» поверх обложки */
@Composable
fun RoundButton(icon: ImageVector, desc: String?, size: Dp = 48.dp, onClick: () -> Unit) {
    Box(
        Modifier.size(size).clip(CircleShape).background(Glass).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Icon(icon, desc, tint = Color.White, modifier = Modifier.size(size * 0.5f)) }
}

/** Главная кнопка страницы: белая «таблетка» с чёрным текстом */
@Composable
fun PlayPill(text: String, enabled: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Row(
        modifier
            .height(48.dp)
            .clip(RoundedCornerShape(24.dp))
            .background(Color.White.copy(alpha = if (enabled) 1f else 0.5f))
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 24.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Icon(Icons.Rounded.PlayArrow, null, tint = Color.Black, modifier = Modifier.size(24.dp))
        Spacer(Modifier.width(6.dp))
        Text(text, color = Color.Black, fontWeight = FontWeight.Bold, fontSize = 17.sp)
    }
}

/** Картинка на всю ширину сверху страницы, низом плавно уходит в фон; content — поверх нижней части */
@Composable
fun Hero(image: String, round: Boolean = false, content: @Composable BoxScope.() -> Unit) {
    Box(Modifier.fillMaxWidth().aspectRatio(0.95f)) {
        if (image == LIKED_ART) {
            LikedArt(Modifier.fillMaxSize())
        } else if (image.isNotEmpty()) {
            AsyncImage(bigCover(image), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        } else {
            Box(Modifier.fillMaxSize().background(Pal.card), contentAlignment = Alignment.Center) {
                Icon(if (round) Icons.Rounded.Person else Icons.Rounded.MusicNote, null, tint = Pal.faint, modifier = Modifier.size(96.dp))
            }
        }
        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(0.45f to Color.Transparent, 0.82f to Pal.bg.copy(alpha = 0.85f), 1f to Pal.bg)))
        content()
    }
}

/** «Назад» кружком поверх страницы, когда есть куда возвращаться */
@UnstableApi
@Composable
fun BoxScope.BackButton(vm: AppState) {
    if (vm.stack.size > 1) {
        Box(Modifier.align(Alignment.TopStart).statusBarsPadding().padding(12.dp)) {
            RoundButton(Icons.AutoMirrored.Rounded.ArrowBack, null, 44.dp) { vm.back() }
        }
    }
}

/** Та же обложка в большом размере — для картинок на весь экран (как big_url на ПК). Сервисы отдают
 *  превью по размеру в ссылке: подставляем побольше */
fun bigCover(url: String): String = url
    // YouTube Music / Google: =w226-h226-… или =s120 → 1080
    .replace(Regex("=w\\d+-h\\d+[^/]*$"), "=w1080-h1080-l90-rj")
    .replace(Regex("=s\\d+([^/]*)$"), "=s1080$1")
    // превью YouTube-видео
    .replace(Regex("/(mq|sd)?default\\.jpg"), "/hqdefault.jpg")
    // SoundCloud
    .replace(Regex("-(t\\d+x\\d+|large|crop|small|badge|tiny)\\."), "-t500x500.")
    // Spotify
    .replace("00001e02", "0000b273").replace("00004851", "0000b273")
    // Apple Music
    .replace(Regex("/\\d+x\\d+(bb|cc)?\\.(jpg|png|webp)$"), "/1000x1000bb.jpg")
    // Яндекс
    .replace(Regex("/\\d+x\\d+$"), "/1000x1000").replace("%%", "1000x1000")

/** Обложка «Любимых треков»: белое сердечко на фиолетово-розовом градиенте */
@Composable
fun LikedArt(modifier: Modifier) {
    BoxWithConstraints(
        modifier.background(Brush.linearGradient(listOf(Color(0xFF3B1D8F), Color(0xFF7C3AED), Color(0xFFDB2777)))),
        contentAlignment = Alignment.Center,
    ) {
        Icon(Icons.Rounded.Favorite, null, tint = Color.White, modifier = Modifier.size(min(maxWidth, maxHeight) * 0.42f))
    }
}
