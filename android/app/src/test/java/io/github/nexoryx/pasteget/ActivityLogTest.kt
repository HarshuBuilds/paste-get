// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The activity log keeps the last entries, newest first, with their deliveries, and never contents. */
class ActivityLogTest {
    private var now = 1_000L
    private val log = ActivityLog(capacity = 3, clock = { now })

    @Test
    fun newestFirstAndOnlyTheLastOnes() {
        for (bytes in 1..5) {
            log.sent(ActivityKind.TEXT, bytes, sentTo = 1, automatic = true)
            now += 10
        }

        assertEquals(listOf(5, 4, 3), log.entries.value.map { it.bytes })
        assertEquals(listOf(1040L, 1030L, 1020L), log.entries.value.map { it.atMillis })
        // Each entry has an ID of its own.
        assertEquals(3, log.entries.value.map { it.id }.toSet().size)
    }

    @Test
    fun aDeliveryGoesToTheNewestMatchingEntry() {
        log.sent(ActivityKind.TEXT, 12, sentTo = 2, automatic = true)
        log.sent(ActivityKind.SCREENSHOT, 12, sentTo = 2, automatic = true)
        log.sent(ActivityKind.TEXT, 12, sentTo = 2, automatic = false)

        log.delivered("hp", "text/plain", 12, millis = 38, accepted = true)
        log.delivered("hp", "image/png", 12, millis = 91, accepted = true)
        log.delivered("arch", "text/plain", 12, millis = 45, accepted = false)

        val (byHand, screenshot, copy) = log.entries.value
        assertEquals(listOf(Delivery("hp", 38, true), Delivery("arch", 45, false)), byHand.deliveries)
        assertEquals(listOf(Delivery("hp", 91, true)), screenshot.deliveries)
        assertEquals(emptyList<Delivery>(), copy.deliveries)

        // The same computer again: the older one it hasn't acknowledged.
        log.delivered("hp", "text/plain", 12, millis = 50, accepted = true)
        assertEquals(listOf(Delivery("hp", 50, true)), log.entries.value[2].deliveries)
    }

    @Test
    fun anAcknowledgementThatBeatsItsEntryIsKept() {
        log.delivered("hp", "image/png", 900, millis = 20, accepted = true)
        now += 50
        log.sent(ActivityKind.SCREENSHOT, 900, sentTo = 1, automatic = true)

        assertEquals(listOf(Delivery("hp", 20, true)), log.entries.value.single().deliveries)
        // Used once.
        log.sent(ActivityKind.SCREENSHOT, 900, sentTo = 1, automatic = true)
        assertEquals(emptyList<Delivery>(), log.entries.value.first().deliveries)
    }

    @Test
    fun aLateAcknowledgementDoesntGoToALaterClip() {
        log.delivered("hp", "text/plain", 5, millis = 20, accepted = true)
        now += 60_000
        log.sent(ActivityKind.TEXT, 5, sentTo = 1, automatic = true)

        assertEquals(emptyList<Delivery>(), log.entries.value.single().deliveries)
    }

    @Test
    fun receivedClipsSayWhetherTheyReachedTheClipboard() {
        log.received("image/png", 4_000, sensitive = false, onClipboard = false)
        log.received("text/plain", 30, sensitive = true, onClipboard = true)

        val (text, image) = log.entries.value
        assertEquals(ActivityKind.TEXT to true, text.kind to text.onClipboard)
        assertTrue(text.sensitive && !text.sent)
        assertEquals(ActivityKind.IMAGE to false, image.kind to image.onClipboard)
        // A received clip is never matched with an acknowledgement.
        log.delivered("hp", "text/plain", 30, millis = 1, accepted = true)
        assertEquals(emptyList<Delivery>(), log.entries.value.first().deliveries)
    }

    /** Metadata only: a field added to an entry must be added here, on purpose. */
    @Test
    fun anEntryHasNoFieldForContents() {
        val fields = Activity::class.java.declaredFields.filter { !java.lang.reflect.Modifier.isStatic(it.modifiers) }
        assertEquals(
            setOf("id", "atMillis", "kind", "sent", "automatic", "sensitive", "bytes", "sentTo", "app", "onClipboard", "deliveries"),
            fields.map { it.name }.toSet(),
        )
        // Nothing that could hold a clip: no bytes, no text but the code's app name.
        assertTrue(fields.none { it.type == ByteArray::class.java || it.type == CharSequence::class.java })
        assertEquals(listOf("app"), fields.filter { it.type == String::class.java }.map { it.name })
        assertEquals(
            setOf("deviceId", "millis", "accepted"),
            Delivery::class.java.declaredFields.filter { !java.lang.reflect.Modifier.isStatic(it.modifiers) }.map { it.name }.toSet(),
        )
    }
}
