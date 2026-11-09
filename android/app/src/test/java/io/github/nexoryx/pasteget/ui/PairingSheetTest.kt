// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.navigation3.runtime.NavKey
import io.github.nexoryx.pasteget.PairingCode
import io.github.nexoryx.pasteget.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** The confirmation sheet, the progress while pairing, and the error with its details. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PairingSheetTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()
    private val confirm = PairingState.Confirm(
        "pasteget://pair?v=0&id=a1&s=AB&a=192.168.1.20%3A47290%2C10.0.0.5%3A47290&n=HP-AIO",
        PairingCode.Invite("a1", "HP-AIO", listOf("192.168.1.20:47290", "10.0.0.5:47290")),
    )

    /** The whole app: the sheet shows over any screen. */
    private fun show(state: AppState, actions: AppActions = AppActions()) {
        val stack = mutableStateListOf<NavKey>(Route.Home)
        compose.setContent { MaterialTheme { PasteGetUi(state, actions, stack) } }
    }

    @Test
    fun theSheetShowsWhatTheCodeSaysAndTheWarning() {
        show(AppFixtures.state().copy(pairing = confirm))

        compose.onNodeWithText(app.getString(R.string.pair_confirm_title)).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.pair_confirm_name, "HP-AIO")).assertIsDisplayed()
        compose.onNodeWithText(
            app.resources.getQuantityString(R.plurals.pair_confirm_addresses, 2, "192.168.1.20:47290, 10.0.0.5:47290"),
        ).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.pair_confirm_warning)).performScrollTo().assertIsDisplayed()
        // A new computer: nothing about pairing again.
        compose.onNodeWithText(app.getString(R.string.pair_confirm_again, "HP-AIO")).assertDoesNotExist()
    }

    @Test
    fun pairOnlyWhenTheUserSaysSo() {
        var confirmed = 0
        var cancelled = 0
        show(AppFixtures.state().copy(pairing = confirm), AppActions(onConfirmPairing = { confirmed++ }, onCancelPairing = { cancelled++ }))

        compose.onNodeWithText(app.getString(R.string.cancel)).performScrollTo().performClick()
        assertEquals(0 to 1, confirmed to cancelled)
        compose.onNodeWithText(app.getString(R.string.pair_confirm_button)).performScrollTo().performClick()
        compose.waitForIdle()
        assertEquals(1 to 1, confirmed to cancelled)
    }

    @Test
    fun aKnownComputerIsNamedAsThePhoneKnowsIt() {
        val known = AppFixtures.hp.copy(name = "Bureau")
        show(AppFixtures.state(computers = listOf(known)).copy(pairing = confirm))

        compose.onNodeWithText(app.getString(R.string.pair_confirm_again, "Bureau")).performScrollTo().assertIsDisplayed()
    }

    @Test
    fun whilePairingThePairButtonWaits() {
        show(AppFixtures.state().copy(pairing = PairingState.Busy))

        compose.onNodeWithText(app.getString(R.string.hero_scan)).assertIsNotEnabled()
        compose.onNodeWithText(app.getString(R.string.pairing)).assertIsDisplayed()
    }

    @Test
    fun anErrorSaysWhatToDoAndHasDetails() {
        var closed = 0
        val error = PairingError(R.string.pair_error_unreachable, "192.168.1.20:47290 did not answer")
        show(AppFixtures.state().copy(pairing = PairingState.Failed(error)), AppActions(onCancelPairing = { closed++ }))

        compose.onNodeWithText(app.getString(R.string.pair_error_unreachable)).assertIsDisplayed()
        compose.onNodeWithText("192.168.1.20:47290 did not answer").assertDoesNotExist()
        compose.onNodeWithText(app.getString(R.string.details)).performClick()
        compose.onNodeWithText("192.168.1.20:47290 did not answer").assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.close)).performClick()
        assertEquals(1, closed)
    }
}
