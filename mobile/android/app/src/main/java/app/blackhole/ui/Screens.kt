package app.blackhole.ui

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
    Column(
        Modifier.fillMaxSize().background(Pal.bg).statusBarsPadding().verticalScroll(rememberScrollState()).padding(24.dp),
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
            color = Pal.text, fontSize = 28.sp, fontWeight = FontWeight.ExtraBold,
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

@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
fun LangChips(onPick: (String) -> Unit = {}) {
    val vmLang = L.lang
    // четыре языка в ряд на узком экране не лезут — переносятся
    androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for ((code, name) in listOf("ru" to "Русский", "en" to "English", "uk" to "Українська", "sk" to "Slovenčina")) {
            FilterChip(
                selected = vmLang == code,
                onClick = { L.lang = code; onPick(code) },
                label = { Text(name) },
                colors = FilterChipDefaults.filterChipColors(selectedContainerColor = LocalAccent.current.c1.copy(alpha = 0.25f)),
            )
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
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = LocalBottomPad.current)) {
        item {
            Title(
                when (hour) {
                    in 5..11 -> tr("Доброе утро", "Good morning", "Доброго ранку", "Dobré ráno")
                    in 12..17 -> tr("Добрый день", "Good afternoon", "Добрий день", "Dobrý deň")
                    in 18..23 -> tr("Добрый вечер", "Good evening", "Добрий вечір", "Dobrý večer")
                    else -> tr("Доброй ночи", "Good night", "Доброї ночі", "Dobrú noc")
                },
                Modifier.statusBarsPadding(),
            )
        }
        if (vm.home.mixes.isEmpty()) {
            item {
                if (vm.homeLoading) Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                    Spacer(Modifier.width(10.dp))
                    Text(tr("Собираю миксы…", "Building mixes…", "Збираю мікси…", "Skladám mixy…"), color = Pal.dim)
                } else Dim(tr("Послушай что-нибудь — тут появятся миксы под тебя: «Твой микс», миксы по любимым артистам и «Открытия».", "Play something — your mixes will show up here: Your mix, mixes from your favourite artists and Discover.", "Послухай щось — тут з'являться мікси для тебе: «Твій мікс», мікси за улюбленими артистами й «Відкриття».", "Vypočuj si niečo — objavia sa tu mixy pre teba: „Tvoj mix“, mixy podľa obľúbených interpretov a „Objavy“."))
            }
        } else {
            item { Section(tr("Миксы для тебя", "Mixes for you", "Мікси для тебе", "Mixy pre teba")) }
            item {
                LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    items(vm.home.mixes) { m ->
                        Column(Modifier.width(150.dp).clickable { vm.go(Screen.Tracks(m.title, m.sub, m.cover, m.tracks)) }) {
                            Cover(m.cover, 150.dp, 14.dp)
                            Spacer(Modifier.height(6.dp))
                            Text(m.title, color = Pal.text, fontWeight = FontWeight.Bold, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(m.sub, color = Pal.dim, fontSize = 12.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        }
                    }
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
            itemsIndexed(top.take(8)) { i, t -> TrackRow(vm, t, { vm.play(top, i) }) }
        }
        if (vm.playlists.isNotEmpty()) {
            item { Section(tr("Твои плейлисты", "Your playlists", "Твої плейлисти", "Tvoje playlisty")) }
            items(vm.playlists.take(6)) { p -> PlaylistRow(p.name, p.tracks) { vm.go(Screen.Pl(p.name)) } }
        }
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
                modifier = Modifier.fillMaxWidth().statusBarsPadding().padding(16.dp),
            )
        }
        item {
            Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                listOf(tr("Все", "All", "Усі", "Všetko"), "YT Music", "SoundCloud", tr("Яндекс", "Yandex", "Яндекс", "Yandex")).forEachIndexed { i, n ->
                    FilterChip(selected = vm.filter == i, onClick = { vm.filter = i }, label = { Text(n, fontSize = 12.sp) })
                }
                if (vm.searching > 0) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
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
        itemsIndexed(list, key = { i, t -> "$i${t.key}" }) { i, t -> TrackRow(vm, t, { vm.play(list, i) }) }
    }
}

// ── библиотека ──

@UnstableApi
@Composable
fun LibraryScreen(vm: AppState) {
    var creating by remember { mutableStateOf(false) }
    var importing by remember { mutableStateOf(false) }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = LocalBottomPad.current)) {
        item {
            Row(Modifier.statusBarsPadding().padding(end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Title(tr("Плейлисты", "Playlists", "Плейлисти", "Playlisty"), Modifier.weight(1f))
                IconButton(onClick = { vm.sync(true) }, enabled = !vm.syncing) { Icon(Icons.Rounded.Refresh, tr("Синхронизировать", "Sync", "Синхронізувати", "Synchronizovať"), tint = Pal.dim) }
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
        if (!vm.loaded) {
            item {
                Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                    Spacer(Modifier.width(10.dp))
                    Text(tr("Загружаю из аккаунта…", "Loading from your account…", "Завантажую з акаунта…", "Načítavam z účtu…"), color = Pal.dim)
                }
            }
        } else if (vm.playlists.isEmpty()) {
            item { Dim(tr("Пока пусто. Создай плейлист или вставь ссылку на плейлист из Spotify, Apple Music, YouTube Music, SoundCloud или Яндекса.", "Nothing yet. Create a playlist or paste a link to a Spotify, Apple Music, YouTube Music, SoundCloud or Yandex playlist.", "Поки порожньо. Створи плейлист або встав посилання на плейлист зі Spotify, Apple Music, YouTube Music, SoundCloud чи Яндекса.", "Zatiaľ prázdno. Vytvor playlist alebo vlož odkaz na playlist zo Spotify, Apple Music, YouTube Music, SoundCloud alebo Yandexu.")) }
        }
        items(vm.playlists, key = { it.name }) { p -> PlaylistRow(p.name, p.tracks) { vm.go(Screen.Pl(p.name)) } }
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
                        Text(title, color = Color.White, fontSize = 26.sp, fontWeight = FontWeight.Bold, textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        if (sub.isNotEmpty()) Text(sub, color = a.c1, fontSize = 20.sp, textAlign = TextAlign.Center, maxLines = 1, overflow = TextOverflow.Ellipsis)
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
                HorizontalDivider(Modifier.padding(horizontal = 16.dp), color = Pal.line)
            }
            if (tracks.size > 8) item {
                GlassField(
                    q, { q = it }, placeholder = { Text(tr("Найти в списке", "Find in list", "Знайти у списку", "Hľadať v zozname")) },
                    leadingIcon = { Icon(Icons.Rounded.Search, null) }, singleLine = true, shape = RoundedCornerShape(14.dp),
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                )
            }
            items(shown, key = { (i, t) -> "$i${t.key}" }) { (i, t) ->
                TrackRow(vm, t, { vm.play(tracks, i) }, inPlaylist = playlist, number = if (numbered) i + 1 else null)
            }
        }
        FastScroll(list)
        BackButton(vm)
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
    val al = album
    when {
        al != null -> TrackList(vm, al.title, al.sub, al.cover, al.tracks, numbered = true) {
            RoundButton(Icons.Rounded.Add, tr("Сохранить как плейлист", "Save as playlist", "Зберегти як плейлист", "Uložiť ako playlist")) { vm.go(Screen.Pl(vm.createPlaylist(al.title, al.tracks))) }
        }
        failed -> Dim(tr("Не загрузилось", "Couldn't load", "Не завантажилося", "Nenačítalo sa"), Modifier.statusBarsPadding())
        else -> Loading()
    }
}

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
    val ar = artist ?: return if (failed) Dim(tr("Артист не нашёлся", "Artist not found", "Артиста не знайдено", "Interpret sa nenašiel"), Modifier.statusBarsPadding()) else Loading()
    Box(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = LocalBottomPad.current)) {
            item {
                // фото на весь верх, имя крупно по центру, под ним «перемешать» — большая ▶ — радио
                Hero(ar.photo, round = true) {
                    Text(
                        ar.name, color = Color.White, fontSize = 38.sp, fontWeight = FontWeight.Bold, textAlign = TextAlign.Center, maxLines = 2,
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
                        Modifier.size(72.dp).clip(CircleShape).background(Color.White).clickable(enabled = ar.top.isNotEmpty()) { vm.play(ar.top, 0) },
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
                item { Dim(ar.desc) }
            }
        }
        BackButton(vm)
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
    Column(Modifier.fillMaxSize().statusBarsPadding().verticalScroll(rememberScrollState()).padding(bottom = 16.dp)) {
        Title(tr("Аккаунт", "Account", "Акаунт", "Účet"))
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(64.dp).clip(CircleShape).background(a.brush()), contentAlignment = Alignment.Center) {
                Text(vm.email.take(1).uppercase().ifEmpty { "?" }, color = Color.White, fontSize = 28.sp, fontWeight = FontWeight.ExtraBold)
            }
            Spacer(Modifier.width(16.dp))
            Column {
                Text(vm.email, color = Pal.text, fontSize = 17.sp, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    when {
                        vm.syncing -> tr("Синхронизирую…", "Syncing…", "Синхронізую…", "Synchronizujem…")
                        vm.syncedAt.isNotEmpty() -> tr("Синхронизировано в ${vm.syncedAt}", "Synced at ${vm.syncedAt}", "Синхронізовано о ${vm.syncedAt}", "Synchronizované o ${vm.syncedAt}")
                        else -> ""
                    },
                    color = Pal.dim, fontSize = 13.sp,
                )
            }
        }
        Dim(tr("Плейлисты хранятся в аккаунте и одинаковые на телефоне и компьютере. Правки уходят на сервер сразу, свежие подтягиваются раз в минуту.", "Playlists live in your account and are the same on your phone and computer. Edits go to the server right away, fresh ones are pulled every minute.", "Плейлисти зберігаються в акаунті й однакові на телефоні та комп'ютері. Зміни йдуть на сервер одразу, свіжі підтягуються щохвилини.", "Playlisty sú uložené v účte a sú rovnaké na telefóne aj na počítači. Úpravy idú na server hneď, nové sa sťahujú raz za minútu."))
        Spacer(Modifier.height(12.dp))
        Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Pill(if (vm.syncing) tr("Синхронизирую…", "Syncing…", "Синхронізую…", "Synchronizujem…") else tr("Синхронизировать", "Sync", "Синхронізувати", "Synchronizovať"), Icons.Rounded.Refresh, enabled = !vm.syncing) { vm.sync(true) }
            Pill(tr("Сменить пароль", "Change password", "Змінити пароль", "Zmeniť heslo"), primary = false) { vm.resetPassword(vm.email) }
        }
        Spacer(Modifier.height(20.dp))
        MenuRow(Icons.Rounded.BarChart, tr("Статистика", "Stats", "Статистика", "Štatistiky")) { vm.go(Screen.Stats) }
        MenuRow(Icons.Rounded.Settings, tr("Настройки", "Settings", "Налаштування", "Nastavenia")) { vm.go(Screen.Settings) }
        MenuRow(Icons.AutoMirrored.Rounded.Logout, tr("Выйти", "Sign out", "Вийти", "Odhlásiť sa"), danger = true) { vm.signOut() }
        if (vm.accMsg.isNotEmpty()) Dim(vm.accMsg)
    }
}

@Composable
fun MenuRow(icon: androidx.compose.ui.graphics.vector.ImageVector, text: String, danger: Boolean = false, onClick: () -> Unit) {
    // стеклянная строка: значок в кружке, подпись, стрелка
    val c = if (danger) Color(0xFFFF5A6E) else Color.White
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 5.dp).glass(18).clickable(onClick = onClick).padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(32.dp).clip(CircleShape).background(c.copy(alpha = 0.14f)), contentAlignment = Alignment.Center) {
            Icon(icon, null, tint = c, modifier = Modifier.size(18.dp))
        }
        Spacer(Modifier.width(14.dp))
        Text(text, color = c, fontSize = 16.sp, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f))
        if (!danger) Icon(Icons.AutoMirrored.Rounded.KeyboardArrowRight, null, tint = Pal.dim)
    }
}

@UnstableApi
@Composable
fun StatsScreen(vm: AppState) {
    LaunchedEffect(Unit) { vm.loadStats() }
    val s = vm.stats
    val a = LocalAccent.current
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = LocalBottomPad.current)) {
        item { Title(tr("Статистика", "Stats", "Статистика", "Štatistiky"), Modifier.statusBarsPadding()) }
        item {
            Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for ((label, ms) in listOf(tr("Сегодня", "Today", "Сьогодні", "Dnes") to s.today, tr("Неделя", "Week", "Тиждень", "Týždeň") to s.week, tr("Месяц", "Month", "Місяць", "Mesiac") to s.month)) {
                    Column(Modifier.weight(1f).clip(RoundedCornerShape(14.dp)).background(Pal.card).padding(12.dp)) {
                        Text(label, color = Pal.dim, fontSize = 12.sp)
                        Text(fmtLong(ms), color = Pal.text, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                    }
                }
            }
        }
        item { Section(tr("Год", "Year", "Рік", "Rok")) }
        item {
            // клетки как на GitHub: колонка — неделя; последние 26 недель влезают в экран телефона
            val weeks = s.heat.chunked(7).takeLast(26)
            Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(3.dp)) {
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
    LaunchedEffect(Unit) {
        var last = 0L
        while (true) withFrameNanos { n ->
            if (last != 0L) clock.bg += ((n - last) / 1e9f).coerceAtMost(0.1f) * 1.2f
            last = n
        }
    }
    Box(
        Modifier.padding(horizontal = 16.dp, vertical = 8.dp).fillMaxWidth().height(260.dp).clip(RoundedCornerShape(24.dp))
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
    Column(Modifier.fillMaxSize().statusBarsPadding().verticalScroll(rememberScrollState()).padding(bottom = 24.dp)) {
        Title(tr("Настройки", "Settings", "Налаштування", "Nastavenia"))
        Section(tr("Язык", "Language", "Мова", "Jazyk"))
        Box(Modifier.padding(horizontal = 16.dp)) { LangChips { vm.setLang(it) } }
        Section(tr("Тема", "Theme", "Тема", "Téma"))
        LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            itemsIndexed(ACCENTS) { i, ac ->
                FilterChip(
                    selected = vm.theme == i, onClick = { vm.setThemeId(i) },
                    label = { Text(ac.name()) },
                    leadingIcon = { Box(Modifier.size(10.dp).clip(CircleShape).background(ac.c1)) },
                )
            }
        }
        Section(tr("Звук", "Sound", "Звук", "Zvuk"))
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(tr("Дальше похожее", "Keep playing similar", "Далі схоже", "Ďalej podobné"), color = Pal.text, fontWeight = FontWeight.Bold)
                Text(tr("Когда очередь заканчивается, музыка не замолкает — включаются похожие треки.", "When the queue runs out, similar tracks keep the music going.", "Коли черга закінчується, музика не замовкає — вмикаються схожі треки.", "Keď sa front minie, hudba neutíchne — pustia sa podobné skladby."), color = Pal.dim, fontSize = 13.sp)
            }
            Switch(
                vm.autoplay, { vm.setAutoplayOn(it) },
                colors = androidx.compose.material3.SwitchDefaults.colors(
                    checkedTrackColor = LocalAccent.current.c1, checkedThumbColor = Color.White,
                    uncheckedTrackColor = Color(0x26FFFFFF), uncheckedThumbColor = Color.White, uncheckedBorderColor = Color.Transparent,
                ),
            )
        }
        // размер считаем при открытии экрана: кеш меняется только пока что-то играет
        var cached by remember { mutableStateOf(App.cache.cacheSpace) }
        Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(tr("Кэш треков", "Track cache", "Кеш треків", "Vyrovnávacia pamäť"), color = Pal.text, fontWeight = FontWeight.Bold)
                Text(tr("Прослушанное играет сразу и без интернета: ${cached / 1048576} МБ из 2 ГБ.", "Played tracks start instantly and offline: ${cached / 1048576} MB of 2 GB.", "Прослухане грає одразу й без інтернету: ${cached / 1048576} МБ із 2 ГБ.", "Prehraté skladby hrajú hneď aj offline: ${cached / 1048576} MB z 2 GB."), color = Pal.dim, fontSize = 13.sp)
            }
            Spacer(Modifier.width(8.dp))
            Pill(tr("Очистить", "Clear", "Очистити", "Vymazať"), primary = false) {
                App.cache.keys.toList().forEach { App.cache.removeResource(it) }
                cached = App.cache.cacheSpace
            }
        }
        Section("YouTube")
        val ytOn by YtAuth.on.collectAsState()
        var login by remember { mutableStateOf(false) }
        Dim(
            if (ytOn) tr("Вошёл: треки берутся под аккаунтом, как на компе — бан «подтверди, что не бот» не мешает.", "Signed in: tracks load via your account, like on the PC — the “confirm you're not a bot” ban doesn't apply.", "Увійшов: треки беруться під акаунтом, як на компі — бан «підтверди, що не бот» не заважає.", "Prihlásený: skladby idú cez účet, ako na PC — ban „potvrď, že nie si bot“ neprekáža.")
            else tr("Если YouTube пишет «подтверди, что не бот» — войди. Проще всего взять вход с компа (там Настройки → Куки YouTube → «На телефон»). Лучше запасной аккаунт.", "If YouTube says “confirm you're not a bot” — sign in. Easiest is to take the sign-in from the PC (Settings → YouTube cookies → “To phone” there). A spare account is better.", "Якщо YouTube пише «підтверди, що не бот» — увійди. Найпростіше взяти вхід з компа (там Налаштування → Куки YouTube → «На телефон»). Краще запасний акаунт.", "Ak YouTube píše „potvrď, že nie si bot“ — prihlás sa. Najľahšie je zobrať prihlásenie z PC (tam Nastavenia → Cookies YouTube → „Na telefón“). Lepšie je použiť záložný účet.")
        )
        Row(Modifier.padding(horizontal = 16.dp, vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Pill(tr("Взять с компа", "Take from PC", "Взяти з компа", "Zobrať z PC")) { vm.pullYtCookies() }
            Pill(tr("Войти", "Sign in", "Увійти", "Prihlásiť sa"), primary = false) { login = true }
            if (ytOn) Pill(tr("Выйти", "Sign out", "Вийти", "Odhlásiť sa"), primary = false, danger = true) { YtAuth.clear(); android.webkit.CookieManager.getInstance().removeAllCookies(null) }
        }
        if (login) YtLoginDialog(close = { login = false }) { vm.ytSignedIn() }
        Section(tr("Тексты", "Lyrics", "Тексти", "Texty"))
        Dim(tr("Ключ Spicy Lyrics — ещё один источник текстов (синки по слогам). Пусто — источник выключен.", "Spicy Lyrics key — one more lyrics source (syllable syncs). Empty — source is off.", "Ключ Spicy Lyrics — ще одне джерело текстів (синки за складами). Порожньо — джерело вимкнено.", "Kľúč Spicy Lyrics — ďalší zdroj textov (synchronizácia po slabikách). Prázdne — zdroj je vypnutý."))
        var key by remember { mutableStateOf(vm.spicyKey) }
        GlassField(
            key, { key = it; vm.setSpicy(it) }, placeholder = { Text("sl_sk_…") }, singleLine = true,
            visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
        )
        Section(tr("О программе", "About", "Про програму", "O aplikácii"))
        Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Rounded.MusicNote, null, tint = LocalAccent.current.c1)
            Spacer(Modifier.width(8.dp))
            Text("Gargantua 0.2.0 · qweowix · GPL-3.0", color = Pal.dim, fontSize = 13.sp)
        }
    }
}
