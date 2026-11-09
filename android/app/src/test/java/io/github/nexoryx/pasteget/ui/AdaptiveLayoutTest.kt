// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.DeviceConfigurationOverride
import androidx.compose.ui.test.ForcedSize
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.DpRect
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.R
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** On a phone a screen uses the whole width; on a tablet or in landscape, at most 640 dp, centred. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class AdaptiveLayoutTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = RuntimeEnvironment.getApplication()

    /** Where the status card's text is drawn, on a screen of [size]. */
    private fun heroOn(size: DpSize): DpRect {
        compose.setContent {
            DeviceConfigurationOverride(DeviceConfigurationOverride.ForcedSize(size)) {
                MaterialTheme { HomeScreen(AppFixtures.state(), AppActions()) }
            }
        }
        return compose.onNodeWithText(app.getString(R.string.hero_pair_title)).getUnclippedBoundsInRoot()
    }

    @Test
    fun onAPhoneTheColumnFillsTheWidth() {
        val hero = heroOn(DpSize(360.dp, 720.dp))

        // 16 dp of margin and the card's 20 dp of padding on each side.
        assertTrue("$hero", hero.left < 40.dp && hero.right > 320.dp)
    }

    @Test
    fun onATabletItStaysNarrowAndCentred() {
        val hero = heroOn(DpSize(1200.dp, 800.dp))

        assertTrue("$hero", hero.right - hero.left <= MAX_WIDTH)
        // As much room on the left as on the right.
        assertTrue("$hero", hero.left > 280.dp && hero.right < 920.dp)
    }
}
