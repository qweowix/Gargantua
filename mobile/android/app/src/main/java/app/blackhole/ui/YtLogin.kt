package app.blackhole.ui

import android.annotation.SuppressLint
import android.webkit.CookieManager
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import app.blackhole.YtAuth
import app.blackhole.tr

// без «; wv» в строке браузера: встроенные окна Google часто не пускает на вход
private const val CHROME_UA = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36"
private const val LOGIN = "https://accounts.google.com/ServiceLogin?service=youtube&passive=true&continue=https%3A%2F%2Fwww.youtube.com%2Fsignin%3Faction_handle_signin%3Dtrue%26next%3Dhttps%253A%252F%252Fm.youtube.com%252F"

/** Запасной вход, если с компа взять нечего: страница входа Google, после неё забираем куки youtube.com */
@SuppressLint("SetJavaScriptEnabled")
@Composable
fun YtLoginDialog(close: () -> Unit, done: () -> Unit) {
    Dialog(close, DialogProperties(usePlatformDefaultWidth = false)) {
        Column(Modifier.fillMaxSize().background(Color(0xFF0B0A10)).statusBarsPadding()) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                IconButton(onClick = close) { Icon(Icons.Rounded.Close, null, tint = Color.White) }
                Text(tr("Вход в YouTube", "YouTube sign-in", "Вхід у YouTube", "Prihlásenie do YouTube"), color = Color.White, fontSize = 17.sp, fontWeight = FontWeight.Bold)
            }
            Box(Modifier.fillMaxSize()) {
                AndroidView(
                    factory = { ctx ->
                        WebView(ctx).apply {
                            settings.javaScriptEnabled = true
                            settings.domStorageEnabled = true
                            settings.userAgentString = CHROME_UA
                            CookieManager.getInstance().setAcceptCookie(true)
                            CookieManager.getInstance().setAcceptThirdPartyCookies(this, true)
                            webViewClient = object : WebViewClient() {
                                override fun onPageFinished(view: WebView, url: String) {
                                    val ck = listOf("https://www.youtube.com", "https://m.youtube.com")
                                        .mapNotNull { CookieManager.getInstance().getCookie(it) }
                                        .joinToString("; ")
                                    val m = YtAuth.parse(ck)
                                    if ("SAPISID" in m || "__Secure-3PAPISID" in m) {
                                        YtAuth.setFromHeader(ck)
                                        done()
                                        close()
                                    }
                                }
                            }
                            loadUrl(LOGIN)
                        }
                    },
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
    }
}
