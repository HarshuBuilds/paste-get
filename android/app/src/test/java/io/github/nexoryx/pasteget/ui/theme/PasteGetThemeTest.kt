// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui.theme

import android.app.Application
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.core.graphics.ColorUtils
import io.github.nexoryx.pasteget.R
import io.github.nexoryx.pasteget.ui.SwitchRow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** paste-get's colors where Android has no wallpaper colors, or the user doesn't want them. */
@RunWith(RobolectricTestRunner::class)
class PasteGetThemeTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    /** The same colors as the brand's: [ColorScheme] has no equals of its own. */
    private fun assertSame(expected: ColorScheme, actual: ColorScheme) {
        assertEquals(expected.primary, actual.primary)
        assertEquals(expected.background, actual.background)
        assertEquals(expected.surfaceContainer, actual.surfaceContainer)
    }

    private fun schemeOf(dark: Boolean, wallpaperColors: Boolean): ColorScheme {
        lateinit var scheme: ColorScheme
        compose.setContent {
            PasteGetTheme(dark = dark, wallpaperColors = wallpaperColors) {
                scheme = MaterialTheme.colorScheme
                SwitchRow("Title", "Text", checked = true, onCheckedChange = {}, icon = R.drawable.ic_content_copy)
            }
        }
        compose.onNodeWithText("Title").assertIsDisplayed()
        return scheme
    }

    @Test
    @Config(sdk = [29])
    fun android10GetsTheBrandColorsByDay() {
        assertSame(BrandLight, schemeOf(dark = false, wallpaperColors = true))
    }

    @Test
    @Config(sdk = [29], qualifiers = "night")
    fun android10GetsTheBrandColorsAtNight() {
        val scheme = schemeOf(dark = true, wallpaperColors = true)
        assertSame(BrandDark, scheme)
        assertTrue(luminance(scheme.background.toArgb()) < 0.1)
    }

    @Test
    @Config(sdk = [35])
    fun wallpaperColorsCanBeTurnedOff() {
        assertSame(BrandLight, schemeOf(dark = false, wallpaperColors = false))
    }

    @Test
    @Config(sdk = [35])
    fun android12AndLaterUseTheWallpapersColors() {
        val scheme = schemeOf(dark = false, wallpaperColors = true)
        assertNotEquals(BrandLight.primary, scheme.primary)
        assertTrue(luminance(scheme.background.toArgb()) > 0.5)
    }

    /** The window drawn before Compose has the same color: nothing flashes. */
    @Test
    @Config(sdk = [29])
    fun theWindowMatchesTheDayBackground() {
        assertEquals(BrandLight.background.toArgb(), app.getColor(R.color.window_background))
    }

    @Test
    @Config(sdk = [29], qualifiers = "night")
    fun theWindowMatchesTheNightBackground() {
        assertEquals(BrandDark.background.toArgb(), app.getColor(R.color.window_background))
    }

    private fun luminance(color: Int) = ColorUtils.calculateLuminance(color)
}
