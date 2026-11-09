// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import io.github.nexoryx.pasteget.BuiltInState
import io.github.nexoryx.pasteget.BuiltInStatus
import io.github.nexoryx.pasteget.PairingFailure
import io.github.nexoryx.pasteget.PairingStep
import io.github.nexoryx.pasteget.SetUpChecks
import org.junit.Assert.assertEquals
import org.junit.Test

/** The set-up's current step is the first one not done, as the phone's settings say. */
class SetUpStepTest {
    private val ready = SetUpChecks(wifi = true, developerOptions = true, wirelessDebugging = true)
    private val notSetUp = BuiltInState(BuiltInStatus.NOT_SET_UP)

    private fun step(
        checks: SetUpChecks = ready,
        canNotify: Boolean = true,
        splitScreen: Boolean = false,
        builtIn: BuiltInState = notSetUp,
        setUp: Boolean = false,
    ) = setUpStep(checks, canNotify, splitScreen, builtIn, setUp)

    @Test
    fun eachMissingSettingIsItsOwnStep() {
        assertEquals(SetUpStep.PREPARE, step(checks = ready.copy(wifi = false)))
        assertEquals(SetUpStep.DEVELOPER_OPTIONS, step(checks = ready.copy(developerOptions = false, wirelessDebugging = false)))
        assertEquals(SetUpStep.WIRELESS_DEBUGGING, step(checks = ready.copy(wirelessDebugging = false)))
        assertEquals(SetUpStep.CODE, step())
    }

    @Test
    fun theFirstMissingOneComesFirst() {
        val nothing = SetUpChecks(wifi = false, developerOptions = false, wirelessDebugging = false)
        assertEquals(SetUpStep.PREPARE, step(checks = nothing))
        assertEquals(SetUpStep.DEVELOPER_OPTIONS, step(checks = nothing.copy(wifi = true)))
    }

    @Test
    fun withoutNotificationsOnlySplitScreenCanTakeTheCode() {
        assertEquals(SetUpStep.PREPARE, step(canNotify = false))
        assertEquals(SetUpStep.CODE, step(canNotify = false, splitScreen = true))
    }

    @Test
    fun aCodeBeingPairedStaysOnItsStep() {
        // Wireless debugging may blink off while pairing: the step doesn't jump back.
        for (pairingStep in listOf(PairingStep.FINDING, PairingStep.PAIRING, PairingStep.GRANTING)) {
            val pairing = BuiltInState(BuiltInStatus.PAIRING, step = pairingStep)
            assertEquals(SetUpStep.CODE, step(checks = ready.copy(wirelessDebugging = false), builtIn = pairing))
        }
        // Waiting for a code (a failed one, too): the settings count again.
        val failed = BuiltInState(BuiltInStatus.PAIRING, "wrong", PairingStep.WAITING_FOR_CODE, PairingFailure.WRONG_CODE)
        assertEquals(SetUpStep.WIRELESS_DEBUGGING, step(checks = ready.copy(wirelessDebugging = false), builtIn = failed))
    }

    @Test
    fun onceSetUpItIsDone() {
        // paste-get turns wireless debugging off once its helper runs.
        val off = SetUpChecks(wifi = false, developerOptions = true, wirelessDebugging = false)
        assertEquals(SetUpStep.DONE, step(checks = off, builtIn = BuiltInState(BuiltInStatus.WATCHING), setUp = true))
    }
}
