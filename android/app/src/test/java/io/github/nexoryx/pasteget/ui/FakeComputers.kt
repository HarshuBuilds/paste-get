// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import uniffi.pasteget_ffi.PairedComputer

/** Stands in for the Rust client: records what it is asked, pairs with [pairsWith] or throws [failure]. */
class FakeComputers(
    var computers: List<PairedComputer> = emptyList(),
    var pairsWith: PairedComputer = AppFixtures.hp,
    var failure: Exception? = null,
) : Computers {
    val pairCalls = mutableListOf<String>()

    override fun paired(): List<PairedComputer> = computers

    override fun thisPhone() = AppFixtures.phone

    override fun pair(code: String): PairedComputer {
        pairCalls += code
        failure?.let { throw it }
        computers = computers.filter { it.deviceId != pairsWith.deviceId } + pairsWith
        return pairsWith
    }

    override fun forget(deviceId: String) {
        computers = computers.filter { it.deviceId != deviceId }
    }
}
