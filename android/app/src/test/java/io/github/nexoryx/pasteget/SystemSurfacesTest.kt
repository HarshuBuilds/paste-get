// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.app.Application
import android.app.Notification
import android.icu.text.ListFormatter
import android.service.quicksettings.Tile
import androidx.core.app.NotificationCompat
import io.github.nexoryx.pasteget.ui.Feature
import io.github.nexoryx.pasteget.ui.FeatureHealth
import io.github.nexoryx.pasteget.ui.Health
import io.github.nexoryx.pasteget.ui.NextStep
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.xmlpull.v1.XmlPullParser

/** The notification, the tile and the launcher shortcuts say who is connected and lead where they should. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class SystemSurfacesTest {
    private val app: Application = RuntimeEnvironment.getApplication()

    private fun notification(paired: List<String>, connected: List<String>, copies: FeatureHealth? = null): Notification =
        ConnectionNotification.build(app, "connection", paired, connected, copies)

    private fun Notification.title() = extras.getCharSequence(Notification.EXTRA_TITLE)?.toString()

    private fun Notification.text() = extras.getCharSequence(Notification.EXTRA_TEXT)?.toString()

    @Test
    fun theNotificationNamesTheConnectedComputers() {
        val shown = notification(listOf("HP-AIO", "Arch-PC"), listOf("HP-AIO"), FeatureHealth(Feature.COPIES, Health.WORKING))

        assertEquals(app.getString(R.string.hero_connected, "HP-AIO"), shown.title())
        assertEquals(app.getString(R.string.notification_copies_auto), shown.text())
        // Send clipboard goes straight to the activity that may read it.
        val send = NotificationCompat.getAction(shown, 0)!!
        assertEquals(app.getString(R.string.tile_label), send.title)
        assertEquals(SendClipboardActivity::class.java.name, shadowOf(send.actionIntent).savedIntent.component!!.className)
    }

    @Test
    fun whileLookingItNamesWhoItLooksFor() {
        val shown = notification(listOf("HP-AIO", "Arch-PC"), emptyList(), FeatureHealth(Feature.COPIES, Health.BLOCKED, NextStep.SET_UP_BUILT_IN))

        val both = ListFormatter.getInstance().format(listOf("HP-AIO", "Arch-PC"))
        assertEquals(app.getString(R.string.hero_looking, both), shown.title())
        assertEquals(app.getString(R.string.notification_copies_step), shown.text())
    }

    @Test
    fun withNothingPairedThereIsNothingToSend() {
        val shown = notification(emptyList(), emptyList())

        assertEquals(app.getString(R.string.notification_no_computer), shown.title())
        assertNull(shown.text())
        assertEquals(0, NotificationCompat.getActionCount(shown))
    }

    @Test
    fun theTileFollowsTheConnection() {
        assertEquals(TileLook(Tile.STATE_ACTIVE, "HP-AIO"), TileLook.of(app, listOf("HP-AIO"), listOf("HP-AIO")))
        // Still a tap away: the clip waits for the computer.
        assertEquals(TileLook(Tile.STATE_INACTIVE, app.getString(R.string.tile_offline)), TileLook.of(app, listOf("HP-AIO"), emptyList()))
        assertEquals(TileLook(Tile.STATE_UNAVAILABLE, app.getString(R.string.tile_not_paired)), TileLook.of(app, emptyList(), emptyList()))
    }

    /** A renamed activity or action would leave a shortcut that does nothing. */
    @Test
    fun eachShortcutLeadsToAnActivityThatHandlesIt() {
        val targets = mutableListOf<Pair<String, String>>()
        val xml = app.resources.getXml(R.xml.shortcuts)
        while (xml.next() != XmlPullParser.END_DOCUMENT) {
            if (xml.eventType == XmlPullParser.START_TAG && xml.name == "intent") {
                val action = xml.getAttributeValue(ANDROID, "action")
                val target = xml.getAttributeValue(ANDROID, "targetClass")
                assertEquals(app.packageName, xml.getAttributeValue(ANDROID, "targetPackage"))
                targets += action to target
            }
        }
        assertEquals(
            listOf(
                "android.intent.action.VIEW" to SendClipboardActivity::class.java.name,
                "io.github.nexoryx.pasteget.action.PAIR" to MainActivity::class.java.name,
                "io.github.nexoryx.pasteget.action.HISTORY" to MainActivity::class.java.name,
            ),
            targets,
        )
    }

    private companion object {
        const val ANDROID = "http://schemas.android.com/apk/res/android"
    }
}
