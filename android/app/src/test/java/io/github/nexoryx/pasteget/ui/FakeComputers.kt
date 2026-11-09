// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import uniffi.pasteget_ffi.InviteSummary
import uniffi.pasteget_ffi.PairedComputer
import java.net.URLDecoder

/** Stands in for the Rust client: records what it is asked, pairs with [pairsWith] or throws [failure]. */
class FakeComputers(
    var computers: List<PairedComputer> = emptyList(),
    var pairsWith: PairedComputer = AppFixtures.hp,
    var failure: Exception? = null,
    /** Thrown by [forget]. */
    var forgetFailure: Exception? = null,
) : Computers {
    val pairCalls = mutableListOf<String>()
    var retries = 0

    override fun paired(): List<PairedComputer> = computers

    override fun thisPhone() = AppFixtures.phone

    /** The fields of a test code, without the core's checks: `null` without an ID, a secret or an address. */
    override fun describe(code: String): InviteSummary? {
        val fields = code.substringAfter('?').split('&').associate {
            it.substringBefore('=') to URLDecoder.decode(it.substringAfter('=', ""), "UTF-8")
        }
        val id = fields["id"] ?: return null
        if (fields["s"].isNullOrEmpty()) return null
        val addresses = fields["a"]?.split(',') ?: return null
        return InviteSummary(id, fields["n"], addresses)
    }

    override fun pair(code: String): PairedComputer {
        pairCalls += code
        failure?.let { throw it }
        computers = computers.filter { it.deviceId != pairsWith.deviceId } + pairsWith
        return pairsWith
    }

    override fun forget(deviceId: String) {
        forgetFailure?.let { throw it }
        computers = computers.filter { it.deviceId != deviceId }
    }

    override fun retry() {
        retries++
    }
}
