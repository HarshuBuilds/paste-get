// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.R
import uniffi.pasteget_ffi.PairedComputer

/** The paired computers, each with its state and a Forget button that asks first. */
@Composable
fun ComputerList(computers: List<PairedComputer>, connected: Set<String>, onForget: (PairedComputer) -> Unit) {
    if (computers.isEmpty()) return
    // The device ID of the computer the dialog asks about: kept when the screen rotates.
    var asking by rememberSaveable { mutableStateOf<String?>(null) }
    Text(
        stringResource(R.string.paired_computers),
        style = MaterialTheme.typography.titleSmall,
        modifier = Modifier.semantics { heading() },
    )
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
