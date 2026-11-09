// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.github.nexoryx.pasteget.Activity
import io.github.nexoryx.pasteget.R
import uniffi.pasteget_ffi.PairedComputer

/**
 * "Is it working, and what went where?": the app's state, what it sends, and the last few clips.
 * [onSettings], [onSeeAll] (the History tab) and [onOpen] (a feature's screen) are the shell's.
 */
@Composable
fun HomeScreen(
    state: AppState,
    actions: AppActions,
    onSettings: () -> Unit = {},
    onSeeAll: () -> Unit = {},
    onOpen: (Route) -> Unit = {},
) {
    ScreenScaffold(
        title = stringResource(R.string.app_name),
        actions = { SettingsButton(onSettings) },
    ) {
        val copiesWork = state.autoCopy.health.health == Health.WORKING
        StatusHero(state.status, copiesWork, pairingBusy = state.pairing == PairingState.Busy, actions, onOpen)
        FeatureRows(state, actions, onOpen)
        Recent(state.activity, state.computers, onSeeAll)
    }
}

/** The last few things sent or received, with where they went and how fast. */
@Composable
private fun Recent(activity: List<Activity>, computers: List<PairedComputer>, onSeeAll: () -> Unit) {
    if (activity.isEmpty()) return
    Column {
        Row(verticalAlignment = Alignment.CenterVertically) {
            SectionTitle(stringResource(R.string.recent_title), modifier = Modifier.weight(1f))
            TextButton(onClick = onSeeAll) { Text(stringResource(R.string.see_all)) }
        }
        val names = computers.associate { it.deviceId to it.name }
        activity.take(RECENT).forEach { ActivityRow(it, names, activity.waits(it)) }
    }
}

/** How many entries Recent shows. */
private const val RECENT = 5
