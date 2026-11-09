// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import java.util.concurrent.atomic.AtomicLong

/** What an entry of the [ActivityLog] was. */
enum class ActivityKind {
    TEXT,
    IMAGE,
    SCREENSHOT,

    /** A one-time code found in a notification. */
    CODE,
}

/**
 * Something that went to the computers or came from one: its kind, size and time, where it
 * arrived and how fast. **Never what it contained**: there is no field for it, and a test
 * checks there never is.
 */
data class Activity(
    val id: Long,
    /** Wall clock. */
    val atMillis: Long,
    val kind: ActivityKind,
    /** From the phone (`true`) or from a computer. */
    val sent: Boolean,
    /** Sent without a tap: a copy, a screenshot, a code. */
    val automatic: Boolean,
    /** Marked sensitive (a password, a code): computers keep it out of their history. */
    val sensitive: Boolean,
    val bytes: Int,
    /** Sent: the computers connected when it left. With none, it waits for them. */
    val sentTo: Int = 0,
    /** A code's app, e.g. "Messages". */
    val app: String? = null,
    /** Received: whether it made it to the phone's clipboard. */
    val onClipboard: Boolean = true,
    /** The computers that acknowledged it, in order. */
    val deliveries: List<Delivery> = emptyList(),
)

/** A computer acknowledged a clip after [millis] (from the screenshot's announcement, for those). */
data class Delivery(val deviceId: String, val millis: Long, val accepted: Boolean)

/**
 * What the phone sent and received since the app started, newest first, for the home screen's
 * Recent list and the History tab. In memory only, the last [capacity] entries, and metadata
 * only: the clips themselves are never kept. Safe to call from any thread.
 */
class ActivityLog(
    private val capacity: Int = CAPACITY,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    private val ids = AtomicLong()
    private val state = MutableStateFlow<List<Activity>>(emptyList())

    /** Newest first. */
    val entries: StateFlow<List<Activity>> = state.asStateFlow()

    /** Acknowledgements that came before their entry, matched when it's added. */
    private val early = ArrayDeque<Pair<Long, PendingDelivery>>()

    private class PendingDelivery(val image: Boolean, val bytes: Int, val delivery: Delivery)

    /** [kind] went to [sentTo] connected computers (0: it waits for them). */
    fun sent(kind: ActivityKind, bytes: Int, sentTo: Int, automatic: Boolean, sensitive: Boolean = false, app: String? = null) {
        val entry = Activity(
            id = ids.incrementAndGet(),
            atMillis = clock(),
            kind = kind,
            sent = true,
            automatic = automatic,
            sensitive = sensitive,
            bytes = bytes,
            sentTo = sentTo,
            app = app,
        )
        add(entry.copy(deliveries = takeEarly(entry)))
    }

    /** A clip from a computer, of [mimeType] and [bytes]. */
    fun received(mimeType: String, bytes: Int, sensitive: Boolean, onClipboard: Boolean) {
        val kind = if (mimeType.startsWith("image/")) ActivityKind.IMAGE else ActivityKind.TEXT
        add(
            Activity(
                id = ids.incrementAndGet(),
                atMillis = clock(),
                kind = kind,
                sent = false,
                automatic = true,
                sensitive = sensitive,
                bytes = bytes,
                onClipboard = onClipboard,
            ),
        )
    }

    /**
     * [deviceId] acknowledged a clip of [mimeType] and [bytes]: it belongs to the newest entry
     * sent like it that this computer hasn't acknowledged yet.
     */
    fun delivered(deviceId: String, mimeType: String, bytes: Int, millis: Long, accepted: Boolean) {
        val image = mimeType.startsWith("image/")
        val delivery = Delivery(deviceId, millis, accepted)
        var matched = false
        state.update { list ->
            val index = list.indexOfFirst { matches(it, image, bytes) && it.deliveries.none { d -> d.deviceId == deviceId } }
            matched = index >= 0
            if (!matched) {
                list
            } else {
                list.toMutableList().also { it[index] = it[index].copy(deliveries = it[index].deliveries + delivery) }
            }
        }
        if (!matched) {
            synchronized(early) {
                early.addLast(clock() to PendingDelivery(image, bytes, delivery))
                while (early.size > EARLY_KEPT) early.removeFirst()
            }
        }
    }

    private fun add(entry: Activity) {
        state.update { (listOf(entry) + it).take(capacity) }
    }

    /** The acknowledgements for [entry] that came first: the send and its answer raced. */
    private fun takeEarly(entry: Activity): List<Delivery> = synchronized(early) {
        val now = clock()
        early.removeAll { now - it.first > EARLY_MS }
        val mine = early.filter { matches(entry, it.second.image, it.second.bytes) }
            .distinctBy { it.second.delivery.deviceId }
        early.removeAll { it in mine }
        mine.map { it.second.delivery }
    }

    private fun matches(entry: Activity, image: Boolean, bytes: Int): Boolean {
        val isImage = entry.kind == ActivityKind.IMAGE || entry.kind == ActivityKind.SCREENSHOT
        return entry.sent && isImage == image && entry.bytes == bytes
    }

    private companion object {
        const val CAPACITY = 100
        const val EARLY_KEPT = 8
        const val EARLY_MS = 10_000L
    }
}
