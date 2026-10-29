package app.blackhole

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
import app.blackhole.ui.Glass
import app.blackhole.ui.LocalBottomPad
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
        setContent { BhTheme(vm.theme) { Root(vm) } }
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

    if (vm.cloudOn && !(vm.signedIn && vm.verified)) {
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
    val bottomPad = (if (vm.current != null) 138.dp else 80.dp) + WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    Box(Modifier.fillMaxSize().background(Pal.bg)) {
        CompositionLocalProvider(LocalBottomPad provides bottomPad) {
            Box(Modifier.fillMaxSize()) {
                when (val s = vm.screen) {
                    Screen.Home -> HomeScreen(vm)
                    Screen.Search -> SearchScreen(vm)
                    Screen.Library -> LibraryScreen(vm)
                    Screen.Account -> AccountScreen(vm)
                    Screen.Settings -> SettingsScreen(vm)
                    Screen.Stats -> StatsScreen(vm)
                    is Screen.Pl -> PlaylistScreen(vm, s.name)
                    is Screen.ArtistPage -> ArtistScreen(vm, s.id, s.name)
                    is Screen.AlbumPage -> AlbumScreen(vm, s.id)
                    is Screen.Tracks -> TrackList(vm, s.title, s.sub, s.cover, s.tracks) {
                        app.blackhole.ui.RoundButton(Icons.Rounded.Add, tr("Сохранить", "Save", "Зберегти", "Uložiť")) { vm.go(Screen.Pl(vm.createPlaylist(s.title, s.tracks))) }
                    }
                }
            }
        }
        Column(Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SnackbarHost(snack)
            MiniPlayer(vm)
            BottomNav(vm)
        }
        AnimatedVisibility(vm.nowOpen, enter = slideInVertically { it }, exit = slideOutVertically { it }) {
            NowPlaying(vm)
        }
    }
}

/** Нижнее меню — плавающая «таблетка», выбранная вкладка в своей подсветке, как в Apple Music */
@UnstableApi
@Composable
private fun BottomNav(vm: AppState) {
    val a = LocalAccent.current
    val root = vm.stack.first()
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 10.dp)
            .shadow(16.dp, RoundedCornerShape(28.dp))
            .clip(RoundedCornerShape(28.dp))
            .background(Glass)
            .border(0.5.dp, Color(0x22FFFFFF), RoundedCornerShape(28.dp))
            .padding(3.dp),
    ) {
        for ((s, icon, label) in listOf(
            Triple(Screen.Home, Icons.Rounded.Home, tr("Главная", "Home", "Головна", "Domov")),
            Triple(Screen.Search, Icons.Rounded.Search, tr("Поиск", "Search", "Пошук", "Hľadať")),
            Triple(Screen.Library, Icons.Rounded.LibraryMusic, tr("Плейлисты", "Playlists", "Плейлисти", "Playlisty")),
            Triple(Screen.Account, Icons.Rounded.Person, tr("Аккаунт", "Account", "Акаунт", "Účet")),
        )) {
            val on = root == s
            Column(
                Modifier
                    .weight(1f)
                    .clip(RoundedCornerShape(25.dp))
                    .background(if (on) Color(0x1FFFFFFF) else Color.Transparent)
                    .clickable { vm.go(s) }
                    .padding(vertical = 4.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Icon(icon, null, tint = if (on) a.c1 else Pal.text, modifier = Modifier.size(22.dp))
                Text(label, color = if (on) a.c1 else Pal.text, fontSize = 10.sp, lineHeight = 12.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
            }
        }
    }
}
