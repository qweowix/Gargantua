package app.blackhole.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color

/** Фон всего приложения, как у Apple Music: цвета обложки того, что играет, мягкими пятнами на подложке темы.
 *  Он неподвижен (стекло поверх него не перерисовывается каждый кадр) и меняется вместе с треком.
 *  Сверху сильнее затемнён вниз — там списки, текст должен читаться. В лайт-режиме — ровный тёмный фон */
@Composable
fun AppBackground(cover: String, modifier: Modifier) {
    if (LocalLite.current) {
        Box(modifier.background(Pal.bg))
        return
    }
    val still = remember { LyrClock() }
    Box(modifier) {
        LyrBackdrop(cover, still)
        Box(
            Modifier.fillMaxSize().background(
                Brush.verticalGradient(0f to Color(0x14000000), 0.45f to Color(0x66000000), 1f to Color(0xB3000000)),
            ),
        )
    }
}
