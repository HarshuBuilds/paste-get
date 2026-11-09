// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.SheetValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.R
import kotlinx.coroutines.launch
import uniffi.pasteget_ffi.PairedComputer

/**
 * Where pairing stands, over whichever screen is open: a scan or a link can start it anywhere.
 * [computers] tells a code for a computer the phone knows already.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PairingFlow(
    pairing: PairingState,
    computers: List<PairedComputer>,
    onConfirm: (PairingState.Confirm) -> Unit,
    onCancel: () -> Unit,
    onRetry: (PairingState.Failed) -> Unit = {},
) {
    // Paired: from Busy back to Idle (a failure goes to Failed). Felt as well as read.
    val haptics = LocalHapticFeedback.current
    var wasBusy by remember { mutableStateOf(false) }
    LaunchedEffect(pairing) {
        if (wasBusy && pairing == PairingState.Idle) haptics.performHapticFeedback(HapticFeedbackType.Confirm)
        wasBusy = pairing == PairingState.Busy
    }
    when (pairing) {
        PairingState.Idle -> {}
        // A sheet of its own for each code: a tap meant for one never lands on another.
        is PairingState.Confirm -> key(pairing) {
            PairingSheet(
                confirm = pairing,
                known = computers.firstOrNull { it.deviceId == pairing.invite.deviceId },
                onConfirm = { onConfirm(pairing) },
                onCancel = onCancel,
            )
        }
        PairingState.Busy, is PairingState.Failed -> {
            // Pairing can't be stopped halfway: the sheet stays until it's done.
            val busy by rememberUpdatedState(pairing == PairingState.Busy)
            val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true, confirmValueChange = { it != SheetValue.Hidden || !busy })
            ModalBottomSheet(onDismissRequest = onCancel, sheetState = sheet) {
                Column(modifier = Modifier.padding(start = 24.dp, end = 24.dp, bottom = 24.dp)) {
                    if (pairing is PairingState.Failed) {
                        PairingErrorCard(pairing.error, onRetry = pairing.retry?.let { { onRetry(pairing) } }, onDismiss = onCancel)
                    } else {
                        PairingProgress()
                    }
                }
            }
        }
    }
}

/**
 * "Pair with this computer?": what the code says (none of it checked), and the one question that
 * matters, since a link from a web page or a message opens this too: is this code on the
 * computer's screen right now? [known] is the paired computer the code is for, if any.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PairingSheet(
    confirm: PairingState.Confirm,
    known: PairedComputer?,
    onConfirm: () -> Unit,
    onCancel: () -> Unit,
) {
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val scope = rememberCoroutineScope()
    val invite = confirm.invite
    ModalBottomSheet(onDismissRequest = onCancel, sheetState = sheet) {
        Column(
            modifier = Modifier
                // Large text can make it taller than the screen.
                .verticalScroll(rememberScrollState())
                .padding(start = 24.dp, end = 24.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                stringResource(R.string.pair_confirm_title),
                style = MaterialTheme.typography.headlineSmall,
                modifier = Modifier.semantics { heading() },
            )
            Text(
                stringResource(R.string.pair_confirm_name, invite.name ?: stringResource(R.string.pair_unnamed)),
                style = MaterialTheme.typography.bodyLarge,
            )
            Text(
                pluralStringResource(
                    R.plurals.pair_confirm_addresses,
                    invite.addresses.size,
                    invite.addresses.joinToString(", "),
                ),
                style = MaterialTheme.typography.bodyMedium,
            )
            if (known != null) {
                Text(stringResource(R.string.pair_confirm_again, known.name), style = MaterialTheme.typography.bodyMedium)
            }
            Text(stringResource(R.string.pair_confirm_effect), style = MaterialTheme.typography.bodyMedium)
            Card(
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.errorContainer,
                    contentColor = MaterialTheme.colorScheme.onErrorContainer,
                ),
            ) {
                Text(
                    stringResource(R.string.pair_confirm_warning),
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.padding(16.dp),
                )
            }
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                TextButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) }
                Button(onClick = {
                    // The sheet slides away, then pairing starts.
                    scope.launch { sheet.hide() }.invokeOnCompletion { onConfirm() }
                }) {
                    Text(stringResource(R.string.pair_confirm_button))
                }
            }
        }
    }
}

/** While pairing runs: it can take ten seconds or more for each address of the code. */
@Composable
private fun PairingProgress() {
    Column(
        modifier = Modifier.fillMaxWidth().semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(stringResource(R.string.pairing), style = MaterialTheme.typography.bodyMedium)
        LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
    }
}

/**
 * What went wrong, what to try, and the technical message behind "Details" for a bug report.
 * [onRetry]: when the same code is worth trying again.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PairingErrorCard(error: PairingError, onRetry: (() -> Unit)?, onDismiss: () -> Unit) {
    var showDetails by rememberSaveable { mutableStateOf(false) }
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.errorContainer,
            contentColor = MaterialTheme.colorScheme.onErrorContainer,
        ),
    ) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(
                modifier = Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(
                    stringResource(R.string.pair_error_title),
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.semantics { heading() },
                )
                Text(stringResource(error.message), style = MaterialTheme.typography.bodyMedium)
            }
            if (showDetails) {
                SelectionContainer {
                    Text(error.details, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
                }
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = { showDetails = !showDetails }) {
                    Text(stringResource(if (showDetails) R.string.hide_details else R.string.details))
                }
                TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
                if (onRetry != null) TextButton(onClick = onRetry) { Text(stringResource(R.string.pair_retry)) }
            }
        }
    }
}
