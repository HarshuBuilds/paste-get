// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.content.ComponentName
import android.content.Intent
import android.net.Uri
import android.provider.Settings

/**
 * Settings pages that only some phone makers have, and that no Android API reads or opens. Their
 * names change between vendors and versions, so each page comes with a fallback: App info,
 * which every Android has.
 */
interface VendorSettings {
    /** Whether the phone has its own switch that lets apps start by themselves after a restart. */
    val hasAutostart: Boolean

    /** Pages to try in order for that switch. The last one is App info. */
    fun autostartPages(packageName: String): List<Intent>

    companion object {
        fun of(vendor: Vendor): VendorSettings = when (vendor) {
            Vendor.XIAOMI -> XiaomiSettings
            Vendor.OTHER -> OtherSettings
        }

        /** paste-get's own page in the system settings. */
        fun appInfo(packageName: String): Intent =
            Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", packageName, null))
    }
}

/** HyperOS and MIUI: Autostart is in the Security app, and in App info on recent versions. */
object XiaomiSettings : VendorSettings {
    override val hasAutostart = true

    override fun autostartPages(packageName: String) = listOf(
        Intent().setComponent(ComponentName("com.miui.securitycenter", "com.miui.permcenter.autostart.AutoStartManagementActivity")),
        VendorSettings.appInfo(packageName),
    )
}

/** Phones with nothing of their own to set: App info is all there is. */
object OtherSettings : VendorSettings {
    override val hasAutostart = false

    override fun autostartPages(packageName: String) = listOf(VendorSettings.appInfo(packageName))
}
