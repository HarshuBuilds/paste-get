// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import io.github.nexoryx.pasteget.BuiltInState
import io.github.nexoryx.pasteget.BuiltInStatus
import io.github.nexoryx.pasteget.KeepAlive
import io.github.nexoryx.pasteget.PhotoAccess
import io.github.nexoryx.pasteget.ui.OnboardingStep.CHOOSE
import io.github.nexoryx.pasteget.ui.OnboardingStep.CODES
import io.github.nexoryx.pasteget.ui.OnboardingStep.COMPUTER
import io.github.nexoryx.pasteget.ui.OnboardingStep.COPIES
import io.github.nexoryx.pasteget.ui.OnboardingStep.KEEP_ALIVE
import io.github.nexoryx.pasteget.ui.OnboardingStep.NOTIFICATIONS
import io.github.nexoryx.pasteget.ui.OnboardingStep.PAIR
import io.github.nexoryx.pasteget.ui.OnboardingStep.SCREENSHOTS
import io.github.nexoryx.pasteget.ui.OnboardingStep.TRY
import io.github.nexoryx.pasteget.ui.OnboardingStep.WELCOME
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The first-run guide's pages follow what the user chose, and each says when it's done. */
class OnboardingStepsTest {
    @Test
    fun eachChosenFeatureGetsItsPage() {
        assertEquals(
            listOf(WELCOME, COMPUTER, PAIR, NOTIFICATIONS, CHOOSE, COPIES, SCREENSHOTS, CODES, KEEP_ALIVE, TRY),
            onboardingSteps(Feature.entries.toSet(), asksForNotifications = true),
        )
        assertEquals(
            listOf(WELCOME, COMPUTER, PAIR, NOTIFICATIONS, CHOOSE, CODES, KEEP_ALIVE, TRY),
            onboardingSteps(setOf(Feature.CODES), asksForNotifications = true),
        )
    }

    @Test
    fun notificationsHaveAPageWhereAndroidAsksForThem() {
        assertEquals(
            listOf(WELCOME, COMPUTER, PAIR, CHOOSE, KEEP_ALIVE, TRY),
            onboardingSteps(emptySet(), asksForNotifications = false),
        )
    }

    @Test
    fun aPageIsDoneOnceItsSettingIs() {
        val fresh = AppFixtures.state()
        assertFalse(onboardingStepDone(PAIR, fresh))
        assertTrue(onboardingStepDone(PAIR, AppFixtures.state(computers = listOf(AppFixtures.hp))))

        assertFalse(onboardingStepDone(COPIES, AppFixtures.state(copiesOn = true)))
        assertTrue(onboardingStepDone(COPIES, AppFixtures.state(copiesOn = true, builtInStatus = BuiltInStatus.WATCHING)))
        assertTrue(onboardingStepDone(COPIES, AppFixtures.state(copiesOn = true, copiesSetUp = true)))

        assertFalse(onboardingStepDone(SCREENSHOTS, fresh))
        assertTrue(onboardingStepDone(SCREENSHOTS, fresh.copy(screenshots = ScreenshotSetting(wanted = true, access = PhotoAccess.FULL))))

        assertFalse(onboardingStepDone(CODES, fresh))

        val noBattery = fresh.copy(keepAlive = KeepAliveState(KeepAlive(battery = false, notifications = false)))
        assertFalse(onboardingStepDone(KEEP_ALIVE, noBattery))
        assertFalse(onboardingStepDone(NOTIFICATIONS, noBattery))
        assertTrue(onboardingStepDone(KEEP_ALIVE, fresh))
    }

    @Test
    fun pagesThatOnlyExplainAreAlwaysDone() {
        val fresh = AppFixtures.state()
        for (step in listOf(WELCOME, COMPUTER, CHOOSE, TRY)) assertTrue(onboardingStepDone(step, fresh))
        // Unused here, but the fixture's built-in state stays as given.
        assertEquals(BuiltInState(BuiltInStatus.OFF), fresh.autoCopy.builtIn)
    }
}
