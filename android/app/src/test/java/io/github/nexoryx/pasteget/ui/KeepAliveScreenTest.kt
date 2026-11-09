// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.nexoryx.pasteget.KeepAlive
import io.github.nexoryx.pasteget.R
import io.github.nexoryx.pasteget.Vendor
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** The keep-alive checklist: live checks for everyone, Xiaomi's own switches on Xiaomi phones only. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class KeepAliveScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    private fun show(state: KeepAliveState, actions: KeepAliveActions = KeepAliveActions()) {
        compose.setContent { MaterialTheme { KeepAliveScreen(state, actions, onBack = {}) } }
    }

    private fun text(id: Int, vararg args: Any) = compose.onNodeWithText(app.getString(id, *args))

    @Test
    fun batteryOptimisationHasItsButton() {
        var asked = 0
        show(KeepAliveState(KeepAlive(battery = false, notifications = true)), KeepAliveActions(onAllowBattery = { asked++ }))

        text(R.string.keepalive_progress, 1, 2).assertIsDisplayed()
        text(R.string.keepalive_battery_needed).performScrollTo().assertIsDisplayed()
        text(R.string.keepalive_allow).performScrollTo().performClick()
        assertEquals(1, asked)
        // Not a Xiaomi phone: nothing of Xiaomi's.
        text(R.string.keepalive_autostart).assertDoesNotExist()
        text(R.string.keepalive_recents).assertDoesNotExist()
    }

    @Test
    fun whenAllIsDoneThereIsNothingToPress() {
        show(KeepAliveState())

        text(R.string.keepalive_all_done).assertIsDisplayed()
        text(R.string.keepalive_allow).assertDoesNotExist()
        text(R.string.builtin_allow_notifications).assertDoesNotExist()
    }

    @Test
    fun xiaomiPhonesGetTheirSwitchesTickedByHand() {
        var opened = 0
        val ticks = mutableListOf<Boolean>()
        show(
            KeepAliveState(vendor = Vendor.XIAOMI),
            KeepAliveActions(onOpenAutostart = { opened++ }, onTickAutostart = { ticks += it }),
        )

        text(R.string.keepalive_progress, 2, 4).assertIsDisplayed()
        text(R.string.keepalive_open_autostart).performScrollTo().performClick()
        assertEquals(1, opened)
        // Named after its item, since both ticks say "It's done".
        val tick = compose.onNode(hasContentDescription(app.getString(R.string.keepalive_autostart)) and isToggleable())
        tick.assert(SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Checkbox)).assertIsOff()
        tick.assert(hasText(app.getString(R.string.keepalive_ticked)))
        tick.performScrollTo().performClick()
        assertEquals(listOf(true), ticks)
        text(R.string.keepalive_recents_text).performScrollTo().assertIsDisplayed()
    }

    @Test
    fun aTickedSwitchCountsAndLosesItsButton() {
        show(KeepAliveState(vendor = Vendor.XIAOMI, autostartTicked = true, batterySaverTicked = true))

        text(R.string.keepalive_all_done).assertIsDisplayed()
        text(R.string.keepalive_open_autostart).assertDoesNotExist()
    }

    @Test
    fun settingsSaysHowFarItIsAndOpensIt() {
        var opened = 0
        compose.setContent {
            MaterialTheme {
                SettingsScreen(
                    SettingsState(),
                    SettingsActions(),
                    onBack = {},
                    onOpenLicences = {},
                    keepAlive = KeepAliveState(KeepAlive(battery = false, notifications = false)),
                    onOpenKeepAlive = { opened++ },
                )
            }
        }

        text(R.string.keepalive_progress, 0, 2).performClick()
        assertEquals(1, opened)
    }
}
