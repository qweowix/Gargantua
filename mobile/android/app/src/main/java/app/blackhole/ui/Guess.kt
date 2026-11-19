package app.blackhole.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.animateIntAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.Groups
import androidx.compose.material.icons.rounded.MusicNote
import androidx.compose.material.icons.rounded.Person
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Quiz
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.SkipNext
import androidx.compose.material.icons.rounded.Whatshot
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.media3.common.util.UnstableApi
import app.blackhole.AppState
import app.blackhole.LIKED_ART
import app.blackhole.Track
import app.blackhole.tr
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.pow
import kotlin.math.sin

private val LEVEL_NAMES = listOf("0.1", "0.5", "1", "2", "5")
private val LEVEL_POINTS = listOf(100, 60, 40, 25, 10)

// деления кольца — как на ПК: доли 1-1-1-2-4, зазоры по 7°, сверху по часовой
private val SEG_START = floatArrayOf(3.5f, 46.61f, 89.72f, 132.83f, 212.06f)
private val SEG_END = floatArrayOf(39.61f, 82.72f, 125.83f, 205.06f, 356.5f)
private val BAD = Color(0xFFFF5A6E)
private val BAD_TEXT = Color(0xFFFF8A9A)

private fun sinD(d: Float) = sin(d * PI.toFloat() / 180f)
private fun cosD(d: Float) = cos(d * PI.toFloat() / 180f)

/** Светлее акцента — подсветки, и почти белый с оттенком */
private fun Accent.soft() = lerp(c1, Color.White, 0.28f)
private fun Accent.pale() = lerp(c1, Color.White, 0.62f)

/** Секунды, пока экран на виду; в лайт-режиме стоят на нуле — ничего не крутится, кадры не тратятся */
@Composable
private fun rememberSeconds(): State<Float> {
    val lite = LocalLite.current
    val t = remember { mutableFloatStateOf(0f) }
    LaunchedEffect(lite) {
        if (lite) { t.floatValue = 0f; return@LaunchedEffect }
        val start = withFrameNanos { it }
        while (true) withFrameNanos { t.floatValue = (it - start) / 1e9f }
    }
    return t
}

@Composable
private fun Eyebrow(text: String, modifier: Modifier = Modifier) {
    Text(text, color = LocalAccent.current.soft(), fontSize = 11.sp, fontWeight = FontWeight.ExtraBold, letterSpacing = 2.5.sp, modifier = modifier)
}

/**
 * Чёрная дыра игры. Снаружи частицы (пляшут, пока звучит), кольцо из пяти делений — сколько открыто
 * (текущее заливается, пока играет фрагмент), за ним вращается аккреционный диск, в центре горизонт событий:
 * «?», на ответе — обложка крутится пластинкой. Угадал — ударная волна и искры.
 * Всё меняющееся каждый кадр читается в отрисовке (лямбды), а не в композиции
 */
@Composable
private fun GuessOrb(
    size: Dp, t: () -> Float, energy: () -> Float, mode: Int, level: Int = -1, fill: () -> Float = { 1f },
    won: Boolean = false, busy: Boolean = false, spin: Float = 70f, icon: ImageVector? = null,
    cover: String = "", reveal: () -> Float = { 1f }, burst: () -> Float = { 0f }, onClick: (() -> Unit)? = null,
) {
    val a = LocalAccent.current
    val soft = a.soft()
    val pale = a.pale()
    val glow = lerp(a.c1, Color.Black, 0.3f)
    val lost = mode == 2 && !won
    val core = size * 0.44f
    Box(Modifier.size(size).then(if (onClick != null) Modifier.pressable(onClick = onClick) else Modifier), contentAlignment = Alignment.Center) {
        Canvas(Modifier.fillMaxSize()) {
            val s = this.size.minDimension
            val c = center
            val tt = t()
            val e = energy()
            val halo = if (lost) BAD else glow
            drawCircle(Brush.radialGradient(0f to halo.copy(alpha = 0.34f + 0.3f * e), 0.45f to halo.copy(alpha = 0.08f), 1f to Color.Transparent, center = c, radius = s * 0.75f), s * 0.75f, c)
            // частицы по кругу: в тишине чуть дышат, под звук разлетаются и вспыхивают
            for (i in 0 until 44) {
                val ang = i * 360f / 44 + tt * 6f
                val w = (0.5f + 0.5f * sinD(tt * 190 + i * 47)) * (0.5f + 0.5f * sinD(tt * 113 + i * 83))
                val r = s * (0.45f + 0.012f * sinD(tt * 40 + i * 30) + 0.075f * e * w)
                val d = (2f + 3.5f * (0.3f + e * w)) * density
                val col = if (lost) BAD else lerp(a.c1, a.c2, (ang % 360f) / 360f)
                val al = (0.25f + 0.12f * sinD(tt * 60 + i * 50) + 0.6f * e * w).coerceIn(0f, 1f)
                drawCircle(col.copy(alpha = al), d / 2, Offset(c.x + r * sinD(ang), c.y - r * cosD(ang)))
            }
            // кольцо делений
            val th = maxOf(6.dp.toPx(), s * 0.03f)
            val arcR = s * 0.4f - th / 2
            val tl = Offset(c.x - arcR, c.y - arcR)
            val sz = Size(arcR * 2, arcR * 2)
            val grad = Brush.sweepGradient(0f to a.c1, 1f to a.c2, center = c)
            rotate(-90f, c) {
                fun arc(b: Brush, from: Float, to: Float, alpha: Float = 1f, width: Float = th) {
                    if (to - from > 0.05f) drawArc(b, from, to - from, false, tl, sz, alpha = alpha, style = Stroke(width, cap = StrokeCap.Butt))
                }
                val dim = SolidColor(Color.White)
                for (i in 0 until 5) {
                    val sa = SEG_START[i]
                    val ea = SEG_END[i]
                    when {
                        mode == 2 && won -> arc(grad, sa, ea)
                        mode == 2 -> arc(SolidColor(BAD), sa, ea, 0.7f)
                        mode == 1 && i < level -> arc(grad, sa, ea, 0.55f)
                        mode == 1 && i == level -> {
                            val m = sa + (ea - sa) * fill()
                            arc(dim, sa, ea, 0.13f)
                            arc(grad, sa, m, 0.28f, th * 2.4f) // свечение
                            arc(grad, sa, m)
                        }
                        else -> arc(dim, sa, ea, 0.07f)
                    }
                }
                // огонёк бежит по кольцу: заставка, лобби, «готовлю фрагмент»
                if (mode == 0 || busy) rotate(tt * (if (busy) 320f else spin), c) {
                    drawArc(
                        Brush.sweepGradient(0f to Color.Transparent, 0.55f to Color.Transparent, 0.92f to soft.copy(alpha = 0.65f), 0.995f to pale, 1f to Color.Transparent, center = c),
                        0f, 360f, false, tl, sz, style = Stroke(th),
                    )
                }
            }
            // аккреционный диск: медленно вращается, под звук ярче; к центру тает
            val discR = s * 0.33f
            val base = if (lost) BAD else a.c1
            val k = 0.55f + 0.45f * e
            rotate(tt * 26f, c) {
                drawCircle(
                    Brush.sweepGradient(
                        0f to base.copy(alpha = 0.08f * k), 0.19f to base.copy(alpha = 0.8f * k), 0.39f to a.c2.copy(alpha = 0.55f * k),
                        0.57f to glow.copy(alpha = 0.15f * k), 0.79f to soft.copy(alpha = 0.7f * k), 1f to base.copy(alpha = 0.08f * k), center = c,
                    ),
                    discR, c,
                )
            }
            drawCircle(Brush.radialGradient(0f to Color(0xFF030205), 0.6f to Color(0xFF030205), 0.99f to Color(0x00030205), center = c, radius = discR), discR, c)
            // горизонт событий
            val coreR = s * 0.22f
            drawCircle(Brush.radialGradient(0.6f to base.copy(alpha = 0.25f + 0.25f * e), 1f to Color.Transparent, center = c, radius = coreR * 1.45f), coreR * 1.45f, c)
            drawCircle(Brush.radialGradient(0f to Color.Black, 0.7f to Color(0xFF040209), 1f to Color(0xFF0B0716), center = c, radius = coreR), coreR, c)
            drawCircle((if (lost) BAD else soft).copy(alpha = 0.4f + 0.35f * e), coreR, c, style = Stroke(1.5.dp.toPx()))
            // звучит — пять столбиков пляшут
            if (mode == 1 && e > 0.01f) {
                val bw = coreR * 2 * 0.07f
                val gap = coreR * 2 * 0.045f
                val total = 5 * bw + 4 * gap
                for (i in 0 until 5) {
                    val h = 0.25f + 0.75f * abs(sinD(tt * (250 + i * 67) + i * 50))
                    val bh = 4.dp.toPx() + coreR * 2 * 0.42f * h * e
                    val x = c.x - total / 2 + i * (bw + gap)
                    drawRoundRect(Brush.verticalGradient(listOf(a.c2, a.c1), startY = c.y - bh / 2, endY = c.y + bh / 2), Offset(x, c.y - bh / 2), Size(bw, bh), CornerRadius(bw / 2), alpha = e)
                }
            }
            // готовлю — три точки кружат
            if (busy && mode == 1) for (i in 0 until 3) {
                val ang = tt * 300 + i * 120
                drawCircle(listOf(pale, soft, a.c2)[i], coreR * 0.07f * (1 - i * 0.2f), Offset(c.x + coreR * 0.4f * sinD(ang), c.y - coreR * 0.4f * cosD(ang)))
            }
        }
        if (icon == null && mode != 2) Text(
            "?", color = pale, fontSize = (core.value * 0.46f).sp, fontWeight = FontWeight.Black,
            modifier = Modifier.graphicsLayer { alpha = if (mode == 0) 0.9f else if (busy) 0.18f else 1f - energy() },
        )
        if (icon != null) Icon(icon, null, tint = pale, modifier = Modifier.size(core * 0.4f))
        // ответ: обложка проявляется и крутится пластинкой
        if (mode == 2) Box(
            Modifier.size(core).graphicsLayer {
                val r = reveal()
                alpha = r; scaleX = 0.72f + 0.28f * r; scaleY = scaleX
            }.clip(CircleShape).background(Brush.linearGradient(listOf(a.deep, Pal.card))),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Rounded.MusicNote, null, tint = Color(0x40FFFFFF), modifier = Modifier.size(core * 0.36f))
            if (cover == LIKED_ART) LikedArt(Modifier.fillMaxSize())
            else if (cover.isNotEmpty()) CoverImage(cover, Modifier.fillMaxSize().graphicsLayer { rotationZ = t() * 9f })
            Canvas(Modifier.fillMaxSize()) {
                drawCircle(Color(0x1AFFFFFF), this.size.minDimension * 0.33f, style = Stroke(1.dp.toPx()))
                drawCircle(Color(0xFF07060B), this.size.minDimension * 0.065f)
                drawCircle(Color(0x30FFFFFF), this.size.minDimension * 0.065f, style = Stroke(2.dp.toPx()))
            }
        }
        // угадал — ударная волна и искры (поверх обложки)
        Canvas(Modifier.fillMaxSize()) {
            val b = burst()
            if (b <= 0f || b >= 1f) return@Canvas
            val s = this.size.minDimension
            val c = center
            val coreR = s * 0.22f
            val k = 1 - (1 - b).pow(3)
            drawCircle(lerp(a.c2, Color.White, 0.3f).copy(alpha = 0.8f * (1 - b)), coreR + s * 0.475f * k, c, style = Stroke(2.dp.toPx()))
            for (i in 0 until 32) {
                val ang = i * 11.25f + (i * 37 % 13)
                val r = coreR + s * (0.32f + 0.3f * (i * 7 % 5) / 4f) * k
                val d = (6f - 3f * b) * (1 + (i % 3) * 0.4f) * density
                val col = when (i % 4) { 0 -> pale; 1 -> a.c1; 2 -> a.c2; else -> soft }
                drawCircle(col.copy(alpha = 1 - b), d / 2, Offset(c.x + r * sinD(ang), c.y - r * cosD(ang)))
            }
        }
    }
}

/** Плитка плейлиста или артиста: всплывает по очереди */
@Composable
private fun GuessTile(title: String, sub: String, cover: String, round: Boolean, idx: Int, modifier: Modifier, onClick: () -> Unit) {
    val a = LocalAccent.current
    val appear = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        delay(minOf(idx, 14) * 38L)
        appear.animateTo(1f, tween(460, easing = FastOutSlowInEasing))
    }
    val shape = if (round) CircleShape else RoundedCornerShape(20.dp)
    Column(modifier.graphicsLayer { alpha = appear.value; translationY = (1 - appear.value) * 18.dp.toPx() }.pressable(onClick = onClick)) {
        Box(
            Modifier.fillMaxWidth().aspectRatio(1f).shadow(14.dp, shape, ambientColor = a.c1, spotColor = a.c1).clip(shape).background(Brush.linearGradient(listOf(a.deep, Pal.card))),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Rounded.MusicNote, null, tint = Color(0x26FFFFFF), modifier = Modifier.size(34.dp))
            if (cover == LIKED_ART) LikedArt(Modifier.fillMaxSize()) else if (cover.isNotEmpty()) CoverImage(cover, Modifier.fillMaxSize())
            if (!round) Box(
                Modifier.align(Alignment.BottomEnd).padding(10.dp).size(36.dp).shadow(8.dp, CircleShape).clip(CircleShape).background(a.brush()),
                contentAlignment = Alignment.Center,
            ) { Icon(Icons.Rounded.PlayArrow, null, tint = Color.White, modifier = Modifier.size(20.dp)) }
            Box(Modifier.fillMaxSize().border(1.dp, Color(0x14FFFFFF), shape))
        }
        Spacer(Modifier.height(8.dp))
        Text(title, color = Pal.text, fontSize = 14.sp, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis, textAlign = if (round) TextAlign.Center else TextAlign.Start, modifier = Modifier.fillMaxWidth())
        if (sub.isNotEmpty()) Text(sub, color = Pal.faint, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, textAlign = if (round) TextAlign.Center else TextAlign.Start, modifier = Modifier.fillMaxWidth())
    }
}

/** Выбор режима: соло или дуэль */
@Composable
private fun ModeTile(icon: ImageVector, title: String, sub: String, active: Boolean, modifier: Modifier, onClick: () -> Unit) {
    val a = LocalAccent.current
    val shape = RoundedCornerShape(22.dp)
    val edge by animateColorAsState(if (active) a.c1.copy(alpha = 0.75f) else Color(0x14FFFFFF), tween(220), label = "edge")
    val dot by animateDpAsState(if (active) 9.dp else 0.dp, spring(dampingRatio = 0.5f, stiffness = 600f), label = "dot")
    Column(
        modifier.pressable(onClick = onClick).clip(shape)
            .background(if (active) Brush.linearGradient(listOf(a.c1.copy(alpha = 0.28f), a.c2.copy(alpha = 0.1f))) else SolidColor(Color(0x0AFFFFFF)))
            .border(1.dp, edge, shape).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(40.dp).clip(CircleShape).background(if (active) a.brush() else SolidColor(Color(0x14FFFFFF))), contentAlignment = Alignment.Center) {
                Icon(icon, null, tint = if (active) Color.White else Pal.dim, modifier = Modifier.size(20.dp))
            }
            Spacer(Modifier.weight(1f))
            Box(Modifier.size(20.dp).border(2.dp, if (active) a.soft() else Color(0x40FFFFFF), CircleShape), contentAlignment = Alignment.Center) {
                Box(Modifier.size(dot).clip(CircleShape).background(a.soft()))
            }
        }
        Text(title, color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.ExtraBold)
        Text(sub, color = Pal.dim, fontSize = 12.sp, lineHeight = 16.sp, minLines = 2)
    }
}

/** Переключатель с ползунком */
@Composable
private fun Tabs(items: List<String>, current: Int, onPick: (Int) -> Unit) {
    val a = LocalAccent.current
    BoxWithConstraints(Modifier.padding(horizontal = 16.dp).fillMaxWidth().height(44.dp).clip(RoundedCornerShape(22.dp)).background(Color(0x0FFFFFFF)).padding(4.dp)) {
        val w = maxWidth / items.size
        val x by animateDpAsState(w * current, spring(dampingRatio = 0.7f, stiffness = 500f), label = "tab")
        Box(Modifier.offset(x = x).width(w).fillMaxHeight().shadow(8.dp, RoundedCornerShape(18.dp), ambientColor = a.c1, spotColor = a.c1).clip(RoundedCornerShape(18.dp)).background(a.brush()))
        Row(Modifier.fillMaxSize()) {
            items.forEachIndexed { i, s ->
                Box(Modifier.weight(1f).fillMaxHeight().clickable(remember { MutableInteractionSource() }, null) { onPick(i) }, contentAlignment = Alignment.Center) {
                    Text(s, color = if (i == current) Color.White else Pal.dim, fontSize = 13.sp, fontWeight = FontWeight.Bold)
                }
            }
        }
    }
}

/** Счёт, серия, угадано */
@Composable
private fun Stat(value: String, label: String, icon: ImageVector, hot: Boolean = false) {
    val a = LocalAccent.current
    val shape = RoundedCornerShape(16.dp)
    Row(
        Modifier.clip(shape).background(if (hot) a.c1.copy(alpha = 0.18f) else Color(0x0DFFFFFF)).border(1.dp, if (hot) a.c1.copy(alpha = 0.45f) else Color(0x14FFFFFF), shape)
            .padding(start = 10.dp, end = 14.dp, top = 7.dp, bottom = 7.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(icon, null, tint = if (hot) a.c2 else Pal.dim, modifier = Modifier.size(16.dp))
        Column {
            Text(value, color = if (hot) a.c2 else Pal.text, fontSize = 16.sp, fontWeight = FontWeight.Black, lineHeight = 18.sp)
            Text(label, color = Pal.faint, fontSize = 10.sp, fontWeight = FontWeight.Bold, lineHeight = 12.sp)
        }
    }
}

/** Ошибка или «не то» — красная плашка */
@Composable
private fun BadPill(text: String, modifier: Modifier = Modifier) {
    val shape = RoundedCornerShape(14.dp)
    Row(
        modifier.fillMaxWidth().clip(shape).background(BAD.copy(alpha = 0.11f)).border(1.dp, BAD.copy(alpha = 0.33f), shape).padding(horizontal = 12.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(Icons.Rounded.Close, null, tint = BAD_TEXT, modifier = Modifier.size(16.dp))
        Text(text, color = Color(0xFFFFB3BF), fontSize = 13.sp)
    }
}

@UnstableApi
@Composable
fun GuessScreen(vm: AppState) {
    val g = vm.guess
    DisposableEffect(Unit) { onDispose { g.release() } }
    val t = rememberSeconds()
    Box(Modifier.fillMaxSize()) {
        GuessSky(t)
        when (g.stage) {
            0 -> GuessSetup(vm, t)
            3 -> GuessLobby(vm, t)
            else -> GuessRound(vm, t)
        }
    }
}

/** Фон: три пятна цветов темы медленно плывут */
@Composable
private fun GuessSky(t: State<Float>) {
    val a = LocalAccent.current
    val glow = lerp(a.c1, Color.Black, 0.3f)
    Canvas(Modifier.fillMaxSize()) {
        val tt = t.value
        val big = maxOf(size.width, size.height)
        listOf(a.c1, a.c2, glow).forEachIndexed { i, col ->
            val r = big * (if (i == 2) 0.48f else 0.4f)
            val cx = size.width * (if (i == 0) 0.1f else if (i == 1) 0.9f else 0.45f) + size.width * 0.08f * sinD(tt * (7 + i * 3) + i * 120)
            val cy = size.height * (if (i == 0) 0.12f else if (i == 1) 0.8f else 0.5f) + size.height * 0.06f * cosD(tt * (5 + i * 2) + i * 70)
            drawCircle(Brush.radialGradient(0f to col.copy(alpha = if (i == 2) 0.11f else 0.13f), 0.55f to col.copy(alpha = 0.04f), 1f to Color.Transparent, center = Offset(cx, cy), radius = r), r, Offset(cx, cy))
        }
    }
}

@UnstableApi
@Composable
private fun GuessSetup(vm: AppState, t: State<Float>) {
    val g = vm.guess
    val a = LocalAccent.current
    var aq by remember { mutableStateOf("") }
    var code by remember { mutableStateOf("") }
    val pls = vm.playlists.filter { it.tracks.size >= 3 }
    val busyDots by remember { derivedStateOf { listOf("", ".", "..", "...")[((t.value * 2.5f).toInt()) % 4] } }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = LocalBottomPad.current)) {
        item {
            Box(Modifier.statusBarsPadding().padding(start = 68.dp, top = 12.dp).height(44.dp), contentAlignment = Alignment.CenterStart) {
                Eyebrow(tr("ИГРА", "GAME", "ГРА", "HRA"))
            }
            // шапка с шаром
            Box(
                Modifier.padding(horizontal = 16.dp, vertical = 8.dp).fillMaxWidth().height(210.dp).clip(RoundedCornerShape(28.dp))
                    .background(Brush.linearGradient(listOf(a.deep, Color(0xFF0D0A16), lerp(a.c2, Color.Black, 0.78f))))
                    .border(1.dp, Color(0x14FFFFFF), RoundedCornerShape(28.dp)),
            ) {
                Box(Modifier.align(Alignment.CenterEnd).offset(x = 70.dp)) {
                    GuessOrb(230.dp, { t.value }, { 0.3f + 0.25f * sinD(t.value * 50) }, mode = 0)
                }
                Box(Modifier.fillMaxSize().background(Brush.horizontalGradient(listOf(Color(0x80000000), Color(0x30000000), Color.Transparent))))
                Column(Modifier.align(Alignment.CenterStart).fillMaxWidth(0.66f).padding(start = 20.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(tr("Угадай песню", "Guess the song", "Вгадай пісню", "Uhádni pieseň"), color = Color.White, fontSize = 30.sp, lineHeight = 34.sp, fontWeight = FontWeight.Black)
                    Text(
                        tr("Случайная песня из плейлиста или артиста: 0.1 с, потом 0.5, 1, 2 и 5. Чем раньше узнаешь — тем больше очков.", "A random song from a playlist or artist: 0.1 s, then 0.5, 1, 2 and 5. The sooner you know it, the more points.", "Випадкова пісня з плейлиста або артиста: 0.1 с, потім 0.5, 1, 2 і 5. Що раніше впізнаєш — то більше очок.", "Náhodná pieseň z playlistu alebo interpreta: 0,1 s, potom 0,5, 1, 2 a 5. Čím skôr ju spoznáš, tým viac bodov."),
                        color = Pal.dim, fontSize = 12.sp, lineHeight = 16.sp,
                    )
                    if (g.rounds > 0) Row(Modifier.padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Stat("${g.score}", tr("очки", "points", "очки", "body"), Icons.Rounded.AutoAwesome, hot = true)
                        Stat("${g.best}", tr("рекорд", "best", "рекорд", "rekord"), Icons.Rounded.Whatshot)
                    }
                }
            }
            if (g.error.isNotEmpty()) BadPill(g.error, Modifier.padding(horizontal = 16.dp, vertical = 6.dp))
            Eyebrow(tr("РЕЖИМ", "MODE", "РЕЖИМ", "REŽIM"), Modifier.padding(start = 20.dp, top = 14.dp, bottom = 10.dp))
            Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                ModeTile(Icons.Rounded.Person, tr("Соло", "Solo", "Соло", "Sólo"), tr("Очки за скорость, серии подряд", "Points for speed, streaks", "Очки за швидкість, серії поспіль", "Body za rýchlosť, série"), g.kind == 0, Modifier.weight(1f)) { g.kind = 0 }
                ModeTile(Icons.Rounded.Groups, tr("1 на 1", "1 vs 1", "1 на 1", "1 na 1"), tr("Дуэль с другом: кто первый — тот и взял", "Duel a friend: first to guess wins", "Дуель із другом: хто перший — той і взяв", "Duel s kamarátom: kto prvý, ten vyhrá"), g.kind == 1, Modifier.weight(1f)) { g.kind = 1 }
            }
        }
        if (g.kind == 1) item {
            Column(
                Modifier.padding(16.dp).fillMaxWidth().clip(RoundedCornerShape(22.dp)).background(Color(0x0AFFFFFF)).border(1.dp, Color(0x14FFFFFF), RoundedCornerShape(22.dp)).padding(14.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Text(tr("Есть код от друга — входи; нет — выбери источник ниже, и код появится у тебя. Фрагмент длиннее, только когда «дальше» нажали оба.", "Got a code from a friend — join; otherwise pick a source below and you'll get a code. The clip gets longer only when both press “next”.", "Є код від друга — заходь; нема — обери джерело нижче, і код з'явиться в тебе. Фрагмент довший, лише коли «далі» натиснули обидва.", "Máš kód od kamaráta — vstúp; nie — vyber zdroj nižšie a kód dostaneš ty. Úryvok sa predĺži, až keď obaja stlačia „ďalej“."), color = Pal.dim, fontSize = 12.sp, lineHeight = 16.sp)
                GlassField(code, { code = it.uppercase() }, Modifier.fillMaxWidth(), placeholder = { Text(tr("Код дуэли", "Duel code", "Код дуелі", "Kód duelu")) }, singleLine = true, keyboardActions = KeyboardActions(onDone = { g.joinDuel(code) }))
                Pill(tr("Войти", "Join", "Увійти", "Vstúpiť"), enabled = code.isNotBlank() && !g.busy) { g.joinDuel(code) }
            }
        }
        item {
            Row(Modifier.padding(start = 20.dp, end = 16.dp, top = 18.dp, bottom = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                Eyebrow(tr("ОТКУДА ПЕСНИ", "SONGS FROM", "ЗВІДКИ ПІСНІ", "ODKIAĽ PIESNE"), Modifier.weight(1f))
                if (g.busy) Text(tr("Собираю песни", "Gathering songs", "Збираю пісні", "Zbieram piesne") + busyDots, color = a.soft(), fontSize = 13.sp, fontWeight = FontWeight.Bold)
            }
            Tabs(listOf(tr("Плейлист", "Playlist", "Плейлист", "Playlist"), tr("Артист", "Artist", "Артист", "Interpret")), g.mode) { g.mode = it }
            Spacer(Modifier.height(14.dp))
        }
        if (g.mode == 0) {
            if (pls.isEmpty()) item { Dim(tr("Плейлистов пока нет — нужен хотя бы один на три песни", "No playlists yet — you need one with at least three songs", "Плейлистів поки немає — потрібен хоча б один на три пісні", "Zatiaľ žiadne playlisty — treba aspoň jeden s tromi skladbami"), Modifier.padding(20.dp)) }
            itemsIndexed(pls.chunked(2)) { row, pair ->
                Row(Modifier.padding(horizontal = 16.dp, vertical = 7.dp), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    pair.forEachIndexed { j, p ->
                        GuessTile(
                            p.name, tr("${p.tracks.size} треков", "${p.tracks.size} tracks", "${p.tracks.size} треків", "${p.tracks.size} skladieb"),
                            p.tracks.firstOrNull { it.cover.isNotEmpty() }?.cover ?: "", false, row * 2 + j, Modifier.weight(1f),
                        ) { g.pickPlaylist(p) }
                    }
                    if (pair.size == 1) Spacer(Modifier.weight(1f))
                }
            }
        }
        if (g.mode == 1) {
            item {
                GlassField(aq, { aq = it; g.searchArtists(it) }, Modifier.padding(horizontal = 16.dp).fillMaxWidth(), placeholder = { Text(tr("Имя артиста", "Artist name", "Ім'я артиста", "Meno interpreta")) }, leadingIcon = { Icon(Icons.Rounded.Search, null) }, singleLine = true)
                Spacer(Modifier.height(12.dp))
            }
            itemsIndexed(g.artists.chunked(3)) { row, three ->
                Row(Modifier.padding(horizontal = 16.dp, vertical = 7.dp), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    three.forEachIndexed { j, c -> GuessTile(c.title, c.sub, c.cover, true, row * 3 + j, Modifier.weight(1f)) { g.pickArtist(c) } }
                    repeat(3 - three.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
}

/** Буква кода дуэли: плашки выпрыгивают по очереди */
@Composable
private fun CodeChar(ch: String, idx: Int) {
    val a = LocalAccent.current
    val k = remember { Animatable(0f) }
    LaunchedEffect(ch) {
        k.snapTo(0f)
        delay(idx * 70L)
        k.animateTo(1f, spring(dampingRatio = 0.45f, stiffness = 300f))
    }
    val shape = RoundedCornerShape(14.dp)
    Box(
        Modifier.graphicsLayer {
            alpha = k.value.coerceIn(0f, 1f); translationY = (1 - k.value) * 22.dp.toPx(); scaleX = 0.8f + 0.2f * k.value; scaleY = scaleX
        }.size(44.dp, 58.dp).shadow(10.dp, shape, ambientColor = a.c1, spotColor = a.c1).clip(shape)
            .background(Brush.verticalGradient(listOf(Color(0x24FFFFFF), Color(0x0AFFFFFF)))).border(1.dp, a.c1.copy(alpha = 0.55f), shape),
        contentAlignment = Alignment.Center,
    ) { Text(ch, color = Color.White, fontSize = 28.sp, fontWeight = FontWeight.Black) }
}

@UnstableApi
@Composable
private fun GuessLobby(vm: AppState, t: State<Float>) {
    val g = vm.guess
    val dots by remember { derivedStateOf { listOf("", ".", "..", "...")[((t.value * 2.5f).toInt()) % 4] } }
    Column(Modifier.fillMaxSize().statusBarsPadding().padding(horizontal = 24.dp, vertical = 28.dp), verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically), horizontalAlignment = Alignment.CenterHorizontally) {
        GuessOrb(200.dp, { t.value }, { 0.35f + 0.2f * sinD(t.value * 70) }, mode = 0, spin = 150f, icon = Icons.Rounded.Groups)
        Eyebrow(tr("КОД ДУЭЛИ", "DUEL CODE", "КОД ДУЕЛІ", "KÓD DUELU"))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { g.duelCode.forEachIndexed { i, c -> CodeChar(c.toString(), i) } }
        Text((if (g.host) tr("Ждём соперника", "Waiting for your opponent", "Чекаємо суперника", "Čakáme na súpera") else tr("Ждём начала", "Waiting for the start", "Чекаємо початку", "Čakáme na začiatok")) + dots, color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.Bold)
        if (g.host) Text(tr("Отправь его сопернику: «Угадай песню» → «1 на 1» → «Войти». Как только он войдёт, начнётся первый раунд.", "Send it to your opponent: “Guess the song” → “1 vs 1” → “Join”. The first round starts as soon as they join.", "Надішли його суперникові: «Вгадай пісню» → «1 на 1» → «Увійти». Щойно він зайде, почнеться перший раунд.", "Pošli ho súperovi: „Uhádni pieseň“ → „1 na 1“ → „Vstúpiť“. Prvé kolo začne, hneď ako vstúpi."), color = Pal.dim, fontSize = 13.sp, textAlign = TextAlign.Center)
        Text(g.source, color = Pal.faint, fontSize = 12.sp)
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Pill(tr("Скопировать", "Copy", "Скопіювати", "Kopírovať"), icon = Icons.Rounded.ContentCopy) { vm.copyText(g.duelCode) }
            Pill(tr("Отменить", "Cancel", "Скасувати", "Zrušiť"), primary = false) { g.change() }
        }
    }
}

/** Найденный трек в ответе: въезжает справа */
@Composable
private fun ResultRow(t: Track, idx: Int, onClick: () -> Unit) {
    val appear = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        delay(minOf(idx, 10) * 26L)
        appear.animateTo(1f, tween(300, easing = FastOutSlowInEasing))
    }
    val shape = RoundedCornerShape(16.dp)
    Row(
        Modifier.padding(horizontal = 16.dp, vertical = 3.dp).graphicsLayer { alpha = appear.value; translationX = (1 - appear.value) * 28.dp.toPx() }
            .fillMaxWidth().pressable(onClick = onClick).clip(shape).background(Color(0x0AFFFFFF)).border(1.dp, Color(0x0FFFFFFF), shape).padding(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Cover(t.cover, 44.dp, 10.dp)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(t.title, color = Pal.text, fontSize = 14.sp, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (t.artist.isNotEmpty()) Text(t.artist, color = Pal.faint, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Icon(Icons.Rounded.Check, null, tint = Pal.faint, modifier = Modifier.padding(end = 6.dp).size(18.dp))
    }
}

@UnstableApi
@Composable
private fun GuessRound(vm: AppState, t: State<Float>) {
    val g = vm.guess
    val a = LocalAccent.current
    val lite = LocalLite.current
    var q by remember { mutableStateOf("") }
    val orb = minOf(LocalConfiguration.current.screenWidthDp.dp - 90.dp, 300.dp)

    // звук: кольцо заливается за длину фрагмента, пока звучит — частицы пляшут
    val fill = remember { Animatable(0f) }
    var sounding by remember { mutableStateOf(false) }
    LaunchedEffect(g.playN, g.ready, g.stage) {
        if (g.stage != 1) { sounding = false; return@LaunchedEffect }
        fill.snapTo(0f)
        if (!g.ready || g.clipMs <= 0) { sounding = false; return@LaunchedEffect }
        sounding = true
        delay(80)
        fill.animateTo(1f, tween(g.clipMs, easing = LinearEasing))
        delay(250)
        sounding = false
    }
    val energy by animateFloatAsState(if (sounding || g.stage == 2) 1f else 0f, tween(420), label = "energy")
    // ответ раскрыт: обложка проявляется, угадал — искры
    val reveal = remember { Animatable(1f) }
    val burst = remember { Animatable(0f) }
    LaunchedEffect(g.stage) {
        if (g.stage != 2) return@LaunchedEffect
        reveal.snapTo(0f); burst.snapTo(0f)
        launch { reveal.animateTo(1f, tween(900, easing = FastOutSlowInEasing)) }
        if (g.won && !lite) { delay(120); burst.animateTo(1f, tween(1300, easing = LinearEasing)) }
    }
    // неверный ответ — поле вздрагивает
    var shake by remember { mutableFloatStateOf(0f) }
    LaunchedEffect(g.wrongN) {
        if (g.wrongN == 0) return@LaunchedEffect
        animate(0f, 1f, animationSpec = tween(520, easing = LinearEasing)) { v, _ -> shake = 11f * sinD(v * 520f * 4.2f) * (1 - v) }
        shake = 0f
    }
    val score by animateIntAsState(g.score, tween(900), label = "score")
    val me by animateIntAsState(g.meScore, tween(900), label = "me")
    val opp by animateIntAsState(g.oppScore, tween(900), label = "opp")
    val gained by animateIntAsState(if (g.stage == 2 && g.won) g.gained else 0, tween(900, delayMillis = 250), label = "gained")

    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = LocalBottomPad.current)) {
        item {
            Row(Modifier.statusBarsPadding().padding(start = 68.dp, end = 16.dp, top = 12.dp).heightIn(min = 44.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Eyebrow(tr("УГАДАЙ ПЕСНЮ", "GUESS THE SONG", "ВГАДАЙ ПІСНЮ", "UHÁDNI PIESEŇ"))
                    Text(g.source, color = Color.White, fontSize = 17.sp, fontWeight = FontWeight.ExtraBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Pill(tr("Сменить", "Change", "Змінити", "Zmeniť"), primary = false) { g.change() }
            }
            Spacer(Modifier.height(10.dp))
            if (g.kind == 0) Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally)) {
                Stat("$score", tr("очки", "points", "очки", "body"), Icons.Rounded.AutoAwesome, hot = true)
                Stat("${g.streak}", tr("серия", "streak", "серія", "séria"), Icons.Rounded.Whatshot, hot = g.streak >= 3)
                Stat("${g.hits}/${g.rounds}", tr("угадано", "guessed", "вгадано", "uhádnuté"), Icons.Rounded.Check)
            } else Row(
                Modifier.padding(horizontal = 16.dp).fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(Color(0x0DFFFFFF)).border(1.dp, Color(0x14FFFFFF), RoundedCornerShape(18.dp)).padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f), horizontalAlignment = Alignment.End) {
                    Text(tr("ТЫ", "YOU", "ТИ", "TY"), color = a.soft(), fontSize = 10.sp, fontWeight = FontWeight.ExtraBold, letterSpacing = 1.5.sp)
                    Text("$me", color = Color.White, fontSize = 22.sp, fontWeight = FontWeight.Black)
                }
                Box(Modifier.padding(horizontal = 14.dp).size(34.dp).shadow(8.dp, CircleShape, ambientColor = a.c1, spotColor = a.c1).clip(CircleShape).background(a.brush()), contentAlignment = Alignment.Center) {
                    Text("VS", color = Color.White, fontSize = 11.sp, fontWeight = FontWeight.Black)
                }
                Column(Modifier.weight(1f)) {
                    Text(g.oppName.uppercase(), color = a.c2, fontSize = 10.sp, fontWeight = FontWeight.ExtraBold, letterSpacing = 0.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text("$opp", color = Color.White, fontSize = 22.sp, fontWeight = FontWeight.Black)
                }
            }
        }
        item {
            Box(Modifier.fillMaxWidth().padding(vertical = 10.dp), contentAlignment = Alignment.Center) {
                GuessOrb(
                    orb, { t.value }, { energy }, mode = if (g.stage == 2) 2 else 1, level = g.level, fill = { fill.value },
                    won = g.won, busy = g.stage == 1 && !g.ready, cover = g.cur?.cover ?: "", reveal = { reveal.value }, burst = { burst.value },
                    onClick = if (g.stage == 1) ({ g.play() }) else null,
                )
            }
            // пять делений подписями
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally)) {
                LEVEL_NAMES.forEachIndexed { i, l ->
                    val cur = i == g.level
                    Box(
                        Modifier.size(52.dp, 26.dp).clip(RoundedCornerShape(13.dp))
                            .background(if (cur) (if (g.stage == 2 && !g.won) SolidColor(BAD.copy(alpha = 0.3f)) else a.brush()) else SolidColor(if (i < g.level) Color(0x14FFFFFF) else Color(0x08FFFFFF))),
                        contentAlignment = Alignment.Center,
                    ) { Text("$l ${tr("с", "s", "с", "s")}", color = if (cur) Color.White else if (i < g.level) Pal.text else Pal.faint, fontSize = 11.sp, fontWeight = FontWeight.ExtraBold) }
                }
            }
            Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 12.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(
                    when {
                        g.stage == 1 && !g.ready -> tr("Готовлю фрагмент…", "Preparing the clip…", "Готую фрагмент…", "Pripravujem úryvok…")
                        g.stage == 1 -> tr("Фрагмент ${LEVEL_NAMES[g.level]} с", "Clip ${LEVEL_NAMES[g.level]} s", "Фрагмент ${LEVEL_NAMES[g.level]} с", "Úryvok ${LEVEL_NAMES[g.level]} s")
                        g.won -> tr("Узнал за ${LEVEL_NAMES[g.level]} с", "Got it in ${LEVEL_NAMES[g.level]} s", "Впізнав за ${LEVEL_NAMES[g.level]} с", "Spoznal si za ${LEVEL_NAMES[g.level]} s")
                        else -> tr("Вот что это было", "Here's what it was", "Ось що це було", "Toto to bolo")
                    },
                    color = Color.White, fontSize = 18.sp, fontWeight = FontWeight.ExtraBold,
                )
                val duelNote = g.kind == 1 && g.stage == 1 && (g.meSkip || g.oppSkip)
                Text(
                    when {
                        g.stage == 2 -> tr("Песня играет целиком — до следующей", "The song plays in full until the next one", "Пісня грає повністю — до наступної", "Pieseň hrá celá — do ďalšej")
                        g.kind == 1 && g.meSkip -> tr("Ждём, пока «дальше» нажмёт соперник", "Waiting for your opponent to press “next”", "Чекаємо, поки «далі» натисне суперник", "Čakáme, kým súper stlačí „ďalej“")
                        g.kind == 1 && g.oppSkip -> tr("Соперник хочет фрагмент подлиннее", "Your opponent wants a longer clip", "Суперник хоче довший фрагмент", "Súper chce dlhší úryvok")
                        else -> tr("Угадаешь сейчас — +${LEVEL_POINTS[g.level]} · нажми на шар, чтобы переслушать", "Guess now — +${LEVEL_POINTS[g.level]} · tap the orb to replay", "Вгадаєш зараз — +${LEVEL_POINTS[g.level]} · торкнись кулі, щоб переслухати", "Uhádneš teraz — +${LEVEL_POINTS[g.level]} · ťukni na guľu a vypočuj znova")
                    },
                    color = if (duelNote) a.soft() else Pal.dim, fontSize = 12.sp, textAlign = TextAlign.Center,
                )
            }
        }
        if (g.stage == 1) {
            item {
                Row(Modifier.fillMaxWidth().padding(bottom = 14.dp), horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterHorizontally)) {
                    Pill(tr("Ещё раз", "Again", "Ще раз", "Znova"), icon = Icons.Rounded.PlayArrow, primary = false, enabled = g.ready) { g.play() }
                    Pill(
                        if (g.level < 4) tr("Не знаю — дальше", "Don't know — next", "Не знаю — далі", "Neviem — ďalej") else tr("Сдаюсь", "Give up", "Здаюся", "Vzdávam"),
                        icon = if (g.level < 4) Icons.Rounded.SkipNext else null, primary = false, enabled = !(g.kind == 1 && g.meSkip),
                    ) { g.skip() }
                }
                GlassField(
                    q, { q = it; g.search(it) }, Modifier.padding(horizontal = 16.dp).fillMaxWidth().graphicsLayer { translationX = shake.dp.toPx() },
                    placeholder = { Text(tr("Название трека…", "Track title…", "Назва треку…", "Názov skladby…")) }, leadingIcon = { Icon(Icons.Rounded.Search, null) }, singleLine = true,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done), keyboardActions = KeyboardActions(onDone = { g.typed(q); q = "" }),
                )
                if (g.wrong.isNotEmpty()) BadPill(g.wrong, Modifier.padding(horizontal = 16.dp, vertical = 8.dp).graphicsLayer { translationX = shake * 0.6f * density })
                if (q.isEmpty()) Column(Modifier.fillMaxWidth().padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Box(Modifier.size(56.dp).clip(CircleShape).background(Color(0x0DFFFFFF)).border(1.dp, Color(0x14FFFFFF), CircleShape), contentAlignment = Alignment.Center) {
                        Icon(Icons.Rounded.Quiz, null, tint = a.soft(), modifier = Modifier.size(26.dp))
                    }
                    Text(
                        tr("Начни писать название — ищу только среди ${g.poolN} песен игры", "Start typing the title — searching only the game's ${g.poolN} songs", "Почни писати назву — шукаю лише серед ${g.poolN} пісень гри", "Začni písať názov — hľadám len medzi ${g.poolN} piesňami hry"),
                        color = Pal.faint, fontSize = 12.sp, textAlign = TextAlign.Center,
                    )
                }
                else if (g.results.isEmpty()) Dim(tr("Среди песен игры такого нет — попробуй иначе", "Nothing like that among the game's songs — try another way", "Серед пісень гри такого немає — спробуй інакше", "Medzi piesňami hry nič také nie je — skús inak"), Modifier.padding(20.dp))
                Spacer(Modifier.height(6.dp))
            }
            itemsIndexed(g.results.distinctBy { it.key }, key = { _, t -> t.key }) { i, tk -> ResultRow(tk, i) { g.pick(tk); q = "" } }
        }
        if (g.stage == 2) item {
            val tk = g.cur
            val shape = RoundedCornerShape(26.dp)
            Column(
                Modifier.padding(horizontal = 16.dp).fillMaxWidth().graphicsLayer { alpha = reveal.value; translationY = (1 - reveal.value) * 26.dp.toPx() }
                    .clip(shape).background(Color(0xD0120F1C)).border(1.dp, Color(0x14FFFFFF), shape).padding(20.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Text(
                    when {
                        g.kind == 1 && g.won -> tr("Ты был первым!", "You were first!", "Ти був першим!", "Bol si prvý!")
                        g.kind == 1 && g.winner.isNotEmpty() -> tr("Раунд взял(а) ${g.winner}", "${g.winner} took the round", "Раунд узяв(ла) ${g.winner}", "Kolo vyhral(a) ${g.winner}")
                        g.kind == 1 -> tr("Никто не угадал", "Nobody got it", "Ніхто не вгадав", "Nikto neuhádol")
                        g.won -> tr("Угадал!", "Got it!", "Вгадав!", "Uhádol si!")
                        else -> tr("Не угадал", "Missed", "Не вгадав", "Neuhádol")
                    },
                    color = if (g.won) a.c2 else BAD_TEXT, fontSize = 30.sp, lineHeight = 34.sp, fontWeight = FontWeight.Black,
                )
                if (g.kind == 0 && g.won) Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Row(
                        Modifier.shadow(10.dp, RoundedCornerShape(17.dp), ambientColor = a.c1, spotColor = a.c1).clip(RoundedCornerShape(17.dp)).background(a.brush()).padding(horizontal = 14.dp, vertical = 7.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Icon(Icons.Rounded.AutoAwesome, null, tint = Color.White, modifier = Modifier.size(14.dp))
                        Text(tr("+$gained очков", "+$gained points", "+$gained очок", "+$gained bodov"), color = Color.White, fontSize = 15.sp, fontWeight = FontWeight.Black)
                    }
                    if (g.streak >= 2) Row(
                        Modifier.clip(RoundedCornerShape(17.dp)).background(Color(0x0DFFFFFF)).border(1.dp, a.c2.copy(alpha = 0.5f), RoundedCornerShape(17.dp)).padding(horizontal = 12.dp, vertical = 7.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Icon(Icons.Rounded.Whatshot, null, tint = a.c2, modifier = Modifier.size(15.dp))
                        Text(tr("серия ${g.streak}", "streak ${g.streak}", "серія ${g.streak}", "séria ${g.streak}"), color = a.c2, fontSize = 13.sp, fontWeight = FontWeight.ExtraBold)
                    }
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(Brush.horizontalGradient(listOf(Color(0x24FFFFFF), Color.Transparent))))
                Text(tk?.title ?: "", color = Color.White, fontSize = 22.sp, lineHeight = 26.sp, fontWeight = FontWeight.ExtraBold, maxLines = 3, overflow = TextOverflow.Ellipsis)
                Text(tk?.artist ?: "", color = Pal.dim, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (g.kind == 0) Row(Modifier.padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Stat("${g.best}", tr("рекорд", "best", "рекорд", "rekord"), Icons.Rounded.Whatshot)
                    Stat("${g.hits}/${g.rounds}", tr("угадано", "guessed", "вгадано", "uhádnuté"), Icons.Rounded.Check)
                }
                Box(Modifier.padding(top = 6.dp)) {
                    if (g.kind == 0 || g.host) Pill(tr("Следующая", "Next", "Наступна", "Ďalšia"), icon = Icons.Rounded.SkipNext) { g.nextRound() }
                    else Text(tr("Следующую включит ${g.oppName}", "${g.oppName} starts the next one", "Наступну ввімкне ${g.oppName}", "Ďalšiu pustí ${g.oppName}"), color = Pal.dim, fontSize = 13.sp)
                }
            }
        }
    }
}
