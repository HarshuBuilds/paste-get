// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.app.Application
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * French puts a space before : ; ? ! and inside « », and it must be a non-breaking one, or the
 * sign can end up alone at the start of a line.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "fr")
class FrenchTypographyTest {
    private val app: Application = RuntimeEnvironment.getApplication()

    @Test
    fun noBreakingSpaceBeforeFrenchPunctuation() {
        val wrong = Regex(" [:;?!»]|« ")
        val broken = R.string::class.java.fields.mapNotNull { field ->
            val text = app.getString(field.getInt(null))
            if (wrong.containsMatchIn(text)) field.name else null
        }
        assertEquals(emptyList<String>(), broken)
    }
}
