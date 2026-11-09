// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.pasteget_ffi.InviteSummary

/** The Rust core reads the codes (tested and fuzzed there); the app only shows what it read. */
class PairingCodeTest {
    @Test
    fun acceptsCodesAndTrimsWhitespace() {
        val code = "pasteget://pair?v=0&id=ABC&s=DEF&a=192.168.1.2%3A47290&n=PC"

        assertEquals(code, PairingCode.extract("  $code\n"))
    }

    @Test
    fun rejectsEverythingElse() {
        assertNull(PairingCode.extract(null))
        assertNull(PairingCode.extract(""))
        assertNull(PairingCode.extract("https://example.com/?pasteget://pair?"))
        assertNull(PairingCode.extract("pasteget://other?v=0"))
    }

    @Test
    fun showsWhatTheCoreRead() {
        // A PC with VPNs, Hyper-V and Docker lists many addresses: all are shown.
        val addresses = (1..12).map { "10.0.$it.2:47290" }

        assertEquals(
            PairingCode.Invite("3F2A", "HP All-in-One", addresses),
            PairingCode.invite(InviteSummary("3F2A", "HP All-in-One", addresses)),
        )
        // No name: shown as such, not as an empty line.
        assertNull(PairingCode.invite(InviteSummary("3F2A", null, addresses)).name)
    }

    @Test
    fun namesCantHideOrReorderLetters() {
        // U+202E RIGHT-TO-LEFT OVERRIDE and a zero-width space, which the core lets through.
        val summary = InviteSummary("3F2A", "‮OIA-PH​", listOf("10.0.0.5:47290"))
        assertEquals("OIA-PH", PairingCode.invite(summary).name)
        assertNull(PairingCode.displayName("‮​ "))
        assertEquals(64, PairingCode.displayName("x".repeat(200))?.length)
    }
}
