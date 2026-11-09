// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.content.Context
import android.os.PowerManager
import androidx.core.app.NotificationManagerCompat

/**
 * What keeps Android from stopping paste-get in the background, as far as the app can read it:
 * battery optimisation and notifications. Vendor switches such as Xiaomi's Autostart can't be
 * read; the user ticks them ([Preferences.autostartTicked]).
 */
data class KeepAlive(
    /** Android's battery optimisation leaves paste-get alone ("Unrestricted"). */
    val battery: Boolean,
    /** The connection's notification can show, which keeps the service in the foreground. */
    val notifications: Boolean,
) {
    companion object {
        fun of(context: Context) = KeepAlive(
            battery = context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName),
            notifications = NotificationManagerCompat.from(context).areNotificationsEnabled(),
        )
    }
}
