package app.blackhole.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.kyant.backdrop.Backdrop as KBackdrop

// Меню и диалоги рисуются не отдельными окнами Android, а поверх приложения в том же окне («портал»):
// только так их стекло видит и преломляет то, что под ними

@Stable
private class Portal(content: @Composable () -> Unit) {
    var content by mutableStateOf(content)
}

object Overlays {
    internal val items = mutableStateListOf<Any>()

    /** Что под порталом: фон с экраном — его и преломляют меню с диалогами */
    var backdrop by mutableStateOf<KBackdrop?>(null)

    /** Открыт экран плеера — тогда под порталом он */
    var player by mutableStateOf<KBackdrop?>(null)
}

/** Поверх всего приложения: сюда приезжают меню и диалоги */
@Composable
fun OverlayHost(content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize()) {
        content()
        val lite = LocalLite.current
        CompositionLocalProvider(LocalBackdrop provides if (lite) null else (Overlays.player ?: Overlays.backdrop)) {
            for (p in Overlays.items) {
                androidx.compose.runtime.key(p) { (p as Portal).content() }
            }
        }
    }
}

/** Пока этот composable на экране — его content живёт в портале */
@Composable
private fun InPortal(content: @Composable () -> Unit) {
    val p = remember { Portal(content) }
    p.content = content
    DisposableEffect(p) {
        Overlays.items.add(p)
        onDispose { Overlays.items.remove(p) }
    }
}

/** Пружинное появление: из 0.9 и прозрачности в норму, как всплывают панели iOS */
@Composable
private fun rememberPop(): Animatable<Float, *> {
    val a = remember { Animatable(0f) }
    LaunchedEffect(Unit) { a.animateTo(1f, spring(dampingRatio = 0.72f, stiffness = 520f)) }
    return a
}

/** Диалог из жидкого стекла посередине экрана, под ним — лёгкое затемнение. Мимо — закрывается, «назад» — тоже */
@Composable
fun GlassDialog(onDismiss: () -> Unit, title: String, buttons: @Composable RowScope.() -> Unit = {}, content: @Composable ColumnScope.() -> Unit) {
    InPortal {
        BackHandler(onBack = onDismiss)
        val pop = rememberPop()
        Box(
            Modifier
                .fillMaxSize()
                .graphicsLayer { alpha = pop.value.coerceIn(0f, 1f) }
                .background(Color(0x73000000))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss)
                .safeDrawingPadding()
                .imePadding()
                .padding(20.dp),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                Modifier
                    .widthIn(max = 420.dp)
                    .fillMaxWidth()
                    .graphicsLayer {
                        val k = 0.9f + 0.1f * pop.value
                        scaleX = k
                        scaleY = k
                    }
                    .liquid(RoundedCornerShape(34.dp), tint = Color(0x8C16131F), refraction = 22.dp, blur = 18.dp)
                    // касание по самой карточке её не закрывает
                    .clickable(remember { MutableInteractionSource() }, indication = null) {}
                    .verticalScroll(rememberScrollState())
                    .padding(22.dp),
            ) {
                Text(title, color = Color.White, fontSize = 20.sp, fontWeight = FontWeight.Bold)
                Column(Modifier.padding(top = 14.dp, bottom = 18.dp)) { content() }
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.End), content = buttons)
            }
        }
    }
}

/** Выпадающее меню из жидкого стекла: всплывает от кнопки, рядом с которой стоит (под ней, а если места нет — над ней) */
@Composable
fun GlassMenu(expanded: Boolean, onDismissRequest: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    // где кнопка: меню объявляют внутри той же коробки, что и кнопку — берём границы коробки
    var anchor by remember { mutableStateOf(Rect.Zero) }
    Spacer(
        Modifier.size(0.dp).onGloballyPositioned { c ->
            anchor = c.parentLayoutCoordinates?.boundsInWindow() ?: c.boundsInWindow()
        },
    )
    if (!expanded) return
    InPortal {
        BackHandler(onBack = onDismissRequest)
        val pop = rememberPop()
        val gap = with(androidx.compose.ui.platform.LocalDensity.current) { 6.dp.roundToPx() }
        val edge = with(androidx.compose.ui.platform.LocalDensity.current) { 10.dp.roundToPx() }
        var above by remember { mutableStateOf(false) }
        Box(Modifier.fillMaxSize().clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismissRequest)) {
            Layout(
                content = {
                    Column(
                        Modifier
                            .width(IntrinsicSize.Max)
                            .widthIn(min = 200.dp, max = 290.dp)
                            .graphicsLayer {
                                val k = 0.6f + 0.4f * pop.value
                                scaleX = k
                                scaleY = k
                                alpha = pop.value.coerceIn(0f, 1f)
                                transformOrigin = TransformOrigin(1f, if (above) 1f else 0f)
                            }
                            .liquid(RoundedCornerShape(26.dp), tint = Color(0x8C16131F), refraction = 18.dp, blur = 16.dp)
                            .clickable(remember { MutableInteractionSource() }, indication = null) {}
                            .padding(vertical = 6.dp),
                        content = content,
                    )
                },
                modifier = Modifier.fillMaxSize(),
            ) { ms, cs ->
                val pl = ms.first().measure(Constraints(maxWidth = cs.maxWidth - edge * 2, maxHeight = cs.maxHeight - edge * 2))
                layout(cs.maxWidth, cs.maxHeight) {
                    // правым краем к правому краю кнопки, но не за экран
                    val x = (anchor.right.toInt() - pl.width).coerceIn(edge, (cs.maxWidth - pl.width - edge).coerceAtLeast(edge))
                    val below = anchor.bottom.toInt() + gap
                    above = below + pl.height > cs.maxHeight - edge * 3
                    val y = if (above) (anchor.top.toInt() - gap - pl.height).coerceAtLeast(edge * 4) else below
                    pl.place(x, y)
                }
            }
        }
    }
}
