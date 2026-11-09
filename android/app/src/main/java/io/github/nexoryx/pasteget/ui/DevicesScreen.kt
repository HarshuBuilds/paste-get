// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.clickable
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
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.R
import uniffi.pasteget_ffi.PairedComputer

/**
 * "Who is paired?": the computers, this phone, and the button that pairs another computer.
 * [onSettings] and [onOpen] (a computer's page, by device ID) are the shell's.
 */
@Composable
fun DevicesScreen(state: AppState, actions: AppActions, onSettings: () -> Unit = {}, onOpen: (String) -> Unit = {}) {
    val scroll = rememberScrollState()
    // Recomposes when the list leaves the top or comes back, not on every pixel scrolled.
    val atTop by remember { derivedStateOf { scroll.value == 0 } }
    ScreenScaffold(
        title = stringResource(R.string.tab_devices),
        actions = { SettingsButton(onSettings) },
        floatingActionButton = {
            ExtendedFloatingActionButton(
                text = { Text(stringResource(R.string.pair_button)) },
                // Material clears the text's semantics: the icon's description is what TalkBack
                // reads, expanded or not.
                icon = {
                    Icon(painterResource(R.drawable.ic_qr_code_scanner), contentDescription = stringResource(R.string.pair_button))
                },
                // One pairing at a time.
                onClick = { if (state.pairing != PairingState.Busy) actions.onPair() },
                // Just the icon once the list scrolls, so it hides less of it.
                expanded = atTop,
            )
        },
        scroll = scroll,
        // Pulling down dials every computer now and searches the network again.
        refreshing = state.refreshing,
        onRefresh = actions.onRefresh,
    ) {
        state.unpairedBy?.let { UnpairedBanner(it, actions) }
        ComputerList(state, onOpen)
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
    SectionTitle(stringResource(R.string.this_phone_title))
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

/** The paired computers, each with its state; a tap opens its page. */
@Composable
private fun ComputerList(state: AppState, onOpen: (String) -> Unit) {
    SectionTitle(stringResource(R.string.paired_computers))
    if (state.computers.isEmpty()) {
        EmptyState(R.drawable.ic_computer, stringResource(R.string.notification_no_computer), stringResource(R.string.devices_empty_text))
    }
    val details = stringResource(R.string.device_details)
    state.computers.forEach { computer ->
        Card(
            modifier = Modifier
                .fillMaxWidth()
                // Read as one: "HP-AIO, Connected · 38 ms, double-tap to see details".
                .clickable(onClickLabel = details) { onOpen(computer.deviceId) },
        ) {
            Row(
                modifier = Modifier.padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                TonalIcon(R.drawable.ic_computer)
                Column(modifier = Modifier.weight(1f)) {
                    Text(computer.name, style = MaterialTheme.typography.bodyLarge)
                    Text(
                        connectionText(computer.deviceId in state.connected, state.activity.lastLatency(computer.deviceId)),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null)
            }
        }
    }
}

/** "Connected · 38 ms" (the last clip it acknowledged), "Connected" or "Offline". */
@Composable
fun connectionText(connected: Boolean, latency: Long?): String = when {
    !connected -> stringResource(R.string.offline)
    latency != null -> stringResource(R.string.device_connected_ms, latency)
    else -> stringResource(R.string.connected)
}

/** Forgetting can't be undone from the phone: it takes a new pairing. */
@Composable
fun ForgetDialog(computer: PairedComputer, onConfirm: () -> Unit, onDismiss: () -> Unit) {
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
