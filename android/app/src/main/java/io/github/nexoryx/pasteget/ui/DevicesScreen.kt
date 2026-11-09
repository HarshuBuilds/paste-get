// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.R
import uniffi.pasteget_ffi.PairedComputer

/**
 * "Who is paired?": the computers, this phone, and the button that pairs another computer.
 * [onSettings] is the shell's.
 */
@Composable
fun DevicesScreen(state: AppState, actions: AppActions, onSettings: () -> Unit = {}) {
    val scroll = rememberScrollState()
    // Recomposes when the list leaves the top or comes back, not on every pixel scrolled.
    val atTop by remember { derivedStateOf { scroll.value == 0 } }
    ScreenScaffold(
        title = stringResource(R.string.tab_devices),
        actions = { SettingsButton(onSettings) },
        floatingActionButton = {
            ExtendedFloatingActionButton(
                text = { Text(stringResource(R.string.pair_button)) },
                icon = { Icon(painterResource(R.drawable.ic_qr_code_scanner), contentDescription = null) },
                // One pairing at a time.
                onClick = { if (state.pairing != PairingState.Busy) actions.onPair() },
                // Just the icon once the list scrolls, so it hides less of it.
                expanded = atTop,
            )
        },
        scroll = scroll,
    ) {
        state.unpairedBy?.let { UnpairedBanner(it, actions) }
        ComputerList(state.computers, state.connected, actions.onForget)
        state.thisPhone?.let { ThisPhoneCard(it) }
    }
}

/** "<PC> unpaired this phone": the computer did it, so the phone can only pair again. */
@Composable
private fun UnpairedBanner(computer: PairedComputer, actions: AppActions) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.errorContainer,
            contentColor = MaterialTheme.colorScheme.onErrorContainer,
        ),
    ) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Icon(painterResource(R.drawable.ic_link_off), contentDescription = null)
                Text(stringResource(R.string.unpaired_title, computer.name), style = MaterialTheme.typography.titleMedium)
            }
            Text(stringResource(R.string.unpaired_text), style = MaterialTheme.typography.bodyMedium)
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                TextButton(onClick = actions.onDismissUnpaired) { Text(stringResource(R.string.close)) }
                TextButton(onClick = actions.onPair) { Text(stringResource(R.string.pair_again)) }
            }
        }
    }
}

/** This phone's name, and the ID its computers know it by, in groups of four. */
@Composable
private fun ThisPhoneCard(phone: ThisPhone) {
    Text(
        stringResource(R.string.this_phone_title),
        style = MaterialTheme.typography.titleSmall,
        modifier = Modifier.semantics { heading() },
    )
    Card(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.padding(16.dp),
            horizontalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            TonalIcon(R.drawable.ic_smartphone)
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(phone.name, style = MaterialTheme.typography.bodyLarge)
                Text(stringResource(R.string.this_phone_id), style = MaterialTheme.typography.labelLarge)
                SelectionContainer {
                    Text(groupedId(phone.deviceId), style = MaterialTheme.typography.bodyMedium, fontFamily = FontFamily.Monospace)
                }
                Text(
                    stringResource(R.string.this_phone_id_text),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/** "3F2A 9C11 …": easier to compare than 52 characters in a row. */
fun groupedId(deviceId: String): String = deviceId.chunked(4).joinToString(" ")

/** The paired computers, each with its state and a Forget button that asks first. */
@Composable
fun ComputerList(computers: List<PairedComputer>, connected: Set<String>, onForget: (PairedComputer) -> Unit) {
    // The device ID of the computer the dialog asks about: kept when the screen rotates.
    var asking by rememberSaveable { mutableStateOf<String?>(null) }
    Text(
        stringResource(R.string.paired_computers),
        style = MaterialTheme.typography.titleSmall,
        modifier = Modifier.semantics { heading() },
    )
    if (computers.isEmpty()) {
        Text(
            stringResource(R.string.notification_no_computer),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
    computers.forEach { computer ->
        Card(modifier = Modifier.fillMaxWidth()) {
            Row(
                modifier = Modifier.padding(start = 16.dp, end = 8.dp, top = 8.dp, bottom = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                TonalIcon(R.drawable.ic_computer)
                val isConnected = computer.deviceId in connected
                // Name and state read as one: "HP-AIO, Connected".
                Column(modifier = Modifier.weight(1f).semantics(mergeDescendants = true) {}) {
                    Text(computer.name, style = MaterialTheme.typography.bodyLarge)
                    Text(
                        stringResource(if (isConnected) R.string.connected else R.string.offline),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                // "Forget HP-AIO" rather than the same "Forget" for every computer.
                val label = stringResource(R.string.forget_named, computer.name)
                TextButton(
                    onClick = { asking = computer.deviceId },
                    modifier = Modifier.semantics { contentDescription = label },
                ) {
                    Text(stringResource(R.string.forget))
                }
            }
        }
    }
    // Gone by itself if the computer is (unpaired from the PC meanwhile).
    computers.firstOrNull { it.deviceId == asking }?.let { computer ->
        ForgetDialog(
            computer = computer,
            onConfirm = {
                asking = null
                onForget(computer)
            },
            onDismiss = { asking = null },
        )
    }
}

/** Forgetting can't be undone from the phone: it takes a new pairing. */
@Composable
private fun ForgetDialog(computer: PairedComputer, onConfirm: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.forget_title, computer.name)) },
        text = { Text(stringResource(R.string.forget_text, computer.name)) },
        confirmButton = {
            TextButton(
                onClick = onConfirm,
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
            ) {
                Text(stringResource(R.string.forget_confirm))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}
