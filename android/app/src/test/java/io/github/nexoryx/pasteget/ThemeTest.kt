// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.view.ContextThemeWrapper
import androidx.core.graphics.ColorUtils
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import androidx.core.splashscreen.R as SplashR

/** The window behind the UI follows the system's dark mode from the first frame. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29, 35])
class ThemeTest {
    @Test
    fun theWindowIsLightByDay() {
        assertTrue(luminance(R.style.Theme_PasteGet, android.R.attr.windowBackground) > 0.5)
        assertTrue(luminance(R.style.Theme_PasteGet_Starting, SplashR.attr.windowSplashScreenBackground) > 0.5)
    }

    @Test
    @Config(qualifiers = "night")
    fun theWindowIsDarkAtNight() {
        assertTrue(luminance(R.style.Theme_PasteGet, android.R.attr.windowBackground) < 0.1)
        assertTrue(luminance(R.style.Theme_PasteGet_Starting, SplashR.attr.windowSplashScreenBackground) < 0.1)
    }

    @Test
    fun theStartScreenHandsOverToTheAppTheme() {
        val context = ContextThemeWrapper(RuntimeEnvironment.getApplication(), R.style.Theme_PasteGet_Starting)
        val values = context.obtainStyledAttributes(intArrayOf(SplashR.attr.postSplashScreenTheme))
        try {
            assertEquals(R.style.Theme_PasteGet, values.getResourceId(0, 0))
        } finally {
            values.recycle()
        }
    }

    /** The luminance of the color [attr] resolves to in [theme]: 0 is black, 1 white. */
    private fun luminance(theme: Int, attr: Int): Double {
        val context = ContextThemeWrapper(RuntimeEnvironment.getApplication(), theme)
        val values = context.obtainStyledAttributes(intArrayOf(attr))
        try {
            return ColorUtils.calculateLuminance(values.getColor(0, 0))
        } finally {
            values.recycle()
        }
    }
}
