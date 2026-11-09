// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isSelectable
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.navigation3.runtime.NavKey
import io.github.nexoryx.pasteget.ActivityKind
import io.github.nexoryx.pasteget.ActivityLog
import io.github.nexoryx.pasteget.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** The tabs, the gear and the back arrow, in the whole app drawn from a fixture. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ShellTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()
    private val stack = mutableStateListOf<NavKey>(Route.Home)

    private fun show(state: AppState = AppFixtures.state(computers = listOf(AppFixtures.hp), connected = setOf("a1"))) {
        compose.setContent { MaterialTheme { PasteGetUi(state, AppActions(), stack) } }
    }

    private fun tab(label: Int) = compose.onNode(hasText(app.getString(label)) and isSelectable())

    @Test
    fun eachTabShowsItsScreen() {
        show()
        tab(R.string.tab_home).assertIsSelected()

        tab(R.string.tab_history).performClick()
        compose.onNodeWithText(app.getString(R.string.history_banner)).assertIsDisplayed()
        tab(R.string.tab_history).assertIsSelected()

        tab(R.string.tab_devices).performClick()
        compose.onNodeWithText(app.getString(R.string.this_phone_title)).performScrollTo().assertIsDisplayed()
        assertEquals(listOf(Route.Home, Route.Devices), stack)

        tab(R.string.tab_home).performClick()
        compose.onNodeWithText(app.getString(R.string.hero_connected, "HP-AIO")).assertIsDisplayed()
        assertEquals(listOf<NavKey>(Route.Home), stack)
    }

    @Test
    fun theGearOpensSettingsAndBackReturns() {
        show()
        tab(R.string.tab_devices).performClick()

        compose.onNodeWithContentDescription(app.getString(R.string.settings_title)).performClick()
        compose.onNodeWithText(app.getString(R.string.settings_appearance)).assertIsDisplayed()
        // Still under the Devices tab.
        tab(R.string.tab_devices).assertIsSelected()

        compose.onNodeWithContentDescription(app.getString(R.string.back)).performClick()
        compose.onNodeWithText(app.getString(R.string.this_phone_title)).performScrollTo().assertIsDisplayed()
        assertEquals(listOf(Route.Home, Route.Devices), stack)
    }

    @Test
    fun aMessageShowsAsASnackbarThenGoes() {
        val shown = mutableListOf<String>()
        compose.setContent {
            MaterialTheme { PasteGetUi(AppFixtures.state(message = "Paired with HP-AIO"), AppActions(onMessageShown = { shown += it }), stack) }
        }

        compose.onNodeWithText("Paired with HP-AIO").assertIsDisplayed()
        // Read out, without moving the focus.
        val polite = SemanticsMatcher.expectValue(SemanticsProperties.LiveRegion, LiveRegionMode.Polite)
        compose.onNode(polite and hasAnyDescendant(hasText("Paired with HP-AIO"))).assertExists()
        compose.mainClock.advanceTimeBy(10_000)
        assertEquals(listOf("Paired with HP-AIO"), shown)
    }

    @Test
    fun seeAllOpensTheHistory() {
        val log = ActivityLog()
        log.sent(ActivityKind.TEXT, 12, sentTo = 1, automatic = true)
        show(AppFixtures.state(computers = listOf(AppFixtures.hp), activity = log.entries.value))

        compose.onNodeWithText(app.getString(R.string.see_all)).performScrollTo().performClick()
        compose.onNodeWithText(app.getString(R.string.history_banner)).assertIsDisplayed()
        tab(R.string.tab_history).assertIsSelected()
    }
}
