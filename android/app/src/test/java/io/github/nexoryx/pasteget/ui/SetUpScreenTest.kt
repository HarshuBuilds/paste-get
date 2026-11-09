// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import io.github.nexoryx.pasteget.BuiltInState
import io.github.nexoryx.pasteget.BuiltInStatus
import io.github.nexoryx.pasteget.PairingFailure
import io.github.nexoryx.pasteget.PairingStep
import io.github.nexoryx.pasteget.R
import io.github.nexoryx.pasteget.SetUpChecks
import io.github.nexoryx.pasteget.Vendor
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** The built-in mode's set-up: its steps tick as the phone's settings change, and the code goes in. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class SetUpScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    private val nothingOn = SetUpChecks(wifi = false, developerOptions = false, wirelessDebugging = false)

    private fun copies(
        checks: SetUpChecks = SetUpChecks(),
        builtIn: BuiltInState = BuiltInState(BuiltInStatus.NOT_SET_UP),
        vendor: Vendor = Vendor.OTHER,
        splitScreen: Boolean = false,
        setUp: Boolean = false,
    ) = AppFixtures.state(copiesOn = true).autoCopy.copy(
        checks = checks,
        builtIn = builtIn,
        vendor = vendor,
        splitScreen = splitScreen,
        setUp = setUp,
    )

    private fun show(copies: CopiesState, actions: CopiesActions = CopiesActions(), onBack: () -> Unit = {}) {
        val state = AppFixtures.state(copiesOn = true).copy(autoCopy = copies)
        compose.setContent { MaterialTheme { SetUpScreen(state, AppActions(autoCopy = actions), onBack) } }
    }

    private fun text(id: Int, vararg args: Any) = compose.onNodeWithText(app.getString(id, *args))

    @Test
    fun theStepsTickAsTheSettingsChange() {
        var copies by mutableStateOf(copies(checks = nothingOn))
        var done = 0
        compose.setContent {
            MaterialTheme {
                SetUpScreen(AppFixtures.state(copiesOn = true).copy(autoCopy = copies), AppActions(), onBack = { done++ })
            }
        }

        text(R.string.setup_progress, 1, 4).assertIsDisplayed()
        text(R.string.setup_wifi_needed).performScrollTo().assertIsDisplayed()
        text(R.string.step_open_wifi).performScrollTo().assertIsDisplayed()

        copies = copies.copy(checks = nothingOn.copy(wifi = true))
        text(R.string.setup_progress, 2, 4).assertIsDisplayed()
        text(R.string.setup_open_about).performScrollTo().assertIsDisplayed()
        text(R.string.step_open_wifi).assertDoesNotExist()

        copies = copies.copy(checks = SetUpChecks(wifi = true, developerOptions = true, wirelessDebugging = false))
        text(R.string.setup_progress, 3, 4).assertIsDisplayed()
        text(R.string.setup_debugging_other).performScrollTo().assertIsDisplayed()

        copies = copies.copy(checks = SetUpChecks())
        text(R.string.setup_progress, 4, 4).assertIsDisplayed()
        text(R.string.setup_code_text).performScrollTo().assertIsDisplayed()
        text(R.string.setup_open_code).performScrollTo().assertIsDisplayed()

        copies = copies.copy(builtIn = BuiltInState(BuiltInStatus.STARTING), setUp = true)
        text(R.string.setup_all_done).assertIsDisplayed()
        text(R.string.setup_done_text).performScrollTo().assertIsDisplayed()
        text(R.string.setup_done_button).performScrollTo().performClick()
        assertEquals(1, done)
        // Nothing else to choose once set up.
        text(R.string.setup_shizuku_link).assertDoesNotExist()
    }

    @Test
    fun theCurrentStepIsAHeadingTalkBackCallsCurrent() {
        show(copies(checks = nothingOn.copy(wifi = true)))

        compose.onNode(hasText(app.getString(R.string.setup_step_developer)) and isHeading())
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, app.getString(R.string.setup_step_current)))
        compose.onNode(hasText(app.getString(R.string.setup_step_prepare)) and isHeading())
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, app.getString(R.string.step_done)))
        compose.onNode(hasText(app.getString(R.string.setup_step_code)) and isHeading())
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, app.getString(R.string.setup_step_later)))
    }

    @Test
    fun xiaomiPhonesGetTheirOwnWords() {
        show(copies(checks = nothingOn.copy(wifi = true), vendor = Vendor.XIAOMI))
        text(R.string.setup_developer_xiaomi).performScrollTo().assertIsDisplayed()
        text(R.string.setup_developer_other).assertDoesNotExist()
    }

    @Test
    fun xiaomiPhonesAreToldAboutTheNotificationStyle() {
        show(copies(checks = nothingOn, vendor = Vendor.XIAOMI))
        text(R.string.setup_xiaomi_notification_style).performScrollTo().assertIsDisplayed()
    }

    @Test
    fun otherPhonesSayBuildNumber() {
        show(copies(checks = nothingOn.copy(wifi = true)))
        text(R.string.setup_developer_other).performScrollTo().assertIsDisplayed()
        text(R.string.setup_xiaomi_notification_style).assertDoesNotExist()
    }

    @Test
    fun withoutNotificationsTheFirstStepLeadsToTheirSettings() {
        var opened = 0
        show(copies().copy(canNotify = false), CopiesActions(onOpenNotificationSettings = { opened++ }))

        text(R.string.setup_progress, 1, 4).assertIsDisplayed()
        text(R.string.builtin_needs_notifications).performScrollTo().assertIsDisplayed()
        text(R.string.builtin_allow_notifications).performScrollTo().performClick()
        assertEquals(1, opened)
        text(R.string.setup_open_code).assertDoesNotExist()
    }

    @Test
    fun theCodeStepShowsTheNotificationAndSettings() {
        var setUps = 0
        show(copies(), CopiesActions(onSetUpBuiltIn = { setUps++ }))

        text(R.string.setup_open_code).performScrollTo().performClick()
        assertEquals(1, setUps)
        // Without split screen, typing the code here isn't offered, only explained.
        compose.onNode(hasSetTextAction()).assertDoesNotExist()
        text(R.string.setup_use_split).performScrollTo().performClick()
        text(R.string.setup_split_how).performScrollTo().assertIsDisplayed()
    }

    @Test
    fun inSplitScreenTheCodeIsTypedHere() {
        val codes = mutableListOf<String>()
        show(
            copies(builtIn = BuiltInState(BuiltInStatus.PAIRING, step = PairingStep.WAITING_FOR_CODE), splitScreen = true),
            CopiesActions(onSubmitCode = { codes += it }),
        )

        text(R.string.setup_code_split).performScrollTo().assertIsDisplayed()
        val field = compose.onNode(hasSetTextAction() and hasContentDescription(app.getString(R.string.builtin_pair_hint)))
        field.performScrollTo().performTextInput("12a34")
        assertEquals(emptyList<String>(), codes)
        field.performTextInput("5678")
        // Sent once, at the sixth digit; the rest is ignored.
        assertEquals(listOf("123456"), codes)
    }

    @Test
    fun whilePairingEachStepShowsAndTheFieldWaits() {
        show(copies(builtIn = BuiltInState(BuiltInStatus.PAIRING, step = PairingStep.PAIRING), splitScreen = true))

        text(R.string.setup_pair_finding).performScrollTo().assertIsDisplayed()
        text(R.string.setup_pair_pairing).performScrollTo().assertIsDisplayed()
        text(R.string.setup_pair_granting).performScrollTo().assertIsDisplayed()
        compose.onNode(hasContentDescription(app.getString(R.string.builtin_pair_hint))).performScrollTo().assertIsNotEnabled()
        // Can't be stopped halfway.
        text(R.string.builtin_cancel).assertDoesNotExist()
    }

    @Test
    fun aCodeThatDidntWorkSaysWhyAndCanBeRetried() {
        var setUps = 0
        val failed = BuiltInState(BuiltInStatus.PAIRING, "wrong or expired code", PairingStep.WAITING_FOR_CODE, PairingFailure.WRONG_CODE)
        show(copies(builtIn = failed), CopiesActions(onSetUpBuiltIn = { setUps++ }))

        text(R.string.setup_failed_wrong_code).performScrollTo().assertIsDisplayed()
        text(R.string.builtin_retry).performScrollTo().performClick()
        assertEquals(1, setUps)
        text(R.string.details).performScrollTo().performClick()
        compose.onNodeWithText("wrong or expired code").performScrollTo().assertIsDisplayed()
    }

    @Test
    fun theOtherWaysWaitBehindTheirLinks() {
        var gets = 0
        show(copies(), CopiesActions(onGetShizuku = { gets++ }))

        text(R.string.setup_shizuku_text).assertDoesNotExist()
        text(R.string.setup_shizuku_link).performScrollTo().performClick()
        text(R.string.setup_shizuku_text).performScrollTo().assertIsDisplayed()
        text(R.string.shizuku_get).performScrollTo().performClick()
        assertEquals(1, gets)

        text(R.string.setup_manual_link).performScrollTo().performClick()
        text(R.string.setup_manual_text).performScrollTo().assertIsDisplayed()
        text(R.string.tile_add).performScrollTo().assertIsDisplayed()
    }
}
