package app.blackhole.ui

import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.foundation.clickable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Slider
import android.content.Intent
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Share
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.layer.drawLayer
import androidx.compose.ui.graphics.rememberGraphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.media3.common.util.UnstableApi
import app.blackhole.App
import app.blackhole.AppState
import app.blackhole.Eq
import app.blackhole.L
import app.blackhole.fmtLong
import app.blackhole.tr
import kotlinx.coroutines.launch
import java.io.File
import kotlin.math.roundToInt

/** Значок умного перемешивания: стрелки перемешки поменьше и три звёздочки (тот же рисунок, что на ПК) */
val SmartShuffle: androidx.compose.ui.graphics.vector.ImageVector by lazy {
    androidx.compose.ui.graphics.vector.ImageVector.Builder("SmartShuffle", 24.dp, 24.dp, 24f, 24f)
        .addPath(
            androidx.compose.ui.graphics.vector.addPathNodes("M8.47 12.14 4.33 8.00 3.20 9.13 l4.14 4.14 zM11.60 8.00 l1.63 1.63 L3.20 19.67 4.33 20.80 14.37 10.77 16.00 12.40 V8.00 zm0.26 7.53 -1.13 1.13 2.50 2.50 L11.60 20.80 H16.00 v-4.40 l-1.63 1.63 zM19.50 1.20L20.51 3.79L23.10 4.80L20.51 5.81L19.50 8.40L18.49 5.81L15.90 4.80L18.49 3.79zM13.20 0.50L13.73 1.87L15.10 2.40L13.73 2.93L13.20 4.30L12.67 2.93L11.30 2.40L12.67 1.87zM22.00 9.50L22.48 10.72L23.70 11.20L22.48 11.68L22.00 12.90L21.52 11.68L20.30 11.20L21.52 10.72z"),
            fill = androidx.compose.ui.graphics.SolidColor(Color.Black),
            pathFillType = androidx.compose.ui.graphics.PathFillType.EvenOdd,
        )
        .build()
}

/** Звёздочка трека, подмешанного умным перемешиванием — та же, что на ПК */
val Sparkle: androidx.compose.ui.graphics.vector.ImageVector by lazy {
    androidx.compose.ui.graphics.vector.ImageVector.Builder("Sparkle", 24.dp, 24.dp, 24f, 24f)
        .addPath(
            androidx.compose.ui.graphics.vector.addPathNodes("M12 1.5 14.6 9.4 22.5 12 14.6 14.6 12 22.5 9.4 14.6 1.5 12 9.4 9.4z"),
            fill = androidx.compose.ui.graphics.SolidColor(Color.Black),
        )
        .build()
}

/** Эквалайзер как на ПК: те же пресеты и 10 полос, шаг 1 дБ */
@UnstableApi
@Composable
fun EqScreen(vm: AppState) {
    val gains = Eq.gains
    val cur = Eq.presetIndex()
    Column(Modifier.fillMaxSize().statusBarsPadding().verticalScroll(rememberScrollState()).padding(bottom = LocalBottomPad.current)) {
        Title(tr("Эквалайзер и скорость", "Equalizer and speed", "Еквалайзер і швидкість", "Ekvalizér a rýchlosť"))
        LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            itemsIndexed(Eq.presets) { i, p ->
                FilterChip(selected = i == cur, onClick = { Eq.set(p.gains) }, label = { Text(if (L.lang == "ru" || L.lang == "uk") p.ru else p.en) })
            }
        }
        Row(Modifier.fillMaxWidth().padding(horizontal = 6.dp, vertical = 12.dp)) {
            val labels = listOf("31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k")
            gains.forEachIndexed { i, g ->
                Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(if (g > 0) "+${g.roundToInt()}" else "${g.roundToInt()}", color = if (g == 0f) Pal.faint else LocalAccent.current.c1, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                    VSlider(g, Modifier.fillMaxWidth().height(240.dp)) { v -> Eq.set(Eq.gains.toMutableList().also { it[i] = v }) }
                    Text(labels[i], color = Pal.dim, fontSize = 10.sp)
                }
            }
        }
        // скорость — как на ПК: ползунок 0.5–2×, быстрые кнопки и «сохранять тон»
        Section(tr("Скорость", "Speed", "Швидкість", "Rýchlosť"))
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            Slider(
                value = Eq.speed, onValueChange = { Eq.setSpeed(it) }, valueRange = 0.5f..2f, modifier = Modifier.weight(1f),
                colors = SliderDefaults.colors(thumbColor = Color.White, activeTrackColor = LocalAccent.current.c1, inactiveTrackColor = Color(0x26FFFFFF)),
            )
            Text("%.2f×".format(Eq.speed), color = Pal.text, fontWeight = FontWeight.Bold, modifier = Modifier.width(56.dp).padding(start = 8.dp))
        }
        LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            itemsIndexed(listOf(0.75f, 0.85f, 1f, 1.15f, 1.25f, 1.5f)) { _, v ->
                FilterChip(selected = kotlin.math.abs(Eq.speed - v) < 0.01f, onClick = { Eq.setSpeed(v) }, label = { Text(if (v == 1f) "1×" else "$v×") })
            }
        }
        Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(tr("Сохранять тон", "Keep pitch", "Зберігати тон", "Zachovať výšku tónu"), color = Pal.text, fontWeight = FontWeight.Bold)
                Text(tr("Выключи — замедление опустит голос, ускорение поднимет: тот самый slowed и sped up.", "Turn it off and slowing down lowers the voice, speeding up raises it: the classic slowed and sped up sound.", "Вимкни — сповільнення опустить голос, прискорення підніме: той самий slowed і sped up.", "Vypni — spomalenie zníži hlas, zrýchlenie ho zvýši: klasický slowed a sped up."), color = Pal.dim, fontSize = 13.sp)
            }
            Switch(
                Eq.keepPitch, { Eq.setSpeed(Eq.speed, it) },
                colors = SwitchDefaults.colors(checkedTrackColor = LocalAccent.current.c1, checkedThumbColor = Color.White, uncheckedTrackColor = Color(0x26FFFFFF), uncheckedThumbColor = Color.White, uncheckedBorderColor = Color.Transparent),
            )
        }
        Dim(tr(
            "Эквалайзер: громкость не проседает — подъём частично уходит в запас, остальное ловит лимитер — как на компе.",
            "Loudness stays put: part of the boost goes into headroom, a limiter catches the rest — same as on the PC.",
            "Гучність не просідає: підйом частково йде в запас, решту ловить лімітер — як на компі.",
            "Hlasitosť neklesá: časť zosilnenia ide do rezervy, zvyšok zachytí limiter — ako na PC.",
        ))
    }
}

/** Вертикальный ползунок полосы: середина — 0 дБ, тянется пальцем, список под ним при этом не листается */
@Composable
private fun VSlider(v: Float, modifier: Modifier, onChange: (Float) -> Unit) {
    val a = LocalAccent.current
    val change by rememberUpdatedState(onChange)
    Canvas(
        modifier.pointerInput(Unit) {
            awaitEachGesture {
                fun at(y: Float) = change(((0.5f - y / size.height) * 2 * Eq.MAX_DB).coerceIn(-Eq.MAX_DB, Eq.MAX_DB).roundToInt().toFloat())
                val d = awaitFirstDown()
                d.consume()
                at(d.position.y)
                drag(d.id) {
                    it.consume()
                    at(it.position.y)
                }
            }
        },
    ) {
        val cx = size.width / 2
        val mid = size.height / 2
        val pad = 10.dp.toPx()
        val y = mid - v / Eq.MAX_DB * (mid - pad)
        drawLine(Color(0x26FFFFFF), Offset(cx, pad), Offset(cx, size.height - pad), 4.dp.toPx(), cap = StrokeCap.Round)
        drawLine(Color(0x40FFFFFF), Offset(cx - 6.dp.toPx(), mid), Offset(cx + 6.dp.toPx(), mid), 1.dp.toPx())
        drawLine(a.c1, Offset(cx, mid), Offset(cx, y), 4.dp.toPx(), cap = StrokeCap.Round)
        drawCircle(Color.White, 8.dp.toPx(), Offset(cx, y))
    }
}

/** Итоги месяца или года карточкой; «Поделиться» отдаёт её картинкой */
@UnstableApi
@Composable
fun RecapDialog(vm: AppState, close: () -> Unit) {
    val r = vm.recap
    val a = LocalAccent.current
    val layer = rememberGraphicsLayer()
    val scope = rememberCoroutineScope()
    val ctx = LocalContext.current
    Dialog(onDismissRequest = close) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Column(
                Modifier
                    .fillMaxWidth()
                    // всё, что нарисовано внутри, заодно пишется в слой — из него потом получается картинка
                    .drawWithContent {
                        layer.record { this@drawWithContent.drawContent() }
                        drawLayer(layer)
                    }
                    .clip(RoundedCornerShape(26.dp))
                    .background(Brush.linearGradient(listOf(a.deep, Color(0xFF0A0812), a.c2.copy(alpha = 0.35f).compositeOverBg())))
                    .padding(22.dp),
            ) {
                Text("GARGANTUA · " + if (vm.recapYear) tr("ИТОГИ ГОДА", "YEAR RECAP", "ПІДСУМКИ РОКУ", "ZHRNUTIE ROKA") else tr("ИТОГИ МЕСЯЦА", "MONTH RECAP", "ПІДСУМКИ МІСЯЦЯ", "ZHRNUTIE MESIACA"), color = Color(0xA0FFFFFF), fontSize = 11.sp, fontWeight = FontWeight.ExtraBold, letterSpacing = 2.sp)
                if (r == null) {
                    Text("…", color = Color.White, fontSize = 28.sp)
                } else {
                    val title = if (vm.recapYear) "${r.year}" else monthName(r.month) + " ${r.year}"
                    Text(title, color = Color.White, fontSize = 28.sp, fontWeight = FontWeight.Black)
                    Spacer(Modifier.height(6.dp))
                    Text(fmtLong(r.ms), color = a.c1, fontSize = 34.sp, fontWeight = FontWeight.Black)
                    val days = tr("дней с музыкой: ${r.active}", "days with music: ${r.active}", "днів з музикою: ${r.active}", "dní s hudbou: ${r.active}")
                    Text(
                        if (r.count > 0) days + " · " + tr("разных треков: ${r.count}", "different tracks: ${r.count}", "різних треків: ${r.count}", "rôznych skladieb: ${r.count}")
                        else days + "\n" + tr("Топ треков и артистов копится с этой версии — загляни позже.", "Top tracks and artists are counted from this version on — check back later.", "Топ треків і артистів збирається з цієї версії — зазирни пізніше.", "Top skladby a interpreti sa počítajú od tejto verzie — pozri sa neskôr."),
                        color = Color(0xB8FFFFFF), fontSize = 13.sp,
                    )
                    if (r.tracks.isNotEmpty()) {
                        Spacer(Modifier.height(12.dp))
                        Text(tr("ТРЕКИ", "TRACKS", "ТРЕКИ", "SKLADBY"), color = Color(0x80FFFFFF), fontSize = 11.sp, fontWeight = FontWeight.ExtraBold, letterSpacing = 1.5.sp)
                        r.tracks.forEachIndexed { i, t ->
                            Row(Modifier.padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                                Text("${i + 1}", color = a.c1, fontWeight = FontWeight.ExtraBold, modifier = Modifier.width(20.dp))
                                Column(Modifier.weight(1f)) {
                                    Text(t.track.title, color = Color.White, fontWeight = FontWeight.Bold, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    Text(t.track.artist, color = Color(0x90FFFFFF), fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                }
                                Text("×${t.plays}", color = Color(0xB8FFFFFF), fontSize = 12.sp)
                            }
                        }
                    }
                    if (r.artists.isNotEmpty()) {
                        Spacer(Modifier.height(12.dp))
                        Text(tr("АРТИСТЫ", "ARTISTS", "АРТИСТИ", "INTERPRETI"), color = Color(0x80FFFFFF), fontSize = 11.sp, fontWeight = FontWeight.ExtraBold, letterSpacing = 1.5.sp)
                        r.artists.forEachIndexed { i, ar ->
                            Row(Modifier.padding(top = 6.dp)) {
                                Text("${i + 1}", color = a.c2, fontWeight = FontWeight.ExtraBold, modifier = Modifier.width(20.dp))
                                Text(ar.name, color = Color.White, fontWeight = FontWeight.Bold, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                                Text(fmtLong(ar.ms), color = Color(0xB8FFFFFF), fontSize = 12.sp)
                            }
                        }
                    }
                }
            }
            Row(Modifier.padding(top = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                FilterChip(selected = !vm.recapYear, onClick = { vm.loadRecap(false) }, label = { Text(tr("Месяц", "Month", "Місяць", "Mesiac")) })
                FilterChip(selected = vm.recapYear, onClick = { vm.loadRecap(true) }, label = { Text(tr("Год", "Year", "Рік", "Rok")) })
                Pill(tr("Поделиться", "Share", "Поділитися", "Zdieľať"), Icons.Rounded.Share, enabled = r != null) {
                    scope.launch {
                        runCatching {
                            val f = File(ctx.cacheDir, "recap.png")
                            f.outputStream().use { layer.toImageBitmap().asAndroidBitmap().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
                            val send = Intent(Intent.ACTION_SEND).apply {
                                type = "image/png"
                                putExtra(Intent.EXTRA_STREAM, ctx.shareUri(f))
                                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                            }
                            ctx.startActivity(Intent.createChooser(send, "Gargantua"))
                        }.onFailure { App.toasts.tryEmit(tr("Не вышло: ${it.message}", "Failed: ${it.message}", "Не вийшло: ${it.message}", "Nevyšlo to: ${it.message}")) }
                    }
                }
            }
        }
    }
}

/** полупрозрачный цвет поверх фона карточки — градиенту нужен непрозрачный, иначе в картинке будет дыра */
private fun Color.compositeOverBg(): Color {
    val bg = Color(0xFF0A0812)
    return Color(red * alpha + bg.red * (1 - alpha), green * alpha + bg.green * (1 - alpha), blue * alpha + bg.blue * (1 - alpha), 1f)
}

private fun monthName(m: Int): String = listOf(
    tr("Январь", "January", "Січень", "Január"), tr("Февраль", "February", "Лютий", "Február"), tr("Март", "March", "Березень", "Marec"),
    tr("Апрель", "April", "Квітень", "Apríl"), tr("Май", "May", "Травень", "Máj"), tr("Июнь", "June", "Червень", "Jún"),
    tr("Июль", "July", "Липень", "Júl"), tr("Август", "August", "Серпень", "August"), tr("Сентябрь", "September", "Вересень", "September"),
    tr("Октябрь", "October", "Жовтень", "Október"), tr("Ноябрь", "November", "Листопад", "November"), tr("Декабрь", "December", "Грудень", "December"),
).getOrElse(m - 1) { "" }

/** Таймер сна: музыка плавно затихнет и встанет на паузу */
@UnstableApi
@Composable
fun SleepDialog(vm: AppState, close: () -> Unit) {
    val at by App.sleepAt.collectAsState()
    val left = ((at - System.currentTimeMillis()) / 60_000 + 1).coerceAtLeast(1)
    GlassDialog(close, tr("Таймер сна", "Sleep timer", "Таймер сну", "Časovač spánku"), buttons = {
        Pill(tr("Закрыть", "Close", "Закрити", "Zavrieť"), primary = false, onClick = close)
    }) {
        Text(
            when {
                at == -1L -> tr("Остановлюсь после этого трека.", "Stopping after this track.", "Зупинюся після цього треку.", "Zastavím po tejto skladbe.")
                at > 0 -> tr("Осталось около $left мин.", "About $left min left.", "Залишилось близько $left хв.", "Zostáva asi $left min.")
                else -> tr("Музыка плавно затихнет и встанет на паузу.", "Music fades out and pauses.", "Музика плавно затихне і стане на паузу.", "Hudba sa stíši a pozastaví.")
            },
            color = Pal.dim, fontSize = 13.sp,
        )
        Spacer(Modifier.height(10.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (m in listOf(15, 30, 60)) Pill(tr("$m мин", "$m min", "$m хв", "$m min"), primary = false) { vm.setSleep(m); close() }
        }
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Pill(tr("После трека", "After this track", "Після треку", "Po skladbe"), primary = false) { vm.setSleep(-1); close() }
            if (at != 0L) Pill(tr("Выключить", "Turn off", "Вимкнути", "Vypnúť"), primary = false, danger = true) { vm.setSleep(0); close() }
        }
    }
}

/** Заметки к моментам трека: новая встаёт на текущую секунду, тап по заметке — перемотка к ней */
@UnstableApi
@Composable
fun NotesDialog(vm: AppState, t: app.blackhole.Track, close: () -> Unit) {
    LaunchedEffect(t.key) { vm.loadNotes(t) }
    var text by remember { mutableStateOf("") }
    GlassDialog(close, tr("Заметки", "Notes", "Нотатки", "Poznámky"), buttons = {
        Pill(tr("Закрыть", "Close", "Закрити", "Zavrieť"), primary = false, onClick = close)
        Pill(tr("Добавить", "Add", "Додати", "Pridať"), enabled = text.isNotBlank()) { vm.addNote(t, text); text = "" }
    }) {
        if (vm.notes.isEmpty()) Text(tr("Заметка встанет на текущую секунду трека.", "The note lands on the current second of the track.", "Нотатка стане на поточну секунду треку.", "Poznámka sa uloží na aktuálnu sekundu skladby."), color = Pal.dim, fontSize = 13.sp)
        vm.notes.forEachIndexed { i, n ->
            Row(Modifier.fillMaxWidth().clickable { vm.seek((n.pos * 1000).toLong()) }.padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(app.blackhole.fmtDur(n.pos.toLong()).ifEmpty { "0:00" }, color = LocalAccent.current.c1, fontWeight = FontWeight.Bold, modifier = Modifier.width(52.dp))
                Text(n.text, color = Pal.text, fontSize = 14.sp, modifier = Modifier.weight(1f))
                IconButton(onClick = { vm.removeNote(t, i) }) { Icon(Icons.Rounded.Delete, null, tint = Pal.dim) }
            }
        }
        Spacer(Modifier.height(8.dp))
        GlassField(text, { text = it }, placeholder = { Text(tr("Что здесь?", "What's here?", "Що тут?", "Čo je tu?")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
    }
}
