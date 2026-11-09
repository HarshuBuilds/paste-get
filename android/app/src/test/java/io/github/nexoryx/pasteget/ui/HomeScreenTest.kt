// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import android.icu.text.ListFormatter
import android.text.format.Formatter
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
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

/** The home screen and its status card in each state, drawn from a plain state: no app, no native library. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class HomeScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    private fun show(state: AppState, actions: AppActions = AppActions()) {
        compose.setContent { MaterialTheme { HomeScreen(state, actions) } }
    }

    @Test
    fun withNoComputerItAsksToPairOne() {
        var pairs = 0
        show(AppFixtures.state(), AppActions(onPair = { pairs++ }))

        compose.onNodeWithText(app.getString(R.string.hero_pair_title)).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.hero_scan)).performClick()
        assertEquals(1, pairs)
        // Nobody to send to.
        compose.onNodeWithText(app.getString(R.string.send_clipboard_button)).assertDoesNotExist()
    }

    @Test
    fun aComputerThatUnpairedThePhoneIsNamed() {
        var pairs = 0
        show(AppFixtures.state(unpaired = AppFixtures.hp), AppActions(onPair = { pairs++ }))

        compose.onNodeWithText(app.getString(R.string.unpaired_title, "HP-AIO")).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.pair_again)).performClick()
        assertEquals(1, pairs)
    }

    @Test
    fun whileLookingTheClipboardCanStillBeSent() {
        var sends = 0
        var retries = 0
        show(AppFixtures.state(computers = listOf(AppFixtures.hp)), AppActions(onSendClipboard = { sends++ }, onRefresh = { retries++ }))

        compose.onNodeWithText(app.getString(R.string.hero_looking, "HP-AIO")).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.send_clipboard_button)).assertIsEnabled().performClick()
        assertEquals(1, sends)
        compose.onNodeWithText(app.getString(R.string.hero_looking_long)).assertDoesNotExist()

        // After half a minute: what usually keeps them apart, and a way to try again.
        compose.mainClock.advanceTimeBy(31_000)
        compose.onNodeWithText(app.getString(R.string.hero_looking_long)).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.hero_try_again)).performClick()
        assertEquals(1, retries)
    }

    @Test
    fun aBlockedFeatureGetsItsNextStep() {
        val opened = mutableListOf<Route>()
        compose.setContent {
            MaterialTheme {
                HomeScreen(
                    AppFixtures.state(computers = listOf(AppFixtures.hp), connected = setOf("a1"), copiesOn = true),
                    AppActions(),
                    onOpen = { opened += it },
                )
            }
        }

        compose.onNodeWithText(app.getString(R.string.hero_connected, "HP-AIO")).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.hero_step_copies)).assertIsDisplayed()
        // The set-up has its own screen.
        compose.onNodeWithText(app.getString(R.string.setup_start)).performClick()
        assertEquals(listOf<Route>(Route.CopiesSetUp), opened)
    }

    @Test
    fun whenItWorksTheOfflineComputersAreNamed() {
        var sends = 0
        show(
            AppFixtures.state(computers = listOf(AppFixtures.hp, AppFixtures.arch), connected = setOf("a1")),
            AppActions(onSendClipboard = { sends++ }),
        )

        compose.onNodeWithText(app.getString(R.string.hero_connected, "HP-AIO")).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.hero_working_manual)).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.hero_offline, "Arch-PC")).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.send_clipboard_button)).performClick()
        assertEquals(1, sends)
    }

    @Test
    fun severalConnectedComputersAreListed() {
        show(AppFixtures.state(computers = listOf(AppFixtures.hp, AppFixtures.arch), connected = setOf("a1", "b2")))

        val both = ListFormatter.getInstance().format(listOf("HP-AIO", "Arch-PC"))
        compose.onNodeWithText(app.getString(R.string.hero_connected, both)).assertIsDisplayed()
    }

    @Test
    fun recentSaysWhereEachClipWentAndHowFast() {
        val log = ActivityLog()
        log.sent(ActivityKind.SCREENSHOT, 1_200_000, sentTo = 2, automatic = true)
        log.delivered("a1", "image/png", 1_200_000, millis = 91, accepted = true)
        log.delivered("b2", "image/png", 1_200_000, millis = 140, accepted = false)
        log.sent(ActivityKind.TEXT, 12, sentTo = 0, automatic = false)
        log.received("text/plain", 30, sensitive = false, onClipboard = true)
        show(AppFixtures.state(computers = listOf(AppFixtures.hp, AppFixtures.arch), activity = log.entries.value))

        compose.onNodeWithText(app.getString(R.string.recent_title)).performScrollTo().assertIsDisplayed()
        val delivered = app.getString(R.string.activity_delivered, "HP-AIO", 91) + ", " +
            app.getString(R.string.activity_refused, "Arch-PC")
        compose.onNodeWithText(delivered).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.send_waits)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.activity_on_clipboard)).performScrollTo().assertIsDisplayed()
    }

    @Test
    fun onlyWhatTheComputersWillGetSaysItWaits() {
        val log = ActivityLog()
        // With the PC away: a text replaced by a newer screenshot, then a password and a code,
        // which the core never keeps for later.
        log.sent(ActivityKind.TEXT, 12, sentTo = 0, automatic = true)
        log.sent(ActivityKind.SCREENSHOT, 340_000, sentTo = 0, automatic = true)
        log.sent(ActivityKind.TEXT, 8, sentTo = 0, automatic = true, sensitive = true)
        log.sent(ActivityKind.CODE, 6, sentTo = 0, automatic = true, sensitive = true, app = "Messages")
        show(AppFixtures.state(computers = listOf(AppFixtures.hp), activity = log.entries.value))

        // The screenshot only.
        val screenshot = app.getString(R.string.activity_screenshot, Formatter.formatShortFileSize(app, 340_000))
        compose.onNode(hasText(screenshot) and hasText(app.getString(R.string.send_waits_for, "HP-AIO")))
            .performScrollTo().assertIsDisplayed()
        compose.onAllNodesWithText(app.getString(R.string.send_waits_for, "HP-AIO")).assertCountEquals(1)
        compose.onAllNodesWithText(app.getString(R.string.activity_not_sent)).assertCountEquals(3)
    }

    @Test
    fun withNothingSentThereIsNoRecentList() {
        show(AppFixtures.state(computers = listOf(AppFixtures.hp)))

        compose.onNodeWithText(app.getString(R.string.recent_title)).assertDoesNotExist()
    }
}
