// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.annotation.DrawableRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.KeepAlive
import io.github.nexoryx.pasteget.R
import io.github.nexoryx.pasteget.Vendor
import io.github.nexoryx.pasteget.ui.theme.groupedShape

/** What the keep-alive checklist shows: what Android says, and what the user ticked. */
data class KeepAliveState(
    val checks: KeepAlive = KeepAlive(battery = true, notifications = true),
    val vendor: Vendor = Vendor.OTHER,
    /** Xiaomi's Autostart and battery saver, ticked by the user: no API reads them. */
    val autostartTicked: Boolean = false,
    val batterySaverTicked: Boolean = false,
) {
    private val items: List<Boolean>
        get() = listOf(checks.battery, checks.notifications) +
            if (vendor == Vendor.XIAOMI) listOf(autostartTicked, batterySaverTicked) else emptyList()

    val done: Int get() = items.count { it }
    val total: Int get() = items.size
}

/** The checklist's buttons. Each does nothing unless given, for tests. */
class KeepAliveActions(
    /** Android's "Let app always run in the background?" question, or its battery settings. */
    val onAllowBattery: () -> Unit = {},
    val onOpenNotificationSettings: () -> Unit = {},
    /** The vendor's Autostart page, or App info. */
    val onOpenAutostart: () -> Unit = {},
    val onOpenAppInfo: () -> Unit = {},
    val onTickAutostart: (Boolean) -> Unit = {},
    val onTickBatterySaver: (Boolean) -> Unit = {},
)

/** Keep paste-get running, from Settings. */
@Composable
fun KeepAliveScreen(state: KeepAliveState, actions: KeepAliveActions, onBack: () -> Unit) {
    ScreenScaffold(title = stringResource(R.string.keepalive_title), onBack = onBack) {
        KeepAliveChecklist(state, actions)
    }
}

/** "2 of 4 done", or "All done". */
@Composable
fun keepAliveProgress(state: KeepAliveState): String =
    if (state.done == state.total) {
        stringResource(R.string.keepalive_all_done)
    } else {
        stringResource(R.string.keepalive_progress, state.done, state.total)
    }

/**
 * What keeps Android from stopping paste-get: battery and notifications, checked live, and on
 * Xiaomi phones the vendor's own switches, which only the user can tick, and a tip. Also a
 * page of the first-run guide.
 */
@Composable
fun ColumnScope.KeepAliveChecklist(state: KeepAliveState, actions: KeepAliveActions) {
    Text(stringResource(R.string.keepalive_intro), style = MaterialTheme.typography.bodyLarge)
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        LinearProgressIndicator(progress = { state.done / state.total.toFloat() }, modifier = Modifier.fillMaxWidth())
        Text(
            keepAliveProgress(state),
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
        )
    }
    val xiaomi = state.vendor == Vendor.XIAOMI
    val count = if (xiaomi) 5 else 2
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Item(
            title = stringResource(R.string.keepalive_battery),
            text = stringResource(if (state.checks.battery) R.string.keepalive_battery_ok else R.string.keepalive_battery_needed),
            done = state.checks.battery,
            shape = groupedShape(0, count),
        ) {
            if (!state.checks.battery) OutlinedButton(onClick = actions.onAllowBattery) { Text(stringResource(R.string.keepalive_allow)) }
        }
        Item(
            title = stringResource(R.string.setup_notifications),
            text = stringResource(if (state.checks.notifications) R.string.keepalive_notifications_ok else R.string.keepalive_notifications_needed),
            done = state.checks.notifications,
            shape = groupedShape(1, count),
        ) {
            if (!state.checks.notifications) {
                OutlinedButton(onClick = actions.onOpenNotificationSettings) { Text(stringResource(R.string.builtin_allow_notifications)) }
            }
        }
        if (xiaomi) {
            Item(
                title = stringResource(R.string.keepalive_autostart),
                text = stringResource(R.string.keepalive_autostart_text),
                done = state.autostartTicked,
                shape = groupedShape(2, count),
            ) {
                if (!state.autostartTicked) OutlinedButton(onClick = actions.onOpenAutostart) { Text(stringResource(R.string.keepalive_open_autostart)) }
                Tick(stringResource(R.string.keepalive_autostart), state.autostartTicked, actions.onTickAutostart)
            }
            Item(
                title = stringResource(R.string.keepalive_battery_saver),
                text = stringResource(R.string.keepalive_battery_saver_text),
                done = state.batterySaverTicked,
                shape = groupedShape(3, count),
            ) {
                if (!state.batterySaverTicked) OutlinedButton(onClick = actions.onOpenAppInfo) { Text(stringResource(R.string.codes_open_app_info)) }
                Tick(stringResource(R.string.keepalive_battery_saver), state.batterySaverTicked, actions.onTickBatterySaver)
            }
            Item(
                title = stringResource(R.string.keepalive_recents),
                text = stringResource(R.string.keepalive_recents_text),
                done = null,
                shape = groupedShape(4, count),
            )
        }
    }
}

/**
 * One item: a check when [done], an empty circle when not, an info sign for a tip that can't be
 * done or checked ([done] `null`); its title and text read as one, then its buttons.
 */
@Composable
private fun Item(title: String, text: String, done: Boolean?, shape: Shape, buttons: @Composable ColumnScope.() -> Unit = {}) {
    Surface(modifier = Modifier.fillMaxWidth(), shape = shape, color = MaterialTheme.colorScheme.surfaceContainer) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (done == null) {
                Tip(R.drawable.ic_info, title, text)
            } else {
                CheckedRow(title = title, text = text, done = done, doneLabel = stringResource(R.string.step_done))
            }
            Column(
                // Under the text, not under the icon.
                modifier = Modifier.padding(start = 36.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
                content = buttons,
            )
        }
    }
}

@Composable
private fun Tip(@DrawableRes icon: Int, title: String, text: String) {
    Row(
        modifier = Modifier.fillMaxWidth().semantics(mergeDescendants = true) {},
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Icon(painterResource(icon), contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Column {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/**
 * "It's done", for what only the user can check. The whole row toggles; TalkBack names it after
 * its [item]: "Autostart, It's done, checkbox".
 */
@Composable
private fun Tick(item: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier
            .heightIn(min = 48.dp)
            .toggleable(value = checked, role = Role.Checkbox, onValueChange = onChange)
            .semantics { contentDescription = item }
            .padding(end = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Checkbox(checked = checked, onCheckedChange = null)
        Text(stringResource(R.string.keepalive_ticked), style = MaterialTheme.typography.bodyLarge, modifier = Modifier.padding(start = 8.dp))
    }
}
