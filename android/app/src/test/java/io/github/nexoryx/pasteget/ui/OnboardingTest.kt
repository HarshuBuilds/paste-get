// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import android.os.Build
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.nexoryx.pasteget.ActivityKind
import io.github.nexoryx.pasteget.ActivityLog
import io.github.nexoryx.pasteget.BuiltInStatus
import io.github.nexoryx.pasteget.Preferences
import io.github.nexoryx.pasteget.R
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** The first-run guide: shown once on a phone with no pairings, page by page, then a real test. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class OnboardingTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    private fun text(id: Int, vararg args: Any) = compose.onNodeWithText(app.getString(id, *args))

    private fun show(state: AppState, actions: AppActions = AppActions(), first: OnboardingStep = OnboardingStep.WELCOME, onFinish: () -> Unit = {}) {
        compose.setContent { MaterialTheme { OnboardingScreen(state, actions, onFinish, first, sdk = Build.VERSION_CODES.VANILLA_ICE_CREAM) } }
    }

    @Test
    fun aFreshPhoneGetsTheGuideOnce() {
        val model = AppViewModel(app, FakeComputers(), Dispatchers.Unconfined)
        model.refresh()
        assertEquals(true, model.onboarding)

        model.finishOnboarding()
        assertEquals(false, model.onboarding)
        assertTrue(Preferences.onboardingDone(app))
        val again = AppViewModel(app, FakeComputers(), Dispatchers.Unconfined).also { it.refresh() }
        assertEquals(false, again.onboarding)
    }

    @Test
    fun aPhoneWithPairingsSkipsIt() {
        val model = AppViewModel(app, FakeComputers(computers = listOf(AppFixtures.hp)), Dispatchers.Unconfined)
        model.refresh()
        assertEquals(false, model.onboarding)
        assertTrue(Preferences.onboardingDone(app))
    }

    @Test
    fun pairingDuringTheGuideDoesntEndIt() {
        val fake = FakeComputers()
        val model = AppViewModel(app, fake, Dispatchers.Unconfined).also { it.refresh() }

        fake.computers = listOf(AppFixtures.hp)
        model.refresh()
        assertEquals(true, model.onboarding)
        assertFalse(Preferences.onboardingDone(app))
    }

    @Test
    fun closingTheAppAfterPairingDoesntEndItEither() {
        val fake = FakeComputers()
        AppViewModel(app, fake, Dispatchers.Unconfined).also { it.refresh() }

        // Paired on the guide's page, then swiped away from recent apps: a new start.
        fake.computers = listOf(AppFixtures.hp)
        val again = AppViewModel(app, fake, Dispatchers.Unconfined).also { it.refresh() }
        assertEquals(true, again.onboarding)

        again.finishOnboarding()
        assertEquals(false, AppViewModel(app, fake, Dispatchers.Unconfined).also { it.refresh() }.onboarding)
    }

    @Test
    fun itGoesPageByPageAndTurnsOnWhatWasChosen() {
        val copies = mutableListOf<Boolean>()
        var finished = 0
        show(AppFixtures.state(), AppActions(autoCopy = CopiesActions(onChange = { copies += it })), onFinish = { finished++ })

        text(R.string.welcome_title).assertIsDisplayed()
        text(R.string.welcome_start).performClick()
        text(R.string.computer_title).assertIsDisplayed()
        text(R.string.computer_ready).performClick()
        // Not paired yet: going on is "Later".
        text(R.string.onboarding_pair_text).assertIsDisplayed()
        text(R.string.onboarding_later).performClick()
        text(R.string.onboarding_notifications_text).assertIsDisplayed()
        text(R.string.onboarding_next).performClick()
        text(R.string.choose_title).assertIsDisplayed()
        // Copies are recommended, and chosen to start with.
        text(R.string.choose_recommended).assertIsDisplayed()
        text(R.string.onboarding_next).performClick()
        assertEquals(listOf(true), copies)
        text(R.string.setup_intro).performScrollTo().assertIsDisplayed()
        text(R.string.onboarding_later).performClick()
        text(R.string.keepalive_intro).performScrollTo().assertIsDisplayed()
        text(R.string.onboarding_next).performClick()
        text(R.string.try_no_computer).assertIsDisplayed()
        text(R.string.onboarding_finish).performClick()
        assertEquals(1, finished)
    }

    @Test
    fun backGoesToThePreviousPageAndSkipLeaves() {
        var finished = 0
        show(AppFixtures.state(), first = OnboardingStep.COMPUTER, onFinish = { finished++ })

        compose.onNodeWithContentDescription(app.getString(R.string.back)).performClick()
        text(R.string.welcome_title).assertIsDisplayed()
        text(R.string.onboarding_skip).performClick()
        assertEquals(1, finished)
    }

    @Test
    fun theDotsSayWhereTheGuideIs() {
        show(AppFixtures.state(), first = OnboardingStep.PAIR)
        compose.onNode(hasContentDescription(app.getString(R.string.setup_progress, 3, 8))).assertIsDisplayed()
    }

    @Test
    fun theTestSentenceArrivesForReal() {
        val log = ActivityLog()
        var state by mutableStateOf(AppFixtures.state(computers = listOf(AppFixtures.hp), connected = setOf("a1")))
        val tries = mutableListOf<Boolean>()
        compose.setContent {
            MaterialTheme {
                OnboardingScreen(
                    state,
                    AppActions(onTryIt = { _, byHand -> tries += byHand }),
                    onFinish = {},
                    first = OnboardingStep.TRY,
                )
            }
        }

        text(R.string.try_button).performClick()
        // The automatic copy isn't on: the guide sends it by hand.
        assertEquals(listOf(true), tries)
        text(R.string.try_waiting).assertIsDisplayed()

        log.sent(ActivityKind.TEXT, 40, sentTo = 1, automatic = false)
        log.delivered("a1", "text/plain", 40, millis = 42, accepted = true)
        state = state.copy(activity = log.entries.value)
        text(R.string.try_arrived, "HP-AIO", 42).assertIsDisplayed()
    }

    @Test
    fun withTheAutomaticCopyItGoesAsAnyCopy() {
        val tries = mutableListOf<Boolean>()
        val state = AppFixtures.state(computers = listOf(AppFixtures.hp), connected = setOf("a1"), copiesOn = true, builtInStatus = BuiltInStatus.WATCHING)
        show(state, AppActions(onTryIt = { _, byHand -> tries += byHand }), first = OnboardingStep.TRY)

        text(R.string.try_button).performClick()
        assertEquals(listOf(false), tries)
    }

    @Test
    fun withoutAnAnswerItSaysWhatToCheck() {
        val tries = mutableListOf<Boolean>()
        show(
            AppFixtures.state(computers = listOf(AppFixtures.hp), connected = setOf("a1")),
            AppActions(onTryIt = { _, byHand -> tries += byHand }),
            first = OnboardingStep.TRY,
        )

        text(R.string.try_button).performClick()
        compose.mainClock.advanceTimeBy(TRY_TIMEOUT_MS + 100)
        text(R.string.try_failed_title).assertIsDisplayed()
        text(R.string.hero_looking_long).assertIsDisplayed()
        text(R.string.hero_try_again).performClick()
        assertEquals(2, tries.size)
        text(R.string.try_waiting).assertIsDisplayed()
    }
}
