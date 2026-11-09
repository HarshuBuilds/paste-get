// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.core.content.ContextCompat
import androidx.core.net.toUri
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.navigation3.runtime.rememberNavBackStack
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import io.github.nexoryx.pasteget.ui.AppActions
import io.github.nexoryx.pasteget.ui.AppState
import io.github.nexoryx.pasteget.ui.AppViewModel
import io.github.nexoryx.pasteget.ui.CopiesActions
import io.github.nexoryx.pasteget.ui.CopiesState
import io.github.nexoryx.pasteget.ui.CodesActions
import io.github.nexoryx.pasteget.ui.CodesCardState
import io.github.nexoryx.pasteget.ui.NativeComputers
import io.github.nexoryx.pasteget.ui.PasteGetUi
import io.github.nexoryx.pasteget.ui.Route
import io.github.nexoryx.pasteget.ui.appStatus
import io.github.nexoryx.pasteget.ui.codesHealth
import io.github.nexoryx.pasteget.ui.copiesHealth
import io.github.nexoryx.pasteget.ui.screenshotsHealth
import io.github.nexoryx.pasteget.ui.shellReady
import io.github.nexoryx.pasteget.ui.theme.PasteGetTheme

/**
 * The app's only activity: its screens ([PasteGetUi]) draw [AppViewModel]'s state; the activity
 * asks for permissions and opens the system settings and other apps.
 */
class MainActivity : ComponentActivity() {
    private val model: AppViewModel by viewModels {
        viewModelFactory { initializer { AppViewModel(application, NativeComputers(pasteGet)) } }
    }

    private val photoPermission =
        registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
            model.photoAccessAnswered()
        }

    private val scanner = registerForActivityResult(ScanContract()) { result ->
        result.contents?.let(model::offerPairing)
    }

    private val notificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) {
            // The service runs either way; without the permission its notification is hidden.
            SyncService.start(this)
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        // Before anything is drawn: the start screen, then Theme.PasteGet. It stays until the
        // paired computers are read (off the main thread), so the screen doesn't first say there
        // are none.
        installSplashScreen().setKeepOnScreenCondition { model.paired == null }
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            PasteGetTheme { App() }
        }
        startSyncService()
        // Not again when the activity is recreated (a rotation): its intent is still the link.
        if (savedInstanceState == null) handlePairingLink(intent)
    }

    @Composable
    private fun App() {
        val connected by pasteGet.connected.collectAsStateWithLifecycle()
        val activity by pasteGet.activity.entries.collectAsStateWithLifecycle()
        val builtIn by pasteGet.autoCopy.builtIn.state.collectAsStateWithLifecycle()
        val shizuku by pasteGet.autoCopy.shizuku.state.collectAsStateWithLifecycle()
        val logs by pasteGet.autoCopy.logs.state.collectAsStateWithLifecycle()
        val lastCode by OneTimeCodes.last.collectAsStateWithLifecycle()
        // Set up, and granting what hides the notices, as it changes.
        LaunchedEffect(builtIn.status) { model.builtInChanged() }

        val computers = model.paired.orEmpty()
        val shellReady = shellReady(builtIn, shizuku)
        val status = appStatus(
            computers,
            connected,
            model.unpaired,
            listOf(
                copiesHealth(model.copiesOn, builtIn, shizuku, logs, model.fallback, model.canNotify),
                screenshotsHealth(model.screenshots.wanted, model.screenshots.access),
                codesHealth(model.codesOn, model.codeAccess, shellReady),
            ),
        )
        val actions = remember { appActions() }
        // Saved with the activity, so the user comes back to the same screen.
        val backStack = rememberNavBackStack(Route.Home)
        PasteGetUi(
            state = AppState(
                status = status,
                computers = computers,
                connected = connected,
                message = model.message,
                pairing = model.pairing,
                activity = activity,
                screenshots = model.screenshots,
                photoRequestDenied = model.photoRequestDenied,
                autoCopy = CopiesState(
                    model.copiesOn,
                    builtIn,
                    shizuku,
                    logs,
                    model.fallback,
                    model.notices,
                    lastCopy = activity.firstOrNull { it.sent && it.automatic && it.kind == ActivityKind.TEXT },
                    canNotify = model.canNotify,
                    setUp = model.builtInSetUp,
                ),
                codes = CodesCardState(model.codesOn, model.codeAccess, shellReady, lastCode),
                thisPhone = model.thisPhone,
                unpairedBy = model.unpaired,
                refreshing = model.refreshing,
            ),
            actions = actions,
            backStack = backStack,
        )
    }

    private fun appActions() = AppActions(
        onPair = ::scan,
        onConfirmPairing = model::confirmPairing,
        onCancelPairing = model::cancelPairing,
        onSendClipboard = ::sendClipboard,
        onForget = model::forget,
        onDismissUnpaired = model::dismissUnpaired,
        onRefresh = model::refreshConnections,
        onMessageShown = model::messageShown,
        onScreenshotsChange = ::toggleScreenshots,
        onOpenAppSettings = ::openAppSettings,
        autoCopy = CopiesActions(
            onChange = model::setSendCopies,
            onSetUpBuiltIn = ::setUpBuiltIn,
            onOpenWirelessDebugging = ::openWirelessDebugging,
            onRetryBuiltIn = pasteGet.autoCopy.builtIn::retry,
            onResetBuiltIn = pasteGet.autoCopy.builtIn::reset,
            onAllowShizuku = pasteGet.autoCopy.shizuku::requestPermission,
            onOpenShizuku = ::openShizuku,
            onGetShizuku = ::getShizuku,
            onGrantFallback = model::grantFallback,
            onAllowOverlay = ::openOverlaySettings,
            onRetryLogs = pasteGet.autoCopy.logs::retry,
            onHideNotices = model::hideNotices,
            onOpenPrivacySettings = ::openPrivacySettings,
            onOpenNotificationSettings = ::openNotificationSettings,
            onOpenWifi = ::openWifiSettings,
        ),
        codes = CodesActions(
            onChange = model::setSendCodes,
            onAllow = ::allowCodes,
            onOpenSettings = ::openNotificationAccess,
        ),
    )

    /** Photo access, notification access and more may have changed in the system settings meanwhile. */
    override fun onResume() {
        super.onResume()
        model.refresh()
        // Android 13+ asks for log access only while the app is in front (mode B).
        pasteGet.autoCopy.setInFront(true)
    }

    override fun onPause() {
        pasteGet.autoCopy.setInFront(false)
        super.onPause()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handlePairingLink(intent)
    }

    /**
     * A pairing QR code scanned with the system camera opens the app with a pasteget:// link,
     * and so can any web page or message: the user confirms it first.
     */
    private fun handlePairingLink(intent: Intent?) {
        intent?.dataString?.let(model::offerPairing)
    }

    private fun startSyncService() {
        val needsPermission = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        if (needsPermission) {
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            SyncService.start(this)
        }
    }

    private fun scan() {
        scanner.launch(
            ScanOptions()
                .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                .setPrompt(getString(R.string.scan_prompt))
                .setBeepEnabled(false)
                .setOrientationLocked(false),
        )
    }

    private fun toggleScreenshots(on: Boolean) {
        if (on && PhotoAccess.of(this) != PhotoAccess.FULL) {
            photoPermission.launch(PhotoAccess.permissions())
        } else {
            model.setSendScreenshots(on)
        }
    }

    /** Shows the code notification, then Settings, where the user asks for a pairing code. */
    private fun setUpBuiltIn() {
        // The code is typed into the notification: without it, set-up could only fail.
        if (!BuiltInShell.canNotify(this)) return openNotificationSettings()
        pasteGet.autoCopy.builtIn.beginPairing()
        openWirelessDebugging()
    }

    /** paste-get's notification settings: the app's switch and each channel's. */
    private fun openNotificationSettings() {
        val intent = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, packageName)
        try {
            startActivity(intent)
        } catch (e: ActivityNotFoundException) {
            openAppSettings()
        }
    }

    /** Android's Wi-Fi panel, over the app. */
    private fun openWifiSettings() {
        try {
            startActivity(Intent(Settings.Panel.ACTION_WIFI))
        } catch (e: ActivityNotFoundException) {
            startActivity(Intent(Settings.ACTION_WIFI_SETTINGS))
        }
    }

    private fun openWirelessDebugging() {
        val developerOptions = Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS)
            // Scrolls to "Wireless debugging" on Pixel-like settings, ignored elsewhere.
            .putExtra(":settings:fragment_args_key", "toggle_adb_wireless")
        try {
            startActivity(developerOptions)
        } catch (e: ActivityNotFoundException) {
            // Developer options are off: "About phone" is where they're turned on.
            startActivity(Intent(Settings.ACTION_DEVICE_INFO_SETTINGS))
        }
    }

    private fun openShizuku() {
        packageManager.getLaunchIntentForPackage(SHIZUKU_PACKAGE)?.let(::startActivity)
    }

    /** One tap through the shell when one runs, otherwise Settings. */
    private fun allowCodes() {
        if (!model.allowCodes()) openNotificationAccess()
    }

    /** paste-get's own page in Notification access, where the phone has one. */
    private fun openNotificationAccess() {
        val intent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Intent(Settings.ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS)
                .putExtra(Settings.EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME, OneTimeCodes.listener(this).flattenToString())
        } else {
            Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)
        }
        try {
            startActivity(intent)
        } catch (e: ActivityNotFoundException) {
            startActivity(Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS))
        }
    }

    /** Where "Show clipboard access" is. Some vendors move it: then the main settings. */
    private fun openPrivacySettings() {
        try {
            startActivity(Intent(Settings.ACTION_PRIVACY_SETTINGS))
        } catch (e: ActivityNotFoundException) {
            startActivity(Intent(Settings.ACTION_SETTINGS))
        }
    }

    private fun openOverlaySettings() {
        startActivity(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.fromParts("package", packageName, null)))
    }

    private fun getShizuku() {
        startActivity(Intent(Intent.ACTION_VIEW, SHIZUKU_DOWNLOAD.toUri()))
    }

    private fun openAppSettings() {
        startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", packageName, null)))
    }

    /** This activity has focus, so it may read the clipboard directly. */
    private fun sendClipboard() {
        model.sendClipboard(getSystemService(ClipboardManager::class.java).primaryClip)
    }

    private companion object {
        const val SHIZUKU_PACKAGE = "moe.shizuku.privileged.api"
        const val SHIZUKU_DOWNLOAD = "https://shizuku.rikka.app/download/"
    }
}
