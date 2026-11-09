// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.nexoryx.pasteget.BuiltInStatus
import io.github.nexoryx.pasteget.R
import io.github.nexoryx.pasteget.ShizukuState
import io.github.nexoryx.pasteget.ShizukuStatus
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** The Copies screen leads to the built-in mode's set-up, one button at a time. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BuiltInSetUpCardTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    private fun show(state: CopiesState, actions: CopiesActions = CopiesActions(), onOpenSetUp: () -> Unit = {}) {
        compose.setContent { MaterialTheme { CopiesScreen(state, actions, onBack = {}, onOpenSetUp = onOpenSetUp) } }
    }

    @Test
    fun beforeSetUpItOffersTheSetUpAlone() {
        var opened = 0
        show(AppFixtures.state(copiesOn = true).autoCopy, onOpenSetUp = { opened++ })

        compose.onNodeWithText(app.getString(R.string.builtin_offer)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.setup_start)).performScrollTo().performClick()
        assertEquals(1, opened)
        // Shizuku isn't installed: it waits behind the set-up's link, not here.
        compose.onNodeWithText(app.getString(R.string.shizuku_get)).assertDoesNotExist()
    }

    @Test
    fun anInstalledShizukuStillGetsItsStep() {
        var allowed = 0
        show(
            AppFixtures.state(copiesOn = true).autoCopy.copy(shizuku = ShizukuState(ShizukuStatus.NO_PERMISSION)),
            CopiesActions(onAllowShizuku = { allowed++ }),
        )

        compose.onNodeWithText(app.getString(R.string.shizuku_allow)).performScrollTo().performClick()
        assertEquals(1, allowed)
    }

    @Test
    fun whileWaitingForTheCodeTheSetUpCanGoOnOrStop() {
        var opened = 0
        var cancelled = 0
        show(
            AppFixtures.state(copiesOn = true, builtInStatus = BuiltInStatus.PAIRING).autoCopy,
            CopiesActions(onResetBuiltIn = { cancelled++ }),
            onOpenSetUp = { opened++ },
        )

        compose.onNodeWithText(app.getString(R.string.builtin_pairing)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.setup_continue)).performScrollTo().performClick()
        compose.onNodeWithText(app.getString(R.string.builtin_cancel)).performScrollTo().performClick()
        assertEquals(1 to 1, opened to cancelled)
    }
}
