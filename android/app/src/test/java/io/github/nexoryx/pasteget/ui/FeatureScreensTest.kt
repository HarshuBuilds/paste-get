// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.nexoryx.pasteget.ActivityKind
import io.github.nexoryx.pasteget.ActivityLog
import io.github.nexoryx.pasteget.CodeAccess
import io.github.nexoryx.pasteget.PhotoAccess
import io.github.nexoryx.pasteget.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** The Screenshots and 2FA codes screens: each state shows its steps, and the steps do something. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class FeatureScreensTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    private fun screenshots(access: PhotoAccess, denied: Boolean = false, actions: AppActions = AppActions(), activity: ActivityLog? = null) {
        val state = AppFixtures.state(
            computers = listOf(AppFixtures.hp),
            screenshots = ScreenshotSetting(wanted = true, access = access),
            activity = activity?.entries?.value.orEmpty(),
        ).copy(photoRequestDenied = denied)
        compose.setContent { MaterialTheme { ScreenshotsScreen(state, actions, onBack = {}) } }
    }

    private fun current(title: Int) =
        compose.onNode(hasText(app.getString(title)) and hasContentDescription(app.getString(R.string.photo_access_current)))

    @Test
    fun onlySelectedPhotosSaysWhyAndOpensSettings() {
        var opened = 0
        screenshots(PhotoAccess.PARTIAL, actions = AppActions(onOpenAppSettings = { opened++ }))

        compose.onNodeWithText(app.getString(R.string.feature_step)).assertIsDisplayed()
        current(R.string.photo_access_selected).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.screenshots_partial)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.open_settings)).performScrollTo().performClick()
        assertEquals(1, opened)
    }

    @Test
    fun withoutAccessItAsksAgainUnlessRefused() {
        val asked = mutableListOf<Boolean>()
        screenshots(PhotoAccess.NONE, actions = AppActions(onScreenshotsChange = { asked += it }))

        compose.onNodeWithText(app.getString(R.string.screenshots_choose_all)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.step_allow_photos)).performScrollTo().performClick()
        assertEquals(listOf(true), asked)
    }

    @Test
    fun refusedAccessCanOnlyChangeInSettings() {
        screenshots(PhotoAccess.NONE, denied = true)

        compose.onNodeWithText(app.getString(R.string.screenshots_denied)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.step_allow_photos)).assertDoesNotExist()
    }

    @Test
    fun fullAccessNeedsNothingAndShowsTheLastScreenshot() {
        val log = ActivityLog()
        log.sent(ActivityKind.SCREENSHOT, 900_000, sentTo = 1, automatic = true)
        log.delivered("a1", "image/png", 900_000, millis = 91, accepted = true)
        screenshots(PhotoAccess.FULL, activity = log)

        current(R.string.photo_access_all).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.open_settings)).assertDoesNotExist()
        compose.onNodeWithText(app.getString(R.string.activity_delivered, "HP-AIO", 91)).performScrollTo().assertIsDisplayed()
    }

    private fun codes(access: CodeAccess, shellReady: Boolean, actions: CodesActions = CodesActions(), onOpenCopies: () -> Unit = {}) {
        val state = CodesState(on = true, access = access, shellReady = shellReady, last = null, codesHidden = true)
        compose.setContent { MaterialTheme { CodesScreen(state, actions, emptyMap(), onBack = {}, onOpenCopies = onOpenCopies) } }
    }

    private fun done(title: Int) =
        compose.onNode(hasText(app.getString(title)) and hasContentDescription(app.getString(R.string.step_done)))

    @Test
    fun android15sHiddenCodesNeedTheAutomaticCopysShell() {
        var opened = 0
        codes(CodeAccess(listening = true, seesCodes = false), shellReady = false, onOpenCopies = { opened++ })

        done(R.string.codes_step_listen).performScrollTo().assertIsDisplayed()
        done(R.string.codes_step_see).assertDoesNotExist()
        compose.onNodeWithText(app.getString(R.string.codes_step_see)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.codes_needs_shell)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.codes_set_up_shell)).performScrollTo().performClick()
        assertEquals(1, opened)
    }

    @Test
    fun withTheShellOneTapAllowsBoth() {
        var allowed = 0
        codes(CodeAccess(listening = false, seesCodes = false), shellReady = true, actions = CodesActions(onAllow = { allowed++ }))

        compose.onNodeWithText(app.getString(R.string.codes_allow)).performScrollTo().performClick()
        assertEquals(1, allowed)
        compose.onNodeWithText(app.getString(R.string.codes_restricted)).assertDoesNotExist()
    }

    @Test
    fun withoutItSettingsAndTheRestrictedSettingsWayOut() {
        var settings = 0
        var appInfo = 0
        codes(
            CodeAccess(listening = false, seesCodes = null),
            shellReady = false,
            actions = CodesActions(onOpenSettings = { settings++ }, onOpenAppInfo = { appInfo++ }),
        )

        compose.onNodeWithText(app.getString(R.string.codes_open_settings)).performScrollTo().performClick()
        compose.onNodeWithText(app.getString(R.string.codes_restricted)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.codes_open_app_info)).performScrollTo().performClick()
        assertEquals(1 to 1, settings to appInfo)
    }

    @Test
    fun theLastCodeNamesItsAppNeverTheCode() {
        val log = ActivityLog()
        log.sent(ActivityKind.CODE, 6, sentTo = 1, automatic = true, sensitive = true, app = "Messages")
        val state = CodesState(on = true, access = CodeAccess(true, true), shellReady = true, last = log.entries.value.first())
        compose.setContent { MaterialTheme { CodesScreen(state, CodesActions(), emptyMap(), onBack = {}, onOpenCopies = {}) } }

        compose.onNodeWithText(app.getString(R.string.activity_code, "Messages")).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.codes_waiting)).assertDoesNotExist()
    }
}
