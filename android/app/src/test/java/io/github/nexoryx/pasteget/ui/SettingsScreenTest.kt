// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import android.os.Build
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.nexoryx.pasteget.Appearance
import io.github.nexoryx.pasteget.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** Settings offers what this Android has, and each control does what it says. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class SettingsScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    private fun show(state: SettingsState, actions: SettingsActions = SettingsActions(), onOpenLicences: () -> Unit = {}) {
        compose.setContent { MaterialTheme { SettingsScreen(state, actions, onBack = {}, onOpenLicences = onOpenLicences) } }
    }

    @Test
    fun theAppearanceIsOneChoice() {
        val chosen = mutableListOf<Appearance>()
        show(SettingsState(Appearance.SYSTEM), SettingsActions(onAppearanceChange = { chosen += it }))

        compose.onNodeWithText(app.getString(R.string.appearance_system)).assertIsSelected()
        compose.onNodeWithText(app.getString(R.string.appearance_dark)).performClick()
        assertEquals(listOf(Appearance.DARK), chosen)
    }

    @Test
    fun android13OffersTheLanguageAndTheTile() {
        val calls = mutableListOf<String>()
        show(
            SettingsState(sdk = Build.VERSION_CODES.TIRAMISU),
            SettingsActions(
                onWallpaperColorsChange = { calls += "colors $it" },
                onOpenLanguage = { calls += "language" },
                onAddTile = { calls += "tile" },
            ),
        )

        compose.onNodeWithText(app.getString(R.string.wallpaper_colors)).performClick()
        compose.onNodeWithText(app.getString(R.string.settings_language_text)).performScrollTo().performClick()
        compose.onNodeWithText(app.getString(R.string.tile_add)).performScrollTo().performClick()
        assertEquals(listOf("colors false", "language", "tile"), calls)
        compose.onNodeWithText(app.getString(R.string.tile_how)).assertDoesNotExist()
    }

    @Test
    @Config(sdk = [29])
    fun android10SaysHowToAddTheTileByHand() {
        show(SettingsState(sdk = Build.VERSION_CODES.Q))

        compose.onNodeWithText(app.getString(R.string.wallpaper_colors)).assertDoesNotExist()
        compose.onNodeWithText(app.getString(R.string.settings_language_text)).assertDoesNotExist()
        compose.onNodeWithText(app.getString(R.string.tile_add)).assertDoesNotExist()
        compose.onNodeWithText(app.getString(R.string.tile_how)).performScrollTo().assertIsDisplayed()
    }

    @Test
    fun aboutLeadsToTheLicences() {
        var opened = 0
        show(SettingsState(), onOpenLicences = { opened++ })

        // The versionName of build.gradle.kts, whatever it is now.
        val version = app.packageManager.getPackageInfo(app.packageName, 0).versionName.orEmpty()
        compose.onNodeWithText(app.getString(R.string.about_version, version)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.licences_title)).performScrollTo().performClick()
        assertEquals(1, opened)
    }

    @Test
    fun theLicencesListTheIconsAndCarryTheApacheText() {
        compose.setContent { MaterialTheme { LicencesScreen(onBack = {}) } }

        compose.onNodeWithText("Material Symbols (icons, © Google LLC)").assertIsDisplayed()
        compose.onNodeWithText(app.getString(R.string.licences_show_apache)).performScrollTo().performClick()
        compose.onNodeWithText("Apache License", substring = true).performScrollTo().assertIsDisplayed()
    }
}
