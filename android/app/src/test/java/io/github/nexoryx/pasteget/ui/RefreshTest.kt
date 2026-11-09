// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import android.os.Looper
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.time.Duration

/** Pull to refresh really dials again, once at a time, and the spinner goes away. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class RefreshTest {
    private val app: Application = RuntimeEnvironment.getApplication()

    @Test
    fun aPullDialsTheComputersAgain() {
        val fake = FakeComputers(listOf(AppFixtures.hp))
        val model = AppViewModel(app, fake, Dispatchers.Unconfined)
        shadowOf(Looper.getMainLooper()).idle()

        model.refreshConnections()
        assertTrue(model.refreshing)
        // A second pull while the first runs does nothing more.
        model.refreshConnections()
        assertEquals(1, fake.retries)

        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(2))
        assertFalse(model.refreshing)
        assertEquals(listOf(AppFixtures.hp), model.paired)
    }
}
