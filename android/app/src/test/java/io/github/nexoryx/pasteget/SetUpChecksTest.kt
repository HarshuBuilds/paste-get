// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.app.Application
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
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
import org.robolectric.shadows.ShadowNetworkCapabilities
import uniffi.pasteget_ffi.AdbException

/** The set-up reads what the user turned on in Settings, and names what went wrong with a code. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29, 35])
class SetUpChecksTest {
    private val app: Application = RuntimeEnvironment.getApplication()
    private val resolver = app.contentResolver

    @Test
    fun aFreshPhoneHasNeitherDeveloperOptionsNorWirelessDebugging() {
        val checks = SetUpChecks.of(app)
        assertFalse(checks.developerOptions)
        assertFalse(checks.wirelessDebugging)
    }

    @Test
    fun developerOptionsAreReadFromTheirSetting() {
        Settings.Global.putInt(resolver, Settings.Global.DEVELOPMENT_SETTINGS_ENABLED, 1)

        val checks = SetUpChecks.of(app)
        assertTrue(checks.developerOptions)
        assertFalse(checks.wirelessDebugging)
    }

    @Test
    fun wirelessDebuggingMeansDeveloperOptionsAreOnToo() {
        // Some vendors leave the first setting at 0: the second can't be on without them.
        Settings.Global.putInt(resolver, "adb_wifi_enabled", 1)

        val checks = SetUpChecks.of(app)
        assertTrue(checks.wirelessDebugging)
        assertTrue(checks.developerOptions)
    }

    @Test
    fun wifiIsTheActiveNetworksTransport() {
        val connectivity = app.getSystemService(ConnectivityManager::class.java)
        val network = connectivity.activeNetwork
        val capabilities = ShadowNetworkCapabilities.newInstance()

        shadowOf(capabilities).addTransportType(NetworkCapabilities.TRANSPORT_CELLULAR)
        shadowOf(connectivity).setNetworkCapabilities(network, capabilities)
        assertFalse(SetUpChecks.of(app).wifi)

        shadowOf(capabilities).addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
        shadowOf(connectivity).setNetworkCapabilities(network, capabilities)
        assertTrue(SetUpChecks.of(app).wifi)
    }

    @Test
    fun xiaomiCoversItsOtherBrands() {
        assertEquals(Vendor.XIAOMI, Vendor.of("Xiaomi", "POCO"))
        assertEquals(Vendor.XIAOMI, Vendor.of("XIAOMI", "Redmi"))
        assertEquals(Vendor.XIAOMI, Vendor.of("unknown", "poco"))
        assertEquals(Vendor.OTHER, Vendor.of("Google", "google"))
        assertEquals(Vendor.OTHER, Vendor.of("", ""))
    }

    @Test
    fun eachPairingErrorHasItsSentence() {
        assertEquals(PairingFailure.WRONG_CODE, PairingFailure.of(AdbException.WrongCode("bad pairing code")))
        assertEquals(PairingFailure.NO_CODE_SCREEN, PairingFailure.of(AdbException.Unreachable("connection refused")))
        assertEquals(PairingFailure.OTHER, PairingFailure.of(AdbException.Protocol("unexpected message")))
        assertEquals(PairingFailure.OTHER, PairingFailure.of(IllegalStateException()))
    }
}
