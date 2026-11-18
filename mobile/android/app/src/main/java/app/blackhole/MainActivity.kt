package app.blackhole

import app.blackhole.ui.Overlays
import app.blackhole.ui.OverlayHost
import app.blackhole.ui.rememberGlassStack
import androidx.compose.runtime.SideEffect
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import android.Manifest
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.rounded.Add
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.blackhole.ui.AppBackground
import app.blackhole.ui.LocalBackdrop
import app.blackhole.ui.LocalLite
import app.blackhole.ui.glassSource
import app.blackhole.ui.liquid
import app.blackhole.ui.rememberGlassLayer
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsTopHeight
import androidx.compose.runtime.getValue
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.unit.IntOffset
import app.blackhole.ui.LocalBottomPad
import app.blackhole.ui.CoverPalette
import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Home
import androidx.compose.material.icons.rounded.LibraryMusic
import androidx.compose.material.icons.rounded.Person
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.media3.common.util.UnstableApi
import app.blackhole.ui.AccountScreen
import app.blackhole.ui.AlbumScreen
import app.blackhole.ui.ArtistScreen
import app.blackhole.ui.AuthScreen
import app.blackhole.ui.BhTheme
import app.blackhole.ui.HomeScreen
import app.blackhole.ui.LibraryScreen
import app.blackhole.ui.LocalAccent
import app.blackhole.ui.MiniPlayer
import app.blackhole.ui.NowPlaying
import app.blackhole.ui.Pal
import app.blackhole.ui.PlaylistScreen
import app.blackhole.ui.SearchScreen
import app.blackhole.ui.SettingsScreen
import app.blackhole.ui.StatsScreen
import app.blackhole.ui.TrackList

@UnstableApi
class MainActivity : ComponentActivity() {
    private val vm: AppState by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        if (Build.VERSION.SDK_INT >= 33) {
            registerForActivityResult(ActivityResultContracts.RequestPermission()) {}.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        shared(intent)
        // OverlayHost — над всем: туда приезжают стеклянные меню и диалоги
        setContent { CoverPalette(vm); BhTheme(vm.theme, vm.coverAccent.takeIf { vm.coverTheme }) { CompositionLocalProvider(LocalLite provides vm.lite) { OverlayHost { Root(vm) } } } }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        shared(intent)
    }

    /** «Поделиться» ссылкой на плейлист из другого приложения → импорт */
    private fun shared(i: Intent?) {
        if (i?.action != Intent.ACTION_SEND) return
        val text = i.getStringExtra(Intent.EXTRA_TEXT) ?: return
        val url = Regex("https?://\\S+").find(text)?.value ?: return
        vm.importUrl(url)
    }

    /** Звук играет на другом устройстве — кнопки громкости телефона крутят громкость там, а не здесь */
    override fun onKeyDown(keyCode: Int, event: android.view.KeyEvent?): Boolean {
        if (keyCode == android.view.KeyEvent.KEYCODE_VOLUME_UP || keyCode == android.view.KeyEvent.KEYCODE_VOLUME_DOWN) {
            if (vm.connVolumeStep(keyCode == android.view.KeyEvent.KEYCODE_VOLUME_UP)) return true
        }
        return super.onKeyDown(keyCode, event)
    }

    override fun onStop() {
        vm.pushNow()
        super.onStop()
    }
}

@UnstableApi
@Composable
private fun Root(vm: AppState) {
    val snack = remember { SnackbarHostState() }
    LaunchedEffect(Unit) { App.toasts.collect { snack.showSnackbar(it) } }
    app.blackhole.ui.UpdateDialog() // вышла новая версия — окно поверх любого экрана, даже входа

    if (vm.cloudOn && !(vm.signedIn && vm.verified)) {
        SideEffect { Overlays.backdrop = null } // у экрана входа слои свои, диалоги на нём — простое стекло
        Box(Modifier.fillMaxSize()) {
            AuthScreen(vm)
            SnackbarHost(snack)
        }
        return
    }

    BackHandler(enabled = vm.nowOpen || vm.stack.size > 1) {
        if (vm.nowOpen) vm.nowOpen = false else vm.back()
    }
    // контент на весь экран, снизу поверх него плавают мини-плеер и меню — списки проезжают под ними
    val bottomPad = (if (vm.current != null) 146.dp else 86.dp) + WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    // слои для жидкого стекла, независимые, друг в друга не вложены: bg — живой фон (его преломляет стекло
    // внутри экранов), page — сам экран на прозрачном. Плавающее поверх (мини-плеер, меню, «назад», всплывающие
    // меню и диалоги) преломляет их стопку
    val bg = rememberGlassLayer()
    val page = rememberGlassLayer()
    val stack = rememberGlassStack(bg, page)
    val under = if (vm.lite) null else stack
    SideEffect { Overlays.backdrop = under }
    Box(Modifier.fillMaxSize().background(Pal.bg)) {
        AppBackground(vm.current?.cover ?: "", Modifier.fillMaxSize().glassSource(bg))
        Box(Modifier.fillMaxSize().glassSource(page)) {
            CompositionLocalProvider(LocalBottomPad provides bottomPad, LocalBackdrop provides if (vm.lite) null else bg) {
                // переходы как в iOS: вглубь — новый экран въезжает справа, старый уходит чуть влево;
                // «назад» — наоборот; между вкладками — мягкий сдвиг в сторону вкладки с проявлением
                androidx.compose.animation.AnimatedContent(
                    targetState = Nav(vm.screen, vm.stack.size),
                    contentKey = { it.screen },
                    transitionSpec = { navTransition(initialState, targetState) },
                    modifier = Modifier.fillMaxSize(),
                    label = "screen",
                ) { nav ->
                    when (val s = nav.screen) {
                        Screen.Home -> HomeScreen(vm)
                        Screen.Search -> SearchScreen(vm)
                        Screen.Library -> LibraryScreen(vm)
                        Screen.Account -> AccountScreen(vm)
                        Screen.Settings -> SettingsScreen(vm)
                        Screen.Stats -> StatsScreen(vm)
                        Screen.Eq -> app.blackhole.ui.EqScreen(vm)
                        is Screen.Pl -> PlaylistScreen(vm, s.name)
                        is Screen.ArtistPage -> ArtistScreen(vm, s.id, s.name)
                        is Screen.AlbumPage -> AlbumScreen(vm, s.id)
                        is Screen.Tracks -> TrackList(vm, s.title, s.sub, s.cover, s.tracks) {
                            app.blackhole.ui.RoundButton(Icons.Rounded.Add, tr("Сохранить", "Save", "Зберегти", "Uložiť")) { vm.go(Screen.Pl(vm.createPlaylist(s.title, s.tracks))) }
                        }
                    }
                }
            }
            // под строкой состояния список уходит в затемнение — часы и значки читаются на любой обложке
            Box(Modifier.fillMaxWidth().windowInsetsTopHeight(WindowInsets.statusBars).background(Brush.verticalGradient(listOf(Color(0xB3000000), Color(0x00000000)))))
        }
        CompositionLocalProvider(LocalBackdrop provides under) {
            // «назад» — над обложкой страниц плейлиста, альбома и артиста
            val s = vm.screen
            if (vm.stack.size > 1 && (s is Screen.Pl || s is Screen.ArtistPage || s is Screen.AlbumPage || s is Screen.Tracks)) {
                Box(Modifier.statusBarsPadding().padding(12.dp)) {
                    app.blackhole.ui.RoundButton(Icons.AutoMirrored.Rounded.ArrowBack, null, 44.dp) { vm.back() }
                }
            }
            Column(Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                SnackbarHost(snack) { d ->
                    Box(Modifier.padding(horizontal = 12.dp).fillMaxWidth().liquid(RoundedCornerShape(22.dp), tint = Color(0x80141220)).padding(horizontal = 18.dp, vertical = 14.dp)) {
                        Text(d.visuals.message, color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                    }
                }
                MiniPlayer(vm)
                BottomNav(vm)
            }
        }
        AnimatedVisibility(vm.nowOpen, enter = slideInVertically { it }, exit = slideOutVertically { it }) {
            NowPlaying(vm)
        }
    }
}

/** Нижнее меню как в iOS 26: стеклянная «таблетка», под выбранной вкладкой — стеклянная капля,
 *  которая пружинисто переезжает к нажатой */
@UnstableApi
@Composable
private fun BottomNav(vm: AppState) {
    val a = LocalAccent.current
    val root = vm.stack.first()
    val tabs = listOf(
        Triple(Screen.Home, Icons.Rounded.Home, tr("Главная", "Home", "Головна", "Domov")),
        Triple(Screen.Search, Icons.Rounded.Search, tr("Поиск", "Search", "Пошук", "Hľadať")),
        Triple(Screen.Library, Icons.Rounded.LibraryMusic, tr("Плейлисты", "Playlists", "Плейлисти", "Playlisty")),
        Triple(Screen.Account, Icons.Rounded.Person, tr("Аккаунт", "Account", "Акаунт", "Účet")),
    )
    val sel = tabs.indexOfFirst { it.first == root }
    val pos by animateFloatAsState(sel.coerceAtLeast(0).toFloat(), spring(dampingRatio = 0.68f, stiffness = 380f), label = "tab")
    BoxWithConstraints(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp)
            .height(62.dp)
            .liquid(RoundedCornerShape(31.dp), refraction = 18.dp)
            .padding(4.dp),
    ) {
        val w = maxWidth / tabs.size
        if (sel >= 0) {
            Box(
                Modifier
                    .offset { IntOffset((w * pos).roundToPx(), 0) }
                    .width(w)
                    .fillMaxHeight()
                    .clip(RoundedCornerShape(27.dp))
                    .background(Color(0x24FFFFFF)),
            )
        }
        Row(Modifier.fillMaxSize()) {
            for ((i, t) in tabs.withIndex()) {
                val (s, icon, label) = t
                val on = i == sel
                val c by animateColorAsState(if (on) a.c1 else Pal.text, label = "tabc")
                Column(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .clip(RoundedCornerShape(27.dp))
                        .clickable(remember { MutableInteractionSource() }, indication = null) { vm.go(s) },
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    Icon(icon, null, tint = c, modifier = Modifier.size(24.dp))
                    Text(label, color = c, fontSize = 10.sp, lineHeight = 12.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
                }
            }
        }
    }
}

/** Экран и глубина стека в момент перехода — по ним понятно, вглубь мы идём, назад или по вкладкам */
private data class Nav(val screen: Screen, val depth: Int)

private val TABS = listOf(Screen.Home, Screen.Search, Screen.Library, Screen.Account)

private fun navTransition(from: Nav, to: Nav): androidx.compose.animation.ContentTransform {
    val slide = androidx.compose.animation.core.spring<androidx.compose.ui.unit.IntOffset>(dampingRatio = 1f, stiffness = 420f)
    val fade = androidx.compose.animation.core.tween<Float>(220)
    return when {
        to.depth > from.depth -> (slideInHorizontally(slide) { it } + fadeIn(fade)) togetherWith
            (slideOutHorizontally(slide) { -it / 4 } + fadeOut(androidx.compose.animation.core.tween(260)))
        to.depth < from.depth -> ((slideInHorizontally(slide) { -it / 4 } + fadeIn(fade)) togetherWith
            (slideOutHorizontally(slide) { it } + fadeOut(androidx.compose.animation.core.tween(260)))).apply { targetContentZIndex = -1f }
        else -> {
            val dir = (TABS.indexOf(to.screen) - TABS.indexOf(from.screen)).coerceIn(-1, 1)
            (slideInHorizontally(slide) { dir * it / 10 } + fadeIn(androidx.compose.animation.core.tween(240, 60)) +
                scaleIn(androidx.compose.animation.core.tween(280), initialScale = 0.98f)) togetherWith
                (slideOutHorizontally(slide) { -dir * it / 10 } + fadeOut(androidx.compose.animation.core.tween(160)))
        }
    }
}
