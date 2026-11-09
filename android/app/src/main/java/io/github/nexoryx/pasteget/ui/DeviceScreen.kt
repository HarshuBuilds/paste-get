// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.R

/**
 * One paired computer: whether it's connected and how fast its last clip arrived, the addresses
 * the phone dials, and Forget, which asks first.
 */
@Composable
fun DeviceScreen(state: AppState, deviceId: String, actions: AppActions, onBack: () -> Unit) {
    val computer = state.computers.firstOrNull { it.deviceId == deviceId }
    // Forgotten here, or unpaired by the PC meanwhile: nothing left to show.
    LaunchedEffect(computer == null) { if (computer == null) onBack() }
    if (computer == null) return
    // Kept when the screen rotates.
    var asking by rememberSaveable { mutableStateOf(false) }
    ScreenScaffold(title = computer.name, onBack = onBack) {
        Card(modifier = Modifier.fillMaxWidth()) {
            Row(
                modifier = Modifier.padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                TonalIcon(R.drawable.ic_computer)
                Text(
                    connectionText(deviceId in state.connected, state.activity.lastLatency(deviceId)),
                    style = MaterialTheme.typography.bodyLarge,
                )
            }
        }

        SectionTitle(stringResource(R.string.device_addresses))
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            SelectionContainer {
                Text(
                    computer.addresses.joinToString("\n"),
                    style = MaterialTheme.typography.bodyMedium,
                    fontFamily = FontFamily.Monospace,
                )
            }
            Text(
                stringResource(R.string.device_addresses_text),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        OutlinedButton(
            onClick = { asking = true },
            colors = ButtonDefaults.outlinedButtonColors(contentColor = MaterialTheme.colorScheme.error),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text(stringResource(R.string.forget_this))
        }
    }
    if (asking) {
        ForgetDialog(
            computer = computer,
            onConfirm = {
                asking = false
                actions.onForget(computer)
            },
            onDismiss = { asking = false },
        )
    }
}
