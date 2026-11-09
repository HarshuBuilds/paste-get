// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.os.Looper
import io.github.nexoryx.pasteget.PasteGetApp
import io.github.nexoryx.pasteget.R
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** "<PC> unpaired this phone" is said once, not again by every new screen. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class UnpairedNoticeTest {
    private val app = RuntimeEnvironment.getApplication() as PasteGetApp
    private val hp = AppFixtures.hp
    private val arch = AppFixtures.arch

    private fun model(computers: FakeComputers) = AppViewModel(app, computers, Dispatchers.Unconfined)

    @Test
    fun theNoticeIsShownOnceThenCleared() {
        app.unpairedBy(hp)

        val first = model(FakeComputers(listOf(arch)))
        assertEquals(app.getString(R.string.unpaired_title, "HP-AIO"), first.message)
        assertNull(app.unpaired.value)

        // The activity recreated, or opened again later: nothing to say any more.
        val second = model(FakeComputers(listOf(arch)))
        assertNull(second.message)
        assertNull(second.unpaired)
    }

    @Test
    fun withNoComputerLeftTheStatusLineSaysItInstead() {
        app.unpairedBy(hp)

        val model = model(FakeComputers(emptyList()))

        assertNull(model.message)
        assertEquals(AppStatus.Unpaired("HP-AIO"), appStatus(model.paired.orEmpty(), emptySet(), model.unpaired, emptyList()))
    }

    @Test
    fun anUnpairingWhileTheScreenIsOpenIsShown() {
        val model = model(FakeComputers(listOf(arch)))
        assertNull(model.message)

        app.unpairedBy(hp)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(app.getString(R.string.unpaired_title, "HP-AIO"), model.message)
        assertNull(app.unpaired.value)
    }

    @Test
    fun pairingAgainEndsTheUnpairedState() {
        app.unpairedBy(hp)
        val model = model(FakeComputers(emptyList()))

        model.offerPairing("pasteget://pair?v=0&id=a1&s=AB&a=192.168.1.20%3A47290&n=HP-AIO")
        model.confirmPairing(model.pairing as PairingState.Confirm)

        assertNull(model.unpaired)
    }

    @Test
    fun aNewerUnpairingIsntCleared() {
        app.unpairedBy(hp)
        app.unpairedBy(arch)

        app.clearUnpaired(hp)

        assertEquals(arch, app.unpaired.value)
    }
}
