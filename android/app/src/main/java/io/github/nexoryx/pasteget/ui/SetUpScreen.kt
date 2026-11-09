// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.os.Build
import androidx.annotation.StringRes
import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.BuiltInState
import io.github.nexoryx.pasteget.BuiltInStatus
import io.github.nexoryx.pasteget.PairingFailure
import io.github.nexoryx.pasteget.PairingStep
import io.github.nexoryx.pasteget.R
import io.github.nexoryx.pasteget.SetUpChecks
import io.github.nexoryx.pasteget.ShizukuStatus
import io.github.nexoryx.pasteget.Vendor
import io.github.nexoryx.pasteget.ui.theme.groupedShape

/** The built-in mode's set-up, in order. */
enum class SetUpStep { PREPARE, DEVELOPER_OPTIONS, WIRELESS_DEBUGGING, CODE, DONE }

/**
 * The first step not done yet. The checks come from the phone's settings, read again whenever
 * they may have changed, so each step ticks itself once the user did it in Settings.
 * [splitScreen]: the code can be typed in the app, so notifications aren't needed.
 */
fun setUpStep(checks: SetUpChecks, canNotify: Boolean, splitScreen: Boolean, builtIn: BuiltInState, setUp: Boolean): SetUpStep {
    val typed = builtIn.status == BuiltInStatus.PAIRING &&
        builtIn.step != null && builtIn.step != PairingStep.WAITING_FOR_CODE
    return when {
        setUp -> SetUpStep.DONE
        // A code is being paired: wait for it, whatever Settings says meanwhile.
        typed -> SetUpStep.CODE
        !checks.wifi || (!canNotify && !splitScreen) -> SetUpStep.PREPARE
        !checks.developerOptions -> SetUpStep.DEVELOPER_OPTIONS
        !checks.wirelessDebugging -> SetUpStep.WIRELESS_DEBUGGING
        else -> SetUpStep.CODE
    }
}

/** The steps the user goes through: [SetUpStep.DONE] is where they end. */
private val STEPS = SetUpStep.entries - SetUpStep.DONE

/**
 * Setting up the built-in mode, step by step: what the phone needs first (checked live), the
 * vendor's words for Developer options, wireless debugging, then the pairing code, typed in a
 * notification or, in split screen, here. Shizuku and sending by hand are the other ways.
 */
@Composable
fun SetUpScreen(state: AppState, actions: AppActions, onBack: () -> Unit) {
    ScreenScaffold(title = stringResource(R.string.setup_title), onBack = onBack) {
        SetUpContent(state, actions, onDone = onBack)
    }
}

/** The set-up's column, also a page of the first-run guide. [onDone] leaves it once set up. */
@Composable
fun ColumnScope.SetUpContent(state: AppState, actions: AppActions, onDone: () -> Unit) {
    val copies = state.autoCopy
    val current = setUpStep(copies.checks, copies.canNotify, copies.splitScreen, copies.builtIn, copies.setUp)
    Text(stringResource(R.string.setup_intro), style = MaterialTheme.typography.bodyLarge)
    if (copies.shizuku.status == ShizukuStatus.WATCHING && current != SetUpStep.DONE) {
        Text(stringResource(R.string.setup_shizuku_running), style = MaterialTheme.typography.bodyMedium)
    }
    Progress(current)
    Column(modifier = Modifier.animateContentSize(), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        STEPS.forEachIndexed { index, step ->
            StepItem(
                number = index + 1,
                title = stringResource(stepTitle(step)),
                progress = when {
                    step.ordinal < current.ordinal -> StepProgress.DONE
                    step == current -> StepProgress.CURRENT
                    else -> StepProgress.LATER
                },
                shape = groupedShape(index, STEPS.size),
            ) {
                when (step) {
                    SetUpStep.PREPARE -> Prepare(copies, actions.autoCopy)
                    SetUpStep.DEVELOPER_OPTIONS -> DeveloperOptions(copies.vendor, actions.autoCopy)
                    SetUpStep.WIRELESS_DEBUGGING -> WirelessDebugging(copies.vendor, actions.autoCopy)
                    SetUpStep.CODE -> Code(copies, actions.autoCopy)
                    SetUpStep.DONE -> {}
                }
            }
        }
    }
    if (current == SetUpStep.DONE) {
        Done(copies, actions.autoCopy, onDone)
    } else {
        OtherWays(state, actions)
    }
}

@StringRes
private fun stepTitle(step: SetUpStep): Int = when (step) {
    SetUpStep.PREPARE -> R.string.setup_step_prepare
    SetUpStep.DEVELOPER_OPTIONS -> R.string.setup_step_developer
    SetUpStep.WIRELESS_DEBUGGING -> R.string.setup_step_debugging
    SetUpStep.CODE -> R.string.setup_step_code
    SetUpStep.DONE -> R.string.setup_step_done
}

/** "Step 2 of 4", and a bar that fills as the steps tick. */
@Composable
private fun Progress(current: SetUpStep) {
    val done = current.ordinal
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        LinearProgressIndicator(progress = { done / STEPS.size.toFloat() }, modifier = Modifier.fillMaxWidth())
        Text(
            if (current == SetUpStep.DONE) {
                stringResource(R.string.setup_all_done)
            } else {
                stringResource(R.string.setup_progress, done + 1, STEPS.size)
            },
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            // Read out as the steps tick, without moving the focus.
            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
        )
    }
}

private enum class StepProgress { DONE, CURRENT, LATER }

/** A step: its number (a check once done) and title; only the current one shows what to do. */
@Composable
private fun StepItem(number: Int, title: String, progress: StepProgress, shape: Shape, content: @Composable ColumnScope.() -> Unit) {
    val current = progress == StepProgress.CURRENT
    val label = stringResource(
        when (progress) {
            StepProgress.DONE -> R.string.step_done
            StepProgress.CURRENT -> R.string.setup_step_current
            StepProgress.LATER -> R.string.setup_step_later
        },
    )
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = shape,
        color = if (current) MaterialTheme.colorScheme.surfaceContainerHigh else MaterialTheme.colorScheme.surfaceContainer,
    ) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            // Read as "2, Developer options, heading, current step".
            Row(
                modifier = Modifier.semantics(mergeDescendants = true) {
                    heading()
                    stateDescription = label
                },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                StepBadge(number, progress)
                Text(
                    title,
                    style = MaterialTheme.typography.titleMedium,
                    color = if (progress == StepProgress.LATER) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
                )
            }
            if (current) {
                Column(
                    // Under the title, not under the badge.
                    modifier = Modifier.padding(start = 48.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    content = content,
                )
            }
        }
    }
}

/** A check once done; the step's number otherwise, filled for the current one. */
@Composable
private fun StepBadge(number: Int, progress: StepProgress) {
    val colors = MaterialTheme.colorScheme
    when (progress) {
        StepProgress.DONE -> Icon(
            painterResource(R.drawable.ic_check_circle),
            contentDescription = null,
            tint = colors.primary,
            modifier = Modifier.size(32.dp),
        )
        StepProgress.CURRENT -> Badge(number.toString(), colors.primary, colors.onPrimary, border = null)
        StepProgress.LATER -> Badge(number.toString(), Color.Transparent, colors.onSurfaceVariant, BorderStroke(1.dp, colors.outline))
    }
}

@Composable
private fun Badge(text: String, container: Color, content: Color, border: BorderStroke?) {
    Box(
        modifier = Modifier
            .size(32.dp)
            .then(if (border != null) Modifier.border(border, CircleShape) else Modifier)
            .background(container, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = MaterialTheme.typography.labelLarge, color = content)
    }
}

/** Step 1: Wi-Fi and notifications, each ticked live. */
@Composable
private fun Prepare(copies: CopiesState, actions: CopiesActions) {
    val done = stringResource(R.string.step_done)
    CheckedRow(
        title = stringResource(R.string.setup_wifi),
        text = stringResource(if (copies.checks.wifi) R.string.connected else R.string.setup_wifi_needed),
        done = copies.checks.wifi,
        doneLabel = done,
    )
    if (!copies.checks.wifi) OutlinedButton(onClick = actions.onOpenWifi) { Text(stringResource(R.string.step_open_wifi)) }
    CheckedRow(
        title = stringResource(R.string.setup_notifications),
        text = stringResource(if (copies.canNotify) R.string.setup_notifications_ok else R.string.builtin_needs_notifications),
        done = copies.canNotify,
        doneLabel = done,
    )
    if (!copies.canNotify) {
        OutlinedButton(onClick = actions.onOpenNotificationSettings) { Text(stringResource(R.string.builtin_allow_notifications)) }
    }
    // HyperOS's own notification style hides the field the code is typed in.
    if (copies.vendor == Vendor.XIAOMI) Hint(stringResource(R.string.setup_xiaomi_notification_style))
}

/** Step 2: the vendor's way to turn on Developer options. */
@Composable
private fun DeveloperOptions(vendor: Vendor, actions: CopiesActions) {
    Body(stringResource(if (vendor == Vendor.XIAOMI) R.string.setup_developer_xiaomi else R.string.setup_developer_other))
    Buttons {
        Button(onClick = actions.onOpenAboutPhone) { Text(stringResource(R.string.setup_open_about)) }
        // Some phones say they're off when they're on: turning on wireless debugging ticks this too.
        TextButton(onClick = actions.onOpenWirelessDebugging) { Text(stringResource(R.string.setup_developer_on)) }
    }
}

/** Step 3: Wireless debugging, in Developer options. */
@Composable
private fun WirelessDebugging(vendor: Vendor, actions: CopiesActions) {
    Body(stringResource(if (vendor == Vendor.XIAOMI) R.string.setup_debugging_xiaomi else R.string.setup_debugging_other))
    Button(onClick = actions.onOpenWirelessDebugging) { Text(stringResource(R.string.builtin_open_settings)) }
}

/**
 * Step 4: the pairing code, typed in paste-get's notification (or here, in split screen), and
 * how far it got. Each failure becomes a sentence and a Retry.
 */
@Composable
private fun Code(copies: CopiesState, actions: CopiesActions) {
    val builtIn = copies.builtIn
    val pairing = builtIn.status == BuiltInStatus.PAIRING
    val step = builtIn.step ?: PairingStep.WAITING_FOR_CODE
    val waiting = step == PairingStep.WAITING_FOR_CODE
    Body(stringResource(if (copies.splitScreen) R.string.setup_code_split else R.string.setup_code_text))
    if (pairing && waiting && builtIn.failure != null) Failure(builtIn, actions.onSetUpBuiltIn)
    if (pairing && copies.splitScreen) CodeField(builtIn, onCode = actions.onSubmitCode)
    if (pairing && !waiting) PairingSteps(step)
    if (!pairing || waiting) {
        Buttons {
            if (!pairing) {
                Button(onClick = actions.onSetUpBuiltIn) { Text(stringResource(R.string.setup_open_code)) }
            } else if (builtIn.failure == null) {
                OutlinedButton(onClick = actions.onSetUpBuiltIn) { Text(stringResource(R.string.setup_open_code)) }
            }
            if (pairing) TextButton(onClick = actions.onResetBuiltIn) { Text(stringResource(R.string.builtin_cancel)) }
        }
    }
    if (!copies.splitScreen) SplitScreenHelp()
}

/** What went wrong with the last code, in a sentence, and Retry. */
@Composable
private fun Failure(builtIn: BuiltInState, onRetry: () -> Unit) {
    var details by rememberSaveable { mutableStateOf(false) }
    val text = when (builtIn.failure) {
        PairingFailure.WRONG_CODE -> R.string.setup_failed_wrong_code
        PairingFailure.NO_CODE_SCREEN -> R.string.setup_failed_no_screen
        PairingFailure.NO_DEBUGGING -> R.string.setup_failed_no_debugging
        PairingFailure.OTHER, null -> R.string.setup_failed_other
    }
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.errorContainer,
            contentColor = MaterialTheme.colorScheme.onErrorContainer,
        ),
    ) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                stringResource(text),
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            )
            if (details) {
                SelectionContainer {
                    Text(builtIn.detail.orEmpty(), style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
                }
            }
            Buttons {
                Button(onClick = onRetry) { Text(stringResource(R.string.builtin_retry)) }
                if (!builtIn.detail.isNullOrEmpty()) {
                    TextButton(onClick = { details = !details }) {
                        Text(stringResource(if (details) R.string.hide_details else R.string.details))
                    }
                }
            }
        }
    }
}

/** Finding the code screen, pairing, getting permission: the one under way turns. */
@Composable
private fun PairingSteps(step: PairingStep) {
    val steps = listOf(
        PairingStep.FINDING to R.string.setup_pair_finding,
        PairingStep.PAIRING to R.string.setup_pair_pairing,
        PairingStep.GRANTING to R.string.setup_pair_granting,
    )
    Column(
        modifier = Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        steps.forEach { (each, label) ->
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Box(modifier = Modifier.size(24.dp), contentAlignment = Alignment.Center) {
                    when {
                        each.ordinal < step.ordinal -> Icon(
                            painterResource(R.drawable.ic_check),
                            contentDescription = stringResource(R.string.step_done),
                            tint = MaterialTheme.colorScheme.primary,
                        )
                        each == step -> CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                        else -> {}
                    }
                }
                Text(
                    stringResource(label),
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (each.ordinal > step.ordinal) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
                )
            }
        }
    }
}

/**
 * Six boxes for the six digits, sent as soon as the sixth is typed. Emptied when a code didn't
 * work, ready for the next one; locked while one is being paired.
 */
@Composable
private fun CodeField(builtIn: BuiltInState, onCode: (String) -> Unit) {
    var code by rememberSaveable { mutableStateOf("") }
    val waiting = (builtIn.step ?: PairingStep.WAITING_FOR_CODE) == PairingStep.WAITING_FOR_CODE
    LaunchedEffect(builtIn) { if (waiting && builtIn.failure != null) code = "" }
    val label = stringResource(R.string.builtin_pair_hint)
    BasicTextField(
        value = code,
        onValueChange = { typed ->
            val digits = typed.filter(Char::isDigit).take(CODE_LENGTH)
            if (digits.length == CODE_LENGTH && digits != code) onCode(digits)
            code = digits
        },
        enabled = waiting,
        singleLine = true,
        // Digits TalkBack reads out: the code is on the screen beside anyway.
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
        // Above the keyboard.
        modifier = Modifier.imePadding().semantics { contentDescription = label },
        decorationBox = {
            // The boxes stand for the field: TalkBack reads the field, named above.
            Row(modifier = Modifier.clearAndSetSemantics {}, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                repeat(CODE_LENGTH) { index ->
                    val filled = index < code.length
                    Box(
                        modifier = Modifier
                            .size(width = 44.dp, height = 56.dp)
                            .border(
                                width = if (index == code.length && waiting) 2.dp else 1.dp,
                                color = if (index == code.length && waiting) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline,
                                shape = MaterialTheme.shapes.medium,
                            ),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(
                            if (filled) code[index].toString() else "",
                            style = MaterialTheme.typography.headlineSmall,
                            textAlign = TextAlign.Center,
                        )
                    }
                }
            }
        },
    )
}

private const val CODE_LENGTH = 6

/** How to put Settings next to paste-get, so the code can be typed here. */
@Composable
private fun SplitScreenHelp() {
    var shown by rememberSaveable { mutableStateOf(false) }
    TextButton(onClick = { shown = !shown }) { Text(stringResource(R.string.setup_use_split)) }
    if (shown) Body(stringResource(R.string.setup_split_how))
}

/** Set up: what that means now, the notice switch it makes possible, and the way out. */
@Composable
private fun Done(copies: CopiesState, actions: CopiesActions, onDone: () -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.extraLarge,
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.primaryContainer,
            contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
        ),
    ) {
        Column(modifier = Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(
                modifier = Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Icon(painterResource(R.drawable.ic_check_circle), contentDescription = null)
                Text(
                    stringResource(R.string.setup_step_done),
                    style = MaterialTheme.typography.headlineSmall,
                    modifier = Modifier.semantics { heading() },
                )
            }
            Text(stringResource(R.string.setup_done_text), style = MaterialTheme.typography.bodyLarge)
            when (copies.builtIn.status) {
                BuiltInStatus.STARTING -> Text(stringResource(R.string.builtin_starting), style = MaterialTheme.typography.bodyMedium)
                BuiltInStatus.NEEDS_WIFI -> Text(stringResource(R.string.builtin_needs_wifi), style = MaterialTheme.typography.bodyMedium)
                BuiltInStatus.FAILED -> {
                    Text(
                        stringResource(R.string.builtin_failed, copies.builtIn.detail.orEmpty()),
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    OutlinedButton(onClick = actions.onRetryBuiltIn) { Text(stringResource(R.string.builtin_retry)) }
                }
                else -> {}
            }
            Button(onClick = onDone) { Text(stringResource(R.string.setup_done_button)) }
        }
    }
    // Possible now: the built-in mode granted what changes it.
    if (copies.notices.switchable) {
        Card(modifier = Modifier.fillMaxWidth()) {
            Box(modifier = Modifier.padding(16.dp)) {
                SwitchRow(
                    title = stringResource(R.string.notices_title),
                    text = stringResource(R.string.notices_text),
                    checked = copies.notices.hidden == true,
                    onCheckedChange = actions.onHideNotices,
                )
            }
        }
    }
}

/** Shizuku instead, or sending by hand: each behind a link, so they don't compete with the steps. */
@Composable
private fun OtherWays(state: AppState, actions: AppActions) {
    var shizuku by rememberSaveable { mutableStateOf(false) }
    var byHand by rememberSaveable { mutableStateOf(false) }
    Column(modifier = Modifier.animateContentSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        TextButton(onClick = { shizuku = !shizuku }) { Text(stringResource(R.string.setup_shizuku_link)) }
        if (shizuku) {
            Body(stringResource(R.string.setup_shizuku_text))
            ShizukuLine(state.autoCopy.shizuku, actions.autoCopy)
        }
        TextButton(onClick = { byHand = !byHand }) { Text(stringResource(R.string.setup_manual_link)) }
        if (byHand) {
            Body(stringResource(R.string.setup_manual_text))
            if (state.settings.sdk >= Build.VERSION_CODES.TIRAMISU) {
                OutlinedButton(onClick = actions.settings.onAddTile) { Text(stringResource(R.string.tile_add)) }
            } else {
                Body(stringResource(R.string.tile_how))
            }
        }
    }
}

/** A tip that applies to this phone only. */
@Composable
private fun Hint(text: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Icon(
            painterResource(R.drawable.ic_info),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(20.dp),
        )
        Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun Body(text: String) {
    Text(text, style = MaterialTheme.typography.bodyMedium)
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Buttons(content: @Composable () -> Unit) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) { content() }
}
