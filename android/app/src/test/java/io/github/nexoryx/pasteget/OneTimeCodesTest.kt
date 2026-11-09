// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.app.Application
import androidx.core.app.NotificationCompat
import androidx.core.app.Person
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** v0.2: what the 2FA codes feature reads in a notification, and what it sends once. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29, 35])
class OneTimeCodesTest {
    private val app: Application = RuntimeEnvironment.getApplication()

    private val bank = Person.Builder().setName("BANK").build()
    private val me = Person.Builder().setName("me").build()

    /** An SMS conversation as Messages posts it: the whole thread, newest last. */
    private fun conversation(vararg messages: Pair<String, Long>, from: Person = bank) = NotificationCompat.Builder(app, "sms")
        .setSmallIcon(android.R.drawable.ic_dialog_info)
        .setStyle(
            NotificationCompat.MessagingStyle(me).also { style ->
                messages.forEach { (text, at) -> style.addMessage(text, at, from) }
            },
        )
        .build()

    @Test
    fun aPlainNotificationIsReadWhole() {
        val plain = NotificationCompat.Builder(app, "sms")
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("Google")
            .setContentText("G-482913 is your Google verification code.")
            .build()
        assertEquals(
            "Google\nG-482913 is your Google verification code.",
            OneTimeCodes.textOf(plain, "google", Conversations(), NOW),
        )
    }

    @Test
    fun inAConversationOnlyTheNewestMessageCounts() {
        val seen = Conversations()
        val code = conversation("old message" to NOW - 60_000, "Votre code : 584921" to NOW - 30_000)
        assertEquals("Votre code : 584921", OneTimeCodes.textOf(code, "sms", seen, NOW))

        // The same conversation posted again (read, or the app refreshed it): nothing new.
        assertEquals("", OneTimeCodes.textOf(code, "sms", seen, NOW + 1_000))

        // A reply comes in: the conversation still holds the code, but only the reply is read.
        val reply = conversation("Votre code : 584921" to NOW - 30_000, "Thanks!" to NOW + 6 * 60_000L)
        assertEquals("Thanks!", OneTimeCodes.textOf(reply, "sms", seen, NOW + 6 * 60_000L))
    }

    @Test
    fun aMessageIsNewInItsOwnConversationOnly() {
        val seen = Conversations()
        val code = conversation("Code 111222" to NOW)
        assertEquals("Code 111222", OneTimeCodes.textOf(code, "bank", seen, NOW))
        assertEquals("Code 111222", OneTimeCodes.textOf(code, "other-bank", seen, NOW))
    }

    @Test
    fun withoutTimesADifferentTextIsANewMessage() {
        val seen = Conversations()
        assertEquals("first", OneTimeCodes.textOf(conversation("first" to 0), "sms", seen, NOW))
        assertEquals("", OneTimeCodes.textOf(conversation("first" to 0), "sms", seen, NOW))
        assertEquals("code 482913", OneTimeCodes.textOf(conversation("first" to 0, "code 482913" to 0), "sms", seen, NOW))
    }

    @Test
    fun anOldMessageSeenForTheFirstTimeIsStale() {
        // The app restarted and the SMS app posts the conversation again, ending with a code
        // from an hour ago.
        val old = conversation("Votre code : 584921" to NOW - 60 * 60_000L)
        assertEquals("", OneTimeCodes.textOf(old, "sms", Conversations(), NOW))
    }

    @Test
    fun aCodeIsSentOnceEvenIfTheNotificationComesBack() {
        val recent = RecentCodes(forMillis = 1_000)

        assertTrue(recent.isNew("482913", now = 0))
        assertFalse(recent.isNew("482913", now = 500))
        assertTrue(recent.isNew("111111", now = 600))
        // Long after, the same digits are a new code.
        assertTrue(recent.isNew("482913", now = 2_000))
    }

    private companion object {
        /** A wall clock time, in milliseconds. */
        const val NOW = 1_800_000_000_000L
    }
}
