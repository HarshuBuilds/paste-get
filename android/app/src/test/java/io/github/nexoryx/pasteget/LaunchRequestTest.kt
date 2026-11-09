// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.content.Intent
import android.net.Uri
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** What opens the app: launcher shortcuts and links, once each. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29, 35])
class LaunchRequestTest {
    @Test
    fun shortcutsAndLinks() {
        assertEquals(LaunchRequest.Pair, LaunchRequest.of(Intent(LaunchRequest.ACTION_PAIR)))
        assertEquals(LaunchRequest.History, LaunchRequest.of(Intent(LaunchRequest.ACTION_HISTORY)))
        assertEquals(LaunchRequest.Link(LINK), LaunchRequest.of(Intent(Intent.ACTION_VIEW, Uri.parse(LINK))))
        // The launcher icon.
        assertNull(LaunchRequest.of(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)))
        assertNull(LaunchRequest.of(null))
    }

    @Test
    fun reopenedFromRecentAppsNothingIsDoneAgain() {
        // The scanner doesn't open by itself, nor an old code's question.
        for (first in listOf(Intent(LaunchRequest.ACTION_PAIR), Intent(Intent.ACTION_VIEW, Uri.parse(LINK)))) {
            assertNull(LaunchRequest.of(first.addFlags(Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY)))
        }
    }

    private companion object {
        const val LINK = "pasteget://pair?v=0&id=a1&s=AB&a=192.168.1.20%3A47290&n=HP-AIO"
    }
}
