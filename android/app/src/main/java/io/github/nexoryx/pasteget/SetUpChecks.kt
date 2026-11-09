// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.Build
import android.provider.Settings

/**
 * What the built-in mode's set-up needs from the phone, read from its settings each time the
 * set-up screen shows: none of it can be changed by the app before it is set up.
 */
data class SetUpChecks(
    /** Wireless debugging only runs on Wi-Fi. */
    val wifi: Boolean = true,
    val developerOptions: Boolean = true,
    val wirelessDebugging: Boolean = true,
) {
    companion object {
        /** `Settings.Global.ADB_WIFI_ENABLED`, hidden from the SDK. */
        const val ADB_WIFI_ENABLED = "adb_wifi_enabled"

        fun of(context: Context): SetUpChecks {
            val debugging = wirelessDebugging(context)
            return SetUpChecks(
                wifi = onWifi(context),
                // Wireless debugging is in Developer options: if it's on, they are, whatever a
                // vendor's setting says.
                developerOptions = debugging ||
                    Settings.Global.getInt(context.contentResolver, Settings.Global.DEVELOPMENT_SETTINGS_ENABLED, 0) == 1,
                wirelessDebugging = debugging,
            )
        }

        fun onWifi(context: Context): Boolean {
            val connectivity = context.getSystemService(ConnectivityManager::class.java)
            return connectivity.getNetworkCapabilities(connectivity.activeNetwork)
                ?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true
        }

        fun wirelessDebugging(context: Context): Boolean =
            Settings.Global.getInt(context.contentResolver, ADB_WIFI_ENABLED, 0) == 1
    }
}

/**
 * Phone makers whose settings need their own words or pages. Xiaomi covers its POCO and Redmi
 * phones (HyperOS, MIUI).
 */
enum class Vendor {
    XIAOMI,
    OTHER,
    ;

    companion object {
        fun of(manufacturer: String, brand: String): Vendor {
            val names = listOf(manufacturer, brand).map { it.lowercase() }
            return if (names.any { it in XIAOMI_NAMES }) XIAOMI else OTHER
        }

        /** This phone's. */
        fun current(): Vendor = of(Build.MANUFACTURER.orEmpty(), Build.BRAND.orEmpty())

        private val XIAOMI_NAMES = setOf("xiaomi", "redmi", "poco")
    }
}
