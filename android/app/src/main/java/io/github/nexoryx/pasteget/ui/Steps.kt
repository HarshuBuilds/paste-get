// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.annotation.StringRes
import io.github.nexoryx.pasteget.R

/** The button that does [step]: its label. */
@StringRes
fun stepLabel(step: NextStep): Int = when (step) {
    NextStep.SET_UP_BUILT_IN -> R.string.setup_start
    NextStep.ALLOW_NOTIFICATIONS -> R.string.builtin_allow_notifications
    NextStep.FINISH_BUILT_IN_PAIRING -> R.string.setup_continue
    NextStep.RETRY_BUILT_IN -> R.string.builtin_retry
    NextStep.CONNECT_WIFI -> R.string.step_open_wifi
    NextStep.ALLOW_SHIZUKU -> R.string.shizuku_allow
    NextStep.ALLOW_LOG_ACCESS -> R.string.logs_retry
    NextStep.ALLOW_OVERLAY -> R.string.fallback_allow_overlay
    NextStep.ALLOW_PHOTOS -> R.string.step_allow_photos
    NextStep.ALLOW_ALL_PHOTOS -> R.string.open_settings
    NextStep.ALLOW_CODES -> R.string.codes_allow
    NextStep.OPEN_NOTIFICATION_ACCESS -> R.string.codes_open_settings
    NextStep.SET_UP_SHELL_FOR_CODES -> R.string.codes_set_up_shell
}

/**
 * Does [step], with the same callbacks as the feature's own buttons. The built-in mode's set-up
 * has its own screen, opened with [onOpen].
 */
fun runStep(step: NextStep, actions: AppActions, onOpen: (Route) -> Unit) {
    when (step) {
        NextStep.SET_UP_BUILT_IN, NextStep.FINISH_BUILT_IN_PAIRING -> onOpen(Route.CopiesSetUp)
        NextStep.ALLOW_NOTIFICATIONS -> actions.autoCopy.onOpenNotificationSettings()
        NextStep.RETRY_BUILT_IN -> actions.autoCopy.onRetryBuiltIn()
        NextStep.CONNECT_WIFI -> actions.autoCopy.onOpenWifi()
        NextStep.ALLOW_SHIZUKU -> actions.autoCopy.onAllowShizuku()
        NextStep.ALLOW_LOG_ACCESS -> actions.autoCopy.onRetryLogs()
        NextStep.ALLOW_OVERLAY -> actions.autoCopy.onAllowOverlay()
        // Asks for photo access, as the switch does.
        NextStep.ALLOW_PHOTOS -> actions.onScreenshotsChange(true)
        NextStep.ALLOW_ALL_PHOTOS -> actions.onOpenAppSettings()
        NextStep.ALLOW_CODES -> actions.codes.onAllow()
        NextStep.OPEN_NOTIFICATION_ACCESS -> actions.codes.onOpenSettings()
        // Codes on Android 15 need the shell the automatic copy runs: turned on, then set up.
        NextStep.SET_UP_SHELL_FOR_CODES -> {
            actions.autoCopy.onChange(true)
            onOpen(Route.CopiesSetUp)
        }
    }
}

/** "Copies need one more step", for the status card. */
@StringRes
fun needsStepText(feature: Feature): Int = when (feature) {
    Feature.COPIES -> R.string.hero_step_copies
    Feature.SCREENSHOTS -> R.string.hero_step_screenshots
    Feature.CODES -> R.string.hero_step_codes
}
