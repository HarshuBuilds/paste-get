// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import io.github.nexoryx.pasteget.PasteGetApp
import io.github.nexoryx.pasteget.SyncService
import uniffi.pasteget_ffi.InviteSummary
import uniffi.pasteget_ffi.PairedComputer
import uniffi.pasteget_ffi.describeInvite

/**
 * What the screens need from the Rust client, so tests can use a fake one. Every call may block
 * (the native library loads on first use, pairing takes seconds): never on the main thread.
 */
interface Computers {
    fun paired(): List<PairedComputer>

    /** This phone's name and device ID. */
    fun thisPhone(): ThisPhone

    /** What [code] says, read as [pair] reads it; `null` if pairing would refuse it. */
    fun describe(code: String): InviteSummary?

    /** Throws `PastegetException` if pairing fails. */
    fun pair(code: String): PairedComputer

    /** Throws `PastegetException` if the computer is unknown or the change can't be saved. */
    fun forget(deviceId: String)

    /** Dials every computer now, rather than after its backoff, and searches the network again. */
    fun retry()
}

/** The app's own client. */
class NativeComputers(private val app: PasteGetApp) : Computers {
    override fun paired(): List<PairedComputer> = app.client.pairedComputers()

    override fun thisPhone() = ThisPhone(app.deviceName(), app.client.deviceId())

    override fun describe(code: String): InviteSummary? = describeInvite(code)

    override fun pair(code: String): PairedComputer = app.client.pair(code)

    override fun forget(deviceId: String) {
        app.client.forget(deviceId)
        app.setConnected(deviceId, false)
    }

    override fun retry() = SyncService.retry(app)
}
