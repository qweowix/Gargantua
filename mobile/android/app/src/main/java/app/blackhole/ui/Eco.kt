package app.blackhole.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.Favorite
import androidx.compose.material.icons.rounded.FavoriteBorder
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material.icons.rounded.KeyboardArrowUp
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.media3.common.util.UnstableApi
import app.blackhole.AppState
import app.blackhole.DnaInfo
import app.blackhole.DnaPick
import app.blackhole.EcoText
import app.blackhole.Room
import app.blackhole.UStar
import app.blackhole.tr
import kotlinx.coroutines.launch
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.pow
import kotlin.math.sin

private fun rgb(c: List<Int>) = Color(c.getOrElse(0) { 160 }, c.getOrElse(1) { 140 }, c.getOrElse(2) { 250 })

private fun originColor(o: Int) = when (o) {
    0 -> Color(0x55E9E4FF)
    1 -> Color(0x6660A5FA)
    else -> Color(0x66FB923C)
}

private fun pct(x: Float) = "${(x * 100).toInt()} %"

private fun dur(ms: Long): String {
    val m = ms / 60_000
    return if (m >= 60) tr("${m / 60} ч ${m % 60} мин", "${m / 60} h ${m % 60} min", "${m / 60} год ${m % 60} хв", "${m / 60} h ${m % 60} min") else tr("$m мин", "$m min", "$m хв", "$m min")
}

/** Карточка-стекло с заголовком — как группы настроек */
@Composable
private fun Card(title: String, sub: String? = null, content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp).fillMaxWidth().liquid(RoundedCornerShape(24.dp)).padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(title, color = Pal.text, fontSize = 17.sp, fontWeight = FontWeight.Bold)
        if (!sub.isNullOrEmpty()) Text(sub, color = Pal.faint, fontSize = 12.sp)
        content()
    }
}

@Composable
private fun Bar(label: String, value: String, frac: Float, sub: String = "", frac2: Float? = null) {
    val a = LocalAccent.current
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.width(130.dp)) {
            Text(label, color = Pal.text, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (sub.isNotEmpty()) Text(sub, color = Pal.faint, fontSize = 10.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Column(Modifier.weight(1f).padding(horizontal = 8.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Box(Modifier.fillMaxWidth().height(6.dp).clip(CircleShape).background(Color(0x14FFFFFF))) {
                Box(Modifier.fillMaxWidth(frac.coerceIn(0.02f, 1f)).height(6.dp).clip(CircleShape).background(a.brush()))
            }
            if (frac2 != null) Box(Modifier.fillMaxWidth().height(6.dp).clip(CircleShape).background(Color(0x14FFFFFF))) {
                Box(Modifier.fillMaxWidth(frac2.coerceIn(0.02f, 1f)).height(6.dp).clip(CircleShape).background(Color(0xFF22D3EE)))
            }
        }
        Text(value, color = Pal.dim, fontSize = 12.sp, modifier = Modifier.width(72.dp), textAlign = androidx.compose.ui.text.style.TextAlign.End)
    }
}

/** Радар Music DNA: восемь осей, до трёх отпечатков друг на друге */
@Composable
fun Radar(polys: List<List<Float>>, modifier: Modifier = Modifier) {
    val a = LocalAccent.current
    val axes = listOf(tr("Энергия", "Energy", "Енергія", "Energia"), tr("Бит", "Beat", "Біт", "Beat"), tr("Темп", "Tempo", "Темп", "Tempo"), tr("Яркость", "Bright", "Яскравість", "Jas"), tr("Шум", "Noise", "Шум", "Šum"), tr("Динамика", "Dynamics", "Динаміка", "Dynamika"), tr("Атаки", "Onsets", "Атаки", "Nástupy"), tr("Низ", "Bass", "Низ", "Basy"))
    val tint = listOf(a.c1, Color(0xFF22D3EE), Color(0xFFFBBF24))
    val px = with(LocalDensity.current) { 11.sp.toPx() }
    Canvas(modifier.aspectRatio(1f)) {
        val c = Offset(size.width / 2, size.height / 2)
        val R = size.minDimension * 0.38f
        fun at(k: Int, r: Float) = Offset(c.x + r * cos(-PI / 2 + k * PI / 4).toFloat(), c.y + r * sin(-PI / 2 + k * PI / 4).toFloat())
        for (ring in 1..3) {
            val p = Path()
            for (k in 0..7) at(k, R * ring / 3).let { if (k == 0) p.moveTo(it.x, it.y) else p.lineTo(it.x, it.y) }
            p.close()
            drawPath(p, Color(0x1FFFFFFF), style = Stroke(1f))
        }
        for (k in 0..7) drawLine(Color(0x1FFFFFFF), c, at(k, R), 1f)
        polys.forEachIndexed { i, v ->
            val p = Path()
            for (k in 0..7) at(k, R * v.getOrElse(k) { 0f }.coerceIn(0.04f, 1f)).let { if (k == 0) p.moveTo(it.x, it.y) else p.lineTo(it.x, it.y) }
            p.close()
            drawPath(p, tint[i % 3].copy(alpha = 0.28f))
            drawPath(p, tint[i % 3], style = Stroke(2.5f))
        }
        val paint = android.graphics.Paint().apply { color = Pal.dim.toArgb(); textSize = px; textAlign = android.graphics.Paint.Align.CENTER; isAntiAlias = true }
        for (k in 0..7) at(k, R * 1.18f).let { drawContext.canvas.nativeCanvas.drawText(axes[k], it.x, it.y + px / 3, paint) }
    }
}

// ════════════════ Вселенная ════════════════

@UnstableApi
@Composable
fun UniverseScreen(vm: AppState) {
    val eco = vm.eco
    LaunchedEffect(Unit) { if (eco.uni == null) eco.buildUniverse() }
    val data = eco.uni
    val scope = rememberCoroutineScope()
    var cx by remember { mutableFloatStateOf(0f) }
    var cy by remember { mutableFloatStateOf(0f) }
    var zoom by remember { mutableFloatStateOf(0.5f) }
    var fitted by remember { mutableStateOf(false) }
    var w by remember { mutableFloatStateOf(1f) }
    var h by remember { mutableFloatStateOf(1f) }
    var bridgeOpen by remember { mutableStateOf(false) }
    var q by remember { mutableStateOf("") }
    val stars = data?.stars ?: emptyList()
    val density = LocalDensity.current.density
    fun fitZoom(bw: Float, bh: Float) = (w / bw.coerceAtLeast(200f)).coerceAtMost(h / bh.coerceAtLeast(200f)).coerceIn(0.08f, 2f)
    fun fit() {
        val b = data?.bounds ?: return
        cx = b[0]; cy = b[1]; zoom = fitZoom(b[2], b[3])
    }
    LaunchedEffect(data, w) { if (data != null && !fitted && w > 1f) { fit(); fitted = true } }
    // плавный перелёт: зум по логарифму, чтобы издалека камера не «ныряла»
    fun flyCam(x: Float, y: Float, z1: Float) {
        val (x0, y0, z0) = Triple(cx, cy, zoom)
        scope.launch {
            Animatable(0f).animateTo(1f, tween(480)) {
                cx = x0 + (x - x0) * value; cy = y0 + (y - y0) * value
                zoom = kotlin.math.exp(kotlin.math.ln(z0) + (kotlin.math.ln(z1) - kotlin.math.ln(z0)) * value)
            }
        }
    }
    fun fly(star: Int) {
        val s = stars.getOrNull(star) ?: return
        flyCam(s.x, s.y, maxOf(zoom, 1.1f))
        eco.pick(star)
    }
    LaunchedEffect(eco.flyTo) { eco.flyTo?.let { fly(it); eco.flyTo = null } }
    LaunchedEffect(eco.flyNeb) { eco.flyNeb?.let { i -> data?.nebulas?.getOrNull(i)?.let { n -> eco.clear(); flyCam(n.x, n.y, (350f / n.r).coerceIn(0.3f, 2f)) }; eco.flyNeb = null } }

    // в фокусе: звёзды моста, иначе выбранная и её связи
    val bridge = eco.brShown?.let { eco.bridges.getOrNull(it) }
    val focus: Set<Int> = remember(eco.sel, eco.info, bridge) {
        bridge?.stars?.toSet() ?: eco.sel?.let { s -> setOf(s) + (eco.info?.links?.map { it.star } ?: emptyList()) } ?: emptySet()
    }
    val cap = when { zoom < 0.4f -> 150; zoom < 0.9f -> 400; else -> 1200 }
    val visible: List<UStar> = remember(data, cap, focus) { stars.filter { it.i < cap || it.i in focus } }
    val labelPx = with(LocalDensity.current) { 11.sp.toPx() }
    val nebPx = with(LocalDensity.current) { 15.sp.toPx() }
    // фон: три пятна цветов темы медленно плывут; в лайт-режиме стоят
    val acc = LocalAccent.current
    val lite = LocalLite.current
    val drift = androidx.compose.animation.core.rememberInfiniteTransition(label = "sky")
    val bgT by drift.animateFloat(0f, 360f, androidx.compose.animation.core.infiniteRepeatable(tween(60_000, easing = androidx.compose.animation.core.LinearEasing)), label = "t")

    Box(Modifier.fillMaxSize().background(Color(0xFF07060B))) {
        Canvas(Modifier.fillMaxSize()) {
            val t = if (lite) 0f else bgT
            val big = maxOf(size.width, size.height)
            listOf(Triple(acc.c1, 0.25f to 0.3f, 0.8f), Triple(acc.c2, 0.75f to 0.75f, 0.8f), Triple(acc.deep, 0.5f to 0.5f, 0.95f)).forEachIndexed { i, (c, at, k) ->
                val r = big * k / 2
                val o = Offset(
                    size.width * at.first + size.width * 0.12f * sin(Math.toRadians((t * (1 + i * 0.3) + i * 120).toDouble())).toFloat(),
                    size.height * at.second + size.height * 0.12f * cos(Math.toRadians((t * (1.2 + i * 0.2) + i * 70).toDouble())).toFloat(),
                )
                drawCircle(androidx.compose.ui.graphics.Brush.radialGradient(listOf(c.copy(alpha = if (i == 2) 0.35f else 0.2f), c.copy(alpha = 0.06f), Color.Transparent), o, r), r, o)
            }
        }
        Canvas(
            Modifier.fillMaxSize()
                .pointerInput(data) {
                    detectTransformGestures { centroid, pan, gz, _ ->
                        // зум к точке между пальцами — она остаётся на месте
                        val nz = (zoom * gz).coerceIn(0.08f, 6f)
                        val wx = cx + (centroid.x - size.width / 2) / zoom
                        val wy = cy + (centroid.y - size.height / 2) / zoom
                        zoom = nz
                        cx = wx - (centroid.x - size.width / 2) / nz - pan.x / nz
                        cy = wy - (centroid.y - size.height / 2) / nz - pan.y / nz
                    }
                }
                .pointerInput(visible) {
                    fun hit(p: Offset): UStar? = visible.minByOrNull { s -> hypot((s.x - cx) * zoom + size.width / 2 - p.x, (s.y - cy) * zoom + size.height / 2 - p.y) }
                        ?.takeIf { s -> hypot((s.x - cx) * zoom + size.width / 2 - p.x, (s.y - cy) * zoom + size.height / 2 - p.y) < 30.dp.toPx() }
                    detectTapGestures(
                        onTap = { p -> hit(p)?.let { eco.pick(it.i) } ?: eco.clear() },
                        onDoubleTap = { p -> hit(p)?.let { fly(it.i) } },
                    )
                },
        ) {
            w = size.width; h = size.height
            fun sx(x: Float) = (x - cx) * zoom + size.width / 2
            fun sy(y: Float) = (y - cy) * zoom + size.height / 2
            val paint = android.graphics.Paint().apply { isAntiAlias = true; textAlign = android.graphics.Paint.Align.CENTER }
            // туманности жанров
            data?.nebulas?.forEach { n ->
                val r = maxOf(30.dp.toPx(), n.r * zoom)
                val o = Offset(sx(n.x), sy(n.y))
                if (o.x + r < 0 || o.x - r > size.width || o.y + r < 0 || o.y - r > size.height) return@forEach
                val c = rgb(n.c)
                drawCircle(androidx.compose.ui.graphics.Brush.radialGradient(listOf(c.copy(alpha = 0.16f), c.copy(alpha = 0.07f), Color.Transparent), o, r), r, o)
                paint.color = c.copy(alpha = 0.75f).toArgb(); paint.textSize = (r / 7f).coerceIn(nebPx * 0.8f, nebPx * 1.8f); paint.isFakeBoldText = true
                drawContext.canvas.nativeCanvas.drawText(n.n, o.x, o.y - r * 0.6f, paint)
            }
            // линии: у каждой звезды — её самые сильные; в фокусе — только связи выбранной (или моста)
            val on = BooleanArray(stars.size)
            visible.forEach { on[it.i] = true }
            val links = data?.links ?: emptyList()
            fun line(li: Int, hi: Boolean) {
                val l = links.getOrNull(li) ?: return
                val a = l[0].toInt(); val b = l[1].toInt()
                if (a !in stars.indices || b !in stars.indices || !on[a] || !on[b]) return
                val sa = stars[a]; val sb = stars[b]
                drawLine(if (hi) LocalAccentColor else originColor(l[2].toInt()), Offset(sx(sa.x), sy(sa.y)), Offset(sx(sb.x), sy(sb.y)), if (hi) 4f else 1.5f)
            }
            if (focus.isEmpty()) {
                val drawn = HashSet<Int>()
                visible.forEach { s -> s.top.forEach { li -> if (drawn.add(li)) line(li, false) } }
            } else if (bridge != null) {
                bridge.stars.zipWithNext().forEach { (a, b) ->
                    if (a in stars.indices && b in stars.indices) drawLine(LocalAccentColor, Offset(sx(stars[a].x), sy(stars[a].y)), Offset(sx(stars[b].x), sy(stars[b].y)), 4f)
                }
            } else eco.sel?.let { s ->
                eco.info?.links?.forEach { l -> if (l.star in stars.indices) drawLine(LocalAccentColor.copy(alpha = 0.7f), Offset(sx(stars[s].x), sy(stars[s].y)), Offset(sx(stars[l.star].x), sy(stars[l.star].y)), 2.5f) }
            }
            // звёзды и подписи
            val rk = zoom.toDouble().pow(0.55).toFloat()
            // подписи — важные первыми, только если не налезают на уже поставленные
            val placed = ArrayList<android.graphics.RectF>()
            for (st in visible) {
                val x = sx(st.x); val y = sy(st.y)
                if (x < -60 || x > size.width + 60 || y < -60 || y > size.height + 60) continue
                val r = maxOf(2.5.dp.toPx(), st.r * rk * density * 0.55f)
                val c = rgb(st.c)
                val faded = focus.isNotEmpty() && st.i !in focus
                drawCircle(androidx.compose.ui.graphics.Brush.radialGradient(listOf(Color.White, c, c.copy(alpha = 0.85f)), Offset(x, y), r), r, Offset(x, y), alpha = if (faded) 0.18f else 1f)
                if (st.i == eco.sel) drawCircle(Color.White, r + 5f, Offset(x, y), style = Stroke(3f))
                if (!faded && placed.size < 60) {
                    val label = st.n.take(24)
                    paint.textSize = labelPx; paint.isFakeBoldText = st.i == eco.sel
                    val tw = paint.measureText(label)
                    val rect = android.graphics.RectF(x - tw / 2, y + r + 2f, x + tw / 2, y + r + labelPx + 6f)
                    if (st.i == eco.sel || placed.none { android.graphics.RectF.intersects(it, rect) }) {
                        placed += rect
                        paint.color = 0xE6ECE8F6.toInt()
                        drawContext.canvas.nativeCanvas.drawText(label, x, y + r + labelPx + 2f, paint)
                    }
                }
            }
        }

        // верх: заголовок, поиск, кнопки
        Column(Modifier.statusBarsPadding().padding(top = 8.dp)) {
            Row(Modifier.padding(start = 64.dp, end = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(tr("Вселенная", "Universe", "Всесвіт", "Vesmír"), color = Pal.text, fontSize = 28.sp, fontWeight = FontWeight.Black, modifier = Modifier.weight(1f))
                if (eco.uniBusy) androidx.compose.material3.CircularProgressIndicator(Modifier.size(20.dp), color = Color.White, strokeWidth = 2.dp)
            }
            GlassField(
                q, { q = it; eco.search(it) }, Modifier.padding(horizontal = 16.dp, vertical = 6.dp).fillMaxWidth(),
                placeholder = { Text(tr("Артист, трек или жанр…", "Artist, track or genre…", "Артист, трек або жанр…", "Interpret, skladba alebo žáner…")) },
                leadingIcon = { Icon(Icons.Rounded.Search, null) }, singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search), keyboardActions = KeyboardActions(onSearch = { eco.found.firstOrNull()?.let { eco.go(it); q = ""; eco.search("") } }),
            )
            if (q.isNotBlank() && eco.found.isNotEmpty()) Column(Modifier.padding(horizontal = 16.dp).fillMaxWidth().liquid(RoundedCornerShape(18.dp)).padding(6.dp)) {
                for (f in eco.found) Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable { eco.go(f); q = ""; eco.search("") }.padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(f.name, color = Pal.text, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        f.sub?.let { Text(it, color = Pal.faint, fontSize = 11.sp, maxLines = 1) }
                    }
                    Text(EcoText.kind(f.kind).lowercase(), color = Pal.faint, fontSize = 11.sp)
                }
            }
            Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                GlassChip(tr("Мосты", "Bridges", "Мости", "Mosty"), bridgeOpen) { bridgeOpen = !bridgeOpen }
                GlassChip(tr("Всё небо", "Whole sky", "Усе небо", "Celá obloha"), false) { eco.clear(); fit() }
                GlassChip(if (eco.fetching) tr("Подтягиваю…", "Fetching…", "Підтягую…", "Doťahujem…") else tr("Подтянуть связи", "Fetch links", "Підтягнути зв'язки", "Doplniť väzby"), eco.fetching) { eco.fetch() }
            }
            if (data?.sparse == true && eco.sel == null) Text(
                tr("Связей пока мало. «Подтянуть связи» добавит жанры, продюсеров и лейблы из MusicBrainz и похожих из YouTube.", "Few links so far. “Fetch links” adds genres, producers and labels from MusicBrainz and similar artists from YouTube.", "Зв'язків поки мало. «Підтягнути зв'язки» додасть жанри, продюсерів і лейбли з MusicBrainz та схожих з YouTube.", "Zatiaľ málo väzieb. „Doplniť väzby“ pridá žánre, producentov a vydavateľstvá z MusicBrainz a podobných z YouTube."),
                color = Pal.dim, fontSize = 12.sp, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp).liquid(RoundedCornerShape(14.dp)).padding(10.dp),
            )
            if (eco.cmpPick) Text(tr("Выбери второго артиста для сравнения", "Pick a second artist to compare", "Обери другого артиста для порівняння", "Vyber druhého interpreta na porovnanie"), color = Pal.text, fontSize = 13.sp, modifier = Modifier.padding(16.dp).liquid(RoundedCornerShape(14.dp)).padding(10.dp))
        }

        if (data != null && stars.isEmpty()) Text(
            tr("Вселенная строится из того, что ты слушал. Послушай что-нибудь — и тут появятся звёзды.", "The universe is built from what you've listened to. Play something and stars will appear.", "Всесвіт будується з того, що ти слухав. Послухай щось — і тут з'являться зірки.", "Vesmír vzniká z toho, čo si počúval. Pusti si niečo a objavia sa hviezdy."),
            color = Pal.dim, fontSize = 15.sp, modifier = Modifier.align(Alignment.Center).padding(32.dp),
        )

        // низ: выбранный артист — связи, треки, факты
        val info = eco.info
        if (eco.sel != null && info != null) {
            LazyColumn(
                Modifier.align(Alignment.BottomCenter).padding(bottom = LocalBottomPad.current).padding(horizontal = 12.dp).fillMaxWidth().heightIn(max = 400.dp)
                    .liquid(RoundedCornerShape(28.dp)),
                contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                item {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(EcoText.kind(1), color = Pal.faint, fontSize = 11.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
                        Icon(Icons.Rounded.Close, null, tint = Pal.dim, modifier = Modifier.size(22.dp).clickable { eco.clear() })
                    }
                    Text(info.name, color = Pal.text, fontSize = 22.sp, fontWeight = FontWeight.Black, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    val sub = listOfNotNull(
                        info.listened.takeIf { it > 0 }?.let { tr("слушал ${dur(it)}", "listened ${dur(it)}", "слухав ${dur(it)}", "počúval si ${dur(it)}") },
                        info.dna?.let { d -> listOfNotNull(d.bpm?.let { "~${it.toInt()} BPM" }, tr("DNA по ${d.n} трекам", "DNA from ${d.n} tracks", "DNA за ${d.n} треками", "DNA z ${d.n} skladieb")).joinToString(" · ") },
                    ).joinToString(" · ")
                    if (sub.isNotEmpty()) Text(sub, color = Pal.dim, fontSize = 12.sp)
                    Row(Modifier.padding(top = 6.dp).horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (info.tracks.isNotEmpty()) GlassChip(tr("Играть", "Play", "Грати", "Hrať"), true) { vm.play(info.tracks, 0) }
                        if (info.dna != null) GlassChip("Music DNA", false) { eco.openDna(info.node) }
                        GlassChip(tr("Мост отсюда", "Bridge from here", "Міст звідси", "Most odtiaľto"), eco.brA == info.node) { eco.bridgeEnd(info.node, true); bridgeOpen = true }
                        GlassChip(tr("Мост сюда", "Bridge to here", "Міст сюди", "Most sem"), eco.brB == info.node) { eco.bridgeEnd(info.node, false); bridgeOpen = true }
                        if (eco.inRoom) info.tracks.firstOrNull { it.src != "lc" }?.let { t -> GlassChip(tr("В комнату", "To the room", "У кімнату", "Do miestnosti"), false) { eco.add(t) } }
                        if (eco.inRoom) GlassChip(tr("Показать всем", "Show everyone", "Показати всім", "Ukázať všetkým"), false) { eco.say(info.name, "🪐") }
                    }
                    val facts = listOfNotNull(
                        info.genres.takeIf { it.isNotEmpty() }?.let { tr("Жанры: ", "Genres: ", "Жанри: ", "Žánre: ") + it.joinToString(", ") },
                        info.albums.takeIf { it.isNotEmpty() }?.let { tr("Альбомы: ", "Albums: ", "Альбоми: ", "Albumy: ") + it.joinToString(", ") },
                        info.people.takeIf { it.isNotEmpty() }?.let { tr("Люди за записями: ", "People behind the records: ", "Люди за записами: ", "Ľudia za nahrávkami: ") + it.take(12).joinToString(", ") },
                    )
                    for (f in facts) Text(f, color = Pal.dim, fontSize = 12.sp, modifier = Modifier.padding(top = 4.dp))
                    Text(if (info.links.isEmpty()) tr("Связей пока нет", "No links yet", "Зв'язків поки немає", "Zatiaľ žiadne väzby") else tr("Связанные артисты · ${info.links.size}", "Related artists · ${info.links.size}", "Пов'язані артисти · ${info.links.size}", "Súvisiaci interpreti · ${info.links.size}"), color = Pal.text, fontSize = 15.sp, fontWeight = FontWeight.Bold, modifier = Modifier.padding(top = 8.dp))
                }
                items(info.links) { l ->
                    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable { fly(l.star) }.padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(8.dp).clip(CircleShape).background(originColor(when (l.origin) { "Confirmed" -> 0; "External" -> 1; else -> 2 }).copy(alpha = 1f)))
                        Spacer(Modifier.width(10.dp))
                        Column(Modifier.weight(1f)) {
                            Text(l.name, color = Pal.text, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(l.why.joinToString(" · ") { r -> when { r.note.isEmpty() || r.rel == "Feat" -> EcoText.rel(r.rel); r.note.startsWith("×") -> "${EcoText.rel(r.rel)} ${r.note}"; else -> "${EcoText.rel(r.rel)}: ${r.note}" } }, color = Pal.dim, fontSize = 11.sp)
                            Text(l.why.map { EcoText.src(it.rel) }.distinct().joinToString(", "), color = Pal.faint, fontSize = 10.sp)
                        }
                    }
                }
                if (info.tracks.isNotEmpty()) item { Text(tr("Треки · ${info.tracks.size}", "Tracks · ${info.tracks.size}", "Треки · ${info.tracks.size}", "Skladby · ${info.tracks.size}"), color = Pal.text, fontSize = 15.sp, fontWeight = FontWeight.Bold, modifier = Modifier.padding(top = 8.dp)) }
                items(info.tracks.size.coerceAtMost(15)) { i ->
                    val t = info.tracks[i]
                    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable { vm.play(info.tracks, i) }.padding(vertical = 5.dp), verticalAlignment = Alignment.CenterVertically) {
                        Cover(t.cover, 40.dp, 8.dp)
                        Spacer(Modifier.width(10.dp))
                        Text(t.title, color = Pal.text, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    }
                }
            }
        }
    }

    if (bridgeOpen) BridgeDialog(vm) { bridgeOpen = false }
    eco.dna?.let { d -> DnaDialog(vm, d) { eco.dna = null } }
}

/** Цвет подсветки моста — акцент темы (константа, чтобы не тянуть CompositionLocal в рисование) */
private val LocalAccentColor = Color(0xFFFFD166)

@UnstableApi
@Composable
private fun BridgeDialog(vm: AppState, close: () -> Unit) {
    val eco = vm.eco
    val byNode = eco.uni?.stars?.associateBy { it.node } ?: emptyMap()
    var depth by remember { mutableFloatStateOf(5f) }
    var groups by remember { mutableStateOf(listOf(true, true, true, true)) }
    GlassDialog(close, tr("Музыкальные мосты", "Musical bridges", "Музичні мости", "Hudobné mosty"), buttons = {
        Pill(tr("Найти мост", "Find a bridge", "Знайти міст", "Nájsť most"), enabled = eco.brA != null && eco.brB != null) { eco.runBridge(depth.toInt(), groups) }
    }) {
        Text(tr("Выбери артиста: «Мост отсюда», потом другого — «Мост сюда». Я найду цепочку связей и объясню каждый шаг.", "Pick two nodes on the map: “Bridge from here” and “Bridge to here”. I'll find a chain of links and explain each step.", "Обери на карті два вузли: «Міст звідси» і «Міст сюди». Я знайду ланцюжок зв'язків і поясню кожен крок.", "Vyber na mape dva uzly: „Most odtiaľto“ a „Most sem“. Nájdem reťaz väzieb a vysvetlím každý krok."), color = Pal.dim, fontSize = 13.sp)
        Text("${tr("От", "From", "Від", "Od")}: ${eco.brA?.let { byNode[it]?.n } ?: "—"}", color = Pal.text, fontWeight = FontWeight.Bold)
        Text("${tr("До", "To", "До", "Do")}: ${eco.brB?.let { byNode[it]?.n } ?: "—"}", color = Pal.text, fontWeight = FontWeight.Bold)
        Text(tr("Шагов до ${depth.toInt()}", "Up to ${depth.toInt()} steps", "Кроків до ${depth.toInt()}", "Krokov do ${depth.toInt()}"), color = Pal.dim, fontSize = 12.sp)
        Slider(depth, { depth = it }, valueRange = 2f..8f, steps = 5, colors = SliderDefaults.colors(thumbColor = Color.White, activeTrackColor = LocalAccent.current.c1))
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            listOf(tr("Факты", "Facts", "Факти", "Fakty"), "YouTube", tr("Звук", "Sound", "Звук", "Zvuk"), tr("Плейлисты", "Playlists", "Плейлисти", "Playlisty")).forEachIndexed { i, t ->
                GlassChip(t, groups[i]) { groups = groups.toMutableList().also { it[i] = !it[i] } }
            }
        }
        if (eco.brSearched && eco.bridges.isEmpty()) Text(tr("Мост не нашёлся. Добавь шагов, включи другие связи или нажми «Подтянуть связи».", "No bridge found. Allow more steps, enable other links or press “Fetch links”.", "Міст не знайшовся. Додай кроків, увімкни інші зв'язки або натисни «Підтягнути зв'язки».", "Most sa nenašiel. Pridaj kroky, zapni iné väzby alebo stlač „Doplniť väzby“."), color = Pal.dim, fontSize = 12.sp)
        Column(Modifier.heightIn(max = 300.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            eco.bridges.forEachIndexed { bi, b ->
                Column(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(if (eco.brShown == bi) LocalAccent.current.c1.copy(alpha = 0.18f) else Color(0x0DFFFFFF)).clickable { eco.brShown = bi; close() }.padding(10.dp),
                    verticalArrangement = Arrangement.spacedBy(3.dp),
                ) {
                    Text(tr("${b.steps.size - 1} шагов · фактов ${b.facts}", "${b.steps.size - 1} steps · ${b.facts} facts", "${b.steps.size - 1} кроків · фактів ${b.facts}", "${b.steps.size - 1} krokov · faktov ${b.facts}"), color = Pal.faint, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                    for (s in b.steps) {
                        if (s.rel != null) Text("  ↓ ${EcoText.rel(s.rel)}${if (s.note.isNotEmpty()) " · ${s.note}" else ""} · ${EcoText.origin(s.origin)} · ${EcoText.src(s.rel)}", color = Pal.dim, fontSize = 11.sp)
                        Text(s.name, color = Pal.text, fontSize = 14.sp, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
    }
}

@UnstableApi
@Composable
private fun DnaDialog(vm: AppState, d: DnaInfo, close: () -> Unit) {
    val eco = vm.eco
    val name = eco.dnaNode?.let { i -> eco.uni?.stars?.firstOrNull { it.node == i }?.n } ?: ""
    GlassDialog(close, "Music DNA · $name", buttons = {
        Pill(tr("Сравнить с…", "Compare with…", "Порівняти з…", "Porovnať s…"), primary = false) { eco.cmpPick = true; close() }
    }) {
        Column(Modifier.heightIn(max = 520.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(if (d.n > 1) tr("Среднее по ${d.n} из ${d.total} треков с DNA", "Average of ${d.n} of ${d.total} tracks with DNA", "Середнє за ${d.n} з ${d.total} треків з DNA", "Priemer ${d.n} z ${d.total} skladieb s DNA") else tr("Разобрано ${d.dna.secs.toInt()} с звука", "${d.dna.secs.toInt()} s of audio analysed", "Розібрано ${d.dna.secs.toInt()} с звуку", "Analyzovaných ${d.dna.secs.toInt()} s zvuku"), color = Pal.dim, fontSize = 12.sp)
            Radar(listOfNotNull(d.radar, d.cmp?.radar), Modifier.fillMaxWidth(0.8f).align(Alignment.CenterHorizontally))
            d.cmp?.let { c ->
                Text(tr("Сравнение: ${c.name}", "Compared with: ${c.name}", "Порівняння: ${c.name}", "Porovnanie: ${c.name}"), color = Color(0xFF22D3EE), fontSize = 13.sp, fontWeight = FontWeight.Bold)
                val p = EcoText.parts
                listOf(c.sim.rhythm, c.sim.harmony, c.sim.timbre, c.sim.energy, c.sim.spectrum).forEachIndexed { i, v -> Bar(p[i], pct(v), v) }
                Bar(tr("Итого", "Overall", "Разом", "Spolu"), pct(c.sim.total), c.sim.total)
            }
            val x = d.dna
            Bar(tr("Темп", "Tempo", "Темп", "Tempo"), x.bpm?.let { "${it.toInt()} BPM" } ?: tr("нет данных", "no data", "немає даних", "žiadne údaje"), ((x.bpm ?: 60f) - 60f) / 140f)
            Bar(tr("Тональность", "Key", "Тональність", "Tónina"), x.keyName?.let { "$it · ${pct(x.key_conf)}" } ?: tr("нет данных", "no data", "немає даних", "žiadne údaje"), x.key_conf)
            Bar(tr("Чёткость бита", "Beat clarity", "Чіткість біту", "Zreteľnosť beatu"), pct(x.pulse), x.pulse)
            Bar(tr("Энергия", "Energy", "Енергія", "Energia"), pct(x.energy), x.energy)
            Bar(tr("Громкость", "Loudness", "Гучність", "Hlasitosť"), "%.1f dBFS".format(x.rms_db), (x.rms_db + 30f) / 24f)
            Bar(tr("Динамика", "Dynamics", "Динаміка", "Dynamika"), "%.1f dB".format(x.dyn_db), x.dyn_db / 18f)
            Bar(tr("Яркость", "Brightness", "Яскравість", "Jas"), "${x.centroid.toInt()} Hz", x.centroid / 4000f)
            Bar(tr("Низ · середина · верх", "Low · mid · high", "Низ · середина · верх", "Basy · stredy · výšky"), x.bands.joinToString("/") { "${(it * 100).toInt()}" } + " %", x.bands.getOrElse(0) { 0f })
            Text(tr("Танцевальность, акустичность, речь, настроение, инструменты и вокал по спектру честно не определить — для них нужна локальная ML-модель, она ещё не подключена.", "Danceability, acousticness, speech, mood, instruments and vocals can't be honestly read off the spectrum — that needs a local ML model, not connected yet.", "Танцювальність, акустичність, мовлення, настрій, інструменти й вокал за спектром чесно не визначити — для них потрібна локальна ML-модель, її ще не підключено.", "Tanečnosť, akustickosť, reč, náladu, nástroje a vokál zo spektra poctivo neurčíš — na to treba lokálny ML model, ešte nie je pripojený."), color = Pal.faint, fontSize = 11.sp)
            if (d.similar.isNotEmpty()) Section(tr("Звучит похоже", "Sounds alike", "Звучить схоже", "Znie podobne"))
            for (p in d.similar) PickRow(p) { close(); eco.goNode(p.id) }
            if (d.far.isNotEmpty()) Section(tr("Совсем не как обычно", "Nothing like your usual", "Зовсім не як зазвичай", "Úplne iné než zvyčajne"))
            for (p in d.far) PickRow(p) { close(); eco.goNode(p.id) }
        }
    }
}

@Composable
private fun PickRow(p: DnaPick, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable(onClick = onClick).padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Text("${(p.sim.total * 100).toInt()}%", color = LocalAccent.current.c2, fontWeight = FontWeight.Black, modifier = Modifier.width(46.dp))
        Column(Modifier.weight(1f)) {
            Text(p.name, color = Pal.text, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(listOfNotNull(p.sub, EcoText.why(p)).joinToString(" · "), color = Pal.faint, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

// ════════════════ Мой вкус ════════════════

@UnstableApi
@Composable
fun TasteScreen(vm: AppState) {
    val eco = vm.eco
    LaunchedEffect(Unit) { if (eco.taste == null) eco.loadTaste() }
    val t = eco.taste
    val fp = t?.fp
    val a = LocalAccent.current
    var friend by remember { mutableStateOf("") }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = LocalBottomPad.current)) {
        item { Title(tr("Мой вкус", "My taste", "Мій смак", "Môj vkus"), Modifier.statusBarsPadding(), inset = 64.dp) }
        item {
            Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(tr("Всё время", "All time", "Весь час", "Celý čas"), tr("Год", "Year", "Рік", "Rok"), tr("Полгода", "6 months", "Пів року", "Pol roka"), tr("3 месяца", "3 months", "3 місяці", "3 mesiace"), tr("Месяц", "Month", "Місяць", "Mesiac")).forEachIndexed { i, s ->
                    GlassChip(s, eco.period == i) { eco.loadTaste(i) }
                }
            }
        }
        if (eco.tasteBusy && t == null) item { Loading() }
        if (fp != null && fp.total_ms == 0L) item { Dim(tr("Пока не из чего собрать отпечаток — послушай музыку, и он появится.", "Nothing to build a fingerprint from yet — listen to some music.", "Поки нема з чого скласти відбиток — послухай музику.", "Zatiaľ nie je z čoho zostaviť odtlačok — počúvaj hudbu."), Modifier.padding(20.dp)) }
        if (fp != null && fp.total_ms > 0) {
            item {
                val top = fp.genres.take(2).joinToString(", ") { "${it.name} (${pct(it.w)})" }
                Text(
                    tr("${dur(fp.total_ms)} музыки, ${fp.tracks} треков", "${dur(fp.total_ms)} of music, ${fp.tracks} tracks", "${dur(fp.total_ms)} музики, ${fp.tracks} треків", "${dur(fp.total_ms)} hudby, ${fp.tracks} skladieb") + (if (top.isNotEmpty()) ". " + top else "") + ". " + tr("Разнообразие артистов ${pct(fp.artist_diversity)}", "Artist diversity ${pct(fp.artist_diversity)}", "Різноманіття артистів ${pct(fp.artist_diversity)}", "Rozmanitosť interpretov ${pct(fp.artist_diversity)}"),
                    color = Pal.dim, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 20.dp, vertical = 10.dp),
                )
            }
            // созвездие: жанры по кругу, артисты вокруг, светлые дуги — необычные сочетания
            item {
                val lines = remember(t.lines) { runCatching { PathParser().parsePathString(t.lines).toPath() }.getOrNull() }
                val arcs = remember(t.arcs) { runCatching { PathParser().parsePathString(t.arcs).toPath() }.getOrNull() }
                val lp = with(LocalDensity.current) { 10.sp.toPx() }
                Box(Modifier.padding(16.dp).fillMaxWidth().aspectRatio(1f).liquid(RoundedCornerShape(28.dp))) {
                    Canvas(Modifier.fillMaxSize().padding(8.dp)) {
                        val k = size.minDimension / 1000f
                        withTransform({ scale(k, k, Offset.Zero) }) {
                            lines?.let { drawPath(it, Color(0x26FFFFFF), style = Stroke(1.5f / k)) }
                            arcs?.let { drawPath(it, a.c2, style = Stroke(3f / k)) }
                        }
                        val paint = android.graphics.Paint().apply { isAntiAlias = true; textAlign = android.graphics.Paint.Align.CENTER }
                        for (s in t.stars) {
                            val c = rgb(s.c)
                            val o = Offset(s.x * k, s.y * k)
                            val r = s.r * k * 1.6f
                            if (s.kind == 3) { drawCircle(c.copy(alpha = 0.4f), r, o); drawCircle(c, r, o, style = Stroke(3f)) } else drawCircle(c, r, o)
                            if (s.lab) {
                                paint.color = if (s.kind == 3) android.graphics.Color.WHITE else 0xCCECE8F6.toInt()
                                paint.textSize = if (s.kind == 3) lp * 1.2f else lp
                                paint.isFakeBoldText = s.kind == 3
                                drawContext.canvas.nativeCanvas.drawText(s.label.take(22), o.x, if (s.kind == 3) o.y + paint.textSize / 3 else o.y + r + paint.textSize, paint)
                            }
                        }
                    }
                }
            }
            item {
                Card(tr("Необычные сочетания", "Unusual combinations", "Незвичні поєднання", "Nezvyčajné kombinácie"), tr("Жанры, которые ты слушаешь оба, хотя у твоих же артистов они почти не встречаются вместе", "Genres you listen to both of, even though your own artists hardly ever combine them", "Жанри, які ти слухаєш обидва, хоча у твоїх же артистів вони майже не трапляються разом", "Žánre, ktoré počúvaš oba, hoci ich tvoji interpreti takmer nespájajú")) {
                    if (fp.combos.isEmpty()) Text(if (fp.genres.isEmpty()) tr("Для сочетаний нужны жанры — нажми «Подтянуть связи» во Вселенной.", "Combinations need genres — press “Fetch links” in the Universe.", "Для поєднань потрібні жанри — натисни «Підтягнути зв'язки» у Всесвіті.", "Kombinácie potrebujú žánre — stlač „Doplniť väzby“ vo Vesmíre.") else tr("Необычных сочетаний не нашлось: твои жанры у твоих артистов и так звучат вместе.", "No unusual combinations: your genres already go together.", "Незвичних поєднань не знайшлося: твої жанри і так звучать разом.", "Nezvyčajné kombinácie sa nenašli."), color = Pal.faint, fontSize = 12.sp)
                    for (c in fp.combos) {
                        Text(c.genres.joinToString(" + "), color = a.c2, fontSize = 16.sp, fontWeight = FontWeight.Black)
                        val shares = c.genres.mapNotNull { g -> fp.genres.firstOrNull { it.name == g }?.let { "${it.name} ${pct(it.w)}" } }.joinToString(", ")
                        Text(tr("$shares твоего времени, а вместе их сочетают ${c.together} из ${c.of} твоих артистов", "$shares of your time, yet only ${c.together} of your ${c.of} artists combine them", "$shares твого часу, а разом їх поєднують ${c.together} з ${c.of} твоїх артистів", "$shares tvojho času, a spolu ich spája ${c.together} z ${c.of} tvojich interpretov"), color = Pal.dim, fontSize = 12.sp)
                    }
                }
            }
            item {
                Card(tr("Твои миры", "Your worlds", "Твої світи", "Tvoje svety"), tr("Кластеры артистов, которые держатся вместе в твоём графе связей", "Clusters of artists that stick together in your link graph", "Кластери артистів, які тримаються разом у твоєму графі зв'язків", "Zhluky interpretov, ktoré držia spolu v tvojom grafe")) {
                    for (c in fp.clusters) {
                        Text("${c.name} · ${pct(c.w)}", color = Pal.text, fontSize = 14.sp, fontWeight = FontWeight.Bold)
                        Text(c.artists.joinToString(", "), color = Pal.faint, fontSize = 12.sp)
                    }
                }
            }
            item {
                Card(tr("Жанры", "Genres", "Жанри", "Žánre"), if (fp.genres.isEmpty()) tr("Жанры дают MusicBrainz: нажми «Подтянуть связи» во Вселенной", "Genres come from MusicBrainz: press “Fetch links” in the Universe", "Жанри дає MusicBrainz: натисни «Підтягнути зв'язки» у Всесвіті", "Žánre dodáva MusicBrainz: stlač „Doplniť väzby“ vo Vesmíre") else tr("Известны для ${pct(fp.genre_coverage)} времени", "Known for ${pct(fp.genre_coverage)} of the time", "Відомі для ${pct(fp.genre_coverage)} часу", "Známe pre ${pct(fp.genre_coverage)} času")) {
                    val mx = fp.genres.firstOrNull()?.w?.coerceAtLeast(1e-3f) ?: 1f
                    for (g in fp.genres.take(12)) Bar(g.name, pct(g.w), g.w / mx, dur(g.ms))
                }
            }
            item {
                val s = fp.sound
                Card(tr("Звучание", "Sound", "Звучання", "Zvuk"), tr("По Music DNA: ${s.tracks} треков · ${pct(s.coverage)} времени", "From Music DNA: ${s.tracks} tracks · ${pct(s.coverage)} of the time", "За Music DNA: ${s.tracks} треків · ${pct(s.coverage)} часу", "Z Music DNA: ${s.tracks} skladieb · ${pct(s.coverage)} času")) {
                    val m = s.mean
                    if (m == null) Text(tr("Music DNA ещё не разобрал твои треки: нужен звук на устройстве (скачанное) или на компьютере (кэш)", "Music DNA hasn't analysed your tracks yet: needs downloaded audio here or cached audio on the computer", "Music DNA ще не розібрав твої треки: потрібен звук на пристрої (завантажене) або на комп'ютері (кеш)", "Music DNA ešte neanalyzovala tvoje skladby: treba stiahnutý zvuk tu alebo cache na počítači"), color = Pal.faint, fontSize = 12.sp)
                    else {
                        Radar(listOf(listOf(m.energy, m.pulse, ((m.bpm ?: 60f) - 60f) / 140f, m.centroid / 4000f, m.flatness * 3f, m.dyn_db / 18f, m.onsets / 8f, m.bands.getOrElse(0) { 0f } * 2f)), Modifier.fillMaxWidth(0.7f).align(Alignment.CenterHorizontally))
                        s.tempo?.let { Bar(tr("Темп", "Tempo", "Темп", "Tempo"), "${it[1].toInt()} BPM", (it[1] - 60f) / 140f, "${it[0].toInt()}–${it[2].toInt()} BPM") }
                        s.minor?.let { Bar(tr("Минор", "Minor", "Мінор", "Mol"), pct(it), it, s.keys.joinToString(" ") { k -> k.name }) }
                        Bar(tr("Энергия", "Energy", "Енергія", "Energia"), pct(s.energy), s.energy)
                        Bar(tr("Чёткость бита", "Beat clarity", "Чіткість біту", "Zreteľnosť beatu"), pct(s.pulse), s.pulse)
                    }
                }
            }
            item {
                Card(tr("Как ты слушаешь", "How you listen", "Як ти слухаєш", "Ako počúvaš")) {
                    Bar(tr("Разнообразие артистов", "Artist diversity", "Різноманіття артистів", "Rozmanitosť interpretov"), pct(fp.artist_diversity), fp.artist_diversity)
                    if (fp.genres.isNotEmpty()) Bar(tr("Разнообразие жанров", "Genre diversity", "Різноманіття жанрів", "Rozmanitosť žánrov"), pct(fp.genre_diversity), fp.genre_diversity)
                    Bar(tr("Повторы", "Repeats", "Повтори", "Opakovania"), "%.1f×".format(fp.repeat), (fp.repeat / 10f).coerceAtMost(1f), tr("в среднем на трек", "plays per track", "у середньому на трек", "priemerne na skladbu"))
                    Bar(tr("Любимое по кругу", "Favourites on repeat", "Улюблене по колу", "Obľúbené dookola"), pct(fp.loyal), fp.loyal)
                    fp.skip_rate?.let { Bar(tr("Пропуски", "Skips", "Пропуски", "Preskočenia"), pct(it), it) }
                    if (fp.hours.size == 24) Row(Modifier.fillMaxWidth().height(56.dp), horizontalArrangement = Arrangement.spacedBy(2.dp), verticalAlignment = Alignment.Bottom) {
                        fp.hours.forEachIndexed { i, v -> Box(Modifier.weight(1f).fillMaxHeight(((v / 0.16f).coerceIn(0.03f, 1f))).clip(RoundedCornerShape(2.dp)).background(androidx.compose.ui.graphics.lerp(a.c1, a.c2, i / 23f))) }
                    }
                }
            }
            item {
                val d = fp.dynamics
                Card(tr("Как меняется вкус", "How your taste changes", "Як змінюється смак", "Ako sa mení vkus"), tr("Последние три месяца против трёх перед ними", "The last three months against the three before", "Останні три місяці проти трьох перед ними", "Posledné tri mesiace oproti trom pred nimi")) {
                    val mx = (d.rising + d.falling).maxOfOrNull { kotlin.math.abs(it.w) }?.coerceAtLeast(1e-3f) ?: 1f
                    for (s in d.rising) Bar("↑ ${s.name}", "+%.1f %%".format(s.w * 100), s.w / mx)
                    for (s in d.falling) Bar("↓ ${s.name}", "%.1f %%".format(s.w * 100), kotlin.math.abs(s.w) / mx)
                    if (d.new_artists.isNotEmpty()) Text(tr("Новые в твоей жизни: ", "New in your life: ", "Нові у твоєму житті: ", "Noví v tvojom živote: ") + d.new_artists.joinToString(", "), color = Pal.dim, fontSize = 12.sp)
                    if (d.stable.isNotEmpty()) Text(tr("С тобой всегда: ", "Always with you: ", "З тобою завжди: ", "Vždy s tebou: ") + d.stable.joinToString(", "), color = Pal.dim, fontSize = 12.sp)
                    if (d.discovered.isNotEmpty()) Text(tr("Открыл(а) направления: ", "Discovered: ", "Відкрив(ла) напрями: ", "Objavil(a) smery: ") + d.discovered.joinToString(", "), color = Pal.dim, fontSize = 12.sp)
                }
            }
            item {
                Card(tr("Музыкальная идентичность", "Musical identity", "Музична ідентичність", "Hudobná identita"), tr("Публичная карточка вкуса. Друг вводит твой код и видит только то, что ты отметил(а).", "A public taste card. A friend enters your code and sees only what you ticked.", "Публічна картка смаку. Друг вводить твій код і бачить лише те, що ти позначив(ла).", "Verejná karta vkusu. Priateľ zadá tvoj kód a uvidí len to, čo si označil(a).")) {
                    val sw = SwitchDefaults.colors(checkedTrackColor = a.c1, checkedThumbColor = Color.White, uncheckedTrackColor = Color(0x33FFFFFF), uncheckedThumbColor = Color.White, uncheckedBorderColor = Color.Transparent, checkedBorderColor = Color.Transparent)
                    listOf(tr("Жанры", "Genres", "Жанри", "Žánre"), tr("Артисты", "Artists", "Артисти", "Interpreti"), tr("Необычные сочетания", "Unusual combinations", "Незвичні поєднання", "Nezvyčajné kombinácie"), tr("Звучание", "Sound", "Звучання", "Zvuk"), tr("Динамика", "Dynamics", "Динаміка", "Dynamika")).forEachIndexed { i, s ->
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(s, color = Pal.text, fontSize = 14.sp, modifier = Modifier.weight(1f))
                            Switch(eco.shows[i], { v -> eco.shows = eco.shows.toMutableList().also { it[i] = v } }, colors = sw)
                        }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Pill(if (t.published) tr("Обновить", "Update", "Оновити", "Aktualizovať") else tr("Опубликовать", "Publish", "Опублікувати", "Zverejniť")) { eco.publish() }
                        if (t.published) Pill(tr("Скрыть", "Hide", "Сховати", "Skryť"), primary = false) { eco.unpublish() }
                    }
                    if (t.published) Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(tr("Твой код: ${t.code}", "Your code: ${t.code}", "Твій код: ${t.code}", "Tvoj kód: ${t.code}"), color = a.c2, fontSize = 16.sp, fontWeight = FontWeight.Black, modifier = Modifier.weight(1f))
                        Icon(Icons.Rounded.ContentCopy, null, tint = Pal.dim, modifier = Modifier.size(22.dp).clickable { vm.copyText(t.code) })
                    }
                }
            }
            item {
                Card(tr("Совместимость вкусов", "Taste compatibility", "Сумісність смаків", "Zhoda vkusov"), tr("Не один процент, а из чего он складывается: общее, разное и неожиданное", "Not one percentage but what it's made of: shared, different and unexpected", "Не один відсоток, а з чого він складається: спільне, різне й несподіване", "Nie jedno percento, ale z čoho sa skladá")) {
                    GlassField(friend, { friend = it.uppercase() }, Modifier.fillMaxWidth(), placeholder = { Text(tr("Код друга", "Friend's code", "Код друга", "Kód priateľa")) }, singleLine = true, keyboardActions = KeyboardActions(onDone = { eco.compare(friend) }))
                    Pill(tr("Сравнить", "Compare", "Порівняти", "Porovnať"), enabled = friend.isNotBlank()) { eco.compare(friend) }
                    if (eco.compatErr.isNotEmpty()) Text(eco.compatErr, color = Color(0xFFFF8A9A), fontSize = 12.sp)
                    eco.compat?.let { cd ->
                        val c = cd.compat
                        Text(tr("Ты и ${cd.name}", "You and ${cd.name}", "Ти і ${cd.name}", "Ty a ${cd.name}"), color = Pal.text, fontSize = 16.sp, fontWeight = FontWeight.Black)
                        Text(tr("Пересечение жанров ${pct(c.genre_overlap)} · артистов ${pct(c.artist_overlap)} · общих артистов ${c.shared_artists.size}", "Genre overlap ${pct(c.genre_overlap)} · artist overlap ${pct(c.artist_overlap)} · ${c.shared_artists.size} shared artists", "Перетин жанрів ${pct(c.genre_overlap)} · артистів ${pct(c.artist_overlap)} · спільних артистів ${c.shared_artists.size}", "Prienik žánrov ${pct(c.genre_overlap)} · interpretov ${pct(c.artist_overlap)}"), color = Pal.dim, fontSize = 12.sp)
                        val rows = c.shared.mapNotNull { arr -> runCatching { Triple(arr[0].toString().trim('"'), arr[1].toString().toFloat(), arr[2].toString().toFloat()) }.getOrNull() }
                        val mx = rows.maxOfOrNull { maxOf(it.second, it.third) }?.coerceAtLeast(1e-3f) ?: 1f
                        for ((n, x, y) in rows.take(8)) Bar(n, "${pct(x)} / ${pct(y)}", x / mx, frac2 = y / mx)
                        if (c.only_me.isNotEmpty()) Text(tr("Только у тебя: ", "Only you: ", "Тільки в тебе: ", "Len ty: ") + c.only_me.take(8).joinToString(", "), color = Pal.dim, fontSize = 12.sp)
                        if (c.only_them.isNotEmpty()) Text(tr("Только у ${cd.name}: ", "Only ${cd.name}: ", "Тільки в ${cd.name}: ", "Len ${cd.name}: ") + c.only_them.take(8).joinToString(", "), color = Pal.dim, fontSize = 12.sp)
                        if (c.shared_artists.isNotEmpty()) Text(tr("Общие артисты: ", "Shared artists: ", "Спільні артисти: ", "Spoloční interpreti: ") + c.shared_artists.take(8).joinToString(", "), color = Pal.dim, fontSize = 12.sp)
                        if (c.surprises.isNotEmpty()) Text(tr("Неожиданно совпали: ", "Unexpected overlap: ", "Несподівано збіглися: ", "Nečakaná zhoda: ") + c.surprises.take(8).joinToString(", "), color = a.c2, fontSize = 12.sp)
                        c.sound?.let { s -> EcoText.parts.take(4).forEachIndexed { i, p -> Bar(p, pct(EcoText.sim(s, i)), EcoText.sim(s, i)) } }
                        if (cd.discover.isNotEmpty()) Section(tr("Может понравиться вам обоим", "You might both like", "Може сподобатися вам обом", "Mohlo by sa páčiť vám obom"))
                        for (d in cd.discover) Row(Modifier.fillMaxWidth().clickable { vm.play(listOf(d.track), 0) }.padding(vertical = 6.dp)) {
                            Column {
                                Text("${d.track.title} — ${d.track.artist}", color = Pal.text, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                Text(EcoText.discover(d), color = Pal.faint, fontSize = 11.sp)
                            }
                        }
                    }
                }
            }
        }
    }
}


// ════════════════ Комнаты ════════════════

@UnstableApi
@Composable
fun RoomsScreen(vm: AppState) {
    val eco = vm.eco
    LaunchedEffect(Unit) { if (!eco.inRoom) eco.loadPublics() }
    val r = eco.room
    if (r == null) RoomsOutside(vm) else RoomInside(vm, r)
}

@UnstableApi
@Composable
private fun RoomsOutside(vm: AppState) {
    val eco = vm.eco
    val a = LocalAccent.current
    var name by remember { mutableStateOf("") }
    var pub by remember { mutableStateOf(false) }
    var qAll by remember { mutableStateOf(true) }
    var cAll by remember { mutableStateOf(false) }
    var code by remember { mutableStateOf("") }
    val sw = SwitchDefaults.colors(checkedTrackColor = a.c1, checkedThumbColor = Color.White, uncheckedTrackColor = Color(0x33FFFFFF), uncheckedThumbColor = Color.White, uncheckedBorderColor = Color.Transparent, checkedBorderColor = Color.Transparent)
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(bottom = LocalBottomPad.current)) {
        Title(tr("Комнаты", "Rooms", "Кімнати", "Miestnosti"), Modifier.statusBarsPadding(), inset = 64.dp)
        Text(tr("Слушайте вместе из разных стран: один трек, одна позиция, общая очередь с голосованием, реакции и сообщения. Звук у каждого свой, плеер держит всех в такт.", "Listen together from different countries: one track, one position, a shared queue with voting, reactions and messages. Everyone plays their own audio; the player keeps everyone in time.", "Слухайте разом з різних країн: один трек, одна позиція, спільна черга з голосуванням, реакції й повідомлення. Звук у кожного свій, плеєр тримає всіх у такт.", "Počúvajte spolu z rôznych krajín: jedna skladba, jedna pozícia, spoločný front s hlasovaním, reakcie a správy."), color = Pal.dim, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 20.dp))
        if (eco.roomErr.isNotEmpty()) Text(eco.roomErr, color = Color(0xFFFF8A9A), fontSize = 13.sp, modifier = Modifier.padding(20.dp))
        Card(tr("Новая комната", "New room", "Нова кімната", "Nová miestnosť")) {
            GlassField(name, { name = it }, Modifier.fillMaxWidth(), placeholder = { Text(tr("Название", "Name", "Назва", "Názov")) }, singleLine = true)
            for ((label, get, set) in listOf(
                Triple(tr("Открытая", "Public", "Відкрита", "Verejná"), pub) { v: Boolean -> pub = v },
                Triple(tr("Очередь пополняют все", "Everyone can add to the queue", "Чергу поповнюють усі", "Do frontu pridáva každý"), qAll) { v: Boolean -> qAll = v },
                Triple(tr("Плеером управляют все", "Everyone controls the player", "Плеєром керують усі", "Prehrávač ovláda každý"), cAll) { v: Boolean -> cAll = v },
            )) Row(verticalAlignment = Alignment.CenterVertically) {
                Text(label, color = Pal.text, fontSize = 14.sp, modifier = Modifier.weight(1f))
                Switch(get, set, colors = sw)
            }
            Pill(if (eco.roomBusy) tr("Создаю…", "Creating…", "Створюю…", "Vytváram…") else tr("Создать", "Create", "Створити", "Vytvoriť"), enabled = !eco.roomBusy) { eco.create(name, pub, qAll, cAll) }
        }
        Card(tr("Войти по коду", "Join by code", "Увійти за кодом", "Vstúpiť s kódom")) {
            GlassField(code, { code = it.uppercase() }, Modifier.fillMaxWidth(), placeholder = { Text("ABC234") }, singleLine = true, keyboardActions = KeyboardActions(onDone = { eco.join(code) }))
            Pill(tr("Войти", "Join", "Увійти", "Vstúpiť"), enabled = code.isNotBlank() && !eco.roomBusy) { eco.join(code) }
        }
        Card(tr("Открытые комнаты", "Public rooms", "Відкриті кімнати", "Verejné miestnosti")) {
            if (eco.publics.isEmpty()) Text(tr("Сейчас открытых комнат нет", "No public rooms right now", "Зараз відкритих кімнат немає", "Teraz nie sú verejné miestnosti"), color = Pal.faint, fontSize = 12.sp)
            for (p in eco.publics) Row(Modifier.fillMaxWidth().clickable { eco.join(p.code) }.padding(vertical = 8.dp)) {
                Column {
                    Text(p.name, color = Pal.text, fontSize = 14.sp, fontWeight = FontWeight.Bold)
                    Text("${p.owner} · ${p.code}", color = Pal.faint, fontSize = 11.sp)
                }
            }
        }
    }
}

@UnstableApi
@Composable
private fun RoomInside(vm: AppState, r: Room) {
    val eco = vm.eco
    val a = LocalAccent.current
    val me = eco.me
    var msg by remember { mutableStateOf("") }
    val sw = SwitchDefaults.colors(checkedTrackColor = a.c1, checkedThumbColor = Color.White, uncheckedTrackColor = Color(0x33FFFFFF), uncheckedThumbColor = Color.White, uncheckedBorderColor = Color.Transparent, checkedBorderColor = Color.Transparent)
    LazyColumn(Modifier.fillMaxSize(), contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = LocalBottomPad.current)) {
        item {
            Row(Modifier.statusBarsPadding().padding(start = 64.dp, end = 16.dp, top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(r.meta.name.ifEmpty { tr("Комната", "Room", "Кімната", "Miestnosť") }, color = Pal.text, fontSize = 26.sp, fontWeight = FontWeight.Black, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(tr("Код ${r.code} · ${r.members.size} в комнате", "Code ${r.code} · ${r.members.size} in the room", "Код ${r.code} · ${r.members.size} у кімнаті", "Kód ${r.code} · ${r.members.size} v miestnosti"), color = Pal.dim, fontSize = 13.sp)
                }
                Icon(Icons.Rounded.ContentCopy, null, tint = Pal.dim, modifier = Modifier.size(24.dp).clickable { vm.copyText(r.code) })
            }
        }
        item {
            Row(Modifier.padding(16.dp).fillMaxWidth().liquid(RoundedCornerShape(22.dp)).padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
                val t = r.state?.track
                if (t != null) Cover(t.cover, 52.dp, 12.dp)
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(t?.title ?: tr("Пока тихо — включи что-нибудь", "Quiet for now — play something", "Поки тихо — увімкни щось", "Zatiaľ ticho — pusti niečo"), color = Pal.text, fontSize = 15.sp, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (t != null) Text(t.artist, color = Pal.dim, fontSize = 13.sp, maxLines = 1)
                }
                Text(
                    when { r.control(me) -> tr("ты управляешь", "you're in control", "ти керуєш", "ovládaš ty"); kotlin.math.abs(eco.drift) < 200 -> tr("в такт", "in sync", "у такт", "v rytme"); else -> tr("догоняю ${eco.drift} мс", "catching up ${eco.drift} ms", "наздоганяю ${eco.drift} мс", "doháňam ${eco.drift} ms") },
                    color = a.c2, fontSize = 11.sp, fontWeight = FontWeight.Bold,
                )
            }
        }
        item {
            Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (r.canQueue(me)) vm.current?.let { cur -> Pill(tr("Предложить играющий", "Suggest what's playing", "Запропонувати поточний", "Navrhnúť hrajúcu"), primary = false) { eco.add(cur) } }
                Pill(if (r.owner(me)) tr("Закрыть", "Close", "Закрити", "Zatvoriť") else tr("Выйти", "Leave", "Вийти", "Odísť"), danger = true) { eco.leave() }
            }
        }
        // очередь того, кто управляет: у всех одна и грузится заранее
        val up = r.state?.upnext ?: emptyList()
        item { Section(tr("Дальше играет", "Up next", "Далі грає", "Ďalej hrá")) }
        if (up.isEmpty()) item { Dim(tr("Очередь пуста", "The queue is empty", "Черга порожня", "Front je prázdny"), Modifier.padding(horizontal = 20.dp)) }
        items(up.size.coerceAtMost(30)) { i ->
            val t = up[i]
            Row(Modifier.padding(horizontal = 16.dp, vertical = 3.dp).fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Cover(t.cover, 42.dp, 8.dp)
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f)) {
                    Text(t.title, color = Pal.text, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(t.artist, color = Pal.faint, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
        item {
            Section(tr("Предложения", "Suggestions", "Пропозиції", "Návrhy"))
            Text(tr("Голосуй: самое популярное встанет следующим за полминуты до конца трека. Предлагать можно и из «Вселенной».", "Vote: the most popular one plays next, queued 30 s before the track ends. You can suggest from the Universe too.", "Голосуй: найпопулярніше стане наступним за пів хвилини до кінця треку. Пропонувати можна й із «Всесвіту».", "Hlasuj: najobľúbenejšia pôjde ako ďalšia 30 s pred koncom skladby."), color = Pal.faint, fontSize = 12.sp, modifier = Modifier.padding(horizontal = 20.dp))
        }
        if (r.queue.isEmpty()) item { Dim(tr("Предложений пока нет", "No suggestions yet", "Пропозицій поки немає", "Zatiaľ žiadne návrhy"), Modifier.padding(horizontal = 20.dp)) }
        val next = r.next()?.id
        items(r.queue.size) { i ->
            val q = r.queue[i]
            Row(Modifier.padding(horizontal = 16.dp, vertical = 3.dp).fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(if (q.id == next) a.c1.copy(alpha = 0.16f) else Color(0x0AFFFFFF)).padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
                Cover(q.track.cover, 40.dp, 8.dp)
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f)) {
                    Text(q.track.title, color = Pal.text, fontSize = 14.sp, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text("${q.track.artist} · ${tr("добавил(а)", "by", "додав(ла)", "pridal(a)")} ${q.by_name}${if (q.id == next) " · " + tr("следующий", "next", "наступний", "ďalšia") else ""}", color = Pal.faint, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Row(Modifier.clip(CircleShape).background(if (q.mine) a.c1.copy(alpha = 0.4f) else Color(0x14FFFFFF)).clickable { eco.vote(q.id, !q.mine) }.padding(horizontal = 10.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(if (q.mine) Icons.Rounded.Favorite else Icons.Rounded.FavoriteBorder, null, tint = Color.White, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(4.dp))
                    Text("${q.votes}", color = Color.White, fontSize = 12.sp)
                }
                if (r.owner(me)) {
                    Icon(Icons.Rounded.KeyboardArrowUp, null, tint = Pal.dim, modifier = Modifier.padding(start = 6.dp).size(26.dp).clickable { eco.requeue(q.id, i - 1) })
                    Icon(Icons.Rounded.KeyboardArrowDown, null, tint = Pal.dim, modifier = Modifier.size(26.dp).clickable { eco.requeue(q.id, i + 1) })
                }
                if (r.owner(me) || q.by == me) Icon(Icons.Rounded.Close, null, tint = Pal.dim, modifier = Modifier.padding(start = 4.dp).size(22.dp).clickable { eco.unqueue(q.id) })
            }
        }
        item { Section(tr("В комнате", "In the room", "У кімнаті", "V miestnosti")) }
        items(r.members.size) { i ->
            val m = r.members[i]
            val online = eco.room?.let { System.currentTimeMillis() - m.seen < 60_000 || m.uid == me } ?: false
            Row(Modifier.padding(horizontal = 20.dp, vertical = 5.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(9.dp).clip(CircleShape).background(if (online) Color(0xFF4ADE80) else Color(0xFF6B7280)))
                Spacer(Modifier.width(10.dp))
                Text(m.name + (if (m.role == "owner") " · " + tr("владелец", "owner", "власник", "vlastník") else "") + (if (m.uid == me) " · " + tr("ты", "you", "ти", "ty") else ""), color = Pal.text, fontSize = 14.sp, modifier = Modifier.weight(1f))
                if (r.owner(me) && m.uid != me) Icon(Icons.Rounded.Close, null, tint = Pal.faint, modifier = Modifier.size(20.dp).clickable { eco.kick(m.uid) })
            }
        }
        if (r.owner(me)) item {
            Card(tr("Настройки комнаты", "Room settings", "Налаштування кімнати", "Nastavenia miestnosti")) {
                Row(verticalAlignment = Alignment.CenterVertically) { Text(tr("Открытая", "Public", "Відкрита", "Verejná"), color = Pal.text, modifier = Modifier.weight(1f)); Switch(r.meta.public, { eco.meta(it, r.meta.queue == "all", r.meta.control == "all") }, colors = sw) }
                Row(verticalAlignment = Alignment.CenterVertically) { Text(tr("Очередь пополняют все", "Everyone can add to the queue", "Чергу поповнюють усі", "Do frontu pridáva každý"), color = Pal.text, modifier = Modifier.weight(1f)); Switch(r.meta.queue == "all", { eco.meta(r.meta.public, it, r.meta.control == "all") }, colors = sw) }
                Row(verticalAlignment = Alignment.CenterVertically) { Text(tr("Плеером управляют все", "Everyone controls the player", "Плеєром керують усі", "Prehrávač ovláda každý"), color = Pal.text, modifier = Modifier.weight(1f)); Switch(r.meta.control == "all", { eco.meta(r.meta.public, r.meta.queue == "all", it) }, colors = sw) }
            }
        }
        item {
            Row(Modifier.padding(horizontal = 20.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(tr("Разговор", "Chat", "Розмова", "Rozhovor"), color = Pal.text, fontSize = 20.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
                Text(tr("поделиться вкусом", "share my taste", "поділитися смаком", "zdieľať vkus"), color = a.c1, fontSize = 13.sp, fontWeight = FontWeight.Bold, modifier = Modifier.clickable { eco.shareTaste() })
            }
        }
        items(r.chat.takeLast(60).size) { i ->
            val m = r.chat.takeLast(60)[i]
            Column(Modifier.padding(horizontal = 16.dp, vertical = 3.dp).fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(if (m.uid == me) a.c1.copy(alpha = 0.18f) else Color(0x0DFFFFFF)).padding(10.dp)) {
                Text(m.name + if (m.key.isNotEmpty() && m.key == vm.current?.key) " · " + tr("про этот трек", "about this track", "про цей трек", "o tejto skladbe") else "", color = Pal.faint, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                Text(if (m.text.isEmpty()) m.emoji else listOf(m.emoji, m.text).filter { it.isNotEmpty() }.joinToString(" "), color = Pal.text, fontSize = if (m.text.isEmpty()) 24.sp else 14.sp)
                if (m.emoji == "🪐") Text(tr("показать во Вселенной →", "show in the Universe →", "показати у Всесвіті →", "ukázať vo Vesmíre →"), color = a.c1, fontSize = 12.sp, fontWeight = FontWeight.Bold, modifier = Modifier.clickable {
                    vm.go(app.blackhole.Screen.Universe)
                    eco.focusName(m.text)
                })
            }
        }
        item {
            Row(Modifier.padding(horizontal = 16.dp, vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                for (e in listOf("🔥", "❤️", "😂", "😮", "👏", "🎧")) Text(e, fontSize = 22.sp, modifier = Modifier.clip(CircleShape).background(Color(0x14FFFFFF)).clickable { eco.say("", e) }.padding(8.dp))
            }
            GlassField(
                msg, { msg = it }, Modifier.padding(horizontal = 16.dp).fillMaxWidth(), placeholder = { Text(tr("Написать…", "Write…", "Написати…", "Napísať…")) }, singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send), keyboardActions = KeyboardActions(onSend = { if (msg.isNotBlank()) { eco.say(msg); msg = "" } }),
            )
        }
    }
}

