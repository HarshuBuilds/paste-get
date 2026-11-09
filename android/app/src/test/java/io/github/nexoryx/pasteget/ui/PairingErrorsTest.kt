// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import io.github.nexoryx.pasteget.R
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.pasteget_ffi.PastegetException

/** Each Rust error becomes a sentence the user can act on, in their language. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PairingErrorsTest {
    private val app: Application = RuntimeEnvironment.getApplication()

    private val errors = listOf(
        PastegetException.InvalidCode("this is not a paste-get pairing code") to R.string.pair_error_invalid,
        PastegetException.Unreachable("192.168.1.20:47290: connection refused") to R.string.pair_error_unreachable,
        PastegetException.PairingFailed("wrong secret") to R.string.pair_error_refused,
        PastegetException.Storage("No space left on device") to R.string.pair_error_storage,
        PastegetException.UnknownDevice("unknown device") to R.string.pair_error_other,
        IllegalStateException("internal error") to R.string.pair_error_other,
    )

    @Test
    fun eachErrorHasItsMessageAndKeepsTheDetails() {
        for ((exception, message) in errors) {
            assertEquals(PairingError(message, exception.message!!), pairingError(exception))
        }
    }

    @Test
    fun theDetailsNameTheErrorWhenItHasNoMessage() {
        assertEquals("IllegalStateException", pairingError(IllegalStateException()).details)
    }

    @Test
    @Config(qualifiers = "fr")
    fun inFrenchTheMessagesAreFrench() {
        val english = app.createConfigurationContext(
            android.content.res.Configuration(app.resources.configuration).apply { setLocale(java.util.Locale.ENGLISH) },
        )
        for ((_, message) in errors) {
            assertNotEquals(english.getString(message), app.getString(message))
        }
        assertEquals("L'appairage n'a pas marché", app.getString(R.string.pair_error_title))
    }
}
