// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
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
        val copies = state.autoCopy
        val copiesWork = copiesHealth(copies.on, copies.builtIn, copies.shizuku, copies.logs, copies.fallback, copies.canNotify)
            .health == Health.WORKING
        StatusHero(state.status, copiesWork, pairingBusy = state.pairing == PairingState.Busy, actions)

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
