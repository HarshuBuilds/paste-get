// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.nexoryx.pasteget.BuiltInStatus
import io.github.nexoryx.pasteget.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** Without notifications, the Copies screen says why set-up can't start and leads to the setting. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BuiltInSetUpCardTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    private fun show(state: AppState, actions: AppActions) {
        compose.setContent { MaterialTheme { CopiesScreen(state.autoCopy, actions.autoCopy, onBack = {}) } }
    }

    @Test
    fun withNotificationsTheSetUpIsOffered() {
        var setUps = 0
        show(AppFixtures.state(copiesOn = true), AppActions(autoCopy = CopiesActions(onSetUpBuiltIn = { setUps++ })))

        compose.onNodeWithText(app.getString(R.string.builtin_set_up)).performScrollTo().performClick()
        assertEquals(1, setUps)
        compose.onNodeWithText(app.getString(R.string.builtin_needs_notifications)).assertDoesNotExist()
    }

    @Test
    fun withoutThemItSaysWhyAndOpensTheirSettings() {
        var setUps = 0
        var opened = 0
        show(
            AppFixtures.state(copiesOn = true, canNotify = false),
            AppActions(autoCopy = CopiesActions(onSetUpBuiltIn = { setUps++ }, onOpenNotificationSettings = { opened++ })),
        )

        compose.onNodeWithText(app.getString(R.string.builtin_needs_notifications)).performScrollTo()
        compose.onNodeWithText(app.getString(R.string.builtin_set_up)).assertDoesNotExist()
        compose.onNodeWithText(app.getString(R.string.builtin_allow_notifications)).performScrollTo().performClick()
        assertEquals(0 to 1, setUps to opened)
    }

    @Test
    fun alsoWhileWaitingForTheCode() {
        show(
            AppFixtures.state(copiesOn = true, builtInStatus = BuiltInStatus.PAIRING, canNotify = false),
            AppActions(),
        )

        compose.onNodeWithText(app.getString(R.string.builtin_needs_notifications)).performScrollTo()
        compose.onNodeWithText(app.getString(R.string.builtin_pairing_steps)).assertDoesNotExist()
        compose.onNodeWithText(app.getString(R.string.builtin_cancel)).performScrollTo()
    }
}
