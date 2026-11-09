// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.app.Application
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** The appearance follows the system and the wallpaper until the user chooses otherwise. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PreferencesTest {
    private val app: Application = RuntimeEnvironment.getApplication()

    @Test
    fun theDefaultsFollowThePhone() {
        assertEquals(Appearance.SYSTEM, Preferences.appearance(app))
        assertTrue(Preferences.wallpaperColors(app))
    }

    @Test
    fun choicesAreKept() {
        Preferences.setAppearance(app, Appearance.DARK)
        Preferences.setWallpaperColors(app, false)

        assertEquals(Appearance.DARK, Preferences.appearance(app))
        assertFalse(Preferences.wallpaperColors(app))
    }
}
