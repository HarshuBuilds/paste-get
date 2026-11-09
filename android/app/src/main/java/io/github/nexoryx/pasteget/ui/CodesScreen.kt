// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.Activity
import io.github.nexoryx.pasteget.CodeAccess
import io.github.nexoryx.pasteget.R

/** What the 2FA codes screen shows. */
data class CodesState(
    val on: Boolean,
    val access: CodeAccess,
    /** The built-in mode or Shizuku runs, so access is one tap away. */
    val shellReady: Boolean,
    /** The last code sent, from the activity log: its app and time, never the code. */
    val last: Activity?,
    /** Android 15 and later hide codes from apps that read notifications, until allowed. */
    val codesHidden: Boolean = false,
)

/** The 2FA codes screen's buttons. Each does nothing unless given, for tests. */
class CodesActions(
    val onChange: (Boolean) -> Unit = {},
    /** Both accesses in one tap, through the built-in mode or Shizuku. */
    val onAllow: () -> Unit = {},
    /** Settings → Notification access, at paste-get's page. */
    val onOpenSettings: () -> Unit = {},
    /** App info, where Android 13+ hides "Allow restricted settings". */
    val onOpenAppInfo: () -> Unit = {},
)

/**
 * Send 2FA codes: on or off, the access it needs as a checklist, and the fastest way to get it:
 * one tap when the built-in mode or Shizuku runs, otherwise Settings. [onOpenCopies]: Android
 * 15's hidden codes can only be allowed through the shell the automatic copy runs.
 */
@Composable
fun CodesScreen(state: CodesState, actions: CodesActions, names: Map<String, String>, onBack: () -> Unit, onOpenCopies: () -> Unit) {
    val health = codesHealth(state.on, state.access, state.shellReady)
    ScreenScaffold(title = stringResource(R.string.codes_title), onBack = onBack) {
        SwitchCard(
            title = stringResource(R.string.codes_title),
            text = stringResource(if (state.on) featureStatus(health) else R.string.codes_off),
            checked = state.on,
            onCheckedChange = actions.onChange,
            icon = R.drawable.ic_password,
        )
        Text(stringResource(R.string.codes_privacy), style = MaterialTheme.typography.bodyMedium)
        if (!state.on) return@ScreenScaffold

        CodesAccess(state, actions, onOpenCopies)

        SectionTitle(stringResource(R.string.codes_last))
        val last = state.last
        if (last == null) {
            Text(stringResource(R.string.codes_waiting), style = MaterialTheme.typography.bodyMedium)
        } else {
            ActivityRow(last, names)
        }
    }
}

/** The access codes need, as a checklist, and the one thing to do next. Also in the first-run guide. */
@Composable
fun CodesAccess(state: CodesState, actions: CodesActions, onOpenCopies: () -> Unit) {
    SectionTitle(stringResource(R.string.codes_access_title))
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            CheckedRow(
                title = stringResource(R.string.codes_step_listen),
                text = stringResource(R.string.codes_needs_access),
                done = state.access.listening,
                doneLabel = stringResource(R.string.step_done),
            )
            if (state.codesHidden) {
                CheckedRow(
                    title = stringResource(R.string.codes_step_see),
                    text = stringResource(R.string.codes_hidden),
                    // Unknown (an Android that names it differently) counts as allowed.
                    done = state.access.seesCodes != false,
                    doneLabel = stringResource(R.string.step_done),
                )
            }
        }
    }
    NextAccessStep(state, actions, onOpenCopies)
}

/** The one thing to do next, if anything is missing. */
@Composable
private fun NextAccessStep(state: CodesState, actions: CodesActions, onOpenCopies: () -> Unit) {
    when {
        state.access.ready -> {}
        // One tap does both.
        state.shellReady -> Button(onClick = actions.onAllow) { Text(stringResource(R.string.codes_allow)) }
        !state.access.listening -> {
            Button(onClick = actions.onOpenSettings) { Text(stringResource(R.string.codes_open_settings)) }
            // Android 13+ greys out the switch for apps installed outside a store.
            Text(stringResource(R.string.codes_restricted), style = MaterialTheme.typography.bodyMedium)
            OutlinedButton(onClick = actions.onOpenAppInfo) { Text(stringResource(R.string.codes_open_app_info)) }
        }
        // Listening, codes hidden, no shell: only the automatic copy's set-up gives one.
        else -> {
            Text(stringResource(R.string.codes_needs_shell), style = MaterialTheme.typography.bodyMedium)
            Button(onClick = onOpenCopies) { Text(stringResource(R.string.codes_set_up_shell)) }
        }
    }
}
