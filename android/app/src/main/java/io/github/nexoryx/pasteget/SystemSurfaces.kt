// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.app.Notification
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.icu.text.ListFormatter
import android.service.quicksettings.Tile
import androidx.core.app.NotificationCompat
import io.github.nexoryx.pasteget.ui.FeatureHealth
import io.github.nexoryx.pasteget.ui.Health

/*
 * What paste-get shows outside its screens: the connection notification and the Quick Settings
 * tile. Both are built from plain values here, so tests check them without the service.
 */

/** The connection service's notification: who is connected, how copies go, and Send clipboard. */
object ConnectionNotification {
    /**
     * [paired] and [connected] are computer names; [copies] is the automatic copy's health, or
     * `null` when it's off.
     */
    fun build(context: Context, channel: String, paired: List<String>, connected: List<String>, copies: FeatureHealth?): Notification {
        val openApp = PendingIntent.getActivity(context, 0, Intent(context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val builder = NotificationCompat.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_tile)
            .setContentTitle(title(context, paired, connected))
            .setContentIntent(openApp)
            .setOngoing(true)
            .setSilent(true)
            .setShowWhen(false)
            .setPriority(NotificationCompat.PRIORITY_LOW)
        when (copies?.health) {
            Health.WORKING -> builder.setContentText(context.getString(R.string.notification_copies_auto))
            Health.BLOCKED -> builder.setContentText(context.getString(R.string.notification_copies_step))
            else -> {}
        }
        if (paired.isNotEmpty()) {
            // Straight to the invisible activity that may read the clipboard: no app opens.
            val send = PendingIntent.getActivity(
                context,
                1,
                Intent(context, SendClipboardActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                PendingIntent.FLAG_IMMUTABLE,
            )
            builder.addAction(R.drawable.ic_tile, context.getString(R.string.tile_label), send)
        }
        return builder.build()
    }

    private fun title(context: Context, paired: List<String>, connected: List<String>): String = when {
        paired.isEmpty() -> context.getString(R.string.notification_no_computer)
        connected.isNotEmpty() -> context.getString(R.string.hero_connected, names(connected))
        else -> context.getString(R.string.hero_looking, names(paired))
    }
}

/**
 * The Send clipboard tile: on while a computer is connected, off (but still sending: the clip
 * waits) while none is, unavailable with nothing paired. [subtitle] says which.
 */
data class TileLook(val state: Int, val subtitle: String) {
    companion object {
        fun of(context: Context, paired: List<String>, connected: List<String>): TileLook = when {
            paired.isEmpty() -> TileLook(Tile.STATE_UNAVAILABLE, context.getString(R.string.tile_not_paired))
            connected.isNotEmpty() -> TileLook(Tile.STATE_ACTIVE, names(connected))
            else -> TileLook(Tile.STATE_INACTIVE, context.getString(R.string.tile_offline))
        }
    }
}

/** "HP-AIO and Arch-PC", in the phone's language. */
private fun names(names: List<String>): String = ListFormatter.getInstance().format(names)
