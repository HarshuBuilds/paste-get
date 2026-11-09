// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Shapes
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.dp

/** Material's corner sizes, written out. The status card uses extraLarge (28 dp). */
val AppShapes = Shapes(
    extraSmall = RoundedCornerShape(4.dp),
    small = RoundedCornerShape(8.dp),
    medium = RoundedCornerShape(12.dp),
    large = RoundedCornerShape(16.dp),
    extraLarge = RoundedCornerShape(28.dp),
)

private val outer = 20.dp
private val inner = 4.dp

/**
 * The shape of row [index] of [count] in a group of rows that read as one surface: round at the
 * group's ends, nearly square between rows (the rows sit 2 dp apart).
 */
fun groupedShape(index: Int, count: Int): Shape = when {
    count == 1 -> RoundedCornerShape(outer)
    index == 0 -> RoundedCornerShape(topStart = outer, topEnd = outer, bottomStart = inner, bottomEnd = inner)
    index == count - 1 -> RoundedCornerShape(topStart = inner, topEnd = inner, bottomStart = outer, bottomEnd = outer)
    else -> RoundedCornerShape(inner)
}
