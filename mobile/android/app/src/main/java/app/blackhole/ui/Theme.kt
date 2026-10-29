package app.blackhole.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/** Темы как на ПК (первые восемь): акцент, второй цвет градиента и тёмная подложка (deep1) */
data class Accent(val name: () -> String, val c1: Color, val c2: Color, val deep: Color)

val ACCENTS = listOf(
    Accent({ app.blackhole.tr("Горизонт", "Horizon", "Горизонт", "Horizont") }, Color(0xFFA855F7), Color(0xFFF97316), Color(0xFF2A1748)),
    Accent({ app.blackhole.tr("Океан", "Ocean", "Океан", "Oceán") }, Color(0xFF3B82F6), Color(0xFF22D3EE), Color(0xFF0F2448)),
    Accent({ app.blackhole.tr("Токсик", "Toxic", "Токсик", "Toxic") }, Color(0xFF22C55E), Color(0xFFA3E635), Color(0xFF0F3320)),
    Accent({ app.blackhole.tr("Сакура", "Sakura", "Сакура", "Sakura") }, Color(0xFFEC4899), Color(0xFFF9A8D4), Color(0xFF3F1030)),
    Accent({ app.blackhole.tr("Закат", "Sunset", "Захід", "Západ slnka") }, Color(0xFFF59E0B), Color(0xFFEF4444), Color(0xFF3A2508)),
    Accent({ app.blackhole.tr("Моно", "Mono", "Моно", "Mono") }, Color(0xFFA1A1AA), Color(0xFFE4E4E7), Color(0xFF1F1F23)),
    Accent({ app.blackhole.tr("Неон", "Neon", "Неон", "Neón") }, Color(0xFF57DDF7), Color(0xFFA855F7), Color(0xFF173F47)),
    Accent({ app.blackhole.tr("Лава", "Lava", "Лава", "Láva") }, Color(0xFFF76C57), Color(0xFFFACC15), Color(0xFF471D17)),
)

object Pal {
    val bg = Color(0xFF07060B)
    val card = Color(0xFF120F1E)
    val line = Color(0x14FFFFFF)
    val text = Color(0xFFF4F1FA)
    val dim = Color(0xFFA59DBB)
    val faint = Color(0xFF6E6784)
}

val LocalAccent = staticCompositionLocalOf { ACCENTS[0] }

/** Полупрозрачная тёмная подложка плавающих панелей (мини-плеер, нижнее меню, круглые кнопки на обложках) */
val Glass = Color(0xE01C1A24)

/** Сколько места снизу у списков: под ними проезжают плавающие мини-плеер и меню */
val LocalBottomPad = compositionLocalOf { 16.dp }

fun Accent.brush() = Brush.linearGradient(listOf(c1, c2))

@Composable
fun BhTheme(theme: Int, content: @Composable () -> Unit) {
    val a = ACCENTS.getOrElse(theme) { ACCENTS[0] }
    val scheme = darkColorScheme(
        primary = a.c1,
        secondary = a.c2,
        background = Pal.bg,
        surface = Pal.bg,
        surfaceVariant = Pal.card,
        surfaceContainer = Pal.card,
        surfaceContainerHigh = Color(0xFF1A1628),
        onBackground = Pal.text,
        onSurface = Pal.text,
        onSurfaceVariant = Pal.dim,
        outline = Color(0x33FFFFFF),
    )
    CompositionLocalProvider(LocalAccent provides a) {
        MaterialTheme(colorScheme = scheme, content = content)
    }
}
