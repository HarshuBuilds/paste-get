// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.text.format.DateFormat
import android.text.format.Formatter
import androidx.annotation.StringRes
import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.AccessNotices
import io.github.nexoryx.pasteget.Activity
import io.github.nexoryx.pasteget.BuiltInState
import io.github.nexoryx.pasteget.BuiltInStatus
import io.github.nexoryx.pasteget.CopyMode
import io.github.nexoryx.pasteget.FallbackAccess
import io.github.nexoryx.pasteget.LogStatus
import io.github.nexoryx.pasteget.R
import io.github.nexoryx.pasteget.SetUpChecks
import io.github.nexoryx.pasteget.ShizukuState
import io.github.nexoryx.pasteget.ShizukuStatus
import io.github.nexoryx.pasteget.Vendor
import java.util.Date

/** What the Copies screen shows. */
data class CopiesState(
    val on: Boolean,
    val builtIn: BuiltInState,
    val shizuku: ShizukuState,
    val logs: LogStatus,
    val fallback: FallbackAccess,
    val notices: AccessNotices,
    /** The last copy sent automatically, from the activity log. */
    val lastCopy: Activity?,
    /** The built-in mode's set-up asks for its code in a notification. */
    val canNotify: Boolean,
    /** The built-in mode is paired with wireless debugging: turning copies on just works. */
    val setUp: Boolean = false,
    /** What the built-in mode's set-up needs from the phone, read live. */
    val checks: SetUpChecks = SetUpChecks(),
    /** Whose words the set-up uses for Developer options and notifications. */
    val vendor: Vendor = Vendor.OTHER,
    /** paste-get shares the screen with another app: the pairing code can be typed here. */
    val splitScreen: Boolean = false,
) {
    val health: FeatureHealth get() = copiesHealth(on, builtIn, shizuku, logs, fallback, canNotify)
}

/** The Copies screen's buttons. Each does nothing unless given, for tests. */
class CopiesActions(
    val onChange: (Boolean) -> Unit = {},
    /** Shows the pairing code notification, then Developer options (beside the app in split screen). */
    val onSetUpBuiltIn: () -> Unit = {},
    /** Developer options, or About phone while they're off. */
    val onOpenWirelessDebugging: () -> Unit = {},
    /** About phone, where Developer options are turned on. */
    val onOpenAboutPhone: () -> Unit = {},
    /** A pairing code typed in the app. */
    val onSubmitCode: (String) -> Unit = {},
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
    /** The Wi-Fi panel: wireless debugging needs Wi-Fi. */
    val onOpenWifi: () -> Unit = {},
)

/**
 * Send what you copy: on or off, how copies are read now, what to do if they aren't, and the
 * technical details for a bug report. Everything the home screen's row leaves out.
 * [onOpenSetUp] opens the built-in mode's set-up.
 */
@Composable
fun CopiesScreen(state: CopiesState, actions: CopiesActions, onBack: () -> Unit, onOpenSetUp: () -> Unit = {}) {
    val health = state.health
    ScreenScaffold(title = stringResource(R.string.copies_title), onBack = onBack) {
        Card(modifier = Modifier.fillMaxWidth()) {
            Box(modifier = Modifier.padding(16.dp)) {
                SwitchRow(
                    title = stringResource(R.string.copies_title),
                    text = if (state.on) stringResource(featureStatus(health)) else stringResource(R.string.copies_off),
                    checked = state.on,
                    onCheckedChange = actions.onChange,
                    icon = R.drawable.ic_content_copy,
                )
            }
        }
        if (!state.on) {
            Body(stringResource(R.string.copies_explain))
            return@ScreenScaffold
        }
        // Until the automatic copy runs: how to send meanwhile.
        if (health.mode == CopyMode.MANUAL) Body(stringResource(R.string.mode_manual))
        Column(modifier = Modifier.animateContentSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            // What isn't working yet, and the step that fixes it.
            val builtInSetUp = state.builtIn.status != BuiltInStatus.NOT_SET_UP &&
                state.builtIn.status != BuiltInStatus.PAIRING && state.builtIn.status != BuiltInStatus.OFF
            if (state.builtIn.status != BuiltInStatus.WATCHING) {
                BuiltInLine(state.builtIn, actions, onOpenSetUp)
                // Not installed: the set-up mentions it, the built-in mode needs no other app.
                val shizukuInUse = state.shizuku.status != ShizukuStatus.WATCHING && state.shizuku.status != ShizukuStatus.NOT_INSTALLED
                if (!builtInSetUp && shizukuInUse) ShizukuLine(state.shizuku, actions)
                if (state.shizuku.status != ShizukuStatus.WATCHING) LogLine(state.logs, actions)
            }
            if (!builtInSetUp) FallbackLine(state.shizuku, state.fallback, actions)
        }
        state.lastCopy?.let { LastCopyText(it) }
        Methods(health.mode)
        // Only once a copy has gone out, when Android shows the notice.
        if (state.lastCopy != null && state.notices.supported) NoticesLine(state.notices, actions)
        Details(state)
    }
}

/** A row's one-line status: "Off", "On, automatic", "One step left"… */
@StringRes
fun featureStatus(health: FeatureHealth): Int = when (health.health) {
    Health.OFF -> R.string.feature_off
    Health.STARTING -> R.string.feature_starting
    Health.BLOCKED -> if (health.step == NextStep.RETRY_BUILT_IN) R.string.feature_broken else R.string.feature_step
    Health.WORKING -> when (health.mode) {
        CopyMode.BUILT_IN, CopyMode.SHIZUKU -> R.string.feature_on_automatic
        CopyMode.LOGS -> R.string.feature_on_backup
        CopyMode.MANUAL, null -> R.string.feature_on
    }
}

@Composable
private fun Body(text: String) {
    Text(text, style = MaterialTheme.typography.bodyMedium)
}

/** The three ways copies can be read, each with its cost, and a check on the one in use. */
@Composable
private fun Methods(mode: CopyMode?) {
    Text(
        stringResource(R.string.methods_title),
        style = MaterialTheme.typography.titleSmall,
        modifier = Modifier.semantics { heading() },
    )
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Method(R.string.method_builtin, R.string.method_builtin_text, mode == CopyMode.BUILT_IN)
            Method(R.string.method_shizuku, R.string.method_shizuku_text, mode == CopyMode.SHIZUKU)
            Method(R.string.method_logs, R.string.method_logs_text, mode == CopyMode.LOGS)
        }
    }
}

@Composable
private fun Method(@StringRes title: Int, @StringRes text: Int, inUse: Boolean) {
    Row(
        modifier = Modifier.semantics(mergeDescendants = true) {},
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(modifier = Modifier.size(24.dp)) {
            if (inUse) {
                Icon(
                    painterResource(R.drawable.ic_check),
                    contentDescription = stringResource(R.string.method_in_use),
                    tint = MaterialTheme.colorScheme.primary,
                )
            }
        }
        Column {
            Text(stringResource(title), style = MaterialTheme.typography.bodyLarge)
            Text(stringResource(text), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/** Each mode's raw state, for a bug report: hidden until asked for. */
@Composable
private fun Details(state: CopiesState) {
    var shown by rememberSaveable { mutableStateOf(false) }
    TextButton(onClick = { shown = !shown }) {
        Text(stringResource(if (shown) R.string.hide_details else R.string.details))
    }
    if (shown) {
        val details = listOf(
            "built-in: ${state.builtIn.status}" + state.builtIn.detail?.let { " ($it)" }.orEmpty(),
            "shizuku: ${state.shizuku.status}" + state.shizuku.detail?.let { " ($it)" }.orEmpty(),
            "log: ${state.logs}, log access: ${state.fallback.readLogs}, overlay: ${state.fallback.overlay}",
            "notices: supported ${state.notices.supported}, hidden ${state.notices.hidden}, can change ${state.notices.canChange}",
        )
        SelectionContainer {
            Text(details.joinToString("\n"), style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
        }
    }
}

/** The built-in mode: its set-up, or what keeps it from running. */
@Composable
private fun BuiltInLine(builtIn: BuiltInState, actions: CopiesActions, onOpenSetUp: () -> Unit) {
    val detail = builtIn.detail.orEmpty()
    when (builtIn.status) {
        BuiltInStatus.OFF, BuiltInStatus.WATCHING -> {}
        BuiltInStatus.NOT_SET_UP -> {
            Body(stringResource(R.string.builtin_offer))
            Button(onClick = onOpenSetUp) { Text(stringResource(R.string.setup_start)) }
        }
        BuiltInStatus.PAIRING -> {
            Body(stringResource(R.string.builtin_pairing))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = onOpenSetUp) { Text(stringResource(R.string.setup_continue)) }
                TextButton(onClick = actions.onResetBuiltIn) { Text(stringResource(R.string.builtin_cancel)) }
            }
        }
        BuiltInStatus.STARTING -> Body(stringResource(R.string.builtin_starting))
        BuiltInStatus.NEEDS_WIFI -> {
            Body(stringResource(R.string.builtin_needs_wifi))
            TextButton(onClick = actions.onOpenWifi) { Text(stringResource(R.string.step_open_wifi)) }
        }
        BuiltInStatus.FAILED -> {
            Body(stringResource(R.string.builtin_failed, detail))
            Row {
                TextButton(onClick = actions.onRetryBuiltIn) { Text(stringResource(R.string.builtin_retry)) }
                TextButton(onClick = actions.onResetBuiltIn) { Text(stringResource(R.string.builtin_pair_again)) }
            }
        }
    }
}

/** Shizuku's state, and the one thing to do about it. Also in the set-up, for those who use it. */
@Composable
fun ShizukuLine(shizuku: ShizukuState, actions: CopiesActions) {
    val text = when (shizuku.status) {
        ShizukuStatus.OFF, ShizukuStatus.CONNECTING -> stringResource(R.string.shizuku_connecting)
        ShizukuStatus.NOT_INSTALLED -> stringResource(R.string.shizuku_not_installed)
        ShizukuStatus.NOT_RUNNING -> stringResource(R.string.shizuku_not_running)
        ShizukuStatus.NO_PERMISSION -> stringResource(R.string.shizuku_no_permission)
        ShizukuStatus.WATCHING -> stringResource(R.string.shizuku_watching, shizuku.detail.orEmpty())
        ShizukuStatus.FAILED -> stringResource(R.string.shizuku_failed, shizuku.detail.orEmpty())
    }
    Body(text)
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
private fun FallbackLine(shizuku: ShizukuState, fallback: FallbackAccess, actions: CopiesActions) {
    val watching = shizuku.status == ShizukuStatus.WATCHING
    when {
        fallback.ready -> if (watching) Body(stringResource(R.string.fallback_ready))
        watching -> {
            Body(stringResource(R.string.fallback_offer))
            TextButton(onClick = actions.onGrantFallback) { Text(stringResource(R.string.fallback_grant)) }
        }
        fallback.readLogs -> {
            Body(stringResource(R.string.fallback_needs_overlay))
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
private fun NoticesLine(notices: AccessNotices, actions: CopiesActions) {
    if (notices.switchable) {
        Card(modifier = Modifier.fillMaxWidth()) {
            Box(modifier = Modifier.padding(16.dp)) {
                SwitchRow(
                    title = stringResource(R.string.notices_title),
                    text = stringResource(R.string.notices_text),
                    checked = notices.hidden == true,
                    onCheckedChange = actions.onHideNotices,
                )
            }
        }
    } else if (notices.hidden != true) {
        Body(stringResource(R.string.notices_by_hand))
        TextButton(onClick = actions.onOpenPrivacySettings) { Text(stringResource(R.string.notices_open_settings)) }
    }
}

/** Mode B, while Shizuku isn't watching. */
@Composable
private fun LogLine(logs: LogStatus, actions: CopiesActions) {
    val text = when (logs) {
        LogStatus.OFF, LogStatus.NEEDS_PERMISSIONS -> return
        LogStatus.WAITING_FOR_APP -> R.string.logs_waiting
        LogStatus.STARTING -> R.string.logs_starting
        LogStatus.RUNNING -> R.string.logs_running
    }
    Body(stringResource(text))
    if (logs == LogStatus.STARTING) {
        TextButton(onClick = actions.onRetryLogs) { Text(stringResource(R.string.logs_retry)) }
    }
}

@Composable
private fun LastCopyText(copy: Activity) {
    val context = LocalContext.current
    val size = Formatter.formatShortFileSize(context, copy.bytes.toLong())
    val time = DateFormat.getTimeFormat(context).format(Date(copy.atMillis))
    Text(
        pluralStringResource(R.plurals.last_copy, copy.sentTo, size, time, copy.sentTo),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}
