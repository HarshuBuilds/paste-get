// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import uniffi.pasteget_ffi.InviteSummary

/**
 * Pairing codes shown as QR codes by the computer. The Rust core reads them, for the
 * confirmation (`describeInvite`) as for pairing: one parser, fuzzed.
 */
object PairingCode {
    private const val PREFIX = "pasteget://pair?"
    private const val MAX_NAME_CHARS = 64

    /** The pairing code in scanned or received [text], or null if it isn't one. */
    fun extract(text: String?): String? = text?.trim()?.takeIf { it.startsWith(PREFIX) }

    /**
     * What the core read in a code, as the confirmation shows it. None of it is checked: anyone
     * can make a code, or a link, with any name and addresses. Pairing itself proves the computer
     * has the code's key.
     */
    fun invite(summary: InviteSummary): Invite =
        Invite(summary.deviceId, summary.name?.let(::displayName), summary.addresses)

    /**
     * Without control and formatting characters, which could hide or reorder what's shown (a
     * right-to-left override can make "HP-AIO" out of other letters). `null` if nothing is left.
     */
    fun displayName(name: String): String? = name
        .filter { it.category != CharCategory.CONTROL && it.category != CharCategory.FORMAT }
        .take(MAX_NAME_CHARS)
        .trim()
        .ifEmpty { null }

    /** A pairing code as shown to the user. [name] is `null` when the code has none. */
    data class Invite(val deviceId: String, val name: String?, val addresses: List<String>)
}
