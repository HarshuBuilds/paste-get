// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import io.github.nexoryx.pasteget.AccessNotices
import io.github.nexoryx.pasteget.Activity
import io.github.nexoryx.pasteget.BuiltInState
import io.github.nexoryx.pasteget.BuiltInStatus
import io.github.nexoryx.pasteget.CodeAccess
import io.github.nexoryx.pasteget.FallbackAccess
import io.github.nexoryx.pasteget.LogStatus
import io.github.nexoryx.pasteget.PhotoAccess
import io.github.nexoryx.pasteget.ShizukuState
import io.github.nexoryx.pasteget.ShizukuStatus
import uniffi.pasteget_ffi.PairedComputer

/** The app's states for the Compose tests, without the app or its native library. */
object AppFixtures {
    val hp = PairedComputer("a1", "HP-AIO", listOf("192.168.1.20:47290"))
    val arch = PairedComputer("b2", "Arch-PC", listOf("192.168.1.21:47290"))
    val phone = ThisPhone("POCO M8 5G", "3F2A9C11ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKL")

    fun state(
        computers: List<PairedComputer> = emptyList(),
        connected: Set<String> = emptySet(),
        message: String? = null,
        copiesOn: Boolean = false,
        screenshots: ScreenshotSetting = ScreenshotSetting(wanted = false, access = PhotoAccess.NONE),
        codesOn: Boolean = false,
        builtInStatus: BuiltInStatus = if (copiesOn) BuiltInStatus.NOT_SET_UP else BuiltInStatus.OFF,
        canNotify: Boolean = true,
        activity: List<Activity> = emptyList(),
        unpaired: PairedComputer? = null,
        copiesSetUp: Boolean = false,
    ): AppState {
        val builtIn = BuiltInState(builtInStatus)
        val shizuku = ShizukuState(if (copiesOn) ShizukuStatus.NOT_INSTALLED else ShizukuStatus.OFF)
        val logs = if (copiesOn) LogStatus.NEEDS_PERMISSIONS else LogStatus.OFF
        val fallback = FallbackAccess(readLogs = false, overlay = false)
        val codeAccess = CodeAccess(listening = false, seesCodes = null)
        val status = appStatus(
            computers,
            connected,
            unpaired,
            listOf(
                copiesHealth(copiesOn, builtIn, shizuku, logs, fallback, canNotify),
                screenshotsHealth(screenshots.wanted, screenshots.access),
                codesHealth(codesOn, codeAccess, shellReady = false),
            ),
        )
        return AppState(
            status = status,
            computers = computers,
            connected = connected,
            message = message,
            pairing = PairingState.Idle,
            activity = activity,
            screenshots = screenshots,
            photoRequestDenied = false,
            autoCopy = CopiesState(
                copiesOn,
                builtIn,
                shizuku,
                logs,
                fallback,
                AccessNotices(supported = false, hidden = null, canChange = false),
                lastCopy = null,
                canNotify = canNotify,
                setUp = copiesSetUp,
            ),
            codes = CodesState(codesOn, codeAccess, shellReady = false, last = null, codesHidden = true),
            thisPhone = phone,
        )
    }
}
