// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.app.Application
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** What the phone says after sending by hand (tile, Share, the home screen's button). */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class SenderTest {
    private val app: Application = RuntimeEnvironment.getApplication()

    @Test
    fun sentToTheConnectedComputers() {
        assertEquals("Sent to 2 computers", Sender.resultText(app, 2, emptyList()))
    }

    @Test
    fun withNoneConnectedItWaitsForThem() {
        assertEquals("It will arrive when HP-AIO is back", Sender.resultText(app, 0, listOf("HP-AIO")))
        assertEquals("It will arrive when your computers are back", Sender.resultText(app, 0, listOf("HP-AIO", "Arch-PC")))
    }

    @Test
    fun withNonePairedThereIsNothingToWaitFor() {
        assertEquals(app.getString(R.string.notification_no_computer), Sender.resultText(app, 0, emptyList()))
    }

    @Test
    @Config(qualifiers = "fr")
    fun inFrench() {
        assertEquals("La copie arrivera dès que HP-AIO sera de retour", Sender.resultText(app, 0, listOf("HP-AIO")))
    }
}
