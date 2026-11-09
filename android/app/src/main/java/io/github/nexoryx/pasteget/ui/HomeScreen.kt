// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import io.github.nexoryx.pasteget.Activity
import io.github.nexoryx.pasteget.R
import uniffi.pasteget_ffi.PairedComputer

/**
 * "Is it working, and what went where?": the app's state, what it sends, and the last few clips.
 * [onSettings] and [onSeeAll] (the History tab) are the shell's.
 */
@Composable
fun HomeScreen(state: AppState, actions: AppActions, onSettings: () -> Unit = {}, onSeeAll: () -> Unit = {}) {
    ScreenScaffold(
        title = stringResource(R.string.app_name),
        actions = { SettingsButton(onSettings) },
    ) {
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
        Recent(state.activity, state.computers, onSeeAll)
    }
}

/** The last few things sent or received, with where they went and how fast. */
@Composable
private fun Recent(activity: List<Activity>, computers: List<PairedComputer>, onSeeAll: () -> Unit) {
    if (activity.isEmpty()) return
    Column {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                stringResource(R.string.recent_title),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.weight(1f).semantics { heading() },
            )
            TextButton(onClick = onSeeAll) { Text(stringResource(R.string.see_all)) }
        }
        val names = computers.associate { it.deviceId to it.name }
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
