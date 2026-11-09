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
import io.github.nexoryx.pasteget.Appearance
import io.github.nexoryx.pasteget.BuiltInShell
import io.github.nexoryx.pasteget.CodeAccess
import io.github.nexoryx.pasteget.FallbackAccess
import io.github.nexoryx.pasteget.KeepAlive
import io.github.nexoryx.pasteget.OneTimeCodes
import io.github.nexoryx.pasteget.PairingCode
import io.github.nexoryx.pasteget.PhotoAccess
import io.github.nexoryx.pasteget.Preferences
import io.github.nexoryx.pasteget.R
import io.github.nexoryx.pasteget.Sender
import io.github.nexoryx.pasteget.SetUpChecks
import io.github.nexoryx.pasteget.SyncService
import io.github.nexoryx.pasteget.pasteGet
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.pasteget_ffi.PairedComputer
import uniffi.pasteget_ffi.PastegetException

/**
 * The app's state, shared by its screens and kept while the activity rotates. What may block (the Rust client) is
 * read on [io]; the rest is read again each time the screen comes back ([refresh]), since the
 * user may have changed it in the system settings meanwhile.
 */
class AppViewModel(
    private val app: Application,
    private val computers: Computers,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    /** `null` until read for the first time. */
    var paired by mutableStateOf<List<PairedComputer>?>(null)
        private set
    var message by mutableStateOf<String?>(null)
        private set
    var pairing by mutableStateOf<PairingState>(PairingState.Idle)
        private set

    /** The computer that unpaired this phone, for the status line once none is left. */
    var unpaired by mutableStateOf<PairedComputer?>(null)
        private set

    /** `null` until read. */
    var thisPhone by mutableStateOf<ThisPhone?>(null)
        private set

    /** The computers are being dialed again ([refreshConnections]). */
    var refreshing by mutableStateOf(false)
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

    /** The built-in mode's pairing notification can show. */
    var canNotify by mutableStateOf(true)
        private set

    /** The built-in mode is paired with wireless debugging. */
    var builtInSetUp by mutableStateOf(false)
        private set

    /** What the built-in mode's set-up needs from the phone. */
    var setUpChecks by mutableStateOf(SetUpChecks())
        private set

    /** Battery optimisation and notifications, as Android says. */
    var keepAlive by mutableStateOf(KeepAlive(battery = true, notifications = true))
        private set
    var autostartTicked by mutableStateOf(Preferences.autostartTicked(app))
        private set
    var batterySaverTicked by mutableStateOf(Preferences.batterySaverTicked(app))
        private set

    /** Read once: only Settings changes them. */
    var appearance by mutableStateOf(Preferences.appearance(app))
        private set
    var wallpaperColors by mutableStateOf(Preferences.wallpaperColors(app))
        private set

    init {
        // Taken from the app as it arrives, so the next screen doesn't say it again.
        viewModelScope.launch {
            app.pasteGet.unpaired.filterNotNull().collect { computer ->
                app.pasteGet.clearUnpaired(computer)
                unpairedBy(computer)
            }
        }
    }

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
        canNotify = BuiltInShell.canNotify(app)
        builtInSetUp = app.pasteGet.autoCopy.builtIn.isSetUp
        refreshChecks()
        keepAlive = KeepAlive.of(app)
    }

    /** The user ticked (or unticked) Xiaomi's Autostart, which no API reads. */
    fun tickAutostart(ticked: Boolean) {
        Preferences.setAutostartTicked(app, ticked)
        autostartTicked = ticked
    }

    /** The same for Xiaomi's own battery saver. */
    fun tickBatterySaver(ticked: Boolean) {
        Preferences.setBatterySaverTicked(app, ticked)
        batterySaverTicked = ticked
    }

    /**
     * Wi-Fi, Developer options or Wireless debugging changed: also while paste-get shares the
     * screen with Settings, where it doesn't come back to the front.
     */
    fun refreshChecks() {
        setUpChecks = SetUpChecks.of(app)
    }

    private fun refreshComputers() {
        viewModelScope.launch {
            paired = withContext(io) { computers.paired() }
            if (thisPhone == null) thisPhone = withContext(io) { computers.thisPhone() }
        }
    }

    /**
     * Pull to refresh: dials every computer now and searches the network again. The list follows
     * the connections as they come; the spinner shows for a moment so the pull is seen to work.
     */
    fun refreshConnections() {
        if (refreshing) return
        refreshing = true
        viewModelScope.launch {
            withContext(io) { computers.retry() }
            paired = withContext(io) { computers.paired() }
            delay(REFRESH_MS)
            refreshing = false
        }
    }

    fun chooseAppearance(chosen: Appearance) {
        Preferences.setAppearance(app, chosen)
        appearance = chosen
    }

    fun chooseWallpaperColors(on: Boolean) {
        Preferences.setWallpaperColors(app, on)
        wallpaperColors = on
    }

    /** Shows [text] as a Snackbar, for results the activity gets (adding the tile). */
    fun say(text: String) {
        message = text
    }

    /** [shown] was on screen and went away. A newer message stays, to be shown next. */
    fun messageShown(shown: String) {
        if (message == shown) message = null
    }

    /** The user closed "<PC> unpaired this phone". */
    fun dismissUnpaired() {
        unpaired = null
    }

    /** The built-in mode changed: it may be set up now, and it grants what hides the notices. */
    fun builtInChanged() {
        notices = AccessNotices.of(app)
        builtInSetUp = app.pasteGet.autoCopy.builtIn.isSetUp
    }

    /**
     * A scanned code or a `pasteget://` link, which any web page or message can open: shown for
     * the user to confirm ([confirmPairing]), never paired right away.
     */
    fun offerPairing(scanned: String) {
        if (pairing == PairingState.Busy) return
        val code = PairingCode.extract(scanned)
        if (code == null) {
            message = app.getString(R.string.not_a_pairing_code)
            return
        }
        val invite = PairingCode.describe(code)
        pairing = if (invite == null) PairingState.Failed(unreadableCode()) else PairingState.Confirm(code, invite)
    }

    /** The user said yes to the code shown. */
    fun confirmPairing() {
        val confirmed = pairing as? PairingState.Confirm ?: return
        pairing = PairingState.Busy
        message = null
        viewModelScope.launch {
            try {
                val computer = withContext(io) { computers.pair(confirmed.code) }
                paired = withContext(io) { computers.paired() }
                SyncService.start(app)
                unpaired = null
                pairing = PairingState.Idle
                message = app.getString(R.string.paired_with, computer.name)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                // Also an unexpected error from the native library: "Pairing…" must not stay.
                pairing = PairingState.Failed(pairingError(e))
            }
        }
    }

    /** No to the code shown, or closing the error. */
    fun cancelPairing() {
        if (pairing != PairingState.Busy) pairing = PairingState.Idle
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

    /**
     * [computer] unpaired this phone. With none left the status line says so; otherwise the
     * message does.
     */
    private fun unpairedBy(computer: PairedComputer) {
        unpaired = computer
        viewModelScope.launch {
            val left = withContext(io) { computers.paired() }
            paired = left
            if (left.isNotEmpty()) message = app.getString(R.string.unpaired_title, computer.name)
        }
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

    private companion object {
        const val REFRESH_MS = 1_500L
    }
}
