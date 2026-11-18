package app.blackhole.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.media3.common.util.UnstableApi
import app.blackhole.AppState
import app.blackhole.GUESS_LEVELS
import app.blackhole.Track
import app.blackhole.tr

private val LEVEL_NAMES = listOf("0.1", "0.5", "1", "2", "5")

@Composable
private fun TrackPick(t: Track, sub: String = t.artist, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable(onClick = onClick).padding(vertical = 6.dp, horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Cover(t.cover, 44.dp, 8.dp)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(t.title, color = Pal.text, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (sub.isNotEmpty()) Text(sub, color = Pal.faint, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@UnstableApi
@Composable
fun GuessScreen(vm: AppState) {
    val g = vm.guess
    DisposableEffect(Unit) { onDispose { g.release() } }
    when (g.stage) {
        0 -> GuessSetup(vm)
        3 -> GuessLobby(vm)
        else -> GuessRound(vm)
    }
}

@UnstableApi
@Composable
private fun GuessSetup(vm: AppState) {
    val g = vm.guess
    var aq by remember { mutableStateOf("") }
    var code by remember { mutableStateOf("") }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = LocalBottomPad.current)) {
        item {
            Title(tr("Угадай песню", "Guess the song", "Вгадай пісню", "Uhádni pieseň"), Modifier.statusBarsPadding(), inset = 64.dp)
            Text(
                tr("Выбери плейлист или артиста — заиграет случайная песня: 0.1 секунды, потом 0.5, 1, 2 и 5. Узнал — начни писать название и выбери трек из поиска.", "Pick a playlist or an artist — a random song plays: 0.1 s, then 0.5, 1, 2 and 5. Know it? Start typing the title and pick the track.", "Обери плейлист або артиста — заграє випадкова пісня: 0.1 секунди, потім 0.5, 1, 2 і 5. Впізнав — почни писати назву й обери трек.", "Vyber playlist alebo interpreta — zahrá náhodná pieseň: 0,1 s, potom 0,5, 1, 2 a 5. Poznáš ju? Začni písať názov."),
                color = Pal.dim, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 20.dp),
            )
            if (g.error.isNotEmpty()) Text(g.error, color = Color(0xFFFF8A9A), fontSize = 13.sp, modifier = Modifier.padding(20.dp))
            Row(Modifier.padding(16.dp).horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                GlassChip(tr("Соло", "Solo", "Соло", "Sólo"), g.kind == 0) { g.kind = 0 }
                GlassChip(tr("1 на 1", "1 vs 1", "1 на 1", "1 na 1"), g.kind == 1) { g.kind = 1 }
            }
        }
        if (g.kind == 1) item {
            Column(Modifier.padding(horizontal = 16.dp).fillMaxWidth().liquid(RoundedCornerShape(22.dp)).padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(tr("Дуэль: у вас одни и те же песни, раунд берёт тот, кто угадал первым. Фрагмент длиннее — только когда «дальше» нажали оба. Есть код — входи; нет — выбери источник ниже.", "Duel: you both get the same songs, whoever guesses first takes the round. The clip gets longer only when both press “next”. Have a code — join; otherwise pick a source below.", "Дуель: у вас ті самі пісні, раунд бере той, хто вгадав першим. Фрагмент довший — лише коли «далі» натиснули обидва. Є код — заходь; нема — обери джерело нижче.", "Duel: rovnaké piesne pre oboch, kolo berie ten, kto uhádne prvý. Úryvok sa predĺži, až keď obaja stlačia „ďalej“."), color = Pal.dim, fontSize = 12.sp)
                GlassField(code, { code = it.uppercase() }, Modifier.fillMaxWidth(), placeholder = { Text(tr("Код дуэли", "Duel code", "Код дуелі", "Kód duelu")) }, singleLine = true, keyboardActions = KeyboardActions(onDone = { g.joinDuel(code) }))
                Pill(tr("Войти", "Join", "Увійти", "Vstúpiť"), enabled = code.isNotBlank() && !g.busy) { g.joinDuel(code) }
            }
        }
        item {
            Row(Modifier.padding(16.dp).horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                GlassChip(tr("Плейлист", "Playlist", "Плейлист", "Playlist"), g.mode == 0) { g.mode = 0 }
                GlassChip(tr("Артист", "Artist", "Артист", "Interpret"), g.mode == 1) { g.mode = 1 }
                if (g.busy) Text(tr("Готовлю…", "Preparing…", "Готую…", "Pripravujem…"), color = Pal.dim, fontSize = 13.sp)
            }
        }
        if (g.mode == 0) items(vm.playlists.filter { it.tracks.size >= 3 }) { p ->
            Row(Modifier.padding(horizontal = 16.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable { g.pickPlaylist(p) }.padding(vertical = 6.dp, horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Cover(p.tracks.firstOrNull { it.cover.isNotEmpty() }?.cover ?: "", 48.dp, 10.dp)
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(p.name, color = Pal.text, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(tr("${p.tracks.size} треков", "${p.tracks.size} tracks", "${p.tracks.size} треків", "${p.tracks.size} skladieb"), color = Pal.faint, fontSize = 12.sp)
                }
            }
        }
        if (g.mode == 1) {
            item {
                GlassField(aq, { aq = it; g.searchArtists(it) }, Modifier.padding(horizontal = 16.dp).fillMaxWidth(), placeholder = { Text(tr("Имя артиста", "Artist name", "Ім'я артиста", "Meno interpreta")) }, leadingIcon = { Icon(Icons.Rounded.Search, null) }, singleLine = true)
            }
            items(g.artists) { c ->
                Row(Modifier.padding(horizontal = 16.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable { g.pickArtist(c) }.padding(vertical = 6.dp, horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(48.dp).clip(CircleShape)) { Cover(c.cover, 48.dp, 24.dp) }
                    Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f)) {
                        Text(c.title, color = Pal.text, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        if (c.sub.isNotEmpty()) Text(c.sub, color = Pal.faint, fontSize = 12.sp, maxLines = 1)
                    }
                }
            }
        }
    }
}

@UnstableApi
@Composable
private fun GuessLobby(vm: AppState) {
    val g = vm.guess
    Column(Modifier.fillMaxSize().statusBarsPadding().padding(32.dp), verticalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterVertically), horizontalAlignment = Alignment.CenterHorizontally) {
        Text(tr("Код дуэли", "Duel code", "Код дуелі", "Kód duelu"), color = Pal.dim, fontSize = 14.sp)
        Text(g.duelCode, color = LocalAccent.current.c2, fontSize = 48.sp, fontWeight = FontWeight.Black, letterSpacing = 6.sp, modifier = Modifier.clickable { vm.copyText(g.duelCode) })
        Text(tr("Отправь его сопернику: «Угадай песню» → «1 на 1» → «Войти». Как только он войдёт, начнётся первый раунд.", "Send it to your opponent: “Guess the song” → “1 vs 1” → “Join”. The first round starts as soon as they join.", "Надішли його суперникові: «Вгадай пісню» → «1 на 1» → «Увійти». Щойно він зайде, почнеться перший раунд.", "Pošli ho súperovi: „Uhádni pieseň“ → „1 na 1“ → „Vstúpiť“."), color = Pal.dim, fontSize = 13.sp, textAlign = TextAlign.Center)
        Text(g.source, color = Pal.faint, fontSize = 12.sp)
        Pill(tr("Отменить", "Cancel", "Скасувати", "Zrušiť"), primary = false) { g.change() }
    }
}

@UnstableApi
@Composable
private fun GuessRound(vm: AppState) {
    val g = vm.guess
    val a = LocalAccent.current
    var q by remember { mutableStateOf("") }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = LocalBottomPad.current)) {
        item {
            Row(Modifier.statusBarsPadding().padding(start = 64.dp, end = 16.dp, top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(tr("Угадай песню", "Guess the song", "Вгадай пісню", "Uhádni pieseň"), color = Pal.text, fontSize = 24.sp, fontWeight = FontWeight.Black)
                    Text(g.source, color = Pal.dim, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(if (g.kind == 1) "${g.meScore} : ${g.oppScore}" else "${g.score}", color = a.c2, fontSize = 22.sp, fontWeight = FontWeight.Black)
                    Text(if (g.kind == 1) tr("ты · ${g.oppName}", "you · ${g.oppName}", "ти · ${g.oppName}", "ty · ${g.oppName}") else tr("очки · серия ${g.streak}", "points · streak ${g.streak}", "очки · серія ${g.streak}", "body · séria ${g.streak}"), color = Pal.faint, fontSize = 11.sp)
                }
            }
        }
        item {
            // пять делений: сколько открыто
            Row(Modifier.padding(16.dp).fillMaxWidth().height(34.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                LEVEL_NAMES.forEachIndexed { i, l ->
                    Box(
                        Modifier.weight(if (i == 4) 4f else if (i == 3) 2f else 1f).fillMaxSize().clip(RoundedCornerShape(8.dp))
                            .background(if (i < g.level) Color(0x14FFFFFF) else if (i == g.level) a.c1.copy(alpha = if (g.stage == 2) 0.4f else 0.7f) else Color(0x08FFFFFF)),
                        contentAlignment = Alignment.Center,
                    ) { Text("$l ${tr("с", "s", "с", "s")}", color = if (i <= g.level) Pal.text else Pal.faint, fontSize = 11.sp, fontWeight = FontWeight.Bold) }
                }
            }
        }
        item {
            Row(Modifier.padding(horizontal = 16.dp).fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(64.dp).clip(CircleShape).background(a.brush()).clickable { g.play() }, contentAlignment = Alignment.Center) {
                    Icon(Icons.Rounded.PlayArrow, null, tint = Color.White, modifier = Modifier.size(34.dp))
                }
                Spacer(Modifier.width(14.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        when {
                            g.stage == 1 && !g.ready -> tr("Готовлю фрагмент…", "Preparing the clip…", "Готую фрагмент…", "Pripravujem úryvok…")
                            g.stage == 1 -> tr("Фрагмент ${LEVEL_NAMES[g.level]} с", "Clip ${LEVEL_NAMES[g.level]} s", "Фрагмент ${LEVEL_NAMES[g.level]} с", "Úryvok ${LEVEL_NAMES[g.level]} s")
                            g.kind == 1 && g.won -> tr("Ты был первым!", "You were first!", "Ти був першим!", "Bol si prvý!")
                            g.kind == 1 && g.winner.isNotEmpty() -> tr("Раунд взял(а) ${g.winner}", "${g.winner} took the round", "Раунд узяв(ла) ${g.winner}", "Kolo vyhral(a) ${g.winner}")
                            g.won -> tr("Угадал! +${g.gained}", "Got it! +${g.gained}", "Вгадав! +${g.gained}", "Uhádol si! +${g.gained}")
                            else -> tr("Не угадал", "Missed", "Не вгадав", "Neuhádol")
                        },
                        color = Pal.text, fontSize = 16.sp, fontWeight = FontWeight.Bold,
                    )
                    if (g.stage == 1 && g.wrong.isNotEmpty()) Text(g.wrong, color = Color(0xFFFF8A9A), fontSize = 12.sp)
                    if (g.kind == 1 && g.stage == 1 && (g.meSkip || g.oppSkip)) Text(if (g.meSkip) tr("Ждём, пока «дальше» нажмёт соперник", "Waiting for your opponent to press “next”", "Чекаємо, поки «далі» натисне суперник", "Čakáme, kým súper stlačí „ďalej“") else tr("Соперник хочет фрагмент подлиннее", "Your opponent wants a longer clip", "Суперник хоче довший фрагмент", "Súper chce dlhší úryvok"), color = a.c1, fontSize = 12.sp)
                }
            }
        }
        if (g.stage == 1) {
            item {
                Row(Modifier.padding(16.dp)) {
                    Pill(if (g.level < 4) tr("Не знаю — дальше", "Don't know — next", "Не знаю — далі", "Neviem — ďalej") else tr("Сдаюсь", "Give up", "Здаюся", "Vzdávam"), primary = false, enabled = !(g.kind == 1 && g.meSkip)) { g.skip() }
                }
                GlassField(
                    q, { q = it; g.search(it) }, Modifier.padding(horizontal = 16.dp).fillMaxWidth(),
                    placeholder = { Text(tr("Название трека…", "Track title…", "Назва треку…", "Názov skladby…")) }, leadingIcon = { Icon(Icons.Rounded.Search, null) }, singleLine = true,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done), keyboardActions = KeyboardActions(onDone = { g.typed(q); q = "" }),
                )
            }
            if (g.searching && g.results.isEmpty()) item { Dim(tr("Ищу…", "Searching…", "Шукаю…", "Hľadám…"), Modifier.padding(20.dp)) }
            items(g.results) { t -> Box(Modifier.padding(horizontal = 12.dp)) { TrackPick(t) { g.pick(t); q = "" } } }
        }
        if (g.stage == 2) item {
            val t = g.cur
            Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                if (t != null) Cover(t.cover, 110.dp, 16.dp)
                Spacer(Modifier.width(16.dp))
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(t?.title ?: "", color = Pal.text, fontSize = 20.sp, fontWeight = FontWeight.Black, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    Text(t?.artist ?: "", color = Pal.dim, fontSize = 14.sp)
                    if (g.kind == 0 || g.host) Pill(tr("Следующая", "Next", "Наступна", "Ďalšia")) { g.nextRound() }
                    else Text(tr("Следующую включит ${g.oppName}", "${g.oppName} starts the next one", "Наступну ввімкне ${g.oppName}", "Ďalšiu pustí ${g.oppName}"), color = Pal.dim, fontSize = 13.sp)
                    Pill(tr("Слушать целиком", "Play in full", "Слухати повністю", "Počúvať celú"), primary = false) { g.playFull() }
                }
            }
        }
        item {
            Row(Modifier.padding(16.dp)) { Pill(tr("Сменить", "Change", "Змінити", "Zmeniť"), primary = false) { g.change() } }
        }
    }
}
