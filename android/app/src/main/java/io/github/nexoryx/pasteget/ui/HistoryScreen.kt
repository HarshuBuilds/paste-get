// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.Activity
import io.github.nexoryx.pasteget.R
import uniffi.pasteget_ffi.PairedComputer

/**
 * "What did I copy?". For now, the activity log: what went where since the app started, never
 * the contents. When the history store lands, its rows take this place.
 */
@Composable
fun HistoryScreen(activity: List<Activity>, computers: List<PairedComputer>, onSettings: () -> Unit = {}) {
    ScreenScaffold(
        title = stringResource(R.string.tab_history),
        actions = { SettingsButton(onSettings) },
    ) {
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.secondaryContainer),
        ) {
            Text(
                stringResource(R.string.history_banner),
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.padding(16.dp),
            )
        }
        if (activity.isEmpty()) {
            EmptyState(R.drawable.ic_history, stringResource(R.string.history_empty_title), stringResource(R.string.history_empty))
        } else {
            val names = computers.associate { it.deviceId to it.name }
            Column { activity.forEach { ActivityRow(it, names, activity.waits(it)) } }
        }
    }
}
