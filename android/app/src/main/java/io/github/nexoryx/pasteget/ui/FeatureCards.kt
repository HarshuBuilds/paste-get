// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.content.ActivityNotFoundException
import android.content.Intent
import android.text.format.DateFormat
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import io.github.nexoryx.pasteget.CodeAccess
import io.github.nexoryx.pasteget.LastCode
import io.github.nexoryx.pasteget.PhotoAccess
import io.github.nexoryx.pasteget.R
import java.util.Date

/** Whether the user wants screenshots sent, and whether the app can see them. */
data class ScreenshotSetting(val wanted: Boolean, val access: PhotoAccess) {
    val on: Boolean
        get() = wanted && access == PhotoAccess.FULL
}

/** New screenshots: on or off, and what keeps paste-get from seeing them. */
@Composable
fun ScreenshotsScreen(state: AppState, actions: AppActions, onBack: () -> Unit) {
    ScreenScaffold(title = stringResource(R.string.screenshots_title), onBack = onBack) {
        ScreenshotsCard(state.screenshots, state.photoRequestDenied, actions.onScreenshotsChange, actions.onOpenAppSettings)
    }
}

/** 2FA codes: on or off, and the access they need. */
@Composable
fun CodesScreen(state: AppState, actions: AppActions, onBack: () -> Unit) {
    ScreenScaffold(title = stringResource(R.string.codes_title), onBack = onBack) {
        CodesCard(state.codes, actions.codes)
    }
}

@Composable
fun ScreenshotsCard(
    setting: ScreenshotSetting,
    photoRequestDenied: Boolean,
    onChange: (Boolean) -> Unit,
    onOpenSettings: () -> Unit,
) {
    val problem = when {
        setting.access == PhotoAccess.PARTIAL -> R.string.screenshots_partial
        setting.access == PhotoAccess.NONE && (setting.wanted || photoRequestDenied) -> R.string.screenshots_denied
        else -> null
    }
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SwitchRow(
                title = stringResource(R.string.screenshots_title),
                text = stringResource(if (setting.on) R.string.screenshots_on else R.string.screenshots_off),
                checked = setting.on,
                onCheckedChange = onChange,
                icon = R.drawable.ic_screenshot,
            )
            if (problem != null) {
                Text(stringResource(problem), style = MaterialTheme.typography.bodySmall)
                TextButton(onClick = onOpenSettings) { Text(stringResource(R.string.open_settings)) }
            }
        }
    }
}

/** A link to donations, when the build has one (`donate_url`). */
@Composable
fun SupportCard() {
    val url = stringResource(R.string.donate_url)
    if (url.isBlank()) return
    val context = LocalContext.current
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                stringResource(R.string.support_title),
                style = MaterialTheme.typography.bodyLarge,
                modifier = Modifier.semantics { heading() },
            )
            Text(stringResource(R.string.support_text), style = MaterialTheme.typography.bodySmall)
            OutlinedButton(onClick = {
                try {
                    context.startActivity(Intent(Intent.ACTION_VIEW, url.toUri()))
                } catch (e: ActivityNotFoundException) {
                    // No browser: nothing to open it with.
                }
            }) { Text(stringResource(R.string.support_button)) }
        }
    }
}

/** What the 2FA codes card shows. */
data class CodesCardState(
    val on: Boolean,
    val access: CodeAccess,
    /** The built-in mode or Shizuku runs, so access is one tap away. */
    val shellReady: Boolean,
    val last: LastCode?,
)

/** The 2FA codes card's buttons. Each does nothing unless given, for tests and previews. */
class CodesActions(
    val onChange: (Boolean) -> Unit = {},
    val onAllow: () -> Unit = {},
    val onOpenSettings: () -> Unit = {},
)

/** One-time codes from notifications: on or off, and what's missing to receive them. */
@Composable
fun CodesCard(state: CodesCardState, actions: CodesActions) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            val last = state.last
            SwitchRow(
                title = stringResource(R.string.codes_title),
                text = when {
                    !state.on -> stringResource(R.string.codes_off)
                    last != null -> lastCodeText(last)
                    else -> stringResource(R.string.codes_waiting)
                },
                checked = state.on,
                onCheckedChange = actions.onChange,
                icon = R.drawable.ic_password,
            )
            if (state.on) CodesAccessLine(state, actions)
        }
    }
}

@Composable
private fun CodesAccessLine(state: CodesCardState, actions: CodesActions) {
    when {
        !state.access.listening -> {
            Text(stringResource(R.string.codes_needs_access), style = MaterialTheme.typography.bodySmall)
            if (state.shellReady) {
                TextButton(onClick = actions.onAllow) { Text(stringResource(R.string.codes_allow)) }
            } else {
                TextButton(onClick = actions.onOpenSettings) { Text(stringResource(R.string.codes_open_settings)) }
                Text(stringResource(R.string.codes_restricted), style = MaterialTheme.typography.bodySmall)
            }
        }
        state.access.seesCodes == false -> {
            Text(stringResource(R.string.codes_hidden), style = MaterialTheme.typography.bodySmall)
            if (state.shellReady) {
                TextButton(onClick = actions.onAllow) { Text(stringResource(R.string.codes_allow)) }
            } else {
                Text(stringResource(R.string.codes_needs_shell), style = MaterialTheme.typography.bodySmall)
            }
        }
        else -> {}
    }
}

@Composable
private fun lastCodeText(code: LastCode): String {
    val time = DateFormat.getTimeFormat(LocalContext.current).format(Date(code.atMillis))
    return pluralStringResource(R.plurals.last_code, code.sentTo, code.app, time, code.sentTo)
}
