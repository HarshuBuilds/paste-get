// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.app.Application
import android.app.NotificationManager
import android.os.PowerManager
import android.provider.Settings
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** What keeps paste-get running: read from Android where it can be, a vendor's page otherwise. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29, 35])
class KeepAliveTest {
    private val app: Application = RuntimeEnvironment.getApplication()

    @Test
    fun batteryOptimisationIsReadFromAndroid() {
        assertFalse(KeepAlive.of(app).battery)

        shadowOf(app.getSystemService(PowerManager::class.java)).setIgnoringBatteryOptimizations(app.packageName, true)
        assertTrue(KeepAlive.of(app).battery)
    }

    @Test
    fun soAreNotifications() {
        assertTrue(KeepAlive.of(app).notifications)

        shadowOf(app.getSystemService(NotificationManager::class.java)).setNotificationsEnabled(false)
        assertFalse(KeepAlive.of(app).notifications)
    }

    @Test
    fun anUnknownVendorFallsBackToAppInfo() {
        val settings = VendorSettings.of(Vendor.OTHER)
        assertFalse(settings.hasAutostart)

        val pages = settings.autostartPages(app.packageName)
        assertEquals(1, pages.size)
        assertEquals(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, pages.single().action)
        assertEquals("package:${app.packageName}", pages.single().dataString)
    }

    @Test
    fun xiaomiTriesItsAutostartPageThenAppInfo() {
        val settings = VendorSettings.of(Vendor.XIAOMI)
        assertTrue(settings.hasAutostart)

        val pages = settings.autostartPages(app.packageName)
        assertEquals("com.miui.securitycenter", pages.first().component?.packageName)
        assertEquals(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, pages.last().action)
    }

    @Test
    fun theTicksAreKept() {
        assertFalse(Preferences.autostartTicked(app))
        assertFalse(Preferences.batterySaverTicked(app))

        Preferences.setAutostartTicked(app, true)
        Preferences.setBatterySaverTicked(app, true)
        assertTrue(Preferences.autostartTicked(app))
        assertTrue(Preferences.batterySaverTicked(app))
    }
}
