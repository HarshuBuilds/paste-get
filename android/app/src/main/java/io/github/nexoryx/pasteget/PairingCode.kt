// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import java.net.URLDecoder

/** Pairing codes shown as QR codes by the computer. The Rust core does the real parsing. */
object PairingCode {
    private const val PREFIX = "pasteget://pair?"

    /** As in the core: longer codes are refused. */
    private const val MAX_LENGTH = 1024
    private const val MAX_NAME_CHARS = 64
    private const val MAX_ADDRESSES = 8

    /** What an address looks like: digits, dots and colons (IPv6 in brackets), a port. */
    private val ADDRESS = Regex("""[0-9A-Fa-f.:\[\]]{1,64}""")

    /** The pairing code in scanned or received [text], or null if it isn't one. */
    fun extract(text: String?): String? = text?.trim()?.takeIf { it.startsWith(PREFIX) }

    /**
     * What [code] says about the computer, for the user to check before pairing; `null` if it
     * can't be read. None of it is checked: anyone can make a code, or a link, with any name and
     * addresses. Pairing itself proves the computer has the code's key.
     */
    fun describe(code: String): Invite? {
        if (code.length > MAX_LENGTH || !code.startsWith(PREFIX)) return null
        val fields = mutableMapOf<String, String>()
        for (pair in code.removePrefix(PREFIX).split('&')) {
            if (pair.isEmpty()) continue
            val key = decode(pair.substringBefore('=')) ?: return null
            val value = decode(pair.substringAfter('=', "")) ?: return null
            // A key given twice could show one value and pair with the other.
            if (fields.put(key, value) != null) return null
        }
        if (fields["v"] != "0" || fields["id"].isNullOrEmpty() || fields["s"].isNullOrEmpty()) return null
        val addresses = fields["a"]?.split(',').orEmpty()
        if (addresses.isEmpty() || addresses.size > MAX_ADDRESSES || !addresses.all(ADDRESS::matches)) return null
        return Invite(fields.getValue("id"), displayName(fields["n"].orEmpty()), addresses)
    }

    /**
     * Without control and formatting characters, which could hide or reorder what's shown (a
     * right-to-left override can make "HP-AIO" out of other letters). `null` if nothing is left.
     */
    fun displayName(name: String): String? = name
        .filter { it.category != CharCategory.CONTROL && it.category != CharCategory.FORMAT }
        .take(MAX_NAME_CHARS)
        .trim()
        .ifEmpty { null }

    private fun decode(text: String): String? = try {
        // The Charset overload is API 33+.
        URLDecoder.decode(text, "UTF-8")
    } catch (e: IllegalArgumentException) {
        // A broken %-escape.
        null
    }

    /** A pairing code as shown to the user. [name] is `null` when the code has none. */
    data class Invite(val deviceId: String, val name: String?, val addresses: List<String>)
}
