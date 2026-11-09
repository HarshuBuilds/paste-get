// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
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

/** Forgetting a computer, on its page, asks first, and Cancel keeps it. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ForgetTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()
    private val forgotten = mutableListOf<String>()
    private var backs = 0

    /** HP-AIO's page, as Devices opens it. */
    private fun showAndTapForget() {
        compose.setContent {
            MaterialTheme {
                DeviceScreen(
                    AppFixtures.state(computers = listOf(AppFixtures.hp, AppFixtures.arch), connected = setOf("a1")),
                    "a1",
                    AppActions(onForget = { forgotten += it.deviceId }),
                    onBack = { backs++ },
                )
            }
        }
        compose.onNodeWithText(app.getString(R.string.forget_this)).performScrollTo().performClick()
    }

    @Test
    fun forgetAsksFirst() {
        showAndTapForget()

        compose.onNodeWithText(app.getString(R.string.forget_title, "HP-AIO")).assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.forget_text, "HP-AIO")).assertIsDisplayed()
        assertEquals(emptyList<String>(), forgotten)
    }

    @Test
    fun cancelKeepsTheComputer() {
        showAndTapForget()

        compose.onNodeWithText(app.getString(R.string.cancel)).performClick()
        compose.onNodeWithText(app.getString(R.string.forget_title, "HP-AIO")).assertDoesNotExist()
        assertEquals(emptyList<String>(), forgotten)
    }

    @Test
    fun confirmingForgetsThatComputerOnly() {
        showAndTapForget()

        compose.onNodeWithText(app.getString(R.string.forget_confirm)).performClick()
        compose.onNodeWithText(app.getString(R.string.forget_title, "HP-AIO")).assertDoesNotExist()
        assertEquals(listOf("a1"), forgotten)
    }

    @Test
    fun theDetailsShowTheConnectionAndAddresses() {
        compose.setContent {
            MaterialTheme {
                DeviceScreen(AppFixtures.state(computers = listOf(AppFixtures.hp), connected = setOf("a1")), "a1", AppActions(), onBack = {})
            }
        }
        compose.onNodeWithText(app.getString(R.string.connected)).assertIsDisplayed()
        compose.onNodeWithText("192.168.1.20:47290").assertIsDisplayed()
    }

    @Test
    fun aComputerNoLongerPairedClosesItsPage() {
        compose.setContent {
            MaterialTheme { DeviceScreen(AppFixtures.state(computers = listOf(AppFixtures.arch)), "a1", AppActions(), onBack = { backs++ }) }
        }
        compose.waitForIdle()
        assertEquals(1, backs)
    }
}
