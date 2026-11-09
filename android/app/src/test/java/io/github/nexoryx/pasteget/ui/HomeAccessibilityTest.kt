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
import androidx.compose.ui.test.junit4.v2.createComposeRule
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

    private fun show(state: AppState, actions: AppActions = AppActions()) {
        compose.setContent { MaterialTheme { HomeScreen(state, actions) } }
    }

    @Test
    fun eachSwitchIsNamedByItsRow() {
        val changes = mutableListOf<Boolean>()
        show(
            AppFixtures.state(screenshots = ScreenshotSetting(wanted = true, access = PhotoAccess.FULL)),
            AppActions(onScreenshotsChange = { changes += it }, autoCopy = AutoCopyActions(onChange = { changes += it })),
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
        show(AppFixtures.state(computers = listOf(AppFixtures.hp)))
        compose.onNodeWithText(app.getString(R.string.app_name)).assert(isHeading())
    }

    @Test
    fun theDevicesSectionsAreHeadings() {
        compose.setContent { MaterialTheme { DevicesScreen(AppFixtures.state(computers = listOf(AppFixtures.hp)), AppActions()) } }

        compose.onNodeWithText(app.getString(R.string.tab_devices)).assert(isHeading())
        compose.onNodeWithText(app.getString(R.string.paired_computers)).performScrollTo().assert(isHeading())
        compose.onNodeWithText(app.getString(R.string.this_phone_title)).performScrollTo().assert(isHeading())
    }

    @Test
    fun theStatusIsReadOutWhenItChanges() {
        show(AppFixtures.state())

        val polite = SemanticsMatcher.expectValue(SemanticsProperties.LiveRegion, LiveRegionMode.Polite)
        // Title and text as one announcement.
        compose.onNode(hasText(app.getString(R.string.hero_pair_title)) and hasText(app.getString(R.string.hero_pair_text)))
            .assert(polite)
    }

    @Test
    fun eachComputerReadsAsOneButton() {
        val opened = mutableListOf<String>()
        compose.setContent {
            MaterialTheme {
                DevicesScreen(AppFixtures.state(computers = listOf(AppFixtures.hp, AppFixtures.arch)), AppActions(), onOpen = { opened += it })
            }
        }

        // Name and state in one node, which opens that computer's page.
        compose.onNode(hasText("Arch-PC") and hasText(app.getString(R.string.offline)))
            .performScrollTo()
            .assertHasClickAction()
            .performClick()
        assertEquals(listOf("b2"), opened)
    }
}
