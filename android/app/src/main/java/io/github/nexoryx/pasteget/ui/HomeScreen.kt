// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.Activity
import io.github.nexoryx.pasteget.R
import uniffi.pasteget_ffi.PairedComputer

/** Everything the home screen shows. It reads nothing else. */
data class HomeState(
    val status: AppStatus,
    val computers: List<PairedComputer>,
    val connected: Set<String>,
    /** The result of the last thing the user did, or what just happened. */
    val message: String?,
    val pairing: PairingState,
    /** Newest first. */
    val activity: List<Activity>,
    val screenshots: ScreenshotSetting,
    val photoRequestDenied: Boolean,
    val autoCopy: AutoCopyCardState,
    val codes: CodesCardState,
)

/** What the home screen's buttons and switches do. Each does nothing unless given, for tests. */
class HomeActions(
    /** Scans a pairing code. */
    val onPair: () -> Unit = {},
    val onConfirmPairing: () -> Unit = {},
    /** No to the code shown, or closing the pairing error. */
    val onCancelPairing: () -> Unit = {},
    val onSendClipboard: () -> Unit = {},
    val onForget: (PairedComputer) -> Unit = {},
    val onScreenshotsChange: (Boolean) -> Unit = {},
    /** paste-get's page in the system settings, where photo access is. */
    val onOpenAppSettings: () -> Unit = {},
    val autoCopy: AutoCopyActions = AutoCopyActions(),
    val codes: CodesActions = CodesActions(),
)

@Composable
fun HomeScreen(state: HomeState, actions: HomeActions) {
    Column(
        modifier = Modifier
            .safeDrawingPadding()
            .verticalScroll(rememberScrollState())
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text(
            stringResource(R.string.app_name),
            style = MaterialTheme.typography.headlineLarge,
            modifier = Modifier.semantics { heading() },
        )
        // Read out when it changes ("Connected to 1 computer"), without moving the focus.
        Text(
            statusTitle(state.status),
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
        )

        // One pairing at a time.
        Button(onClick = actions.onPair, enabled = state.pairing != PairingState.Busy, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(R.string.pair_button))
        }
        when (val pairing = state.pairing) {
            PairingState.Idle -> {}
            is PairingState.Confirm -> PairingSheet(
                confirm = pairing,
                known = state.computers.firstOrNull { it.deviceId == pairing.invite.deviceId },
                onConfirm = actions.onConfirmPairing,
                onCancel = actions.onCancelPairing,
            )
            PairingState.Busy -> PairingProgress()
            is PairingState.Failed -> PairingErrorCard(pairing.error, actions.onCancelPairing)
        }
        // Also while the computers are away: the clip waits for them.
        OutlinedButton(
            onClick = actions.onSendClipboard,
            enabled = state.computers.isNotEmpty(),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text(stringResource(R.string.send_clipboard_button))
        }
        state.message?.let {
            Text(
                it,
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            )
        }

        ScreenshotsCard(state.screenshots, state.photoRequestDenied, actions.onScreenshotsChange, actions.onOpenAppSettings)
        AutoCopyCard(state.autoCopy, actions.autoCopy)
        CodesCard(state.codes, actions.codes)
        Recent(state.activity, state.computers)
        SupportCard()

        ComputerList(state.computers, state.connected, actions.onForget)
    }
}

/** The last few things sent or received, with where they went and how fast. */
@Composable
private fun Recent(activity: List<Activity>, computers: List<PairedComputer>) {
    if (activity.isEmpty()) return
    Text(
        stringResource(R.string.recent_title),
        style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.semantics { heading() },
    )
    val names = computers.associate { it.deviceId to it.name }
    Column {
        activity.take(RECENT).forEach { ActivityRow(it, names) }
    }
}

/** How many entries Recent shows. */
private const val RECENT = 5

/** The app's state in one line. */
@Composable
private fun statusTitle(status: AppStatus): String = when (status) {
    AppStatus.NoComputer -> stringResource(R.string.status_no_computer)
    is AppStatus.Unpaired -> stringResource(R.string.unpaired_title, status.name)
    is AppStatus.Looking -> stringResource(R.string.notification_connecting)
    is AppStatus.NeedsStep -> connectedText(status.connected.size)
    is AppStatus.Working -> connectedText(status.connected.size)
}

@Composable
private fun connectedText(count: Int): String = pluralStringResource(R.plurals.notification_connected, count, count)
