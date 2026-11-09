// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.app.AppOpsManager
import android.app.Notification
import android.content.ComponentName
import android.content.Context
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.os.Build
import android.os.Process
import android.os.SystemClock
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.os.BundleCompat
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.pasteget_ffi.findOneTimeCode
import java.util.concurrent.Executors

/**
 * One-time codes (2FA) from the phone's notifications go to the computers' clipboards: an SMS
 * or an app shows "G-482913 is your verification code", and Ctrl+V on the PC pastes 482913.
 *
 * Off until the user turns it on, and it needs notification access. The built-in mode or Shizuku
 * grants that in one tap ([IShellClipboard.grantCodeAccess]), with the permission Android 15
 * added to see codes at all: without it, Android shows notification listeners "Sensitive
 * notification content hidden" instead. Otherwise the user allows access in Settings.
 *
 * Only the code leaves the phone, marked as a secret: computers keep it out of history. Neither
 * the code nor the notification's text is ever logged.
 */
object OneTimeCodes {
    private const val TAG = "paste-get"

    /** `AppOpsManager.OPSTR_RECEIVE_SENSITIVE_NOTIFICATIONS`, hidden from the SDK. */
    private const val SENSITIVE_OP = "android:receive_sensitive_notifications"

    /** `Notification.EXTRA_BUILDER_APPLICATION_INFO`, hidden: the posting app, set by Android. */
    private const val EXTRA_APP_INFO = "android.appInfo"

    private val lastState = MutableStateFlow<LastCode?>(null)

    /** The last code sent, for the home screen. */
    val last: StateFlow<LastCode?> = lastState.asStateFlow()

    private val recent = RecentCodes()
    private val conversations = Conversations()
    private val worker = Executors.newSingleThreadExecutor()

    /** On the listener's thread. Returns at once: the work happens on a thread of its own. */
    fun posted(context: Context, notification: StatusBarNotification) {
        if (!Preferences.sendCodes(context)) return
        if (notification.packageName == context.packageName || notification.isOngoing) return
        val text = textOf(notification.notification, notification.key, conversations)
        if (text.isEmpty()) return
        val app = context.applicationContext
        worker.execute {
            val code = findOneTimeCode(text) ?: return@execute
            if (!recent.isNew(code, SystemClock.elapsedRealtime())) return@execute
            val name = appName(app, notification)
            val sentTo = app.pasteGet.client.sendOneTimeCode(code, name).toInt()
            // The app that sent it, never the code.
            Log.i(TAG, "one-time code from ${notification.packageName} sent to $sentTo computers")
            lastState.value = LastCode(System.currentTimeMillis(), name, sentTo)
        }
    }

    /**
     * Where to look for a code: a notification's title and text. In a conversation
     * (MessagingStyle, as SMS apps post), only its newest message, and only the first time it is
     * seen: the app posts the whole conversation again for each new message or reply, and an old
     * code must not go out again. [key] names the notification, [now] is the wall clock.
     */
    fun textOf(notification: Notification, key: String, seen: Conversations, now: Long = System.currentTimeMillis()): String {
        val extras = notification.extras ?: return ""
        val messages = NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(notification)?.messages
        if (!messages.isNullOrEmpty()) {
            // Newest last, as Android shows them.
            val newest = messages.last()
            val text = newest.text?.toString() ?: return ""
            return if (seen.isNew(key, newest.timestamp, text, now)) text else ""
        }
        return listOfNotNull(
            extras.getCharSequence(Notification.EXTRA_TITLE),
            extras.getCharSequence(Notification.EXTRA_TEXT),
            extras.getCharSequence(Notification.EXTRA_BIG_TEXT),
        ).joinToString("\n")
    }

    /** "Messages", or the package name if Android won't say. */
    private fun appName(context: Context, notification: StatusBarNotification): String {
        val packageManager = context.packageManager
        val info = try {
            packageManager.getApplicationInfo(notification.packageName, 0)
        } catch (e: PackageManager.NameNotFoundException) {
            // Not visible to this app (Android 11+): the notification carries it.
            BundleCompat.getParcelable(notification.notification.extras, EXTRA_APP_INFO, ApplicationInfo::class.java)
        }
        return info?.let { runCatching { packageManager.getApplicationLabel(it).toString() }.getOrNull() }
            ?: notification.packageName
    }

    /** What sending codes needs, and what it has now. */
    fun access(context: Context): CodeAccess {
        val listening = context.packageName in NotificationManagerCompat.getEnabledListenerPackages(context)
        val sensitive = if (Build.VERSION.SDK_INT < Build.VERSION_CODES.VANILLA_ICE_CREAM) {
            true
        } else {
            try {
                context.getSystemService(AppOpsManager::class.java)
                    .unsafeCheckOpNoThrow(SENSITIVE_OP, Process.myUid(), context.packageName) == AppOpsManager.MODE_ALLOWED
            } catch (e: IllegalArgumentException) {
                // An Android that names it differently: can't tell.
                Log.w(TAG, "can't check $SENSITIVE_OP: $e")
                null
            }
        }
        return CodeAccess(listening, sensitive)
    }

    /** The listener, as Settings and `cmd notification` name it. */
    fun listener(context: Context) = ComponentName(context, CodeListener::class.java)
}

/** Notification access, and on Android 15+ the permission to see codes (`null`: unknown). */
data class CodeAccess(val listening: Boolean, val seesCodes: Boolean?) {
    val ready: Boolean get() = listening && seesCodes != false
}

/** A code sent at [atMillis] (wall clock), from the app [app], to [sentTo] computers. */
data class LastCode(val atMillis: Long, val app: String, val sentTo: Int)

/**
 * The codes sent in the last [forMillis]: an SMS app posts a notification again when the
 * conversation changes, and the code must go once. Kept in memory only.
 */
class RecentCodes(private val forMillis: Long = FIVE_MINUTES_MS) {
    private val sent = ArrayDeque<Pair<String, Long>>()

    /** Whether [code], seen at [now] (a monotonic clock), wasn't sent lately. Remembers it. */
    @Synchronized
    fun isNew(code: String, now: Long): Boolean {
        while (sent.isNotEmpty() && now - sent.first().second > forMillis) sent.removeFirst()
        if (sent.any { it.first == code }) return false
        sent.addLast(code to now)
        if (sent.size > KEPT) sent.removeFirst()
        return true
    }

    private companion object {
        const val FIVE_MINUTES_MS = 5 * 60_000L
        const val KEPT = 16
    }
}

/**
 * The newest message seen in each conversation, by notification key, so that each message is
 * looked at once however often its conversation is posted again. In memory only, and only a
 * hash of the text.
 */
class Conversations {
    private val newest = object : LinkedHashMap<String, Pair<Long, Int>>(16, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, Pair<Long, Int>>) = size > KEPT
    }

    /**
     * Whether the message [text] sent at [timestamp] (wall clock, 0 if the app gave none) is new
     * in the conversation [key]. Remembers it.
     */
    @Synchronized
    fun isNew(key: String, timestamp: Long, text: String, now: Long): Boolean {
        // A conversation seen for the first time (after a restart of the app, say) may end with
        // an old message: a code that old is stale.
        if (timestamp > 0 && now - timestamp > MAX_AGE_MS) return false
        val seen = newest[key]
        // Some apps give every message the same time: then a different text is a new message.
        val new = seen == null || timestamp > seen.first || (timestamp == seen.first && text.hashCode() != seen.second)
        if (new) newest[key] = timestamp to text.hashCode()
        return new
    }

    private companion object {
        const val KEPT = 64
        const val MAX_AGE_MS = 10 * 60_000L
    }
}

/** Android hands it every notification once the user allows notification access. */
class CodeListener : NotificationListenerService() {
    override fun onNotificationPosted(notification: StatusBarNotification) {
        OneTimeCodes.posted(this, notification)
    }
}
