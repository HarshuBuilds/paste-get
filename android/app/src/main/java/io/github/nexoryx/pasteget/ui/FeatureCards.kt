// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.content.ActivityNotFoundException
import android.content.Intent
import android.text.format.DateFormat
import android.text.format.Formatter
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import io.github.nexoryx.pasteget.AccessNotices
import io.github.nexoryx.pasteget.BuiltInState
import io.github.nexoryx.pasteget.BuiltInStatus
import io.github.nexoryx.pasteget.CodeAccess
import io.github.nexoryx.pasteget.CopyMode
import io.github.nexoryx.pasteget.FallbackAccess
import io.github.nexoryx.pasteget.LastCode
import io.github.nexoryx.pasteget.LastCopy
import io.github.nexoryx.pasteget.LogStatus
import io.github.nexoryx.pasteget.PhotoAccess
import io.github.nexoryx.pasteget.R
import io.github.nexoryx.pasteget.ShizukuState
import io.github.nexoryx.pasteget.ShizukuStatus
import java.util.Date

/** Whether the user wants screenshots sent, and whether the app can see them. */
data class ScreenshotSetting(val wanted: Boolean, val access: PhotoAccess) {
    val on: Boolean
        get() = wanted && access == PhotoAccess.FULL
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

/** What the automatic copy card shows. */
data class AutoCopyCardState(
    val on: Boolean,
    val builtIn: BuiltInState,
    val shizuku: ShizukuState,
    val logs: LogStatus,
    val fallback: FallbackAccess,
    val notices: AccessNotices,
    val lastCopy: LastCopy?,
    /** The built-in mode's set-up asks for its code in a notification. */
    val canNotify: Boolean,
)

/** The automatic copy card's buttons. Each does nothing unless given, for tests and previews. */
class AutoCopyActions(
    val onChange: (Boolean) -> Unit = {},
    val onSetUpBuiltIn: () -> Unit = {},
    val onOpenWirelessDebugging: () -> Unit = {},
    val onRetryBuiltIn: () -> Unit = {},
    val onResetBuiltIn: () -> Unit = {},
    val onAllowShizuku: () -> Unit = {},
    val onOpenShizuku: () -> Unit = {},
    val onGetShizuku: () -> Unit = {},
    val onGrantFallback: () -> Unit = {},
    val onAllowOverlay: () -> Unit = {},
    val onRetryLogs: () -> Unit = {},
    val onHideNotices: (Boolean) -> Unit = {},
    val onOpenPrivacySettings: () -> Unit = {},
    /** paste-get's notification settings. */
    val onOpenNotificationSettings: () -> Unit = {},
)

/** The automatic copy: on or off, how it works right now, and what to do if it doesn't. */
@Composable
fun AutoCopyCard(state: AutoCopyCardState, actions: AutoCopyActions) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SwitchRow(
                title = stringResource(R.string.copies_title),
                text = if (state.on) modeText(state) else stringResource(R.string.copies_off),
                checked = state.on,
                onCheckedChange = actions.onChange,
                icon = R.drawable.ic_content_copy,
            )
            if (state.on) {
                // What isn't working yet, and the step that fixes it.
                val builtInSetUp = state.builtIn.status != BuiltInStatus.NOT_SET_UP &&
                    state.builtIn.status != BuiltInStatus.PAIRING && state.builtIn.status != BuiltInStatus.OFF
                if (state.builtIn.status != BuiltInStatus.WATCHING) {
                    BuiltInLine(state.builtIn, state.canNotify, actions)
                    if (!builtInSetUp && state.shizuku.status != ShizukuStatus.WATCHING) ShizukuLine(state.shizuku, actions)
                    if (state.shizuku.status != ShizukuStatus.WATCHING) LogLine(state.logs, actions)
                }
                if (!builtInSetUp) FallbackLine(state.shizuku, state.fallback, actions)
                state.lastCopy?.let { LastCopyText(it) }
                if (state.notices.supported) NoticesLine(state.notices, actions)
            }
        }
    }
}

/** The health of the automatic copy in one line: how copies are read right now. */
@Composable
private fun modeText(state: AutoCopyCardState): String =
    when (copiesHealth(state.on, state.builtIn, state.shizuku, state.logs, state.fallback, state.canNotify).mode) {
        CopyMode.BUILT_IN -> stringResource(R.string.mode_builtin)
        CopyMode.SHIZUKU -> stringResource(R.string.mode_shizuku, state.shizuku.detail.orEmpty())
        CopyMode.LOGS -> stringResource(R.string.mode_logs)
        CopyMode.MANUAL, null -> stringResource(R.string.mode_manual)
    }

/** The built-in mode: set it up, or what keeps it from running. */
@Composable
private fun BuiltInLine(builtIn: BuiltInState, canNotify: Boolean, actions: AutoCopyActions) {
    val detail = builtIn.detail.orEmpty()
    when (builtIn.status) {
        BuiltInStatus.OFF, BuiltInStatus.WATCHING -> {}
        BuiltInStatus.NOT_SET_UP -> {
            Text(stringResource(R.string.builtin_offer), style = MaterialTheme.typography.bodySmall)
            if (canNotify) {
                TextButton(onClick = actions.onSetUpBuiltIn) { Text(stringResource(R.string.builtin_set_up)) }
            } else {
                NeedsNotificationsLine(actions)
            }
        }
        BuiltInStatus.PAIRING -> {
            if (canNotify) {
                Text(stringResource(R.string.builtin_pairing_steps), style = MaterialTheme.typography.bodySmall)
                if (detail.isNotEmpty()) Text(detail, style = MaterialTheme.typography.bodySmall)
            } else {
                NeedsNotificationsLine(actions)
            }
            Row {
                if (canNotify) {
                    TextButton(onClick = actions.onOpenWirelessDebugging) { Text(stringResource(R.string.builtin_open_settings)) }
                }
                TextButton(onClick = actions.onResetBuiltIn) { Text(stringResource(R.string.builtin_cancel)) }
            }
        }
        BuiltInStatus.STARTING -> Text(stringResource(R.string.builtin_starting), style = MaterialTheme.typography.bodySmall)
        BuiltInStatus.NEEDS_WIFI -> Text(stringResource(R.string.builtin_needs_wifi), style = MaterialTheme.typography.bodySmall)
        BuiltInStatus.FAILED -> {
            Text(stringResource(R.string.builtin_failed, detail), style = MaterialTheme.typography.bodySmall)
            Row {
                TextButton(onClick = actions.onRetryBuiltIn) { Text(stringResource(R.string.builtin_retry)) }
                TextButton(onClick = actions.onResetBuiltIn) { Text(stringResource(R.string.builtin_pair_again)) }
            }
        }
    }
}

/** The code is typed into a notification: without notifications, set-up can't be done. */
@Composable
private fun NeedsNotificationsLine(actions: AutoCopyActions) {
    Text(stringResource(R.string.builtin_needs_notifications), style = MaterialTheme.typography.bodySmall)
    TextButton(onClick = actions.onOpenNotificationSettings) { Text(stringResource(R.string.builtin_allow_notifications)) }
}

@Composable
private fun ShizukuLine(shizuku: ShizukuState, actions: AutoCopyActions) {
    val text = when (shizuku.status) {
        ShizukuStatus.OFF, ShizukuStatus.CONNECTING -> stringResource(R.string.shizuku_connecting)
        ShizukuStatus.NOT_INSTALLED -> stringResource(R.string.shizuku_not_installed)
        ShizukuStatus.NOT_RUNNING -> stringResource(R.string.shizuku_not_running)
        ShizukuStatus.NO_PERMISSION -> stringResource(R.string.shizuku_no_permission)
        ShizukuStatus.WATCHING -> stringResource(R.string.shizuku_watching, shizuku.detail.orEmpty())
        ShizukuStatus.FAILED -> stringResource(R.string.shizuku_failed, shizuku.detail.orEmpty())
    }
    Text(text, style = MaterialTheme.typography.bodySmall)
    when (shizuku.status) {
        ShizukuStatus.NOT_INSTALLED ->
            TextButton(onClick = actions.onGetShizuku) { Text(stringResource(R.string.shizuku_get)) }
        ShizukuStatus.NOT_RUNNING ->
            TextButton(onClick = actions.onOpenShizuku) { Text(stringResource(R.string.shizuku_open)) }
        ShizukuStatus.NO_PERMISSION ->
            TextButton(onClick = actions.onAllowShizuku) { Text(stringResource(R.string.shizuku_allow)) }
        else -> {}
    }
}

/** The permissions that keep copies going when Shizuku stops, and how to get them. */
@Composable
private fun FallbackLine(shizuku: ShizukuState, fallback: FallbackAccess, actions: AutoCopyActions) {
    val watching = shizuku.status == ShizukuStatus.WATCHING
    when {
        fallback.ready -> if (watching) Text(stringResource(R.string.fallback_ready), style = MaterialTheme.typography.bodySmall)
        watching -> {
            Text(stringResource(R.string.fallback_offer), style = MaterialTheme.typography.bodySmall)
            TextButton(onClick = actions.onGrantFallback) { Text(stringResource(R.string.fallback_grant)) }
        }
        fallback.readLogs -> {
            Text(stringResource(R.string.fallback_needs_overlay), style = MaterialTheme.typography.bodySmall)
            TextButton(onClick = actions.onAllowOverlay) { Text(stringResource(R.string.fallback_allow_overlay)) }
        }
        else -> {}
    }
}

/**
 * Android's "Shell pasted from your clipboard" after each copy: an opt-in switch to hide it (for
 * every app, as the system's own switch does), or the way to that switch.
 */
@Composable
private fun NoticesLine(notices: AccessNotices, actions: AutoCopyActions) {
    if (notices.switchable) {
        SwitchRow(
            title = stringResource(R.string.notices_title),
            text = stringResource(R.string.notices_text),
            checked = notices.hidden == true,
            onCheckedChange = actions.onHideNotices,
            titleStyle = MaterialTheme.typography.bodyMedium,
        )
    } else if (notices.hidden != true) {
        Text(stringResource(R.string.notices_by_hand), style = MaterialTheme.typography.bodySmall)
        TextButton(onClick = actions.onOpenPrivacySettings) { Text(stringResource(R.string.notices_open_settings)) }
    }
}

/** Mode B, while Shizuku isn't watching. */
@Composable
private fun LogLine(logs: LogStatus, actions: AutoCopyActions) {
    val text = when (logs) {
        LogStatus.OFF, LogStatus.NEEDS_PERMISSIONS -> return
        LogStatus.WAITING_FOR_APP -> R.string.logs_waiting
        LogStatus.STARTING -> R.string.logs_starting
        LogStatus.RUNNING -> R.string.logs_running
    }
    Text(stringResource(text), style = MaterialTheme.typography.bodySmall)
    if (logs == LogStatus.STARTING) {
        TextButton(onClick = actions.onRetryLogs) { Text(stringResource(R.string.logs_retry)) }
    }
}

@Composable
private fun LastCopyText(copy: LastCopy) {
    val context = LocalContext.current
    val size = Formatter.formatShortFileSize(context, copy.bytes.toLong())
    val time = DateFormat.getTimeFormat(context).format(Date(copy.atMillis))
    Text(
        pluralStringResource(R.plurals.last_copy, copy.sentTo, size, time, copy.sentTo),
        style = MaterialTheme.typography.bodySmall,
    )
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
