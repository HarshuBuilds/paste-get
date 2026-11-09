// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import io.github.nexoryx.pasteget.R
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.pasteget_ffi.PastegetException
import kotlin.coroutines.CoroutineContext

/** A code from a link or a scan never pairs before the user confirms it. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PairingFlowTest {
    private val app: Application = RuntimeEnvironment.getApplication()
    private val fake = FakeComputers()

    private fun model(io: CoroutineDispatcher = Dispatchers.Unconfined) = AppViewModel(app, fake, io).also { it.refresh() }

    @Test
    fun aLinkWaitsForConfirmation() {
        val model = model()

        model.offerPairing(LINK)

        val confirm = model.pairing as PairingState.Confirm
        assertEquals("HP-AIO", confirm.invite.name)
        assertEquals(listOf("192.168.1.20:47290"), confirm.invite.addresses)
        assertEquals(emptyList<String>(), fake.pairCalls)

        model.confirmPairing()
        assertEquals(listOf(LINK), fake.pairCalls)
        assertEquals(PairingState.Idle, model.pairing)
        assertEquals(app.getString(R.string.paired_with, "HP-AIO"), model.message)
        assertEquals(listOf(AppFixtures.hp), model.paired)
        // Nothing left to confirm: a second yes does nothing.
        model.confirmPairing()
        assertEquals(1, fake.pairCalls.size)
    }

    @Test
    fun cancelNeverPairs() {
        val model = model()

        model.offerPairing(LINK)
        model.cancelPairing()

        assertEquals(PairingState.Idle, model.pairing)
        model.confirmPairing()
        assertEquals(emptyList<String>(), fake.pairCalls)
    }

    @Test
    fun whatIsntACodeIsSaidSo() {
        val model = model()

        model.offerPairing("https://example.com/")
        assertEquals(app.getString(R.string.not_a_pairing_code), model.message)
        assertEquals(PairingState.Idle, model.pairing)

        model.offerPairing("pasteget://pair?v=0&a=nothing-else")
        assertEquals(R.string.pair_error_invalid, (model.pairing as PairingState.Failed).error.message)
        assertEquals(emptyList<String>(), fake.pairCalls)
    }

    @Test
    fun aFailureIsExplainedAndCanBeClosed() {
        fake.failure = PastegetException.Unreachable("192.168.1.20:47290 did not answer")
        val model = model()

        model.offerPairing(LINK)
        model.confirmPairing()

        val error = (model.pairing as PairingState.Failed).error
        assertEquals(R.string.pair_error_unreachable, error.message)
        assertEquals("192.168.1.20:47290 did not answer", error.details)
        model.cancelPairing()
        assertEquals(PairingState.Idle, model.pairing)
    }

    @Test
    fun oneAtATime() {
        val io = QueueDispatcher()
        val model = model(io)
        io.runAll()

        model.offerPairing(LINK)
        model.confirmPairing()
        assertEquals(PairingState.Busy, model.pairing)
        // Another link meanwhile is ignored, and cancelling doesn't stop what runs.
        model.offerPairing(LINK.replace("HP-AIO", "Other"))
        model.cancelPairing()
        assertEquals(PairingState.Busy, model.pairing)

        io.runAll()
        assertEquals(listOf(LINK), fake.pairCalls)
        assertEquals(PairingState.Idle, model.pairing)
    }

    @Test
    fun theSecretStaysOutOfLogs() {
        val model = model()
        model.offerPairing(LINK)
        assertTrue("SECRETSECRET" !in model.pairing.toString())
    }

    /** Runs what's sent to it only when asked, to see the state in between. */
    private class QueueDispatcher : CoroutineDispatcher() {
        private val queue = ArrayDeque<Runnable>()

        override fun dispatch(context: CoroutineContext, block: Runnable) {
            queue.addLast(block)
        }

        fun runAll() {
            while (queue.isNotEmpty()) queue.removeFirst().run()
        }
    }

    private companion object {
        const val LINK = "pasteget://pair?v=0&id=a1&s=SECRETSECRET&a=192.168.1.20%3A47290&n=HP-AIO"
    }
}
