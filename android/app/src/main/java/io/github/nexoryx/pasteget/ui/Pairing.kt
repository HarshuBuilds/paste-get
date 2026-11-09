// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.annotation.StringRes
import io.github.nexoryx.pasteget.PairingCode
import io.github.nexoryx.pasteget.R
import uniffi.pasteget_ffi.PastegetException

/**
 * Where pairing stands. A code, scanned or from a `pasteget://` link (which any web page or
 * message can open), only ever pairs after the user confirmed it in [Confirm].
 */
sealed interface PairingState {
    data object Idle : PairingState

    /** Waiting for the user to check [invite] and say yes. */
    data class Confirm(val code: String, val invite: PairingCode.Invite) : PairingState {
        // The code holds the pairing secret: never in a log or a crash report.
        override fun toString() = "Confirm($invite)"
    }

    /** Pairing: about 10 s at most per address. */
    data object Busy : PairingState

    data class Failed(val error: PairingError) : PairingState
}

/** A failed pairing: what to tell the user ([message]), and the technical [details]. */
data class PairingError(@param:StringRes val message: Int, val details: String)

/**
 * What to tell the user when pairing fails. The Rust errors are in English and technical: they
 * go under "Details", for bug reports.
 */
fun pairingError(e: Exception): PairingError {
    val message = when (e) {
        is PastegetException.InvalidCode -> R.string.pair_error_invalid
        is PastegetException.Unreachable -> R.string.pair_error_unreachable
        is PastegetException.PairingFailed -> R.string.pair_error_refused
        is PastegetException.Storage -> R.string.pair_error_storage
        else -> R.string.pair_error_other
    }
    return PairingError(message, e.message ?: e.javaClass.simpleName)
}

/** A code that starts like one but can't be read. */
fun unreadableCode(): PairingError = PairingError(R.string.pair_error_invalid, "the code's fields can't be read")
