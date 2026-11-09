// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.app.Application
import android.content.ClipData
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.nexoryx.pasteget.AccessNotices
import io.github.nexoryx.pasteget.CodeAccess
import io.github.nexoryx.pasteget.FallbackAccess
import io.github.nexoryx.pasteget.OneTimeCodes
import io.github.nexoryx.pasteget.PhotoAccess
import io.github.nexoryx.pasteget.Preferences
import io.github.nexoryx.pasteget.R
import io.github.nexoryx.pasteget.Sender
import io.github.nexoryx.pasteget.SyncService
import io.github.nexoryx.pasteget.pasteGet
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.pasteget_ffi.PairedComputer
import uniffi.pasteget_ffi.PastegetException

/**
 * The home screen's state, kept while the screen rotates. What may block (the Rust client) is
 * read on [io]; the rest is read again each time the screen comes back ([refresh]), since the
 * user may have changed it in the system settings meanwhile.
 */
class HomeViewModel(
    private val app: Application,
    private val computers: Computers,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    /** `null` until read for the first time. */
    var paired by mutableStateOf<List<PairedComputer>?>(null)
        private set
    var message by mutableStateOf<String?>(null)
        private set
    var screenshots by mutableStateOf(ScreenshotSetting(wanted = false, access = PhotoAccess.NONE))
        private set
    var photoRequestDenied by mutableStateOf(false)
        private set
    var copiesOn by mutableStateOf(false)
        private set
    var fallback by mutableStateOf(FallbackAccess(readLogs = false, overlay = false))
        private set
    var notices by mutableStateOf(AccessNotices(supported = false, hidden = null, canChange = false))
        private set
    var codesOn by mutableStateOf(false)
        private set
    var codeAccess by mutableStateOf(CodeAccess(listening = false, seesCodes = null))
        private set

    /** On the main thread, when the screen shows and whenever it comes back. */
    fun refresh() {
        // A computer may have unpaired this phone meanwhile.
        refreshComputers()
        refreshScreenshots()
        copiesOn = Preferences.sendCopies(app)
        fallback = FallbackAccess.of(app)
        notices = AccessNotices.of(app)
        codesOn = Preferences.sendCodes(app)
        codeAccess = OneTimeCodes.access(app)
    }

    private fun refreshComputers() {
        viewModelScope.launch { paired = withContext(io) { computers.paired() } }
    }

    /** The built-in mode grants the permission that lets the app hide the notices. */
    fun refreshNotices() {
        notices = AccessNotices.of(app)
    }

    fun showMessage(text: String) {
        message = text
    }

    fun pair(code: String) {
        message = app.getString(R.string.pairing)
        viewModelScope.launch {
            message = try {
                val computer = withContext(io) { computers.pair(code) }
                paired = withContext(io) { computers.paired() }
                SyncService.start(app)
                app.getString(R.string.paired_with, computer.name)
            } catch (e: PastegetException) {
                e.message
            }
        }
    }

    fun forget(computer: PairedComputer) {
        viewModelScope.launch {
            try {
                withContext(io) { computers.forget(computer.deviceId) }
            } catch (e: PastegetException) {
                message = e.message
            }
            paired = withContext(io) { computers.paired() }
        }
    }

    /** [computer] unpaired this phone. */
    fun unpairedBy(computer: PairedComputer) {
        refreshComputers()
        message = app.getString(R.string.unpaired_title, computer.name)
    }

    /** The user answered the photo permission request. */
    fun photoAccessAnswered() {
        val granted = PhotoAccess.of(app) == PhotoAccess.FULL
        photoRequestDenied = !granted
        setSendScreenshots(granted)
    }

    fun setSendScreenshots(on: Boolean) {
        Preferences.setSendScreenshots(app, on)
        refreshScreenshots()
        // The service picks up the change.
        SyncService.start(app)
    }

    private fun refreshScreenshots() {
        screenshots = ScreenshotSetting(Preferences.sendScreenshots(app), PhotoAccess.of(app))
    }

    fun setSendCopies(on: Boolean) {
        Preferences.setSendCopies(app, on)
        copiesOn = on
        // The service picks up the change.
        SyncService.start(app)
    }

    fun setSendCodes(on: Boolean) {
        Preferences.setSendCodes(app, on)
        codesOn = on
        codeAccess = OneTimeCodes.access(app)
    }

    /** Android may restart the app to apply the log permission: the screen then comes back fresh. */
    fun grantFallback() {
        message = app.getString(R.string.fallback_granting)
        app.pasteGet.autoCopy.shizuku.grantFallback { failed ->
            viewModelScope.launch {
                fallback = FallbackAccess.of(app)
                message = failed?.let { app.getString(R.string.fallback_failed, it) } ?: app.getString(R.string.fallback_granted)
            }
        }
    }

    /**
     * Through the shell user: notification access, and on Android 15+ seeing the codes. Returns
     * `false` if no shell runs, for the caller to open the settings instead.
     */
    fun allowCodes(): Boolean {
        val shell = app.pasteGet.autoCopy.shell() ?: return false
        message = app.getString(R.string.codes_allowing)
        viewModelScope.launch {
            val failed = withContext(io) {
                try {
                    shell.grantCodeAccess().ifEmpty { null }
                } catch (e: Exception) {
                    e.message ?: e.javaClass.simpleName
                }
            }
            codeAccess = OneTimeCodes.access(app)
            message = failed?.let { app.getString(R.string.codes_allow_failed, it) } ?: app.getString(R.string.codes_allowed)
        }
        return true
    }

    /** Only when the user asks: it applies to every app. */
    fun hideNotices(hide: Boolean) {
        if (!AccessNotices.set(app, hide)) message = app.getString(R.string.notices_failed)
        notices = AccessNotices.of(app)
    }

    /** [clip] was read by the activity, which has focus. */
    fun sendClipboard(clip: ClipData?) {
        viewModelScope.launch { message = Sender.send(app, clip) }
    }
}
