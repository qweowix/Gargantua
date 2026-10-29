package app.blackhole.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Computer
import androidx.compose.material.icons.rounded.Smartphone
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.media3.common.util.UnstableApi
import app.blackhole.AppState
import app.blackhole.tr

/** Где играть: устройства аккаунта с открытым BlackHole. Тап — звук переезжает туда с того же места */
@UnstableApi
@Composable
fun ConnDialog(vm: AppState, close: () -> Unit) {
    val accent = LocalAccent.current.c1
    GlassDialog(close, tr("Где играть", "Play on", "Де грати", "Kde hrať"), buttons = {
        Pill(tr("Закрыть", "Close", "Закрити", "Zavrieť"), primary = false, onClick = close)
    }) {
        Text(tr("Все устройства твоего аккаунта, где открыт Gargantua", "All devices on your account with Gargantua open", "Усі пристрої твого акаунта, де відкритий Gargantua", "Všetky zariadenia tvojho účtu, kde je otvorený Gargantua"), color = Pal.dim, fontSize = 13.sp)
        Spacer(Modifier.size(8.dp))
        Column {
            for (d in vm.connDevs) {
                // online тут значит «звук идёт на нём»
                val here = d.online
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(if (here) accent.copy(alpha = 0.2f) else Color.Transparent)
                        .clickable { vm.connPick(d.id); close() }.padding(horizontal = 12.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(if (d.kind == "phone") Icons.Rounded.Smartphone else Icons.Rounded.Computer, null, tint = if (here) accent else Color.White, modifier = Modifier.size(26.dp))
                    Spacer(Modifier.width(12.dp))
                    Column {
                        Text(d.name, color = if (here) accent else Color.White, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                        if (here) Text(tr("звук тут", "playing here", "звук тут", "hrá tu"), color = Pal.dim, fontSize = 12.sp)
                    }
                }
            }
        }
    }
}
