// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.nexoryx.pasteget.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** The home screen, drawn from a plain state: no app, no native library. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class HomeScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    private fun show(state: HomeState, actions: HomeActions = HomeActions()) {
        compose.setContent { MaterialTheme { HomeScreen(state, actions) } }
    }

    @Test
    fun withNoComputerItAsksToPairOne() {
        var pairs = 0
        show(HomeFixtures.state(), HomeActions(onPair = { pairs++ }))

        compose.onNodeWithText(app.getString(R.string.status_no_computer)).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.pair_button)).performClick()
        assertEquals(1, pairs)
        compose.onNodeWithText(app.getString(R.string.paired_computers)).assertDoesNotExist()
    }

    @Test
    fun pairedComputersAreListedWithTheirConnection() {
        show(HomeFixtures.state(computers = listOf(HomeFixtures.hp, HomeFixtures.arch), connected = setOf("a1")))

        compose.onNodeWithText(app.resources.getQuantityString(R.plurals.notification_connected, 1, 1)).assertIsDisplayed()
        compose.onNodeWithText("HP-AIO").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Arch-PC").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.offline)).performScrollTo().assertIsDisplayed()
    }

    @Test
    fun theMessageIsShown() {
        show(HomeFixtures.state(message = "Paired with HP-AIO"))

        compose.onNodeWithText("Paired with HP-AIO").assertIsDisplayed()
    }
}
