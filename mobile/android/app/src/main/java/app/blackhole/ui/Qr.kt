package app.blackhole.ui

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowRight
import androidx.compose.material.icons.rounded.Share
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.FileProvider
import androidx.media3.common.util.UnstableApi
import app.blackhole.AppState
import app.blackhole.Playlist
import app.blackhole.tr
import coil3.compose.AsyncImage
import java.io.File

private fun Context.shareUri(f: File): Uri = FileProvider.getUriForFile(this, "$packageName.files", f)

/** QR плейлиста: по коду на часть (большой плейлист — несколько), листаются стрелками. «Поделиться» отдаёт
 *  все картинки разом — на другом телефоне или компе их выбирают все вместе */
@UnstableApi
@Composable
fun QrShareDialog(vm: AppState, p: Playlist, close: () -> Unit) {
    val ctx = LocalContext.current
    var pages by remember { mutableStateOf<List<String>?>(null) }
    var page by remember { mutableIntStateOf(0) }
    LaunchedEffect(p.name) { pages = vm.qrImages(p) }
    val list = pages
    GlassDialog(close, tr("QR плейлиста", "Playlist QR", "QR плейлиста", "QR playlistu"), buttons = {
        Pill(tr("Закрыть", "Close", "Закрити", "Zavrieť"), primary = false, onClick = close)
        Pill(tr("Поделиться", "Share", "Поділитися", "Zdieľať"), Icons.Rounded.Share, enabled = !list.isNullOrEmpty()) {
            val uris = ArrayList(list.orEmpty().map { ctx.shareUri(File(it)) })
            val send = Intent(if (uris.size > 1) Intent.ACTION_SEND_MULTIPLE else Intent.ACTION_SEND).apply {
                type = "image/png"
                if (uris.size > 1) putParcelableArrayListExtra(Intent.EXTRA_STREAM, uris) else putExtra(Intent.EXTRA_STREAM, uris.first())
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            ctx.startActivity(Intent.createChooser(send, p.name))
        }
    }) {
        Box(Modifier.fillMaxWidth().aspectRatio(1f).clip(RoundedCornerShape(18.dp)).background(Color.White), contentAlignment = Alignment.Center) {
            when {
                list == null -> CircularProgressIndicator(color = Color.Black)
                list.isEmpty() -> Text(tr("Не получилось", "Didn't work", "Не вийшло", "Nepodarilo sa"), color = Color.Black)
                else -> AsyncImage(File(list[page]), null, contentScale = ContentScale.Fit, modifier = Modifier.fillMaxWidth().aspectRatio(1f))
            }
        }
        if (list != null && list.size > 1) {
            Row(Modifier.fillMaxWidth().padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) {
                IconButton(onClick = { page = (page - 1).coerceAtLeast(0) }, enabled = page > 0) { Icon(Icons.AutoMirrored.Rounded.KeyboardArrowLeft, null, tint = Color.White) }
                Text(tr("Часть ${page + 1} из ${list.size}", "Part ${page + 1} of ${list.size}", "Частина ${page + 1} з ${list.size}", "Časť ${page + 1} z ${list.size}"), color = Color.White, fontSize = 14.sp)
                IconButton(onClick = { page = (page + 1).coerceAtMost(list.size - 1) }, enabled = page < list.size - 1) { Icon(Icons.AutoMirrored.Rounded.KeyboardArrowRight, null, tint = Color.White) }
            }
            Text(
                tr("Большой плейлист — несколько кодов. При импорте выбери их все сразу.", "A big playlist takes several codes. Pick them all at once when importing.", "Великий плейлист — кілька кодів. При імпорті вибери їх усі разом.", "Veľký playlist zaberie viac kódov. Pri importe vyber všetky naraz."),
                color = Pal.dim, fontSize = 12.sp,
            )
        }
    }
}

/** Импорт плейлиста по QR: из галереи (можно сразу все части) и снимком с камеры */
class QrImport(val gallery: () -> Unit, val camera: () -> Unit)

@UnstableApi
@Composable
fun rememberQrImport(vm: AppState): QrImport {
    val ctx = LocalContext.current
    val dir = remember { File(ctx.cacheDir, "qr-in").apply { mkdirs() } }
    // картинки из галереи копируем в кэш: ядро читает обычные файлы
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(12)) { uris ->
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        dir.listFiles()?.forEach { it.delete() }
        val paths = uris.mapIndexedNotNull { i, u ->
            runCatching {
                val f = File(dir, "pick-$i")
                ctx.contentResolver.openInputStream(u)!!.use { inp -> f.outputStream().use { inp.copyTo(it) } }
                f.path
            }.getOrNull()
        }
        vm.importQr(paths)
    }
    val shot = remember { File(dir, "camera.jpg") }
    val snap = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { ok ->
        if (ok) vm.importQr(listOf(shot.path))
    }
    return remember {
        QrImport(
            gallery = { pick.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
            camera = { snap.launch(ctx.shareUri(shot)) },
        )
    }
}

/** Пункт меню «+»: значок в кружке и подпись */
@Composable
fun MenuIcon(icon: androidx.compose.ui.graphics.vector.ImageVector) {
    Box(Modifier.size(30.dp).clip(CircleShape).background(Color(0x22FFFFFF)), contentAlignment = Alignment.Center) {
        Icon(icon, null, tint = Color.White, modifier = Modifier.size(18.dp))
    }
}
