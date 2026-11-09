// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** The built-in mode's code is typed into a notification: set-up must know when it can't show. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29, 35])
class BuiltInNotificationsTest {
    private val app: Application = RuntimeEnvironment.getApplication()
    private val notifications = app.getSystemService(NotificationManager::class.java)

    @Test
    fun byDefaultTheNotificationCanShow() {
        assertTrue(BuiltInShell.canNotify(app))
    }

    @Test
    fun notWhenTheAppsNotificationsAreOff() {
        // Refused on Android 13+, or turned off in Settings.
        shadowOf(notifications).setNotificationsEnabled(false)
        assertFalse(BuiltInShell.canNotify(app))
    }

    @Test
    fun notWhenTheSetUpChannelIsBlocked() {
        // BuiltInShell's channel ID.
        notifications.createNotificationChannel(NotificationChannel("builtin_setup", "Set-up", NotificationManager.IMPORTANCE_NONE))
        assertFalse(BuiltInShell.canNotify(app))
    }
}
