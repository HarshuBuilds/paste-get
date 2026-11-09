// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.SystemClock
import android.service.quicksettings.TileService
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import io.github.nexoryx.pasteget.ui.copiesHealth
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import uniffi.pasteget_ffi.ClipDelivery
import uniffi.pasteget_ffi.PairedComputer
import uniffi.pasteget_ffi.PhoneListener

/**
 * Keeps the connections to the paired computers alive while the app is in the background.
 *
 * A `connectedDevice` foreground service: unlike `dataSync`, Android 15 doesn't cap its running
 * time. Its notification names the connected computers. While one of them isn't, it
 * looks for it on the local network. When the user turned them on, it also sends new screenshots
 * and what they copy ([AutoCopy]).
 */
class SyncService : Service() {
    private val main = Handler(Looper.getMainLooper())
    private lateinit var finder: ComputerFinder
    private lateinit var screenshots: ScreenshotWatcher

    /** Follows the automatic copy's state, on the main thread, while the service runs. */
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    /** Callbacks from the client's threads may still arrive after onDestroy. */
    private var running = false

    private val listener = object : PhoneListener {
        override fun onConnectionChanged(deviceId: String, connected: Boolean) {
            pasteGet.setConnected(deviceId, connected)
            main.post {
                if (running) {
                    updateNotification()
                    updateFinder()
                }
            }
        }

        override fun onUnpaired(computer: PairedComputer) {
            pasteGet.setConnected(computer.deviceId, false)
            pasteGet.unpairedBy(computer)
            main.post {
                if (running) {
                    updateNotification()
                    updateFinder()
                }
            }
            showUnpaired(computer)
        }

        override fun onClipReceived(mimeType: String, data: ByteArray, sensitive: Boolean) {
            ReceivedClips.apply(applicationContext, mimeType, data, sensitive)
        }

        override fun onClipDelivered(delivery: ClipDelivery) {
            val size = delivery.size.toLong()
            val screenshot = lastScreenshot?.takeIf { delivery.mimeType.startsWith("image/") && it.size == size }
            val millis = screenshot?.let { SystemClock.elapsedRealtime() - it.announcedAtMs } ?: delivery.millis.toLong()
            Log.i(
                TAG,
                "${delivery.mimeType}, $size bytes: delivered ${delivery.millis} ms after sending" +
                    (screenshot?.let { ", $millis ms after Android announced the screenshot" } ?: "") +
                    if (delivery.accepted) "" else ", refused",
            )
            pasteGet.activity.delivered(delivery.deviceId, delivery.mimeType, size.toInt(), millis, delivery.accepted)
        }
    }

    /** The last screenshot sent, to time it from its announcement to the computer's ack. */
    private class SentScreenshot(val size: Long, val announcedAtMs: Long)

    @Volatile
    private var lastScreenshot: SentScreenshot? = null

    /** On a new network, computers are dialed right away instead of after their backoff. */
    private val network = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            main.post {
                if (running) {
                    pasteGet.client.networkChanged()
                    finder.restart()
                }
            }
        }
    }

    override fun onCreate() {
        super.onCreate()
        createChannel()
        ServiceCompat.startForeground(
            this,
            NOTIFICATION_ID,
            notification(),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE,
        )
        finder = ComputerFinder(this, pasteGet.client)
        screenshots = ScreenshotWatcher(this, ::sendScreenshot)
        running = true
        pasteGet.client.start(listener)
        // The notification says how copies go: it follows the automatic copy's modes.
        val autoCopy = pasteGet.autoCopy
        scope.launch {
            combine(autoCopy.builtIn.state, autoCopy.shizuku.state, autoCopy.logs.state) { _, _, _ -> }
                .collect { if (running) updateNotification() }
        }
        getSystemService(ConnectivityManager::class.java).registerDefaultNetworkCallback(network)
    }

    /** Also called again by the app after pairing or a settings change. */
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // A pairing or a setting may have changed what it says.
        updateNotification()
        if (intent?.action == ACTION_RETRY) {
            // As on a new network: dial now, and search again.
            pasteGet.client.networkChanged()
            finder.restart()
        }
        updateFinder()
        updateScreenshots()
        updateAutoCopy()
        return START_STICKY
    }

    override fun onDestroy() {
        running = false
        scope.cancel()
        getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(network)
        finder.setActive(false)
        screenshots.stop()
        pasteGet.autoCopy.stop()
        main.removeCallbacksAndMessages(null)
        pasteGet.client.stop()
        pasteGet.clearConnected()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun notification(): Notification {
        val connected = pasteGet.connected.value
        val paired = pasteGet.client.pairedComputers()
        val copies = if (Preferences.sendCopies(this)) {
            val autoCopy = pasteGet.autoCopy
            copiesHealth(
                on = true,
                builtIn = autoCopy.builtIn.state.value,
                shizuku = autoCopy.shizuku.state.value,
                logs = autoCopy.logs.state.value,
                fallback = FallbackAccess.of(this),
                canNotify = BuiltInShell.canNotify(this),
            )
        } else {
            null
        }
        return ConnectionNotification.build(
            this,
            CHANNEL_ID,
            paired.map { it.name },
            paired.filter { it.deviceId in connected }.map { it.name },
            copies,
        )
    }

    /** "HP-AIO (Windows) unpaired this phone": the computer did it, so the user should know. */
    private fun showUnpaired(computer: PairedComputer) {
        val notifications = getSystemService(NotificationManager::class.java)
        notifications.createNotificationChannel(
            NotificationChannel(
                UNPAIRED_CHANNEL_ID,
                getString(R.string.unpaired_channel),
                NotificationManager.IMPORTANCE_DEFAULT,
            ),
        )
        val openApp = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val notification = NotificationCompat.Builder(this, UNPAIRED_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_tile)
            .setContentTitle(getString(R.string.unpaired_title, computer.name))
            .setContentText(getString(R.string.unpaired_text))
            .setContentIntent(openApp)
            .setAutoCancel(true)
            .build()
        // One per computer.
        notifications.notify(UNPAIRED_NOTIFICATION_TAG, computer.deviceId.hashCode(), notification)
    }

    /** Searches the network only while a paired computer is missing. */
    private fun updateFinder() {
        val connected = pasteGet.connected.value
        finder.setActive(pasteGet.client.pairedComputers().any { it.deviceId !in connected })
    }

    private fun updateScreenshots() {
        if (Preferences.sendScreenshots(this) && PhotoAccess.of(this) == PhotoAccess.FULL) {
            screenshots.start()
        } else {
            screenshots.stop()
        }
    }

    private fun updateAutoCopy() {
        if (Preferences.sendCopies(this)) pasteGet.autoCopy.start() else pasteGet.autoCopy.stop()
    }

    /** Runs on a background thread. Never logs contents: size only. */
    private fun sendScreenshot(image: ByteArray, announcedAtMs: Long) {
        val sendable = Sender.sendable(image)
        lastScreenshot = SentScreenshot(sendable.size.toLong(), announcedAtMs)
        val sentTo = pasteGet.client.sendImage(sendable).toInt()
        pasteGet.activity.sent(ActivityKind.SCREENSHOT, sendable.size, sentTo, automatic = true)
        val readMs = SystemClock.elapsedRealtime() - announcedAtMs
        Log.i(TAG, "screenshot of ${sendable.size} bytes read in $readMs ms, sent to $sentTo computers")
    }

    /** The notification and the tile both show who is connected. */
    private fun updateNotification() {
        getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, notification())
        TileService.requestListeningState(this, ComponentName(this, ClipboardTileService::class.java))
    }

    private fun createChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID,
            getString(R.string.notification_channel),
            NotificationManager.IMPORTANCE_LOW,
        )
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    companion object {
        private const val TAG = "paste-get"
        private const val CHANNEL_ID = "connection"
        private const val UNPAIRED_CHANNEL_ID = "unpaired"
        private const val UNPAIRED_NOTIFICATION_TAG = "unpaired"
        private const val NOTIFICATION_ID = 1
        private const val ACTION_RETRY = "io.github.nexoryx.pasteget.RETRY"

        fun start(context: Context) {
            ContextCompat.startForegroundService(context, Intent(context, SyncService::class.java))
        }

        /** Dials every paired computer now, rather than after its backoff, and searches the network. */
        fun retry(context: Context) {
            ContextCompat.startForegroundService(context, Intent(context, SyncService::class.java).setAction(ACTION_RETRY))
        }
    }
}
