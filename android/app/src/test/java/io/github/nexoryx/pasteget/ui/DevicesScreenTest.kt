// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeDown
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

/** Who is paired: the computers, this phone, and pairing another one. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class DevicesScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    private fun show(state: AppState, actions: AppActions = AppActions()) {
        compose.setContent { MaterialTheme { DevicesScreen(state, actions) } }
    }

    @Test
    fun pairedComputersAreListedWithTheirConnection() {
        show(AppFixtures.state(computers = listOf(AppFixtures.hp, AppFixtures.arch), connected = setOf("a1")))

        compose.onNodeWithText("HP-AIO").assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.connected)).assertIsDisplayed()
        compose.onNodeWithText("Arch-PC").assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.offline)).assertIsDisplayed()
    }

    @Test
    fun aConnectedComputerShowsItsLastLatency() {
        val log = ActivityLog()
        log.sent(ActivityKind.TEXT, 12, sentTo = 1, automatic = true)
        log.delivered("a1", "text/plain", 12, millis = 38, accepted = true)
        show(AppFixtures.state(computers = listOf(AppFixtures.hp), connected = setOf("a1"), activity = log.entries.value))

        compose.onNodeWithText(app.getString(R.string.device_connected_ms, 38)).assertIsDisplayed()
    }

    @Test
    fun pullingDownDialsAgain() {
        var refreshes = 0
        show(AppFixtures.state(computers = listOf(AppFixtures.hp)), AppActions(onRefresh = { refreshes++ }))

        compose.onNodeWithText("HP-AIO").performTouchInput { swipeDown(startY = top, endY = top + 1_000f) }
        compose.waitForIdle()
        assertEquals(1, refreshes)
    }

    @Test
    fun withNoneItSaysSo() {
        show(AppFixtures.state())

        compose.onNodeWithText(app.getString(R.string.notification_no_computer)).assertIsDisplayed()
    }

    @Test
    fun thisPhonesIdIsShownInGroups() {
        show(AppFixtures.state())

        compose.onNodeWithText("POCO M8 5G").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("3F2A 9C11 ABCD EFGH IJKL MNOP QRST UVWX YZ23 4567 ABCD EFGH IJKL").performScrollTo().assertIsDisplayed()
    }

    @Test
    fun theButtonPairsAnotherComputer() {
        var pairs = 0
        show(AppFixtures.state(), AppActions(onPair = { pairs++ }))

        // The button's label is in its unmerged tree.
        compose.onNodeWithText(app.getString(R.string.pair_button), useUnmergedTree = true).performClick()
        assertEquals(1, pairs)
    }

    @Test
    fun aComputerThatUnpairedThePhoneIsSaidSo() {
        var pairs = 0
        var closed = 0
        show(
            AppFixtures.state().copy(unpairedBy = AppFixtures.hp),
            AppActions(onPair = { pairs++ }, onDismissUnpaired = { closed++ }),
        )

        compose.onNodeWithText(app.getString(R.string.unpaired_title, "HP-AIO")).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.pair_again)).performClick()
        compose.onNodeWithText(app.getString(R.string.close)).performClick()
        assertEquals(1 to 1, pairs to closed)
    }
}
