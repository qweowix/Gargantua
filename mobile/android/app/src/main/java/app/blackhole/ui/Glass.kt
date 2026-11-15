package app.blackhole.ui

import androidx.compose.runtime.CompositionLocalProvider
import android.os.Build
import android.view.WindowManager
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogWindowProvider
import com.kyant.backdrop.drawBackdrop
import com.kyant.backdrop.effects.blur
import com.kyant.backdrop.effects.lens
import com.kyant.backdrop.effects.vibrancy
import com.kyant.backdrop.highlight.Highlight
import com.kyant.backdrop.shadow.Shadow
import com.kyant.backdrop.backdrops.LayerBackdrop as KLayer
import com.kyant.backdrop.backdrops.layerBackdrop as kLayerBackdrop
import com.kyant.backdrop.backdrops.rememberCombinedBackdrop as kCombined
import com.kyant.backdrop.backdrops.rememberLayerBackdrop as kRememberLayer
import com.kyant.backdrop.Backdrop as KBackdrop

// Жидкое стекло как в iOS 26: то, что под плашкой, размыто, ярче и сочнее, а по краям преломляется линзой;
// сверху блик кромки и мягкая тень. Стекло преломляет слой из LocalBackdrop — фон приложения или экран плеера.
// Лайт-режим (и Android до 12, где нет размытия) — прежняя тёмная полупрозрачная плашка с тонкой кромкой

private val Edge = Color(0x22FFFFFF)

/** Подкраска стекла: чуть темнее того, что под ним, чтобы белый текст читался на любой обложке */
val GlassTint = Color(0x3312101A)

/** Слой, который преломляет стекло. null — стекла нет (лайт-режим) */
val LocalBackdrop = staticCompositionLocalOf<KBackdrop?> { null }

/** Включён лайт-режим: без стекла, живого фона и размытий */
val LocalLite = staticCompositionLocalOf { false }

/** Размытие есть с Android 12, преломление — с 13 */
private val canBlur = Build.VERSION.SDK_INT >= 31

typealias GlassLayer = KLayer

@Composable
fun rememberGlassLayer(): GlassLayer = kRememberLayer()

@Composable
fun rememberGlassStack(vararg layers: KBackdrop): KBackdrop = kCombined(*layers)

/** Записывать содержимое в слой для стекла — только когда стекло вообще есть */
@Composable
fun Modifier.glassSource(layer: GlassLayer): Modifier = if (LocalLite.current || !canBlur) this else this.kLayerBackdrop(layer)

/**
 * Плашка из жидкого стекла.
 * @param refraction насколько далеко от края заходит преломление (у мелких кнопок — меньше)
 * @param onClick есть — плашка нажимается: пружинисто проседает под пальцем, как в iOS
 */
@Composable
fun Modifier.liquid(
    shape: Shape,
    backdrop: KBackdrop? = LocalBackdrop.current,
    tint: Color = GlassTint,
    refraction: Dp = 14.dp,
    blur: Dp = 8.dp,
    enabled: Boolean = true,
    onClick: (() -> Unit)? = null,
): Modifier {
    val src = remember { MutableInteractionSource() }
    val pressed by src.collectIsPressedAsState()
    val k by animateFloatAsState(if (pressed) 0.94f else 1f, spring(dampingRatio = 0.55f, stiffness = 700f), label = "press")
    val click = if (onClick != null) Modifier.clickable(src, indication = null, enabled = enabled, onClick = onClick) else Modifier
    if (backdrop == null || LocalLite.current || !canBlur) {
        return this
            .graphicsLayer { scaleX = k; scaleY = k }
            .shadow(14.dp, shape)
            .clip(shape)
            .background(Glass)
            .border(0.5.dp, Edge, shape)
            .then(click)
    }
    val d = LocalDensity.current
    val refPx = with(d) { refraction.toPx() }
    val blurPx = with(d) { blur.toPx() }
    val shapeFn = remember(shape) { { shape } }
    val effects: com.kyant.backdrop.BackdropEffectScope.() -> Unit = remember(refPx, blurPx) {
        {
            vibrancy()
            blur(blurPx)
            lens(refPx, refPx * 2)
        }
    }
    val surface: androidx.compose.ui.graphics.drawscope.DrawScope.() -> Unit = remember(tint) { { drawRect(tint) } }
    return this
        .drawBackdrop(
            backdrop = backdrop,
            shape = shapeFn,
            effects = effects,
            highlight = { Highlight.Default },
            shadow = { Shadow(radius = 18.dp, color = Color.Black.copy(alpha = 0.28f)) },
            layerBlock = if (onClick != null) {
                { scaleX = k; scaleY = k }
            } else null,
            onDrawSurface = surface,
        )
        .clip(shape)
        .then(click)
}

/** Старое имя: плашка из стекла со скруглением radius */
@Composable
fun Modifier.glass(radius: Int = 22) = liquid(RoundedCornerShape(radius.dp))

/** Кнопка-чип: выбранный — белый, остальные — стекло */
@Composable
fun GlassChip(text: String, selected: Boolean, leading: (@Composable () -> Unit)? = null, onClick: () -> Unit) {
    val shape = RoundedCornerShape(18.dp)
    Row(
        Modifier
            .height(36.dp)
            .then(
                if (selected) {
                    Modifier.liquid(shape, tint = Color(0xE6FFFFFF), refraction = 8.dp, onClick = onClick)
                } else {
                    Modifier.liquid(shape, refraction = 8.dp, onClick = onClick)
                },
            )
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        leading?.invoke()
        Text(text, color = if (selected) Color.Black else Color.White, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
    }
}

/** Группа как в настройках iOS: подпись сверху, строки на одной стеклянной карточке, пояснение снизу */
@Composable
fun GlassGroup(title: String? = null, footer: String? = null, content: @Composable ColumnScope.() -> Unit) {
    if (title != null) {
        Text(
            title.uppercase(), color = Pal.dim, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.8.sp,
            modifier = Modifier.padding(start = 32.dp, end = 32.dp, top = 22.dp, bottom = 7.dp),
        )
    } else {
        Spacer(Modifier.height(16.dp))
    }
    Column(Modifier.padding(horizontal = 16.dp).fillMaxWidth().liquid(RoundedCornerShape(24.dp), refraction = 10.dp), content = content)
    if (footer != null) {
        Text(footer, color = Pal.faint, fontSize = 12.sp, lineHeight = 16.sp, modifier = Modifier.padding(start = 32.dp, end = 32.dp, top = 7.dp))
    }
}

/** Тонкая линия между строками группы — от текста до правого края, как у Apple */
@Composable
fun GroupDivider(inset: Dp = 16.dp) {
    HorizontalDivider(Modifier.padding(start = inset), thickness = 0.5.dp, color = Color(0x1FFFFFFF))
}

/** Строка группы: заголовок, пояснение, справа — переключатель или кнопка */
@Composable
fun GroupRow(title: String, sub: String? = null, onClick: (() -> Unit)? = null, trailing: @Composable RowScope.() -> Unit = {}) {
    Row(
        Modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, color = Pal.text, fontSize = 16.sp, fontWeight = FontWeight.Medium)
            if (!sub.isNullOrEmpty()) Text(sub, color = Pal.dim, fontSize = 13.sp, lineHeight = 17.sp, modifier = Modifier.padding(top = 2.dp))
        }
        Spacer(Modifier.width(10.dp))
        trailing()
    }
}

/** Значок строки в цветном скруглённом квадрате — как в настройках iOS */
@Composable
fun RowIcon(icon: androidx.compose.ui.graphics.vector.ImageVector, color: Color) {
    Box(Modifier.size(30.dp).clip(RoundedCornerShape(8.dp)).background(color), contentAlignment = Alignment.Center) {
        androidx.compose.material3.Icon(icon, null, tint = Color.White, modifier = Modifier.size(18.dp))
    }
}

/** Диалог из стекла. Всё, что под ним, система размывает целиком (Android 12+), как алерты iOS */
@Composable
fun GlassDialog(onDismiss: () -> Unit, title: String, buttons: @Composable RowScope.() -> Unit = {}, content: @Composable ColumnScope.() -> Unit) {
    Dialog(onDismissRequest = onDismiss) {
        // диалог — отдельное окно: слои приложения отсюда не преломить, кнопки внутри — простое стекло
        CompositionLocalProvider(LocalBackdrop provides null) {
        val lite = LocalLite.current
        val window = (LocalView.current.parent as? DialogWindowProvider)?.window
        val blurPx = with(LocalDensity.current) { 28.dp.roundToPx() }
        DisposableEffect(window, lite) {
            if (window != null && Build.VERSION.SDK_INT >= 31 && !lite) {
                window.addFlags(WindowManager.LayoutParams.FLAG_BLUR_BEHIND)
                window.attributes = window.attributes.apply { blurBehindRadius = blurPx }
                window.setDimAmount(0.35f)
            }
            onDispose {}
        }
        val shape = RoundedCornerShape(32.dp)
        Column(
            Modifier
                .widthIn(max = 420.dp)
                .fillMaxWidth()
                .shadow(30.dp, shape)
                .clip(shape)
                .background(if (lite || !canBlur) Color(0xF21C1A24) else Color(0xB8201D2B))
                .border(0.5.dp, Color(0x2EFFFFFF), shape)
                .padding(22.dp),
        ) {
            Text(title, color = Color.White, fontSize = 20.sp, fontWeight = FontWeight.Bold)
            Column(Modifier.padding(top = 14.dp, bottom = 18.dp)) { content() }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.End), content = buttons)
        }
        }
    }
}

/** Поле ввода на стекле: без рамки Material, мягкая светлая подложка */
@Composable
fun GlassField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    label: (@Composable () -> Unit)? = null,
    placeholder: (@Composable () -> Unit)? = null,
    leadingIcon: (@Composable () -> Unit)? = null,
    trailingIcon: (@Composable () -> Unit)? = null,
    singleLine: Boolean = false,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    @Suppress("UNUSED_PARAMETER") shape: Shape = RoundedCornerShape(16.dp),
) {
    val a = LocalAccent.current
    TextField(
        value, onValueChange, modifier.liquid(RoundedCornerShape(18.dp), refraction = 8.dp),
        label = label, placeholder = placeholder, leadingIcon = leadingIcon, trailingIcon = trailingIcon,
        singleLine = singleLine, keyboardOptions = keyboardOptions, keyboardActions = keyboardActions,
        visualTransformation = visualTransformation,
        shape = RoundedCornerShape(18.dp),
        colors = TextFieldDefaults.colors(
            focusedContainerColor = Color(0x14FFFFFF),
            unfocusedContainerColor = Color.Transparent,
            disabledContainerColor = Color.Transparent,
            focusedIndicatorColor = Color.Transparent,
            unfocusedIndicatorColor = Color.Transparent,
            disabledIndicatorColor = Color.Transparent,
            cursorColor = a.c1,
            focusedLabelColor = a.c1,
            unfocusedLabelColor = Pal.dim,
            focusedTextColor = Color.White,
            unfocusedTextColor = Color.White,
            focusedLeadingIconColor = Color.White,
            unfocusedLeadingIconColor = Pal.dim,
            focusedPlaceholderColor = Pal.dim,
            unfocusedPlaceholderColor = Pal.dim,
        ),
    )
}

/** Выпадающее меню. Это отдельное окно — фон приложения ему не виден, поэтому почти непрозрачное */
@Composable
fun GlassMenu(expanded: Boolean, onDismissRequest: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    DropdownMenu(
        expanded = expanded,
        onDismissRequest = onDismissRequest,
        shape = RoundedCornerShape(22.dp),
        containerColor = Color(0xF51E1B28),
        border = androidx.compose.foundation.BorderStroke(0.5.dp, Color(0x2EFFFFFF)),
        shadowElevation = 22.dp,
        content = content,
    )
}

/** Круг из стекла (кнопки «назад», «ещё», «перемешать») */
@Composable
fun GlassCircle(size: Dp, onClick: () -> Unit, content: @Composable () -> Unit) {
    Box(Modifier.size(size).liquid(CircleShape, refraction = size / 4, onClick = onClick), contentAlignment = Alignment.Center) { content() }
}
