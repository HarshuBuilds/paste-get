// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.hapticfeedback.HapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.test.junit4.v2.createComposeRule
import io.github.nexoryx.pasteget.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** A pairing that worked is felt as well as read; one that failed isn't. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PairingHapticsTest {
    @get:Rule
    val compose = createComposeRule()

    private val felt = mutableListOf<HapticFeedbackType>()
    private val haptics = object : HapticFeedback {
        override fun performHapticFeedback(hapticFeedbackType: HapticFeedbackType) {
            felt += hapticFeedbackType
        }
    }

    private fun show(initial: PairingState): (PairingState) -> Unit {
        var pairing by mutableStateOf(initial)
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(LocalHapticFeedback provides haptics) {
                    PairingFlow(pairing, emptyList(), onConfirm = {}, onCancel = {})
                }
            }
        }
        return { pairing = it }
    }

    @Test
    fun pairedBuzzesOnce() {
        val set = show(PairingState.Busy)
        set(PairingState.Idle)
        compose.waitForIdle()
        assertEquals(listOf(HapticFeedbackType.Confirm), felt)
    }

    @Test
    fun aFailureDoesnt() {
        val set = show(PairingState.Busy)
        set(PairingState.Failed(PairingError(R.string.pair_error_unreachable, "no answer")))
        compose.waitForIdle()
        assertEquals(emptyList<HapticFeedbackType>(), felt)
    }
}
