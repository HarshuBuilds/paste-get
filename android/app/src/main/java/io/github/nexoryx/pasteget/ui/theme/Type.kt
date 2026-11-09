// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.ui.text.font.FontWeight

/**
 * The system font (MiSans on HyperOS, Roboto elsewhere), never a downloaded one: that would need
 * Play Services and the network. Headlines and titles are a weight heavier than Material's.
 */
val AppTypography: Typography = Typography().run {
    copy(
        headlineLarge = headlineLarge.copy(fontWeight = FontWeight.SemiBold),
        headlineMedium = headlineMedium.copy(fontWeight = FontWeight.SemiBold),
        headlineSmall = headlineSmall.copy(fontWeight = FontWeight.SemiBold),
        titleLarge = titleLarge.copy(fontWeight = FontWeight.SemiBold),
        titleMedium = titleMedium.copy(fontWeight = FontWeight.SemiBold),
    )
}
