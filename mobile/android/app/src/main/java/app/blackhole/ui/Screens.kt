package app.blackhole.ui

import androidx.compose.animation.togetherWith
import androidx.compose.foundation.lazy.grid.items as gridItems
import androidx.compose.foundation.layout.imePadding
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.foundation.border
import androidx.compose.material.icons.rounded.Key
import androidx.compose.material.icons.rounded.Public
import androidx.compose.material.icons.rounded.Fingerprint
import androidx.compose.material.icons.rounded.Groups
import androidx.compose.ui.draw.shadow
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.horizontalScroll
import kotlinx.coroutines.launch
import androidx.compose.material.icons.rounded.Download
import androidx.compose.runtime.collectAsState
import app.blackhole.YtAuth
import androidx.compose.material.icons.rounded.PhotoCamera
import androidx.compose.material.icons.rounded.PhotoLibrary
import androidx.compose.material.icons.rounded.QrCode2
import androidx.compose.material.icons.rounded.Favorite
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowRight
import androidx.compose.material.icons.rounded.MoreHoriz
import androidx.compose.material.icons.rounded.Radio
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.BarChart
import androidx.compose.material.icons.rounded.Clear
import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.material.icons.rounded.Link
import androidx.compose.material.icons.automirrored.rounded.Logout
import androidx.compose.material.icons.rounded.MusicNote
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Shuffle
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.media3.common.util.UnstableApi
import app.blackhole.Album
import app.blackhole.App
import app.blackhole.FavArtist
import app.blackhole.AppState
import app.blackhole.Artist
import app.blackhole.L
import app.blackhole.Screen
import app.blackhole.Track
import app.blackhole.fmtLong
import app.blackhole.tr
import app.blackhole.LIKED
import app.blackhole.LIKED_ART
import app.blackhole.plName

// ── вход ──

@UnstableApi
@Composable
fun AuthScreen(vm: AppState) {
    var mail by remember { mutableStateOf("") }
    var pw by remember { mutableStateOf("") }
    var pw2 by remember { mutableStateOf("") }
    var local by remember { mutableStateOf("") }
    val a = LocalAccent.current
    // свой живой фон цвета темы: стекло полей и кнопок преломляет его
    val bg = rememberGlassLayer()
    Box(Modifier.fillMaxSize().background(Pal.bg)) {
    AppBackground("", Modifier.fillMaxSize().glassSource(bg))
    CompositionLocalProvider(LocalBackdrop provides if (LocalLite.current) null else bg) {
    Column(
        Modifier.fillMaxSize().statusBarsPadding().imePadding().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Spacer(Modifier.height(40.dp))
        Box(Modifier.size(64.dp).clip(CircleShape).background(a.brush()), contentAlignment = Alignment.Center) {
            Box(Modifier.size(30.dp).clip(CircleShape).background(Color.Black))
        }
        Text(
            when {
                vm.signedIn && !vm.verified -> tr("Подтверди почту", "Confirm your email", "Підтверди пошту", "Potvrď e-mail")
                vm.register -> tr("Регистрация", "Sign up", "Реєстрація", "Registrácia")
                else -> tr("Вход в Gargantua", "Sign in to Gargantua", "Вхід у Gargantua", "Prihlásenie do Gargantua")
            },
            color = Color.White, fontSize = 32.sp, lineHeight = 38.sp, fontWeight = FontWeight.Black, letterSpacing = (-0.5).sp,
        )
        LangChips { vm.setLang(it) }
        if (vm.signedIn && !vm.verified) {
            Text(
                tr("Подтверди почту ${vm.email}: открой ссылку из письма. Плейлисты начнут синхронизироваться сразу после этого.", "Confirm your email ${vm.email}: open the link from the email. Playlists start syncing right after that.", "Підтверди пошту ${vm.email}: відкрий посилання з листа. Плейлисти почнуть синхронізуватися одразу після цього.", "Potvrď e-mail ${vm.email}: otvor odkaz z e-mailu. Playlisty sa začnú synchronizovať hneď potom."),
                color = Pal.dim, fontSize = 14.sp,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Pill(tr("Я подтвердил", "I've confirmed", "Я підтвердив", "Potvrdil som"), enabled = !vm.accBusy) { vm.checkVerified() }
                Pill(tr("Ещё раз", "Resend", "Ще раз", "Znova"), primary = false, enabled = !vm.accBusy) { vm.resendVerify() }
            }
            Pill(tr("Выйти", "Sign out", "Вийти", "Odhlásiť sa"), primary = false) { vm.signOut() }
        } else {
            Text(
                if (vm.register) tr("Почта и пароль от 6 символов. На почту придёт письмо со ссылкой для подтверждения.", "Email and a password of at least 6 characters. You'll get an email with a confirmation link.", "Пошта й пароль від 6 символів. На пошту прийде лист із посиланням для підтвердження.", "E-mail a heslo aspoň so 6 znakmi. Príde ti e-mail s odkazom na potvrdenie.")
                else tr("Плейлисты хранятся в аккаунте и одинаковые на телефоне и компьютере.", "Playlists live in your account and are the same on your phone and computer.", "Плейлисти зберігаються в акаунті й однакові на телефоні та комп'ютері.", "Playlisty sú uložené v účte a sú rovnaké na telefóne aj na počítači."),
                color = Pal.dim, fontSize = 14.sp,
            )
            GlassField(mail, { mail = it }, label = { Text(tr("Почта", "Email", "Пошта", "E-mail")) }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Next), modifier = Modifier.fillMaxWidth())
            GlassField(pw, { pw = it }, label = { Text(tr("Пароль", "Password", "Пароль", "Heslo")) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password), modifier = Modifier.fillMaxWidth())
            if (vm.register) {
                GlassField(pw2, { pw2 = it }, label = { Text(tr("Пароль ещё раз", "Repeat password", "Пароль ще раз", "Heslo znova")) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password), modifier = Modifier.fillMaxWidth())
                Pill(tr("Создать аккаунт", "Create account", "Створити акаунт", "Vytvoriť účet"), enabled = !vm.accBusy) {
                    if (pw != pw2) local = tr("Пароли не совпадают", "Passwords don't match", "Паролі не збігаються", "Heslá sa nezhodujú") else { local = ""; vm.signUp(mail, pw) }
                }
            } else {
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Pill(tr("Войти", "Sign in", "Увійти", "Prihlásiť sa"), enabled = !vm.accBusy) { local = ""; vm.signIn(mail, pw) }
                    Pill(tr("Забыл пароль", "Forgot password", "Забув пароль", "Zabudol som heslo"), primary = false, enabled = !vm.accBusy) { local = ""; vm.resetPassword(mail) }
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(if (vm.register) tr("Уже есть аккаунт?", "Already have an account?", "Вже є акаунт?", "Už máš účet?") else tr("Нет аккаунта?", "No account?", "Немає акаунта?", "Nemáš účet?"), color = Pal.dim, fontSize = 14.sp)
                Spacer(Modifier.width(6.dp))
                Text(
                    if (vm.register) tr("Войти", "Sign in", "Увійти", "Prihlásiť sa") else tr("Зарегистрироваться", "Sign up", "Зареєструватися", "Zaregistrovať sa"),
                    color = a.c1, fontSize = 14.sp, fontWeight = FontWeight.Bold,
                    modifier = Modifier.clickable { vm.register = !vm.register; local = ""; vm.accMsg = "" }.padding(vertical = 8.dp),
                )
            }
        }
        val msg = if (vm.accBusy) tr("Секунду…", "One sec…", "Секунду…", "Moment…") else local.ifEmpty { vm.accMsg }
        if (msg.isNotEmpty()) Text(msg, color = Pal.dim, fontSize = 14.sp)
    }
    }
    }
}

@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
fun LangChips(onPick: (String) -> Unit = {}) {
    val vmLang = L.lang
    // четыре языка в ряд на узком экране не лезут — переносятся
    androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for ((code, name) in listOf("ru" to "Русский", "en" to "English", "uk" to "Українська", "sk" to "Slovenčina")) {
            GlassChip(name, vmLang == code) { L.lang = code; onPick(code) }
        }
    }
}

// ── главная ──

@UnstableApi
@Composable
fun HomeScreen(vm: AppState) {
    LaunchedEffect(Unit) {
        vm.loadHome()
        vm.loadStats()
    }
    val hour = java.util.Calendar.getInstance().get(java.util.Calendar.HOUR_OF_DAY)
    val today = remember {
        java.text.SimpleDateFormat("EEEE, d MMMM", java.util.Locale.forLanguageTag(L.lang)).format(java.util.Date())
    }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = LocalBottomPad.current)) {
        item {
            // как «Слушать» в Apple Music: дата мелко сверху, приветствие крупно
            Column(Modifier.statusBarsPadding().padding(top = 10.dp)) {
                Text(today.uppercase(), color = Pal.dim, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.5.sp, modifier = Modifier.padding(start = 20.dp))
                Title(
                    when (hour) {
                        in 5..11 -> tr("Доброе утро", "Good morning", "Доброго ранку", "Dobré ráno")
                        in 12..17 -> tr("Добрый день", "Good afternoon", "Добрий день", "Dobrý deň")
                        in 18..23 -> tr("Добрый вечер", "Good evening", "Добрий вечір", "Dobrý večer")
                        else -> tr("Доброй ночи", "Good night", "Доброї ночі", "Dobrú noc")
                    },
                )
            }
        }
        if (vm.home.mixes.isEmpty()) {
            item {
                if (vm.homeLoading) Row(Modifier.padding(20.dp), verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.size(20.dp), color = Color.White, strokeWidth = 2.dp)
                    Spacer(Modifier.width(10.dp))
                    Text(tr("Собираю миксы…", "Building mixes…", "Збираю мікси…", "Skladám mixy…"), color = Pal.dim)
                } else Dim(tr("Послушай что-нибудь — тут появятся миксы под тебя: «Твой микс», миксы по любимым артистам и «Открытия».", "Play something — your mixes will show up here: Your mix, mixes from your favourite artists and Discover.", "Послухай щось — тут з'являться мікси для тебе: «Твій мікс», мікси за улюбленими артистами й «Відкриття».", "Vypočuj si niečo — objavia sa tu mixy pre teba: „Tvoj mix“, mixy podľa obľúbených interpretov a „Objavy“."))
            }
        } else {
            item { Section(tr("Миксы для тебя", "Mixes for you", "Мікси для тебе", "Mixy pre teba")) }
            item {
                LazyRow(contentPadding = PaddingValues(horizontal = 20.dp), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    items(vm.home.mixes) { m -> MixCard(m) { vm.go(Screen.Tracks(m.title, m.sub, m.cover, m.tracks)) } }
                }
            }
        }
        if (vm.home.similar.isNotEmpty()) {
            item { Section(tr("Похоже на ${vm.home.similar_to}", "Similar to ${vm.home.similar_to}", "Схоже на ${vm.home.similar_to}", "Podobné ako ${vm.home.similar_to}")) }
            item { CardRow(vm.home.similar, round = true) { vm.go(Screen.ArtistPage(it.id, it.title)) } }
        }
        val top = vm.stats.top.map { it.track }
        if (top.isNotEmpty()) {
            item { Section(tr("Чаще всего", "Most played", "Найчастіше", "Najčastejšie")) }
            // пятёрка на одной стеклянной карточке, как подборки у Apple
            item {
                Column(Modifier.padding(horizontal = 12.dp).fillMaxWidth().liquid(RoundedCornerShape(26.dp), refraction = 12.dp).padding(vertical = 6.dp)) {
                    top.take(5).forEachIndexed { i, t ->
                        if (i > 0) GroupDivider(76.dp)
                        TrackRow(vm, t, { vm.play(top, i) })
                    }
                }
            }
        }
        if (vm.playlists.isNotEmpty()) {
            item { Section(tr("Твои плейлисты", "Your playlists", "Твої плейлисти", "Tvoje playlisty")) }
            item {
                LazyRow(contentPadding = PaddingValues(horizontal = 20.dp), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    items(vm.playlists, key = { it.name }) { p -> PlaylistTile(p, Modifier.width(150.dp).animateItem()) { vm.go(Screen.Pl(p.name)) } }
                }
            }
        }
    }
}

/** Микс: большая обложка, снизу на ней — стеклянная плашка с названием, сквозь неё видна сама обложка */
@Composable
private fun MixCard(m: app.blackhole.Mix, onClick: () -> Unit) {
    val a = LocalAccent.current
    val layer = rememberGlassLayer()
    val shape = RoundedCornerShape(26.dp)
    Box(Modifier.width(230.dp).aspectRatio(0.8f).pressable(onClick = onClick).shadow(18.dp, shape).clip(shape)) {
        Box(Modifier.fillMaxSize().glassSource(layer).background(a.brush())) {
            if (m.cover.isNotEmpty()) CoverImage(m.cover, Modifier.fillMaxSize(), big = true)
        }
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .padding(8.dp)
                .fillMaxWidth()
                .liquid(RoundedCornerShape(20.dp), backdrop = layer, tint = Color(0x33000000), refraction = 12.dp)
                .padding(horizontal = 14.dp, vertical = 11.dp),
        ) {
            Text(m.title, color = Color.White, fontWeight = FontWeight.Bold, fontSize = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (m.sub.isNotEmpty()) Text(m.sub, color = Color.White.copy(alpha = 0.78f), fontSize = 12.sp, lineHeight = 15.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** Плейлист плиткой: большая обложка с тенью, под ней название и сколько треков */
@Composable
fun PlaylistTile(p: app.blackhole.Playlist, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val cover = if (p.name == LIKED) LIKED_ART else p.cover.ifEmpty { p.tracks.firstOrNull { it.cover.isNotEmpty() }?.cover ?: "" }
    val shape = RoundedCornerShape(18.dp)
    Column(modifier.pressable(onClick = onClick)) {
        Box(Modifier.fillMaxWidth().aspectRatio(1f).shadow(14.dp, shape).clip(shape).background(Pal.card), contentAlignment = Alignment.Center) {
            if (cover == LIKED_ART) LikedArt(Modifier.fillMaxSize())
            else {
                Icon(Icons.Rounded.MusicNote, null, tint = Pal.faint, modifier = Modifier.size(48.dp))
                if (cover.isNotEmpty()) CoverImage(cover, Modifier.fillMaxSize(), big = true)
            }
        }
        Spacer(Modifier.height(8.dp))
        Text(plName(p.name), color = Pal.text, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(tr("${p.tracks.size} треков", "${p.tracks.size} tracks", "${p.tracks.size} треків", "${p.tracks.size} skladieb"), color = Pal.dim, fontSize = 13.sp)
    }
}

@Composable
fun PlaylistRow(name: String, tracks: List<Track>, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Cover(if (name == LIKED) LIKED_ART else tracks.firstOrNull { it.cover.isNotEmpty() }?.cover ?: "", 52.dp, 12.dp)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(plName(name), color = Pal.text, fontWeight = FontWeight.SemiBold, fontSize = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(tr("${tracks.size} треков", "${tracks.size} tracks", "${tracks.size} треків", "${tracks.size} skladieb"), color = Pal.dim, fontSize = 13.sp)
        }
    }
}

// ── поиск ──

@UnstableApi
@Composable
fun SearchScreen(vm: AppState) {
    var q by remember { mutableStateOf(vm.query) }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = LocalBottomPad.current)) {
        item { Title(tr("Поиск", "Search", "Пошук", "Hľadať"), Modifier.statusBarsPadding()) }
        item {
            GlassField(
                q, { q = it; vm.query = it },
                placeholder = { Text(tr("Трек, артист или ссылка", "Track, artist or link", "Трек, артист або посилання", "Skladba, interpret alebo odkaz")) },
                leadingIcon = { Icon(Icons.Rounded.Search, null) },
                trailingIcon = { if (q.isNotEmpty()) IconButton(onClick = { q = ""; vm.query = "" }) { Icon(Icons.Rounded.Clear, null) } },
                singleLine = true,
                shape = RoundedCornerShape(14.dp),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = {
                    // ссылка на плейлист — сразу импорт
                    if (q.trim().startsWith("http")) vm.importUrl(q) else vm.search(q)
                }),
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
            )
        }
        item {
            Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                listOf(tr("Все", "All", "Усі", "Všetko"), "YT Music", "SoundCloud", tr("Яндекс", "Yandex", "Яндекс", "Yandex"), tr("В текстах", "In lyrics", "У текстах", "V textoch")).forEachIndexed { i, n ->
                    GlassChip(n, vm.filter == i) { vm.filter = i }
                }
                if (vm.searching > 0) CircularProgressIndicator(Modifier.size(18.dp), color = Color.White, strokeWidth = 2.dp)
            }
        }
        if (vm.filter == 0 && vm.cards.artists.isNotEmpty()) {
            item { Section(tr("Артисты", "Artists", "Артисти", "Interpreti")) }
            item { CardRow(vm.cards.artists, round = true) { vm.go(Screen.ArtistPage(it.id, it.title)) } }
        }
        if (vm.filter == 0 && vm.cards.albums.isNotEmpty()) {
            item { Section(tr("Альбомы", "Albums", "Альбоми", "Albumy")) }
            item { CardRow(vm.cards.albums, round = false) { vm.go(Screen.AlbumPage(it.id)) } }
        }
        val list = vm.shownResults
        if (list.isNotEmpty()) item { Section(tr("Треки", "Tracks", "Треки", "Skladby")) }
        itemsIndexed(list, key = { i, t -> "$i${t.key}" }) { i, t ->
            Box(Modifier.animateItem()) { TrackRow(vm, t, { vm.play(list, i) }) }
            // поиск по строчке: под треком — сама найденная строка
            if (vm.filter == 4) vm.lyrHits.getOrNull(i)?.let { Dim("«${it.line}»", Modifier.padding(start = 56.dp)) }
        }
        if (vm.filter == 4 && list.isEmpty() && vm.query.isNotBlank()) item {
            Dim(tr("Ищу среди текстов, которые ты уже открывал. Пока ничего не нашлось.", "Searching the lyrics you've already opened. Nothing found yet.", "Шукаю серед текстів, які ти вже відкривав. Поки нічого не знайшлося.", "Hľadám v textoch, ktoré si už otvoril. Zatiaľ nič."))
        }
    }
}

// ── библиотека ──

@UnstableApi
@Composable
fun LibraryScreen(vm: AppState) {
    var creating by remember { mutableStateOf(false) }
    var importing by remember { mutableStateOf(false) }
    // как «Медиатека» у Apple: плитки по две в ряд
    val full = androidx.compose.foundation.lazy.grid.GridItemSpan(2)
    androidx.compose.foundation.lazy.grid.LazyVerticalGrid(
        androidx.compose.foundation.lazy.grid.GridCells.Fixed(2),
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = LocalBottomPad.current),
        horizontalArrangement = Arrangement.spacedBy(16.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp),
    ) {
        item(span = { full }) {
            Row(Modifier.statusBarsPadding().padding(start = 0.dp), verticalAlignment = Alignment.CenterVertically) {
                Title(tr("Плейлисты", "Playlists", "Плейлисти", "Playlisty"), Modifier.weight(1f), inset = 0.dp)
                RoundButton(Icons.Rounded.Refresh, tr("Синхронизировать", "Sync", "Синхронізувати", "Synchronizovať"), 40.dp) { if (!vm.syncing) vm.sync(true) }
                Spacer(Modifier.width(10.dp))
                // «+»: новый, по ссылке, по QR из галереи или камерой
                var addMenu by remember { mutableStateOf(false) }
                val qr = rememberQrImport(vm)
                Box {
                    RoundButton(Icons.Rounded.Add, tr("Добавить", "Add", "Додати", "Pridať"), 40.dp) { addMenu = true }
                    GlassMenu(expanded = addMenu, onDismissRequest = { addMenu = false }) {
                        DropdownMenuItem(text = { Text(tr("Новый плейлист", "New playlist", "Новий плейлист", "Nový playlist")) }, leadingIcon = { MenuIcon(Icons.Rounded.Add) }, onClick = { addMenu = false; creating = true })
                        DropdownMenuItem(text = { Text(tr("Импорт по ссылке", "Import by link", "Імпорт за посиланням", "Import cez odkaz")) }, leadingIcon = { MenuIcon(Icons.Rounded.Link) }, onClick = { addMenu = false; importing = true })
                        DropdownMenuItem(text = { Text(tr("QR из галереи", "QR from gallery", "QR з галереї", "QR z galérie")) }, leadingIcon = { MenuIcon(Icons.Rounded.PhotoLibrary) }, onClick = { addMenu = false; qr.gallery() })
                        DropdownMenuItem(text = { Text(tr("QR камерой", "QR with camera", "QR камерою", "QR fotoaparátom")) }, leadingIcon = { MenuIcon(Icons.Rounded.PhotoCamera) }, onClick = { addMenu = false; qr.camera() })
                    }
                }
            }
        }
        // вселенная, вкус, комнаты — главные входы рядом с плейлистами
        item(span = { full }) {
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                GlassChip(tr("Вселенная", "Universe", "Всесвіт", "Vesmír"), false) { vm.go(Screen.Universe) }
                GlassChip(tr("Мой вкус", "My taste", "Мій смак", "Môj vkus"), false) { vm.go(Screen.Taste) }
                GlassChip(if (vm.eco.inRoom) tr("Комната · ${vm.eco.room?.code}", "Room · ${vm.eco.room?.code}", "Кімната · ${vm.eco.room?.code}", "Miestnosť · ${vm.eco.room?.code}") else tr("Комнаты", "Rooms", "Кімнати", "Miestnosti"), vm.eco.inRoom) { vm.go(Screen.Rooms) }
            }
        }
        if (!vm.loaded) {
            item(span = { full }) {
                Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                    Spacer(Modifier.width(10.dp))
                    Text(tr("Загружаю из аккаунта…", "Loading from your account…", "Завантажую з акаунта…", "Načítavam z účtu…"), color = Pal.dim)
                }
            }
        } else if (vm.playlists.isEmpty()) {
            item(span = { full }) { Dim(tr("Пока пусто. Создай плейлист или вставь ссылку на плейлист из Spotify, Apple Music, YouTube Music, SoundCloud или Яндекса.", "Nothing yet. Create a playlist or paste a link to a Spotify, Apple Music, YouTube Music, SoundCloud or Yandex playlist.", "Поки порожньо. Створи плейлист або встав посилання на плейлист зі Spotify, Apple Music, YouTube Music, SoundCloud чи Яндекса.", "Zatiaľ prázdno. Vytvor playlist alebo vlož odkaz na playlist zo Spotify, Apple Music, YouTube Music, SoundCloud alebo Yandexu.")) }
        }
        gridItems(vm.playlists, key = { it.name }) { p -> PlaylistTile(p) { vm.go(Screen.Pl(p.name)) } }
    }
    if (creating) NameDialog(tr("Новый плейлист", "New playlist", "Новий плейлист", "Nový playlist"), "") { n ->
        creating = false
        if (n != null) vm.go(Screen.Pl(vm.createPlaylist(n)))
    }
    if (importing) NameDialog(tr("Импорт по ссылке", "Import by link", "Імпорт за посиланням", "Import cez odkaz"), "", label = "Spotify / Apple Music / YT Music / SoundCloud / Yandex") { u ->
        importing = false
        if (!u.isNullOrBlank()) vm.importUrl(u)
    }
}

/** Список треков с шапкой в духе Apple Music: обложка на весь верх, название по центру, «перемешать» —
 *  белая «Слушать» — свои действия. Альбом (numbered) — номера вместо обложек */
@UnstableApi
@Composable
fun TrackList(vm: AppState, title: String, sub: String, cover: String, tracks: List<Track>, playlist: String? = null, numbered: Boolean = false, actions: @Composable () -> Unit = {}) {
    var q by remember { mutableStateOf("") }
    // поиск только помогает найти трек — играет весь список с него (номер в полном списке)
    val shown = tracks.withIndex().filter { q.isBlank() || (it.value.title + " " + it.value.artist).contains(q.trim(), ignoreCase = true) }
    val list = rememberLazyListState()
    val a = LocalAccent.current
    Box(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize(), state = list, contentPadding = PaddingValues(bottom = LocalBottomPad.current)) {
            item {
                Hero(cover.ifEmpty { tracks.firstOrNull { it.cover.isNotEmpty() }?.cover ?: "" }) {
                    Column(Modifier.align(Alignment.BottomCenter).padding(horizontal = 24.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                        Text(title, color = Color.White, fontSize = 28.sp, lineHeight = 32.sp, fontWeight = FontWeight.Black, letterSpacing = (-0.4).sp, textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        if (sub.isNotEmpty()) Text(sub, color = a.c1, fontSize = 19.sp, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(tr("${tracks.size} треков", "${tracks.size} tracks", "${tracks.size} треків", "${tracks.size} skladieb"), color = Pal.dim, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                    }
                }
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 16.dp),
                    horizontalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterHorizontally),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    RoundButton(Icons.Rounded.Shuffle, tr("Вперемешку", "Shuffle", "Упереміш", "Náhodne")) { if (tracks.isNotEmpty()) vm.play(tracks, 0, shuffled = true) }
                    PlayPill(tr("Слушать", "Play", "Слухати", "Počúvať"), tracks.isNotEmpty(), Modifier.weight(1f, fill = false).widthIn(min = 170.dp)) { vm.play(tracks, 0) }
                    actions()
                }
            }
            if (tracks.size > 8) item {
                GlassField(
                    q, { q = it }, placeholder = { Text(tr("Найти в списке", "Find in list", "Знайти у списку", "Hľadať v zozname")) },
                    leadingIcon = { Icon(Icons.Rounded.Search, null) }, singleLine = true, shape = RoundedCornerShape(14.dp),
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                )
            }
            items(shown, key = { (i, t) -> "$i${t.key}" }) { (i, t) ->
                Box(Modifier.animateItem()) { TrackRow(vm, t, { vm.play(tracks, i) }, inPlaylist = playlist, number = if (numbered) i + 1 else null) }
            }
        }
        FastScroll(list)
    }
}

@UnstableApi
@Composable
fun PlaylistScreen(vm: AppState, name: String) {
    val p = vm.playlists.firstOrNull { it.name == name }
    if (p == null) {
        Dim(tr("Плейлиста больше нет", "This playlist is gone", "Плейлиста більше немає", "Tento playlist už neexistuje"), Modifier.statusBarsPadding())
        return
    }
    var editing by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf(false) }
    var sharing by remember { mutableStateOf(false) }
    if (sharing) QrShareDialog(vm, p) { sharing = false }
    TrackList(vm, plName(p.name), p.desc, if (p.name == LIKED) LIKED_ART else p.cover, p.tracks, playlist = p.name) {
        var menu by remember { mutableStateOf(false) }
        Box {
            RoundButton(Icons.Rounded.MoreHoriz, null) { menu = true }
            GlassMenu(expanded = menu, onDismissRequest = { menu = false }) {
                val have by app.blackhole.Offline.have.collectAsState()
                val prog by app.blackhole.Offline.progress.collectAsState()
                val got = p.tracks.count { it.key in have }
                val pr = prog
                DropdownMenuItem(
                    text = {
                        Text(when {
                            pr != null -> tr("Качаю ${pr.first} из ${pr.second} — остановить", "Downloading ${pr.first} of ${pr.second} — stop", "Завантажую ${pr.first} з ${pr.second} — зупинити", "Sťahujem ${pr.first} z ${pr.second} — zastaviť")
                            got == p.tracks.size && got > 0 -> tr("Скачан целиком", "Fully downloaded", "Завантажено повністю", "Stiahnutý celý")
                            else -> tr("Скачать ($got из ${p.tracks.size} уже есть)", "Download ($got of ${p.tracks.size} done)", "Завантажити ($got з ${p.tracks.size} вже є)", "Stiahnuť ($got z ${p.tracks.size} hotovo)")
                        })
                    },
                    leadingIcon = { Icon(Icons.Rounded.Download, null) },
                    onClick = {
                        menu = false
                        if (pr != null) app.blackhole.Offline.stop() else app.blackhole.Offline.download(p.tracks)
                    },
                )
                if (p.name != LIKED) {
                    DropdownMenuItem(text = { Text(tr("Лайкнуть все", "Like all", "Вподобати всі", "Lajknúť všetky")) }, leadingIcon = { Icon(Icons.Rounded.Favorite, null) }, onClick = { menu = false; vm.likeAll(p.tracks) })
                }
                DropdownMenuItem(text = { Text(tr("Поделиться QR", "Share as QR", "Поділитися QR", "Zdieľať ako QR")) }, leadingIcon = { Icon(Icons.Rounded.QrCode2, null) }, onClick = { menu = false; sharing = true })
                // любимые не переименовать и не удалить: по имени их узнают и ПК, и телефон
                if (p.name != LIKED) {
                    DropdownMenuItem(text = { Text(tr("Переименовать", "Rename", "Перейменувати", "Premenovať")) }, leadingIcon = { Icon(Icons.Rounded.Edit, null) }, onClick = { menu = false; editing = true })
                    DropdownMenuItem(
                        text = { Text(tr("Удалить", "Delete", "Видалити", "Zmazať"), color = Color(0xFFE5294A)) },
                        leadingIcon = { Icon(Icons.Rounded.Delete, null, tint = Color(0xFFE5294A)) },
                        onClick = { menu = false; deleting = true },
                    )
                }
            }
        }
    }
    if (editing) NameDialog(tr("Название", "Name", "Назва", "Názov"), p.name) { n ->
        editing = false
        if (n != null) vm.renamePlaylist(p.name, n, p.desc)
    }
    if (deleting) GlassDialog({ deleting = false }, tr("Удалить «${p.name}»?", "Delete “${p.name}”?", "Видалити «${p.name}»?", "Zmazať „${p.name}“?"), buttons = {
        Pill(tr("Отмена", "Cancel", "Скасувати", "Zrušiť"), primary = false) { deleting = false }
        Pill(tr("Удалить", "Delete", "Видалити", "Zmazať"), danger = true) { deleting = false; vm.deletePlaylist(p.name) }
    }) {
        Text(tr("Плейлист пропадёт и с компьютера.", "It will be gone from your computer too.", "Плейлист зникне і з комп'ютера.", "Playlist zmizne aj z počítača."), color = Pal.dim)
    }
}

@UnstableApi
@Composable
fun AlbumScreen(vm: AppState, id: String) {
    var album by remember(id) { mutableStateOf<Album?>(null) }
    var failed by remember(id) { mutableStateOf(false) }
    LaunchedEffect(id) {
        album = vm.loadAlbum(id)
        failed = album == null
    }
    Loaded(album, failed, tr("Не загрузилось", "Couldn't load", "Не завантажилося", "Nenačítalo sa")) { al ->
        TrackList(vm, al.title, al.sub, al.cover, al.tracks, numbered = true) {
            RoundButton(Icons.Rounded.Add, tr("Сохранить как плейлист", "Save as playlist", "Зберегти як плейлист", "Uložiť ako playlist")) { vm.go(Screen.Pl(vm.createPlaylist(al.title, al.tracks))) }
        }
    }
}

/** Загрузка → содержимое: крутилка гаснет, экран мягко проявляется и чуть поднимается, а не выскакивает */
@Composable
fun <T : Any> Loaded(value: T?, failed: Boolean, failText: String, content: @Composable (T) -> Unit) {
    androidx.compose.animation.AnimatedContent(
        targetState = value ?: if (failed) FAILED else LOADING,
        transitionSpec = {
            (androidx.compose.animation.fadeIn(androidx.compose.animation.core.tween(320, 40)) +
                androidx.compose.animation.slideInVertically(androidx.compose.animation.core.spring(dampingRatio = 1f, stiffness = 300f)) { it / 24 }) togetherWith
                androidx.compose.animation.fadeOut(androidx.compose.animation.core.tween(140))
        },
        contentKey = { if (it === LOADING || it === FAILED) it else 0 },
        label = "loaded",
    ) { v ->
        when {
            v === LOADING -> Loading()
            v === FAILED -> Dim(failText, Modifier.statusBarsPadding())
            else -> @Suppress("UNCHECKED_CAST") content(v as T)
        }
    }
}

private val LOADING = Any()
private val FAILED = Any()

@Composable
fun Loading() {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
}

@UnstableApi
@Composable
fun ArtistScreen(vm: AppState, id: String, name: String) {
    var artist by remember(id, name) { mutableStateOf<Artist?>(null) }
    var failed by remember(id, name) { mutableStateOf(false) }
    LaunchedEffect(id, name) {
        artist = vm.loadArtist(id, name)
        failed = artist == null
    }
    Loaded(artist, failed, tr("Артист не нашёлся", "Artist not found", "Артиста не знайдено", "Interpret sa nenašiel")) { ar -> ArtistBody(vm, ar) }
}

@UnstableApi
@Composable
private fun ArtistBody(vm: AppState, ar: Artist) {
    Box(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = LocalBottomPad.current)) {
            item {
                // фото на весь верх, имя крупно по центру, под ним «перемешать» — большая ▶ — радио
                Hero(ar.photo, round = true) {
                    Text(
                        ar.name, color = Color.White, fontSize = 40.sp, lineHeight = 44.sp, fontWeight = FontWeight.Black, letterSpacing = (-0.6).sp, textAlign = TextAlign.Center, maxLines = 2,
                        modifier = Modifier.align(Alignment.BottomCenter).padding(horizontal = 20.dp),
                    )
                }
                if (ar.stats.isNotEmpty()) Text(ar.stats, color = Pal.dim, fontSize = 13.sp, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(top = 4.dp))
                Row(
                    Modifier.fillMaxWidth().padding(vertical = 16.dp),
                    horizontalArrangement = Arrangement.spacedBy(24.dp, Alignment.CenterHorizontally),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    RoundButton(Icons.Rounded.Shuffle, tr("Вперемешку", "Shuffle", "Упереміш", "Náhodne"), 52.dp) { if (ar.top.isNotEmpty()) vm.play(ar.top, 0, shuffled = true) }
                    Box(
                        Modifier.size(72.dp).pressable(ar.top.isNotEmpty()) { vm.play(ar.top, 0) }.shadow(16.dp, CircleShape).clip(CircleShape).background(Color.White),
                        contentAlignment = Alignment.Center,
                    ) { Icon(Icons.Rounded.PlayArrow, tr("Слушать", "Play", "Слухати", "Počúvať"), tint = Color.Black, modifier = Modifier.size(42.dp)) }
                    RoundButton(Icons.Rounded.Radio, tr("Радио", "Radio", "Радіо", "Rádio"), 52.dp) { ar.top.firstOrNull()?.let { vm.startRadio(it) } }
                }
            }
            if (ar.top.isNotEmpty()) item { BigSection(tr("Популярное", "Top Songs", "Популярне", "Populárne")) }
            itemsIndexed(ar.top) { i, t -> TrackRow(vm, t, { vm.play(ar.top, i) }) }
            if (ar.albums.isNotEmpty()) {
                item { BigSection(tr("Альбомы", "Albums", "Альбоми", "Albumy")) }
                item { CardRow(ar.albums, round = false) { vm.go(Screen.AlbumPage(it.id)) } }
            }
            if (ar.singles.isNotEmpty()) {
                item { BigSection(tr("Синглы", "Singles", "Сингли", "Single")) }
                item { CardRow(ar.singles, round = false) { vm.go(Screen.AlbumPage(it.id)) } }
            }
            if (ar.similar.isNotEmpty()) {
                item { BigSection(tr("Похожие", "Similar", "Схожі", "Podobní")) }
                item { CardRow(ar.similar, round = true) { vm.go(Screen.ArtistPage(it.id, it.title)) } }
            }
            if (ar.desc.isNotEmpty()) {
                item { BigSection(tr("Об артисте", "About", "Про артиста", "O interpretovi")) }
                item {
                    // описание — на стеклянной карточке
                    Text(ar.desc, color = Pal.text.copy(alpha = 0.85f), fontSize = 14.sp, lineHeight = 20.sp, modifier = Modifier.padding(horizontal = 16.dp).fillMaxWidth().liquid(RoundedCornerShape(22.dp), refraction = 10.dp).padding(16.dp))
                }
            }
        }
    }
}

/** Заголовок раздела крупно, со стрелкой — как «Top Songs ›» у Apple */
@Composable
fun BigSection(text: String) {
    Row(Modifier.padding(start = 16.dp, end = 16.dp, top = 22.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(text, color = Color.White, fontSize = 22.sp, fontWeight = FontWeight.Bold)
        Icon(Icons.AutoMirrored.Rounded.KeyboardArrowRight, null, tint = Pal.dim, modifier = Modifier.size(26.dp))
    }
}

// ── аккаунт, статистика, настройки ──

@UnstableApi
@Composable
fun AccountScreen(vm: AppState) {
    val a = LocalAccent.current
    Column(Modifier.fillMaxSize().statusBarsPadding().verticalScroll(rememberScrollState()).padding(bottom = LocalBottomPad.current + 16.dp)) {
        Title(tr("Аккаунт", "Account", "Акаунт", "Účet"))
        // карточка профиля, как Apple ID в настройках iOS
        Row(
            Modifier.padding(horizontal = 16.dp, vertical = 8.dp).fillMaxWidth().liquid(RoundedCornerShape(28.dp), refraction = 14.dp).padding(18.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(68.dp).shadow(12.dp, CircleShape).clip(CircleShape).background(a.brush()), contentAlignment = Alignment.Center) {
                Text(vm.email.take(1).uppercase().ifEmpty { "?" }, color = Color.White, fontSize = 30.sp, fontWeight = FontWeight.Black)
            }
            Spacer(Modifier.width(16.dp))
            Column(Modifier.weight(1f)) {
                Text(vm.email, color = Pal.text, fontSize = 18.sp, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    when {
                        vm.syncing -> tr("Синхронизирую…", "Syncing…", "Синхронізую…", "Synchronizujem…")
                        vm.syncedAt.isNotEmpty() -> tr("Синхронизировано в ${vm.syncedAt}", "Synced at ${vm.syncedAt}", "Синхронізовано о ${vm.syncedAt}", "Synchronizované o ${vm.syncedAt}")
                        else -> tr("Аккаунт Gargantua", "Gargantua account", "Акаунт Gargantua", "Účet Gargantua")
                    },
                    color = Pal.dim, fontSize = 13.sp,
                )
            }
        }
        GlassGroup(footer = tr("Плейлисты хранятся в аккаунте и одинаковые на телефоне и компьютере. Правки уходят на сервер сразу, свежие подтягиваются раз в минуту.", "Playlists live in your account and are the same on your phone and computer. Edits go to the server right away, fresh ones are pulled every minute.", "Плейлисти зберігаються в акаунті й однакові на телефоні та комп'ютері. Зміни йдуть на сервер одразу, свіжі підтягуються щохвилини.", "Playlisty sú uložené v účte a sú rovnaké na telefóne aj na počítači. Úpravy idú na server hneď, nové sa sťahujú raz za minútu.")) {
            NavRow(Icons.Rounded.Refresh, Color(0xFF0A84FF), if (vm.syncing) tr("Синхронизирую…", "Syncing…", "Синхронізую…", "Synchronizujem…") else tr("Синхронизировать", "Sync", "Синхронізувати", "Synchronizovať"), chevron = false) { if (!vm.syncing) vm.sync(true) }
            GroupDivider(62.dp)
            NavRow(Icons.Rounded.Key, Color(0xFFFF9F0A), tr("Сменить пароль", "Change password", "Змінити пароль", "Zmeniť heslo"), chevron = false) { vm.resetPassword(vm.email) }
        }
        GlassGroup {
            NavRow(Icons.Rounded.BarChart, Color(0xFFFF375F), tr("Статистика", "Stats", "Статистика", "Štatistiky")) { vm.go(Screen.Stats) }
            GroupDivider(60.dp)
            NavRow(Icons.Rounded.Public, Color(0xFF5E5CE6), tr("Вселенная", "Universe", "Всесвіт", "Vesmír")) { vm.go(Screen.Universe) }
            GroupDivider(60.dp)
            NavRow(Icons.Rounded.Fingerprint, Color(0xFFFF9F0A), tr("Мой вкус", "My taste", "Мій смак", "Môj vkus")) { vm.go(Screen.Taste) }
            GroupDivider(60.dp)
            NavRow(Icons.Rounded.Groups, Color(0xFF30D158), if (vm.eco.inRoom) tr("Комната · ${vm.eco.room?.code}", "Room · ${vm.eco.room?.code}", "Кімната · ${vm.eco.room?.code}", "Miestnosť · ${vm.eco.room?.code}") else tr("Комнаты", "Rooms", "Кімнати", "Miestnosti")) { vm.go(Screen.Rooms) }
            GroupDivider(62.dp)
            NavRow(Icons.Rounded.Settings, Color(0xFF8E8E93), tr("Настройки", "Settings", "Налаштування", "Nastavenia")) { vm.go(Screen.Settings) }
        }
        GlassGroup {
            Row(Modifier.fillMaxWidth().clickable { vm.signOut() }.padding(16.dp), horizontalArrangement = Arrangement.Center) {
                Text(tr("Выйти", "Sign out", "Вийти", "Odhlásiť sa"), color = Color(0xFFFF5A6E), fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
            }
        }
        if (vm.accMsg.isNotEmpty()) Dim(vm.accMsg, Modifier.padding(top = 10.dp))
    }
}

/** Строка группы-меню: значок в цветном квадрате, подпись, стрелка */
@Composable
fun NavRow(icon: androidx.compose.ui.graphics.vector.ImageVector, color: Color, text: String, chevron: Boolean = true, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
        RowIcon(icon, color)
        Spacer(Modifier.width(16.dp))
        Text(text, color = Pal.text, fontSize = 16.sp, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f))
        if (chevron) Icon(Icons.AutoMirrored.Rounded.KeyboardArrowRight, null, tint = Pal.faint)
    }
}

@UnstableApi
@Composable
fun StatsScreen(vm: AppState) {
    LaunchedEffect(Unit) { vm.loadStats() }
    val s = vm.stats
    val a = LocalAccent.current
    var recapOpen by remember { mutableStateOf(false) }
    if (recapOpen) RecapDialog(vm) { recapOpen = false }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = LocalBottomPad.current)) {
        item { Title(tr("Статистика", "Stats", "Статистика", "Štatistiky"), Modifier.statusBarsPadding()) }
        item {
            Row(Modifier.horizontalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, bottom = 10.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Pill(tr("История", "History", "Історія", "História"), primary = false) { vm.openHistory() }
                Pill(tr("Итоги месяца", "Month recap", "Підсумки місяця", "Zhrnutie mesiaca"), primary = false) { vm.recap = null; vm.loadRecap(false); recapOpen = true }
                Pill(tr("Итоги года", "Year recap", "Підсумки року", "Zhrnutie roka"), primary = false) { vm.recap = null; vm.loadRecap(true); recapOpen = true }
            }
        }
        item {
            Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for ((label, ms) in listOf(tr("Сегодня", "Today", "Сьогодні", "Dnes") to s.today, tr("Неделя", "Week", "Тиждень", "Týždeň") to s.week, tr("Месяц", "Month", "Місяць", "Mesiac") to s.month)) {
                    Column(Modifier.weight(1f).liquid(RoundedCornerShape(20.dp), refraction = 10.dp).padding(14.dp)) {
                        Text(label, color = Pal.dim, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                        Text(fmtLong(ms), color = Pal.text, fontWeight = FontWeight.Bold, fontSize = 16.sp)
                    }
                }
            }
        }
        item { Section(tr("Год", "Year", "Рік", "Rok")) }
        item {
            // клетки как на GitHub: колонка — неделя; с отступами карточки в экран влезают последние 23 недели
            val weeks = s.heat.chunked(7).takeLast(23)
            Row(Modifier.padding(horizontal = 16.dp).liquid(RoundedCornerShape(20.dp), refraction = 10.dp).padding(14.dp), horizontalArrangement = Arrangement.spacedBy(3.dp)) {
                for (w in weeks) Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    for (lv in w) Box(Modifier.size(10.dp).clip(RoundedCornerShape(2.dp)).background(if (lv == 0) Color(0x14FFFFFF) else a.c1.copy(alpha = 0.25f + lv * 0.18f)))
                }
            }
        }
        s.fav?.let { f -> item { FavArtistCard(vm, f, vm.favPhoto) } }
        if (s.artists.size > 1) {
            item { Section(tr("Артисты", "Artists", "Артисти", "Interpreti")) }
            items(s.artists.drop(1).take(4)) { ar ->
                Row(Modifier.fillMaxWidth().clickable { vm.go(Screen.ArtistPage("", ar.name)) }.padding(horizontal = 16.dp, vertical = 10.dp)) {
                    Text(ar.name, color = Pal.text, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(fmtLong(ar.ms), color = Pal.dim, fontSize = 13.sp)
                }
            }
        }
        if (s.top.isNotEmpty()) {
            item { Section(tr("Треки", "Tracks", "Треки", "Skladby")) }
            val list = s.top.map { it.track }
            itemsIndexed(s.top) { i, t -> TrackRow(vm, t.track, { vm.play(list, i) }, trailing = "${t.plays}×") }
        }
        item { Dim(tr("Всего: ${fmtLong(s.total)}", "Total: ${fmtLong(s.total)}", "Усього: ${fmtLong(s.total)}", "Spolu: ${fmtLong(s.total)}")) }
    }
}

/** Любимый артист: фото, живой фон из него (как в текстах), время вместе и его самый частый трек */
@UnstableApi
@Composable
private fun FavArtistCard(vm: AppState, f: FavArtist, photo: String) {
    val clock = remember { LyrClock() }
    val lite = LocalLite.current
    LaunchedEffect(lite) {
        if (lite) return@LaunchedEffect // в лайт-режиме фон карточки стоит
        var last = 0L
        while (true) withFrameNanos { n ->
            if (last != 0L) clock.bg += ((n - last) / 1e9f).coerceAtMost(0.1f) * 1.2f
            last = n
        }
    }
    Box(
        Modifier.padding(horizontal = 16.dp, vertical = 8.dp).fillMaxWidth().height(260.dp).shadow(18.dp, RoundedCornerShape(28.dp)).clip(RoundedCornerShape(28.dp))
            .clickable { vm.go(Screen.ArtistPage("", f.name)) },
    ) {
        LyrBackdrop(photo, clock)
        Box(Modifier.matchParentSize().background(Brush.verticalGradient(listOf(Color(0x22000000), Color(0xB0000000)))))
        Column(Modifier.align(Alignment.BottomStart).padding(20.dp)) {
            Box(Modifier.size(96.dp).clip(CircleShape).background(LocalAccent.current.c1), contentAlignment = Alignment.Center) {
                if (photo.isEmpty()) Text("#1", color = Color.White, fontSize = 24.sp, fontWeight = FontWeight.ExtraBold)
                else CoverImage(photo, Modifier.matchParentSize())
            }
            Spacer(Modifier.height(14.dp))
            Text(tr("ЛЮБИМЫЙ ИСПОЛНИТЕЛЬ", "FAVORITE ARTIST", "УЛЮБЛЕНИЙ ВИКОНАВЕЦЬ", "OBĽÚBENÝ INTERPRET"), color = Color.White.copy(alpha = 0.7f), fontSize = 11.sp, fontWeight = FontWeight.ExtraBold, letterSpacing = 2.sp)
            Text(f.name, color = Color.White, fontSize = 32.sp, fontWeight = FontWeight.Black, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val time = fmtLong(f.ms)
            Text(tr("$time вместе", "$time together", "$time разом", "$time spolu"), color = Color.White.copy(alpha = 0.88f), fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
            f.track?.let { t ->
                Text(
                    tr("Любимый трек — «${t.title}» · ${f.plays}×", "Favorite track — “${t.title}” · ${f.plays}×", "Улюблений трек — «${t.title}» · ${f.plays}×", "Obľúbená skladba — „${t.title}“ · ${f.plays}×"),
                    color = Color.White.copy(alpha = 0.7f), fontSize = 13.sp, maxLines = 2, overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@UnstableApi
@Composable
fun SettingsScreen(vm: AppState) {
    val a = LocalAccent.current
    val sw = androidx.compose.material3.SwitchDefaults.colors(
        checkedTrackColor = a.c1, checkedThumbColor = Color.White, checkedBorderColor = Color.Transparent,
        uncheckedTrackColor = Color(0x33FFFFFF), uncheckedThumbColor = Color.White, uncheckedBorderColor = Color.Transparent,
    )
    Column(Modifier.fillMaxSize().statusBarsPadding().verticalScroll(rememberScrollState()).padding(bottom = LocalBottomPad.current + 16.dp)) {
        Title(tr("Настройки", "Settings", "Налаштування", "Nastavenia"))

        // ── вид ──
        GlassGroup(tr("Внешний вид", "Appearance", "Вигляд", "Vzhľad")) {
            Column(Modifier.padding(start = 16.dp, top = 14.dp, bottom = 12.dp)) {
                Text(tr("Тема", "Theme", "Тема", "Téma"), color = Pal.text, fontSize = 16.sp, fontWeight = FontWeight.Medium)
                // кружки с градиентом темы, выбранный — в кольце
                Row(Modifier.horizontalScroll(rememberScrollState()).padding(top = 12.dp, end = 16.dp), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    ACCENTS.forEachIndexed { i, ac ->
                        Column(Modifier.width(56.dp).pressable { vm.setThemeId(i) }, horizontalAlignment = Alignment.CenterHorizontally) {
                            Box(
                                Modifier.size(46.dp).then(if (vm.theme == i && !vm.coverTheme) Modifier.border(2.dp, Color.White, CircleShape) else Modifier).padding(4.dp)
                                    .shadow(8.dp, CircleShape).clip(CircleShape).background(ac.brush()),
                            )
                            Text(ac.name(), color = if (vm.theme == i && !vm.coverTheme) Color.White else Pal.dim, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 5.dp))
                        }
                    }
                }
            }
            GroupDivider()
            GroupRow(
                tr("Цвета из обложки", "Colours from the cover", "Кольори з обкладинки", "Farby z obalu"),
                tr("Тема подстраивается под обложку того, что играет. Выберешь тему выше — выключится.", "The theme follows the cover of what's playing. Picking a theme above turns it off.", "Тема підлаштовується під обкладинку того, що грає. Обереш тему вище — вимкнеться.", "Téma sa prispôsobí obalu toho, čo hrá. Keď vyberieš tému vyššie, vypne sa."),
            ) { Switch(vm.coverTheme, { vm.setCoverThemeOn(it) }, colors = sw) }
            GroupDivider()
            GroupRow(
                tr("Лайт-режим", "Lite mode", "Лайт-режим", "Lite režim"),
                tr("Без жидкого стекла, живого фона и размытий: плашки простые, фон ровный, фон плеера не плавает. Для слабых телефонов и чтобы беречь батарею.", "No liquid glass, live background or blur: plain panels, a flat background, the player background stands still. For weaker phones and to save battery.", "Без рідкого скла, живого фону й розмиття: прості плашки, рівний фон, фон плеєра не пливе. Для слабких телефонів і щоб берегти батарею.", "Bez tekutého skla, živého pozadia a rozmazania: jednoduché panely, rovné pozadie, pozadie prehrávača sa nehýbe. Pre slabšie telefóny a na šetrenie batérie."),
            ) { Switch(vm.lite, { vm.setLiteOn(it) }, colors = sw) }
            GroupDivider()
            Column(Modifier.padding(16.dp)) {
                Text(tr("Язык", "Language", "Мова", "Jazyk"), color = Pal.text, fontSize = 16.sp, fontWeight = FontWeight.Medium, modifier = Modifier.padding(bottom = 10.dp))
                LangChips { vm.setLang(it) }
            }
        }

        // ── игры ──
        GlassGroup(tr("Игры", "Games", "Ігри", "Hry")) {
            GroupRow(tr("Угадай песню", "Guess the song", "Вгадай пісню", "Uhádni pieseň"), tr("Соло или 1 на 1: узнай песню по долям секунды", "Solo or 1 vs 1: name the song from a split second", "Соло або 1 на 1: впізнай пісню за частки секунди", "Sólo alebo 1 na 1: uhádni pieseň zo zlomku sekundy"), onClick = { vm.go(Screen.Guess) }) {}
        }

        // ── звук ──
        GlassGroup(tr("Звук", "Sound", "Звук", "Zvuk")) {
            GroupRow(
                tr("Дальше похожее", "Keep playing similar", "Далі схоже", "Ďalej podobné"),
                tr("Когда очередь заканчивается, музыка не замолкает — включаются похожие треки.", "When the queue runs out, similar tracks keep the music going.", "Коли черга закінчується, музика не замовкає — вмикаються схожі треки.", "Keď sa front minie, hudba neutíchne — pustia sa podobné skladby."),
            ) { Switch(vm.autoplay, { vm.setAutoplayOn(it) }, colors = sw) }
            GroupDivider()
            GroupRow(
                tr("Эквалайзер и скорость", "Equalizer and speed", "Еквалайзер і швидкість", "Ekvalizér a rýchlosť"),
                tr("10 полос и пресеты — те же, что на компе.", "10 bands and presets — the same as on the PC.", "10 смуг і пресети — ті самі, що на компі.", "10 pásiem a predvoľby — rovnaké ako na PC."),
                onClick = { vm.go(Screen.Eq) },
            ) { Icon(Icons.AutoMirrored.Rounded.KeyboardArrowRight, null, tint = Pal.faint) }
            GroupDivider()
            Column(Modifier.padding(16.dp)) {
                Text(tr("Кроссфейд", "Crossfade", "Кросфейд", "Crossfade"), color = Pal.text, fontSize = 16.sp, fontWeight = FontWeight.Medium)
                Text(tr("Конец трека плавно перетекает в начало следующего.", "The end of a track blends into the start of the next.", "Кінець треку плавно перетікає в початок наступного.", "Koniec skladby plynule prejde do ďalšej."), color = Pal.dim, fontSize = 13.sp, modifier = Modifier.padding(top = 2.dp))
                Row(Modifier.padding(top = 10.dp).horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (x in listOf(0, 3, 5, 8, 12)) GlassChip(if (x == 0) tr("Выкл", "Off", "Вимк", "Vyp") else tr("$x с", "$x s", "$x с", "$x s"), vm.xfade == x) { vm.setXfadeSecs(x) }
                }
            }
        }

        // ── память ──
        val have by app.blackhole.Offline.have.collectAsState()
        var offBytes by remember(have) { mutableStateOf(app.blackhole.Offline.size()) }
        // размер считаем при открытии экрана: кеш меняется только пока что-то играет
        var cached by remember { mutableStateOf(App.cache.cacheSpace) }
        GlassGroup(tr("Память", "Storage", "Пам'ять", "Úložisko")) {
            GroupRow(
                tr("Скачанное", "Downloads", "Завантажене", "Stiahnuté"),
                tr("Треков: ${have.size}, ${offBytes / 1048576} МБ. Играют без интернета и не вытесняются. Скачать плейлист — в его меню «•••».", "Tracks: ${have.size}, ${offBytes / 1048576} MB. They play offline and are never evicted. Download a playlist from its “•••” menu.", "Треків: ${have.size}, ${offBytes / 1048576} МБ. Грають без інтернету і не витісняються. Завантажити плейлист — у його меню «•••».", "Skladieb: ${have.size}, ${offBytes / 1048576} MB. Hrajú offline a nemažú sa. Playlist stiahneš v jeho menu „•••“."),
            ) {
                Pill(tr("Удалить", "Delete", "Видалити", "Zmazať"), primary = false, enabled = have.isNotEmpty()) {
                    app.blackhole.Offline.clear()
                    offBytes = 0
                }
            }
            GroupDivider()
            GroupRow(
                tr("Кэш треков", "Track cache", "Кеш треків", "Vyrovnávacia pamäť"),
                tr("Прослушанное играет сразу и без интернета: ${cached / 1048576} МБ из 2 ГБ.", "Played tracks start instantly and offline: ${cached / 1048576} MB of 2 GB.", "Прослухане грає одразу й без інтернету: ${cached / 1048576} МБ із 2 ГБ.", "Prehraté skladby hrajú hneď aj offline: ${cached / 1048576} MB z 2 GB."),
            ) {
                Pill(tr("Очистить", "Clear", "Очистити", "Vymazať"), primary = false) {
                    App.cache.keys.toList().forEach { App.cache.removeResource(it) }
                    cached = App.cache.cacheSpace
                }
            }
        }

        // ── YouTube ──
        val ytOn by YtAuth.on.collectAsState()
        var login by remember { mutableStateOf(false) }
        GlassGroup(
            "YouTube",
            footer = if (ytOn) tr("Вошёл: треки берутся под аккаунтом, как на компе. Не дал звук — телефон сам попробует без аккаунта и через другие пути.", "Signed in: tracks load via your account, like on the PC. If it gives no audio, the phone tries without the account and other ways by itself.", "Увійшов: треки беруться під акаунтом, як на компі. Не дав звук — телефон сам спробує без акаунта й іншими шляхами.", "Prihlásený: skladby idú cez účet, ako na PC. Ak nedá zvuk, telefón sám skúsi bez účtu a inými cestami.")
            else tr("Без входа тоже играет: телефон сам перебирает пути к звуку. Вход с компа (там Настройки → Куки YouTube → «На телефон») — ещё один путь. Лучше запасной аккаунт.", "It plays without signing in too: the phone tries different ways to get audio by itself. Taking the sign-in from the PC (Settings → YouTube cookies → “To phone” there) adds one more. A spare account is better.", "Без входу теж грає: телефон сам перебирає шляхи до звуку. Вхід з компа (там Налаштування → Куки YouTube → «На телефон») — ще один шлях. Краще запасний акаунт.", "Hrá aj bez prihlásenia: telefón sám skúša cesty k zvuku. Prihlásenie z PC (tam Nastavenia → Cookies YouTube → „Na telefón“) je ďalšia cesta. Lepšie je použiť záložný účet."),
        ) {
            GroupRow(
                if (ytOn) tr("Вход есть", "Signed in", "Вхід є", "Prihlásený") else tr("Не вошёл", "Not signed in", "Не увійшов", "Neprihlásený"),
            ) {
                if (ytOn) Pill(tr("Выйти", "Sign out", "Вийти", "Odhlásiť sa"), primary = false, danger = true) { YtAuth.clear(); android.webkit.CookieManager.getInstance().removeAllCookies(null) }
            }
            GroupDivider()
            Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Pill(tr("Взять с компа", "Take from PC", "Взяти з компа", "Zobrať z PC")) { vm.pullYtCookies() }
                Pill(tr("Войти", "Sign in", "Увійти", "Prihlásiť sa"), primary = false) { login = true }
            }
        }
        if (login) YtLoginDialog(close = { login = false }) { vm.ytSignedIn() }

        // ── тексты ──
        var key by remember { mutableStateOf(vm.spicyKey) }
        GlassGroup(
            tr("Тексты", "Lyrics", "Тексти", "Texty"),
            footer = tr("Ключ Spicy Lyrics — ещё один источник текстов (синки по слогам). Пусто — источник выключен.", "Spicy Lyrics key — one more lyrics source (syllable syncs). Empty — source is off.", "Ключ Spicy Lyrics — ще одне джерело текстів (синки за складами). Порожньо — джерело вимкнено.", "Kľúč Spicy Lyrics — ďalší zdroj textov (synchronizácia po slabikách). Prázdne — zdroj je vypnutý."),
        ) {
            androidx.compose.material3.TextField(
                key, { key = it; vm.setSpicy(it) }, placeholder = { Text("sl_sk_…") }, singleLine = true,
                visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth(),
                colors = androidx.compose.material3.TextFieldDefaults.colors(
                    focusedContainerColor = Color.Transparent, unfocusedContainerColor = Color.Transparent,
                    focusedIndicatorColor = Color.Transparent, unfocusedIndicatorColor = Color.Transparent,
                    cursorColor = a.c1, focusedTextColor = Color.White, unfocusedTextColor = Color.White,
                    focusedPlaceholderColor = Pal.dim, unfocusedPlaceholderColor = Pal.dim,
                ),
            )
        }

        // ── обновление ──
        val newVer by app.blackhole.Updater.version.collectAsState()
        val updating by app.blackhole.Updater.busy.collectAsState()
        val ctx = androidx.compose.ui.platform.LocalContext.current
        val scope = androidx.compose.runtime.rememberCoroutineScope()
        if (newVer.isNotEmpty()) GlassGroup {
            GroupRow(
                tr("Вышла версия $newVer", "Version $newVer is out", "Вийшла версія $newVer", "Vyšla verzia $newVer"),
                tr("Скачаю и открою установщик. Плейлисты и вход останутся.", "I'll download it and open the installer. Playlists and sign-in stay.", "Завантажу і відкрию встановлювач. Плейлисти і вхід залишаться.", "Stiahnem ju a otvorím inštalátor. Playlisty a prihlásenie zostanú."),
            ) {
                Pill(if (updating) tr("Качаю…", "Downloading…", "Завантажую…", "Sťahujem…") else tr("Обновить", "Update", "Оновити", "Aktualizovať"), enabled = !updating) {
                    scope.launch { app.blackhole.Updater.install(ctx) }
                }
            }
        }

        // ── о программе ──
        GlassGroup(tr("О программе", "About", "Про програму", "O aplikácii")) {
            Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(52.dp).shadow(10.dp, RoundedCornerShape(14.dp)).clip(RoundedCornerShape(14.dp)).background(a.brush()), contentAlignment = Alignment.Center) {
                    Box(Modifier.size(22.dp).clip(CircleShape).background(Color.Black))
                }
                Spacer(Modifier.width(14.dp))
                Column {
                    Text("Gargantua", color = Color.White, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                    Text("${app.blackhole.BuildConfig.VERSION_NAME} · qweowix · MIT", color = Pal.dim, fontSize = 13.sp)
                }
            }
            Text(
                tr(
                    "Бесплатный плеер без рекламы: YouTube Music, SoundCloud и Яндекс в одном поиске, импорт плейлистов из Spotify и Apple Music по ссылке или QR. Тексты по словам, эквалайзер и скорость, кроссфейд, умное перемешивание, скачивание плейлистов для офлайна, статистика с итогами месяца и года, заметки к моментам трека. Плейлисты общие с плеером на компьютере, а через Connect можно управлять им с телефона и перекидывать звук между устройствами.",
                    "A free player with no ads: YouTube Music, SoundCloud and Yandex in one search, playlist import from Spotify and Apple Music by link or QR. Word-by-word lyrics, equalizer and speed, crossfade, smart shuffle, playlist downloads for offline, stats with month and year recaps, notes at moments of a track. Playlists are shared with the desktop player, and Connect lets you control it from the phone and move the sound between devices.",
                    "Безкоштовний плеєр без реклами: YouTube Music, SoundCloud і Яндекс в одному пошуку, імпорт плейлистів зі Spotify та Apple Music за посиланням або QR. Тексти по словах, еквалайзер і швидкість, кросфейд, розумне перемішування, завантаження плейлистів для офлайну, статистика з підсумками місяця й року, нотатки до моментів треку. Плейлисти спільні з плеєром на комп'ютері, а через Connect ним можна керувати з телефона й перекидати звук між пристроями.",
                    "Bezplatný prehrávač bez reklám: YouTube Music, SoundCloud a Yandex v jednom vyhľadávaní, import playlistov zo Spotify a Apple Music cez odkaz alebo QR. Texty po slovách, ekvalizér a rýchlosť, crossfade, inteligentné miešanie, sťahovanie playlistov na offline, štatistiky so zhrnutím mesiaca a roka, poznámky k momentom skladby. Playlisty sú spoločné s prehrávačom na počítači a cez Connect ho ovládaš z telefónu a presúvaš zvuk medzi zariadeniami.",
                ),
                color = Pal.dim, fontSize = 13.sp, lineHeight = 18.sp, modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp),
            )
            GroupDivider()
            Text("GitHub: github.com/qweowix/Gargantua\nTelegram: t.me/qweowixsrealm", color = Pal.dim, fontSize = 13.sp, lineHeight = 18.sp, modifier = Modifier.padding(16.dp))
            GroupDivider()
            GroupRow(
                tr("Журнал ошибок", "Error log", "Журнал помилок", "Záznam chýb"),
                tr("Трек не играет или что-то сломалось — скопируй журнал и пришли его.", "A track won't play or something broke — copy the log and send it.", "Трек не грає або щось зламалося — скопіюй журнал і надішли його.", "Skladba nehrá alebo sa niečo pokazilo — skopíruj záznam a pošli ho."),
            ) { Pill(tr("Скопировать", "Copy", "Скопіювати", "Kopírovať"), primary = false) { vm.copyLog() } }
        }
    }
}
