// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.nexoryx.pasteget.PhotoAccess
import io.github.nexoryx.pasteget.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** What TalkBack gets: named switches, headings, and status changes read out. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class HomeAccessibilityTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    private val isSwitch = SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Switch)

    private fun show(state: HomeState, actions: HomeActions = HomeActions()) {
        compose.setContent { MaterialTheme { HomeScreen(state, actions) } }
    }

    @Test
    fun eachSwitchIsNamedByItsRow() {
        val changes = mutableListOf<Boolean>()
        show(
            HomeFixtures.state(screenshots = ScreenshotSetting(wanted = true, access = PhotoAccess.FULL)),
            HomeActions(onScreenshotsChange = { changes += it }, autoCopy = AutoCopyActions(onChange = { changes += it })),
        )

        val screenshots = compose.onNode(hasText(app.getString(R.string.screenshots_title)) and isToggleable())
        screenshots.performScrollTo().assert(isSwitch).assertIsOn()
        val copies = compose.onNode(hasText(app.getString(R.string.copies_title)) and isToggleable())
        copies.performScrollTo().assert(isSwitch).assertIsOff()
        compose.onNode(hasText(app.getString(R.string.codes_title)) and isToggleable()).performScrollTo().assert(isSwitch)

        // A tap anywhere on the row toggles it.
        screenshots.performClick()
        copies.performClick()
        assertEquals(listOf(false, true), changes)
    }

    @Test
    fun sectionTitlesAreHeadings() {
        show(HomeFixtures.state(computers = listOf(HomeFixtures.hp)))

        compose.onNodeWithText(app.getString(R.string.app_name)).assert(isHeading())
        compose.onNodeWithText(app.getString(R.string.paired_computers)).performScrollTo().assert(isHeading())
    }

    @Test
    fun theStatusAndTheMessageAreReadOutWhenTheyChange() {
        show(HomeFixtures.state(message = "Paired with HP-AIO"))

        val polite = SemanticsMatcher.expectValue(SemanticsProperties.LiveRegion, LiveRegionMode.Polite)
        compose.onNodeWithText(app.getString(R.string.status_no_computer)).assert(polite)
        compose.onNodeWithText("Paired with HP-AIO").assert(polite)
    }

    @Test
    fun forgetButtonsNameTheirComputer() {
        show(HomeFixtures.state(computers = listOf(HomeFixtures.hp, HomeFixtures.arch)))

        compose.onNodeWithContentDescription(app.getString(R.string.forget_named, "Arch-PC"))
            .performScrollTo()
            .assertHasClickAction()
            .performClick()
        // It asks about that one.
        compose.onNodeWithText(app.getString(R.string.forget_title, "Arch-PC")).assertExists()
    }
}
