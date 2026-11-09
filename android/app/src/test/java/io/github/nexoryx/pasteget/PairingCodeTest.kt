// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import kotlin.random.Random

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
    fun describesWhatTheCodeSays() {
        val code = "pasteget://pair?v=0&id=3f2a9c&s=ABCDEFGH&a=192.168.1.20%3A47290%2C%5Bfe80%3A%3A1%5D%3A47290&n=HP+All-in-One"

        assertEquals(
            PairingCode.Invite("3f2a9c", "HP All-in-One", listOf("192.168.1.20:47290", "[fe80::1]:47290")),
            PairingCode.describe(code),
        )
        // No name: shown as such, not as an empty line.
        assertNull(PairingCode.describe("pasteget://pair?v=0&id=3f&s=AB&a=10.0.0.5%3A47290")?.name)
        assertEquals("10.0.0.5:47290", PairingCode.describe("pasteget://pair?v=0&id=3f&s=AB&a=10.0.0.5%3A47290")?.addresses?.single())
    }

    @Test
    fun namesCantHideOrReorderLetters() {
        // U+202E RIGHT-TO-LEFT OVERRIDE, a zero-width space and a newline.
        val code = "pasteget://pair?v=0&id=3f&s=AB&a=10.0.0.5%3A47290&n=%E2%80%AEOIA-PH%E2%80%8B%0A"
        assertEquals("OIA-PH", PairingCode.describe(code)?.name)
        assertNull(PairingCode.displayName("\u202E\u200B "))
        assertEquals(64, PairingCode.displayName("x".repeat(200))?.length)
    }

    @Test
    fun refusesCodesItCantShowFaithfully() {
        val valid = "pasteget://pair?v=0&id=3f&s=AB&a=10.0.0.5%3A47290&n=PC"
        val broken = listOf(
            // A key twice: one value could be shown and the other used.
            "$valid&n=Other",
            "$valid&a=10.0.0.6%3A47290",
            valid.replace("v=0", "v=1"),
            valid.replace("id=3f&", ""),
            valid.replace("s=AB&", "s=&"),
            valid.replace("a=10.0.0.5%3A47290&", ""),
            valid.replace("10.0.0.5", "evil.example"),
            valid.replace("n=PC", "n=%ZZ"),
            "$valid&pad=${"a".repeat(1024)}",
            "pasteget://other?v=0",
        )
        for (code in broken) assertNull(code, PairingCode.describe(code))
    }

    /** Untrusted input: whatever a link holds, reading it never throws. */
    @Test
    fun anyInputIsReadWithoutThrowing() {
        val random = Random(42)
        val alphabet = "pasteget://pair?v=0&id=s=a=n=%,:.[]+ZzE2%80%AE0123456789"
        repeat(20_000) {
            val text = "pasteget://pair?" + String(CharArray(random.nextInt(0, 80)) { alphabet[random.nextInt(alphabet.length)] })
            PairingCode.describe(text)
        }
    }
}
