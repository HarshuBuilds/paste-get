// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.ActivityKind
import io.github.nexoryx.pasteget.PhotoAccess
import io.github.nexoryx.pasteget.R

/** Whether the user wants screenshots sent, and whether the app can see them. */
data class ScreenshotSetting(val wanted: Boolean, val access: PhotoAccess) {
    val on: Boolean
        get() = wanted && access == PhotoAccess.FULL
}

/**
 * Send new screenshots: on or off, the photo access Android gave (all, the photos the user
 * picked, none) and what to change, and the last screenshot sent.
 */
@Composable
fun ScreenshotsScreen(state: AppState, actions: AppActions, onBack: () -> Unit) {
    val setting = state.screenshots
    val health = screenshotsHealth(setting.wanted, setting.access)
    ScreenScaffold(title = stringResource(R.string.screenshots_title), onBack = onBack) {
        Card(modifier = Modifier.fillMaxWidth()) {
            Box(modifier = Modifier.padding(16.dp)) {
                SwitchRow(
                    title = stringResource(R.string.screenshots_title),
                    text = stringResource(if (setting.wanted) featureStatus(health) else R.string.screenshots_off),
                    checked = setting.wanted,
                    onCheckedChange = actions.onScreenshotsChange,
                    icon = R.drawable.ic_screenshot,
                )
            }
        }
        Text(stringResource(R.string.screenshots_explain), style = MaterialTheme.typography.bodyMedium)

        // Only once it matters: when they're wanted, or the user just said no.
        if (setting.wanted || state.photoRequestDenied || setting.access != PhotoAccess.NONE) {
            PhotoAccessCard(setting, state.photoRequestDenied, actions)
        }

        state.activity.firstOrNull { it.kind == ActivityKind.SCREENSHOT }?.let { last ->
            Text(
                stringResource(R.string.screenshots_last),
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.semantics { heading() },
            )
            ActivityRow(last, state.computers.associate { it.deviceId to it.name })
        }
    }
}

/** The three answers Android's photo dialog can give, the current one checked, and the fix. */
@Composable
fun PhotoAccessCard(setting: ScreenshotSetting, denied: Boolean, actions: AppActions) {
    Text(
        stringResource(R.string.photo_access_title),
        style = MaterialTheme.typography.titleSmall,
        modifier = Modifier.semantics { heading() },
    )
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            AccessOption(R.string.photo_access_all, R.string.photo_access_all_text, setting.access == PhotoAccess.FULL)
            AccessOption(R.string.photo_access_selected, R.string.photo_access_selected_text, setting.access == PhotoAccess.PARTIAL)
            AccessOption(R.string.photo_access_none, R.string.photo_access_none_text, setting.access == PhotoAccess.NONE)
        }
    }
    when {
        setting.access == PhotoAccess.FULL -> {}
        setting.access == PhotoAccess.PARTIAL -> Fix(R.string.screenshots_partial, R.string.open_settings, actions.onOpenAppSettings)
        // Asked and refused: Android won't ask again, only Settings can change it.
        denied -> Fix(R.string.screenshots_denied, R.string.open_settings, actions.onOpenAppSettings)
        else -> Fix(R.string.screenshots_choose_all, R.string.step_allow_photos) { actions.onScreenshotsChange(true) }
    }
}

@Composable
private fun AccessOption(@StringRes title: Int, @StringRes text: Int, current: Boolean) {
    CheckedRow(
        title = stringResource(title),
        text = stringResource(text),
        done = current,
        doneLabel = stringResource(R.string.photo_access_current),
    )
}

/** What to do, and the button that does it. */
@Composable
private fun Fix(@StringRes text: Int, @StringRes button: Int, onClick: () -> Unit) {
    Text(stringResource(text), style = MaterialTheme.typography.bodyMedium)
    Button(onClick = onClick) { Text(stringResource(button)) }
}
