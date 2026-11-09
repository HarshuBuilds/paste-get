// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import io.github.nexoryx.pasteget.Activity
import uniffi.pasteget_ffi.PairedComputer

/** Everything the app's screens show. They read nothing else, so tests draw them from fixtures. */
data class AppState(
    val status: AppStatus,
    val computers: List<PairedComputer>,
    val connected: Set<String>,
    /** The result of the last thing the user did, or what just happened. */
    val message: String?,
    val pairing: PairingState,
    /** Newest first. */
    val activity: List<Activity>,
    val screenshots: ScreenshotSetting,
    val photoRequestDenied: Boolean,
    val autoCopy: AutoCopyCardState,
    val codes: CodesCardState,
    /** `null` until read from the Rust client. */
    val thisPhone: ThisPhone? = null,
    /** The computer that last unpaired this phone, until the user pairs again or closes the notice. */
    val unpairedBy: PairedComputer? = null,
    /** The computers are being dialed again ([AppActions.onRefresh]). */
    val refreshing: Boolean = false,
)

/** This phone as its computers know it. */
data class ThisPhone(val name: String, val deviceId: String)

/** What the screens' buttons and switches do. Each does nothing unless given, for tests. */
class AppActions(
    /** Scans a pairing code. */
    val onPair: () -> Unit = {},
    val onConfirmPairing: () -> Unit = {},
    /** No to the code shown, or closing the pairing error. */
    val onCancelPairing: () -> Unit = {},
    val onSendClipboard: () -> Unit = {},
    val onForget: (PairedComputer) -> Unit = {},
    /** Closes "<PC> unpaired this phone". */
    val onDismissUnpaired: () -> Unit = {},
    /** Dials every computer now and searches the network again. */
    val onRefresh: () -> Unit = {},
    val onScreenshotsChange: (Boolean) -> Unit = {},
    /** paste-get's page in the system settings, where photo access is. */
    val onOpenAppSettings: () -> Unit = {},
    val autoCopy: AutoCopyActions = AutoCopyActions(),
    val codes: CodesActions = CodesActions(),
)
