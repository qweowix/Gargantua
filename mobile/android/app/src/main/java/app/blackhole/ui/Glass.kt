package app.blackhole.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog

// «Жидкое стекло» как у нижнего меню: полупрозрачная тёмная подложка, тонкая светлая кромка, большие скругления

private val Edge = Color(0x22FFFFFF)

/** Модификатор стеклянной плашки */
fun Modifier.glass(radius: Int = 22) = this
    .shadow(18.dp, RoundedCornerShape(radius.dp))
    .clip(RoundedCornerShape(radius.dp))
    .background(Glass)
    .border(0.5.dp, Edge, RoundedCornerShape(radius.dp))

/** Диалог-карточка из стекла: заголовок, содержимое, кнопки в ряд справа */
@Composable
fun GlassDialog(onDismiss: () -> Unit, title: String, buttons: @Composable RowScope.() -> Unit = {}, content: @Composable ColumnScope.() -> Unit) {
    Dialog(onDismissRequest = onDismiss) {
        Column(Modifier.widthIn(max = 420.dp).fillMaxWidth().glass(28).padding(20.dp)) {
            Text(title, color = Color.White, fontSize = 20.sp, fontWeight = FontWeight.Bold)
            Column(Modifier.padding(top = 14.dp, bottom = 18.dp)) { content() }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp, androidx.compose.ui.Alignment.End), content = buttons)
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
        value, onValueChange, modifier,
        label = label, placeholder = placeholder, leadingIcon = leadingIcon, trailingIcon = trailingIcon,
        singleLine = singleLine, keyboardOptions = keyboardOptions, keyboardActions = keyboardActions,
        visualTransformation = visualTransformation,
        shape = RoundedCornerShape(16.dp),
        colors = TextFieldDefaults.colors(
            focusedContainerColor = Color(0x1FFFFFFF),
            unfocusedContainerColor = Color(0x14FFFFFF),
            disabledContainerColor = Color(0x0AFFFFFF),
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

/** Выпадающее меню из стекла */
@Composable
fun GlassMenu(expanded: Boolean, onDismissRequest: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    DropdownMenu(
        expanded = expanded,
        onDismissRequest = onDismissRequest,
        shape = RoundedCornerShape(20.dp),
        containerColor = Color(0xF21C1A24),
        border = androidx.compose.foundation.BorderStroke(0.5.dp, Edge),
        shadowElevation = 18.dp,
        content = content,
    )
}
