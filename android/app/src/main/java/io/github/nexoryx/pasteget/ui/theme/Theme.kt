// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext

/**
 * Material 3 with the colors of [colorScheme], and paste-get's shapes and type. Material 3
 * Expressive (its theme and springier motion) is internal in material3 1.4.0, the version in the
 * Compose BOM: it comes with a BOM that ships material3 1.5.
 */
@Composable
fun PasteGetTheme(
    dark: Boolean = isSystemInDarkTheme(),
    wallpaperColors: Boolean = true,
    content: @Composable () -> Unit,
) {
    MaterialTheme(
        colorScheme = colorScheme(dark, wallpaperColors),
        shapes = AppShapes,
        typography = AppTypography,
    ) {
        Surface(modifier = Modifier.fillMaxSize(), content = content)
    }
}

/** The wallpaper's colors (Material You, Android 12+) when wanted, otherwise paste-get's indigo. */
@Composable
fun colorScheme(dark: Boolean, wallpaperColors: Boolean): ColorScheme {
    val context = LocalContext.current
    return when {
        wallpaperColors && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
            if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        dark -> BrandDark
        else -> BrandLight
    }
}
