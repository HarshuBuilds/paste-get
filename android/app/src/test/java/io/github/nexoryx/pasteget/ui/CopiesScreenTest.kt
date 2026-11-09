// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.navigation3.runtime.NavKey
import io.github.nexoryx.pasteget.BuiltInState
import io.github.nexoryx.pasteget.BuiltInStatus
import io.github.nexoryx.pasteget.CodeAccess
import io.github.nexoryx.pasteget.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** The Copies screen, and how the home screen's row leads to it. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class CopiesScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()
    private val stack = mutableStateListOf<NavKey>(Route.Home)

    private fun showApp(state: AppState, actions: AppActions) {
        compose.setContent { MaterialTheme { PasteGetUi(state, actions, stack) } }
    }

    private fun copiesSwitch() =
        compose.onNode(hasContentDescription(app.getString(R.string.copies_title)) and isToggleable())

    @Test
    fun turningCopiesOnBeforeSetUpOpensTheSetUp() {
        val changes = mutableListOf<Boolean>()
        showApp(AppFixtures.state(computers = listOf(AppFixtures.hp)), AppActions(autoCopy = CopiesActions(onChange = { changes += it })))

        copiesSwitch().performScrollTo().performClick()

        assertEquals(listOf(true), changes)
        // Back from the set-up leads to the copies' screen.
        assertEquals(listOf(Route.Home, Route.Copies, Route.CopiesSetUp), stack)
        compose.onNodeWithText(app.getString(R.string.setup_intro)).assertIsDisplayed()
    }

    @Test
    fun theCodesShellStepOpensTheSetUpWithCopiesOn() {
        val changes = mutableListOf<Boolean>()
        stack.add(Route.Codes)
        val state = AppFixtures.state(computers = listOf(AppFixtures.hp), codesOn = true).let {
            it.copy(codes = it.codes.copy(access = CodeAccess(listening = true, seesCodes = false)))
        }
        showApp(state, AppActions(autoCopy = CopiesActions(onChange = { changes += it })))

        compose.onNodeWithText(app.getString(R.string.codes_set_up_shell)).performScrollTo().performClick()

        assertEquals(listOf(true), changes)
        assertEquals(listOf(Route.Home, Route.Codes, Route.Copies, Route.CopiesSetUp), stack)
    }

    @Test
    fun onceSetUpTheSwitchJustTurnsThemOn() {
        val changes = mutableListOf<Boolean>()
        showApp(
            AppFixtures.state(computers = listOf(AppFixtures.hp), copiesSetUp = true),
            AppActions(autoCopy = CopiesActions(onChange = { changes += it })),
        )

        copiesSwitch().performScrollTo().performClick()

        assertEquals(listOf(true), changes)
        assertEquals(listOf<NavKey>(Route.Home), stack)
    }

    @Test
    fun offItSaysWhatTurningItOnDoes() {
        compose.setContent { MaterialTheme { CopiesScreen(AppFixtures.state().autoCopy, CopiesActions(), onBack = {}) } }

        compose.onNodeWithText(app.getString(R.string.copies_explain)).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.methods_title)).assertDoesNotExist()
    }

    @Test
    fun theMethodInUseIsChecked() {
        val state = AppFixtures.state(copiesOn = true).autoCopy.copy(builtIn = BuiltInState(BuiltInStatus.WATCHING))
        compose.setContent { MaterialTheme { CopiesScreen(state, CopiesActions(), onBack = {}) } }

        compose.onNodeWithText(app.getString(R.string.feature_on_automatic)).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.method_builtin)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithContentDescription(app.getString(R.string.method_in_use)).performScrollTo().assertIsDisplayed()
        // Nothing to do: no set-up offered, no "by hand meanwhile".
        compose.onNodeWithText(app.getString(R.string.setup_start)).assertDoesNotExist()
        compose.onNodeWithText(app.getString(R.string.mode_manual)).assertDoesNotExist()
    }

    @Test
    fun theDetailsShowEachModesState() {
        val state = AppFixtures.state(copiesOn = true).autoCopy.copy(builtIn = BuiltInState(BuiltInStatus.FAILED, "the shell helper keeps stopping"))
        compose.setContent { MaterialTheme { CopiesScreen(state, CopiesActions(), onBack = {}) } }

        compose.onNodeWithText(app.getString(R.string.details)).performScrollTo().performClick()
        compose.onNodeWithText("built-in: FAILED (the shell helper keeps stopping)", substring = true).performScrollTo().assertIsDisplayed()
    }
}
