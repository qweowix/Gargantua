package app.blackhole.ui

import android.os.SystemClock
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.draw.BlurredEdgeTreatment
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.media3.common.util.UnstableApi
import app.blackhole.AppState
import app.blackhole.LyrLine
import app.blackhole.tr
import coil3.compose.LocalPlatformContext
import coil3.compose.rememberAsyncImagePainter
import coil3.request.ImageRequest
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/** Часы экрана плеера: позиция от плеера приходит раз в 100 мс, между опросами досчитываем по кадрам
 *  и мягко подтягиваемся к плееру, как на ПК — иначе заливка дёргается. Назад не ходим, перемотку берём сразу.
 *  bg — время плавания фона, идёт только пока играет */
@Stable
class LyrClock {
    var pos by mutableDoubleStateOf(0.0)
    var bg by mutableFloatStateOf(0f)
}

@UnstableApi
@Composable
fun rememberLyrClock(vm: AppState): LyrClock {
    val c = remember { LyrClock() }
    LaunchedEffect(Unit) {
        var seen = -1L
        var seenAt = 0L
        var last = 0L
        while (true) withFrameNanos { now ->
            if (vm.position != seen) {
                seen = vm.position
                seenAt = now
            }
            val dt = if (last == 0L) 0.0 else min((now - last) / 1e9, 0.1)
            last = now
            val player = seen / 1000.0 + if (vm.playing) min((now - seenAt) / 1e9, 1.5) else 0.0
            val ahead = if (vm.playing) c.pos + dt else c.pos
            val err = player - ahead
            c.pos = if (abs(err) > 1.0) player else max(ahead + err * min(dt * 3, 1.0), c.pos)
            if (vm.playing) c.bg += dt.toFloat() * 1.5f
        }
    }
    return c
}

/** Фон как на экране текста ПК: подложка цвета темы, крошечная размытая обложка в 1.7 экрана медленно
 *  вращается, поверх плавают по кругу два пятна с мягкими краями, сверху затемнение под белый текст.
 *  Картинки готовятся один раз на трек (backdrop), дальше только сдвиги и повороты слоёв */
@Composable
fun LyrBackdrop(cover: String, clock: LyrClock) {
    val deep = LocalAccent.current.deep
    val ctx = LocalContext.current
    var b by remember(cover) { mutableStateOf<Backdrop?>(null) }
    LaunchedEffect(cover) { if (cover.isNotEmpty()) b = runCatching { backdrop(ctx, cover) }.getOrNull() }
    // фон проявляется из-под подложки, когда картинки готовы
    val on by animateFloatAsState(if (b != null) 1f else 0f, tween(450), label = "bg")
    // слои больше экрана: без обрезки они торчат за его края, пока экран выезжает и уезжает
    BoxWithConstraints(
        Modifier.fillMaxSize().clipToBounds().drawBehind {
            // linear-gradient(160deg, deep1, #07060b)
            val v = Offset(sin(160f * PI.toFloat() / 180), -cos(160f * PI.toFloat() / 180))
            val half = (abs(v.x) * size.width + abs(v.y) * size.height) / 2
            drawRect(Brush.linearGradient(listOf(deep, Color(0xFF07060B)), center - v * half, center + v * half))
        },
    ) {
        val img = b
        if (img != null) {
            val big = max(maxWidth.value, maxHeight.value).dp
            val (w, h) = maxWidth to maxHeight
            Image(
                img.bg, null, contentScale = ContentScale.FillBounds, filterQuality = FilterQuality.Low,
                modifier = Modifier.align(Alignment.Center).requiredSize(big * 1.7f).graphicsLayer { rotationZ = clock.bg * 3f; alpha = on },
            )
            for (k in 0..1) {
                Image(
                    img.blob, null, contentScale = ContentScale.FillBounds, filterQuality = FilterQuality.Low,
                    modifier = Modifier
                        .align(Alignment.Center)
                        .requiredSize(big * if (k == 0) 0.95f else 0.8f)
                        .graphicsLayer {
                            val t = clock.bg
                            // слой больше экрана и стоит центром в центре экрана — двигаем от центра
                            val (cx, cy) = if (k == 0) {
                                0.25f * w.toPx() + big.toPx() * 0.12f * cos(t * 0.13f) to 0.3f * h.toPx() + big.toPx() * 0.1f * sin(t * 0.17f)
                            } else {
                                0.78f * w.toPx() + big.toPx() * 0.1f * sin(t * 0.11f) to 0.72f * h.toPx() + big.toPx() * 0.12f * cos(t * 0.09f)
                            }
                            translationX = cx - w.toPx() / 2
                            translationY = cy - h.toPx() / 2
                            rotationZ = if (k == 0) -t * 7f else t * 5f + 140f
                            alpha = on
                        },
                )
            }
        }
        // затемнение, чтобы белый текст читался на любой обложке
        Box(Modifier.fillMaxSize().background(Color(0x70000000)))
    }
}

/** Переход между строками: мягкий разгон и торможение, как ny в LyricLine на ПК */
private val LyrEase = CubicBezierEasing(0.35f, 0f, 0.15f, 1f)
private const val LYR_MOVE_MS = 650

/** По какой строке держать прокрутку: последняя начавшаяся основная (у дуэтов строки идут внахлёст),
 *  а не её бэк-вокал. В короткой паузе — последняя спетая */
private fun current(lines: List<LyrLine>, pos: Double): Int {
    var cur = lines.indexOfLast { it.start <= pos }
    lines.indices.lastOrNull { lines[it].start <= pos && pos < lines[it].end && !lines[it].bg }?.let { cur = it }
    while (cur > 0 && lines[cur].bg) cur--
    return cur
}

/** Докуда залита строка, в символах текста: по таймингам слов, а без них — равномерно до fill */
private fun edgeOf(l: LyrLine, pos: Double): Double {
    if (l.words.isEmpty()) return l.text.length * ((pos - l.start) / max(l.fill - l.start, 0.01)).coerceIn(0.0, 1.0)
    val k = l.words.indexOfLast { it.s <= pos }
    if (k < 0) return 0.0
    val w = l.words[k]
    return w.a + (w.b - w.a) * ((pos - w.s) / (w.e - w.s)).coerceIn(0.0, 1.0)
}

/** Сила волны: в паузе между словами волна оседает, а не висит над последней буквой */
private fun restOf(l: LyrLine, pos: Double): Float {
    val w = l.words.lastOrNull { it.s <= pos } ?: return 1f
    return (1 - (pos - w.e) / 0.4).coerceIn(0.0, 1.0).toFloat()
}

@UnstableApi
@Composable
fun LyricsView(vm: AppState, clock: LyrClock) {
    val l = vm.lyrics
    when {
        l == null -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        l.kind == "plain" -> LazyColumn(contentPadding = PaddingValues(24.dp)) {
            itemsIndexed(l.plain) { _, s -> Text(s, color = Color.White, fontSize = 22.sp, fontWeight = FontWeight.ExtraBold, modifier = Modifier.padding(vertical = 5.dp)) }
            item { Credit(l.credit) }
        }
        l.kind == "synced" -> BoxWithConstraints(Modifier.fillMaxSize().fadeEdges(36.dp, 56.dp)) {
            val lines = l.lines
            val cur = remember(lines) { derivedStateOf { current(lines, clock.pos) } }
            val state = rememberLazyListState()
            // палец листает текст — автопрокрутка ждёт 3 с тишины
            var userAt by remember { mutableLongStateOf(0L) }
            val touch = remember {
                object : NestedScrollConnection {
                    override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                        if (source == NestedScrollSource.UserInput) userAt = SystemClock.uptimeMillis()
                        return Offset.Zero
                    }
                }
            }
            LaunchedEffect(cur.value, userAt) {
                val wait = userAt + 3000 - SystemClock.uptimeMillis()
                if (wait > 0) kotlinx.coroutines.delay(wait)
                val target = cur.value.coerceAtLeast(0)
                // строка на экране — едем к ней сами одной мягкой кривой (как на ПК), а не пружиной Compose:
                // так прокрутка идёт ровно на каждом кадре экрана, хоть 120 Гц. Далеко — прыжком
                val item = state.layoutInfo.visibleItemsInfo.firstOrNull { it.index == target }
                if (item != null) state.animateScrollBy(item.offset.toFloat(), tween(LYR_MOVE_MS, easing = LyrEase))
                else state.scrollToItem(target)
            }
            LazyColumn(
                state = state,
                // строка, которая поётся, стоит на трети экрана сверху
                // бока не отступом списка, а внутри строк: размытию нужно место по краям, иначе его срезает
                contentPadding = PaddingValues(top = maxHeight * 0.28f, bottom = maxHeight * 0.7f),
                modifier = Modifier.nestedScroll(touch),
            ) {
                itemsIndexed(lines) { i, line ->
                    if (line.dots) Box(Modifier.padding(horizontal = 24.dp)) { Dots(line, clock) } else LyricLine(line, i, cur, clock, vm.lyrTrMap[line.text.trim()].takeIf { vm.lyrTr }) { vm.seek((line.start * 1000).toLong()) }
                }
                item { Box(Modifier.padding(horizontal = 24.dp)) { Credit(l.credit) } }
            }
        }
        else -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text(tr("Текста не нашлось", "No lyrics found", "Тексту не знайшлося", "Text sa nenašiel"), color = Pal.dim)
        }
    }
}

/** Строка текста. Пока её поют — рисуется по буквам: заливка идёт внутри буквы, у края бежит волна
 *  (буква приподнимается и раздувается до +12%) с ореолом, спетые оседают — как на ПК.
 *  Вся строка всегда рисуется тут вручную одними и теми же цветами: до начала — «неспетым», после — белым,
 *  поэтому на стыках с режимом букв ничего не скачет */
@Composable
private fun LyricLine(line: LyrLine, idx: Int, cur: State<Int>, clock: LyrClock, tr: String?, onClick: () -> Unit) {
    // как на ПК: подсвечена строка, за которой держится прокрутка, и те, что звучат после неё (бэк-вокал,
    // дуэт). Спетая гаснет сразу, как началась следующая, — раньше держалась ещё до конца своих таймингов + 0.6 с.
    // Следующая загорается за 0.4 с до начала, чтобы к первой букве уже стоять яркой
    val active by remember(line) { derivedStateOf { clock.pos >= line.start && clock.pos < line.end } }
    val soon by remember(line) { derivedStateOf { clock.pos >= line.start - 0.4 && clock.pos < line.start } }
    val lit = idx == cur.value || (active && idx > cur.value) || (soon && idx == cur.value + 1)
    val past = !lit && idx < cur.value
    // будущие рисуются цветом «неспетого» (0.5), поэтому их слой ярче — на экране выходит одинаково тускло
    // подсветка, масштаб и размытие меняются в такт прокрутке — той же кривой и почти за то же время
    val alpha by animateFloatAsState(if (lit) 1f else if (past) 0.36f else 0.72f, tween(LYR_MOVE_MS - 150, easing = LyrEase), label = "a")
    val zoom by animateFloatAsState(if (lit) 1f else 0.96f, tween(LYR_MOVE_MS, easing = LyrEase), label = "z")
    // как у Apple: чем дальше строка от звучащей, тем сильнее размыта (Android 12+, ниже просто без размытия)
    // загоревшаяся строка теряет размытие сразу: пока оно сходило, свечение букв резалось квадратами
    val blurDp by animateFloatAsState(if (lit) 0f else min(abs(idx - cur.value), 4) * 1.6f, if (lit) snap() else tween(LYR_MOVE_MS, easing = LyrEase), label = "b")
    val style = remember(line.bg, line.right) {
        TextStyle(
            fontSize = if (line.bg) 19.sp else 30.sp,
            lineHeight = if (line.bg) 24.sp else 37.sp,
            fontWeight = FontWeight.ExtraBold,
            textAlign = if (line.right) TextAlign.End else TextAlign.Start,
        )
    }
    val measurer = rememberTextMeasurer()
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    // каждая буква меряется один раз, когда строку впервые запели
    val glyphs = remember(line.text, style) {
        lazy {
            val out = ArrayList<Pair<Int, TextLayoutResult>>()
            var i = 0
            while (i < line.text.length) {
                val n = Character.charCount(line.text.codePointAt(i))
                if (!line.text[i].isWhitespace()) out += i to measurer.measure(line.text.substring(i, i + n), style)
                i += n
            }
            out
        }
    }
    // ореол: копии буквы по кругу на двух радиусах
    val halo = with(LocalDensity.current) {
        remember { listOf(1.5.dp.toPx(), 3.5.dp.toPx()).flatMap { r -> (0 until 8).map { Offset(r * cos(it * PI / 4).toFloat(), r * sin(it * PI / 4).toFloat()) } } }
    }
    val unsung = Color.White.copy(alpha = 0.5f)
    // показанный край заливки: [край, позиция трека на прошлом кадре]
    val shown = remember(line) { doubleArrayOf(-1.0, 0.0) }
    Column(
        Modifier
            .fillMaxWidth()
            .padding(top = if (line.bg) 2.dp else 12.dp, bottom = if (line.bg) 6.dp else 12.dp)
            .graphicsLayer {
                this.alpha = alpha
                scaleX = zoom
                scaleY = zoom
                transformOrigin = TransformOrigin(if (line.right) 1f else 0f, 0.5f)
            }
            // звучащей строке слой размытия не нужен: с ним даже при радиусе около нуля свечение букв резалось квадратами
            .then(if (blurDp > 0.3f) Modifier.blur(blurDp.dp, BlurredEdgeTreatment.Unbounded) else Modifier)
            .clickable(onClick = onClick)
            .padding(horizontal = 24.dp),
    ) {
    Text(
        line.text,
        style = style,
        color = unsung,
        onTextLayout = { layout = it },
        modifier = Modifier
            .fillMaxWidth()
            .drawWithContent {
                val lay = layout ?: return@drawWithContent drawContent()
                val pos = clock.pos
                if (pos < line.start) return@drawWithContent drawText(lay, unsung)
                if (pos >= line.end + 0.6) return@drawWithContent drawText(lay, Color.White)
                // край догоняет настоящий плавно (~90 мс): иначе на стыке медленного и быстрого слова заливка
                // резко меняла скорость и перескакивала пробел. Время берём по треку — на паузе стоит.
                // Перемотка или первый кадр — сразу на место
                val target = edgeOf(line, pos)
                val dt = pos - shown[1]
                shown[0] = if (shown[0] < 0 || dt < 0 || dt > 0.5) target else shown[0] + (target - shown[0]) * (1 - exp(-dt / 0.09))
                shown[1] = pos
                val edge = shown[0]
                // после конца строки волна и ореол гаснут за 0.6 с
                val rest = restOf(line, pos) * (1 - ((pos - line.end) / 0.6).coerceIn(0.0, 1.0)).toFloat()
                for ((i, g) in glyphs.value) {
                    val box = lay.getBoundingBox(i)
                    val dc = i + 0.5 - edge
                    val bump = if (dc >= 0) max(0.0, 1 - dc / 1.7) else max(0.0, 1 + dc / 2.9)
                    val swell = ((1 - cos(bump * PI)) / 2).toFloat() * rest
                    val lf = (edge - i).coerceIn(0.0, 1.0).toFloat()
                    val lift = -style.fontSize.toPx() * 0.09f * swell
                    val grow = 1 + 0.12f * swell
                    val gw = g.size.width.toFloat()
                    translate(box.left, lay.getLineTop(lay.getLineForOffset(i)) + lift) {
                        scale(grow, Offset(gw / 2, g.size.height * 0.78f)) {
                            if (swell > 0.02f) {
                                val hc = Color.White.copy(alpha = 0.09f * swell)
                                for (o in halo) drawText(g, hc, topLeft = o)
                            }
                            when {
                                lf <= 0f -> drawText(g, unsung)
                                lf >= 1f -> drawText(g, Color.White)
                                else -> {
                                    // край заливки внутри буквы: слева белое, справа тусклое
                                    clipRect(right = gw * lf) { drawText(g, Color.White) }
                                    clipRect(left = gw * lf) { drawText(g, unsung) }
                                }
                            }
                        }
                    }
                }
            },
    )
    // перевод мелко под строкой — без караоке и анимаций, гаснет вместе со строкой
    if (tr != null) {
        Text(
            tr,
            color = Color.White.copy(alpha = 0.6f),
            fontSize = style.fontSize * 0.55f,
            lineHeight = style.fontSize * 0.55f * 1.35f,
            fontWeight = FontWeight.SemiBold,
            textAlign = style.textAlign,
            modifier = Modifier.fillMaxWidth().padding(top = 2.dp),
        )
    }
    }
}

/** Точки проигрыша: место занимают, только пока проигрыш идёт; заполняются по очереди и дышат */
@Composable
private fun Dots(line: LyrLine, clock: LyrClock) {
    val on by remember(line) { derivedStateOf { clock.pos >= line.start && clock.pos < line.end } }
    AnimatedVisibility(on, enter = expandVertically() + fadeIn(), exit = shrinkVertically() + fadeOut()) {
        Canvas(Modifier.fillMaxWidth().height(48.dp)) {
            val pos = clock.pos
            val p = ((pos - line.start) / max(line.end - line.start, 0.01)).toFloat().coerceIn(0f, 1f)
            // под конец проигрыша точки сжимаются и исчезают — как в Apple Music
            val out = ((1 - p) * 8).coerceIn(0f, 1f)
            val breath = 1 + 0.12f * sin(pos.toFloat() * 3f)
            val r = 6.dp.toPx() * breath * out
            for (j in 0..2) {
                val a = 0.3f + 0.7f * (p * 3 - j).coerceIn(0f, 1f)
                drawCircle(Color.White.copy(alpha = a), r, Offset(r + j * 22.dp.toPx(), size.height / 2))
            }
        }
    }
}

@Composable
private fun Credit(c: String) {
    if (c.isNotEmpty()) Text(tr("Текст: $c", "Lyrics: $c", "Текст: $c", "Text: $c"), color = Color.White.copy(alpha = 0.45f), fontSize = 12.sp, modifier = Modifier.padding(vertical = 16.dp))
}

/** Края области текста как у Apple: строки у верха и низа плавно растворяются, а не обрезаются */
private fun Modifier.fadeEdges(top: androidx.compose.ui.unit.Dp, bottom: androidx.compose.ui.unit.Dp) = this
    .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
    .drawWithContent {
        drawContent()
        val h = size.height.coerceAtLeast(1f)
        val t = (top.toPx() / h).coerceIn(0f, 0.5f)
        val b = (bottom.toPx() / h).coerceIn(0f, 0.5f)
        drawRect(
            Brush.verticalGradient(0f to Color.Transparent, t to Color.Black, 1 - b to Color.Black, 1f to Color.Transparent),
            blendMode = BlendMode.DstIn,
        )
    }
