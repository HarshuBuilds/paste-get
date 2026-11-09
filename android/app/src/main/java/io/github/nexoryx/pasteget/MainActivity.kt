// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.Manifest
import android.annotation.SuppressLint
import android.app.StatusBarManager
import android.app.UiModeManager
import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.database.ContentObserver
import android.graphics.Color
import android.graphics.drawable.Icon
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
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
import io.github.nexoryx.pasteget.ui.CodesActions
import io.github.nexoryx.pasteget.ui.CodesState
import io.github.nexoryx.pasteget.ui.CopiesActions
import io.github.nexoryx.pasteget.ui.CopiesState
import io.github.nexoryx.pasteget.ui.KeepAliveActions
import io.github.nexoryx.pasteget.ui.KeepAliveState
import io.github.nexoryx.pasteget.ui.NativeComputers
import io.github.nexoryx.pasteget.ui.OnboardingScreen
import io.github.nexoryx.pasteget.ui.PasteGetUi
import io.github.nexoryx.pasteget.ui.Route
import io.github.nexoryx.pasteget.ui.SettingsActions
import io.github.nexoryx.pasteget.ui.SettingsState
import io.github.nexoryx.pasteget.ui.Tab
import io.github.nexoryx.pasteget.ui.appStatus
import io.github.nexoryx.pasteget.ui.codesHealth
import io.github.nexoryx.pasteget.ui.copiesHealth
import io.github.nexoryx.pasteget.ui.screenshotsHealth
import io.github.nexoryx.pasteget.ui.selectTab
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

    /** A tab a launcher shortcut asked for, until shown. */
    private var openTab by mutableStateOf<Tab?>(null)

    /** paste-get shares the screen with another app: Settings can open beside it. */
    private var splitScreen by mutableStateOf(false)

    /**
     * Developer options and Wireless debugging, turned on in Settings: in split screen paste-get
     * stays in front meanwhile, so [onResume] doesn't tell.
     */
    private val settingsChanged = object : ContentObserver(Handler(Looper.getMainLooper())) {
        override fun onChange(selfChange: Boolean) = model.refreshChecks()
    }

    /** Wi-Fi coming and going, for the set-up's first step. On a system thread. */
    private val networkChanged = object : ConnectivityManager.NetworkCallback() {
        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
            runOnUiThread { model.refreshChecks() }
        }

        override fun onLost(network: Network) {
            runOnUiThread { model.refreshChecks() }
        }
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
            model.refresh()
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        // Before anything is drawn: the start screen, then Theme.PasteGet. It stays until the
        // paired computers are read (off the main thread), so the screen doesn't first say there
        // are none.
        installSplashScreen().setKeepOnScreenCondition { model.paired == null }
        super.onCreate(savedInstanceState)
        setContent {
            val dark = when (model.appearance) {
                Appearance.SYSTEM -> isSystemInDarkTheme()
                Appearance.LIGHT -> false
                Appearance.DARK -> true
            }
            // Status and navigation bar icons that read on the app's background.
            LaunchedEffect(dark) {
                enableEdgeToEdge(
                    statusBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT) { dark },
                    navigationBarStyle = SystemBarStyle.auto(LIGHT_SCRIM, DARK_SCRIM) { dark },
                )
            }
            PasteGetTheme(dark = dark, wallpaperColors = model.wallpaperColors) { App() }
        }
        startSyncService()
        // Not again when the activity is recreated (a rotation): its intent is still the link.
        if (savedInstanceState == null) handleIntent(intent)
        splitScreen = isInMultiWindowMode
    }

    override fun onMultiWindowModeChanged(isInMultiWindowMode: Boolean, newConfig: Configuration) {
        super.onMultiWindowModeChanged(isInMultiWindowMode, newConfig)
        splitScreen = isInMultiWindowMode
    }

    override fun onStart() {
        super.onStart()
        for (setting in listOf(SetUpChecks.ADB_WIFI_ENABLED, Settings.Global.DEVELOPMENT_SETTINGS_ENABLED)) {
            contentResolver.registerContentObserver(Settings.Global.getUriFor(setting), false, settingsChanged)
        }
        getSystemService(ConnectivityManager::class.java).registerDefaultNetworkCallback(networkChanged)
    }

    override fun onStop() {
        contentResolver.unregisterContentObserver(settingsChanged)
        getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(networkChanged)
        super.onStop()
    }

    @Composable
    private fun App() {
        val connected by pasteGet.connected.collectAsStateWithLifecycle()
        val activity by pasteGet.activity.entries.collectAsStateWithLifecycle()
        val builtIn by pasteGet.autoCopy.builtIn.state.collectAsStateWithLifecycle()
        val shizuku by pasteGet.autoCopy.shizuku.state.collectAsStateWithLifecycle()
        val logs by pasteGet.autoCopy.logs.state.collectAsStateWithLifecycle()
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
        LaunchedEffect(openTab) {
            openTab?.let { selectTab(backStack, it) }
            openTab = null
        }
        val state = AppState(
            status = status,
            computers = computers,
            computersLoaded = model.paired != null,
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
                checks = model.setUpChecks,
                vendor = VENDOR,
                splitScreen = splitScreen,
            ),
            codes = CodesState(
                model.codesOn,
                model.codeAccess,
                shellReady,
                last = activity.firstOrNull { it.kind == ActivityKind.CODE },
                codesHidden = Build.VERSION.SDK_INT >= Build.VERSION_CODES.VANILLA_ICE_CREAM,
            ),
            thisPhone = model.thisPhone,
            unpairedBy = model.unpaired,
            refreshing = model.refreshing,
            settings = SettingsState(model.appearance, model.wallpaperColors),
            keepAlive = KeepAliveState(model.keepAlive, VENDOR, model.autostartTicked, model.batterySaverTicked),
        )
        if (model.onboarding == true) {
            OnboardingScreen(state, actions, onFinish = model::finishOnboarding)
        } else {
            PasteGetUi(state, actions, backStack)
        }
    }

    private fun appActions() = AppActions(
        onPair = ::scan,
        onConfirmPairing = model::confirmPairing,
        onCancelPairing = model::cancelPairing,
        onRetryPairing = model::retryPairing,
        onSendClipboard = ::sendClipboard,
        onForget = model::forget,
        onDismissUnpaired = model::dismissUnpaired,
        onRefresh = model::refreshConnections,
        onMessageShown = model::messageShown,
        onRequestNotifications = ::requestNotifications,
        onTryIt = ::tryIt,
        onScreenshotsChange = ::toggleScreenshots,
        onOpenAppSettings = ::openAppSettings,
        autoCopy = CopiesActions(
            onChange = model::setSendCopies,
            onSetUpBuiltIn = ::setUpBuiltIn,
            onOpenWirelessDebugging = { openWirelessDebugging() },
            onOpenAboutPhone = { openAboutPhone() },
            onSubmitCode = pasteGet.autoCopy.builtIn::submitCode,
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
            onOpenAppInfo = ::openAppSettings,
        ),
        settings = SettingsActions(
            onAppearanceChange = ::setAppearance,
            onWallpaperColorsChange = model::chooseWallpaperColors,
            onOpenLanguage = ::openLanguageSettings,
            onAddTile = ::addTile,
        ),
        keepAlive = KeepAliveActions(
            onAllowBattery = ::allowBattery,
            onOpenNotificationSettings = ::openNotificationSettings,
            onOpenAutostart = { openFirst(VendorSettings.of(VENDOR).autostartPages(packageName)) },
            onOpenAppInfo = ::openAppSettings,
            onTickAutostart = model::tickAutostart,
            onTickBatterySaver = model::tickBatterySaver,
        ),
    )

    /**
     * Android's own question, "Let app always run in the background?". Where a vendor removed
     * it: the list of battery optimisations, then App info. paste-get keeps a connection to the
     * user's computers open, which is what the exemption is for.
     */
    @SuppressLint("BatteryLife")
    private fun allowBattery() {
        openFirst(
            listOf(
                Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.fromParts("package", packageName, null)),
                Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS),
                VendorSettings.appInfo(packageName),
            ),
        )
    }

    /** The first of [pages] this phone opens: vendors remove or rename settings pages. */
    private fun openFirst(pages: List<Intent>) {
        for (page in pages) {
            try {
                startActivity(page)
                return
            } catch (e: ActivityNotFoundException) {
                // Not on this phone: the next one.
            } catch (e: SecurityException) {
                // There, but not open to other apps.
            }
        }
    }

    /**
     * Also tells Android (12+), so the window behind the app and the start screen match the
     * choice from the next start, not only the screens.
     */
    private fun setAppearance(appearance: Appearance) {
        model.chooseAppearance(appearance)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val mode = when (appearance) {
                Appearance.SYSTEM -> UiModeManager.MODE_NIGHT_AUTO
                Appearance.LIGHT -> UiModeManager.MODE_NIGHT_NO
                Appearance.DARK -> UiModeManager.MODE_NIGHT_YES
            }
            getSystemService(UiModeManager::class.java).setApplicationNightMode(mode)
        }
    }

    /** Android's language page for paste-get alone (Android 13+). */
    private fun openLanguageSettings() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        try {
            startActivity(Intent(Settings.ACTION_APP_LOCALE_SETTINGS, Uri.fromParts("package", packageName, null)))
        } catch (e: ActivityNotFoundException) {
            openAppSettings()
        }
    }

    /** Android 13+ asks the user to add the Send clipboard tile, without opening the shade's editor. */
    private fun addTile() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        getSystemService(StatusBarManager::class.java).requestAddTileService(
            ComponentName(this, ClipboardTileService::class.java),
            getString(R.string.tile_label),
            Icon.createWithResource(this, R.drawable.ic_tile),
            mainExecutor,
        ) { result ->
            when (result) {
                StatusBarManager.TILE_ADD_REQUEST_RESULT_TILE_ADDED -> model.say(getString(R.string.tile_added))
                StatusBarManager.TILE_ADD_REQUEST_RESULT_TILE_ALREADY_ADDED -> model.say(getString(R.string.tile_already))
                else -> {}
            }
        }
    }

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
        handleIntent(intent)
    }

    /**
     * A launcher shortcut (Pair a computer, History), or a pairing link: a pairing QR code
     * scanned with the system camera opens the app with a pasteget:// link, and so can any web
     * page or message. The user confirms it first.
     */
    private fun handleIntent(intent: Intent?) {
        when (val request = LaunchRequest.of(intent)) {
            LaunchRequest.Pair -> scan()
            LaunchRequest.History -> openTab = Tab.HISTORY
            is LaunchRequest.Link -> model.offerPairing(request.uri)
            null -> {}
        }
    }

    /** The first run asks for notifications on its own page, once it has said why. */
    private fun startSyncService() {
        if (needsNotificationPermission() && Preferences.onboardingDone(this)) {
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            SyncService.start(this)
        }
    }

    private fun needsNotificationPermission(): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED

    /** Android's question where it still asks; their settings otherwise. */
    private fun requestNotifications() {
        if (needsNotificationPermission()) {
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            openNotificationSettings()
        }
    }

    /**
     * The first-run guide's test: [text] on the clipboard, where the automatic copy sees it as
     * any copy. Without it ([byHand]), sent the way the tile sends.
     */
    private fun tryIt(text: String, byHand: Boolean) {
        val clip = ClipData.newPlainText(getString(R.string.app_name), text)
        getSystemService(ClipboardManager::class.java).setPrimaryClip(clip)
        if (byHand) model.sendClipboard(clip)
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

    /**
     * Shows the code notification, then Settings, where the user asks for a pairing code. In
     * split screen, Settings opens in the other half and the code can be typed in paste-get.
     */
    private fun setUpBuiltIn() {
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

    /** [beside]: in the other half of the screen, as it is in split screen. */
    private fun openWirelessDebugging(beside: Boolean = isInMultiWindowMode) {
        val developerOptions = Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS)
            // Scrolls to "Wireless debugging" on Pixel-like settings, ignored elsewhere.
            .putExtra(":settings:fragment_args_key", "toggle_adb_wireless")
        try {
            startActivity(besideIf(beside, developerOptions))
        } catch (e: ActivityNotFoundException) {
            // Developer options are off: "About phone" is where they're turned on.
            openAboutPhone(beside)
        }
    }

    private fun openAboutPhone(beside: Boolean = isInMultiWindowMode) {
        startActivity(besideIf(beside, Intent(Settings.ACTION_DEVICE_INFO_SETTINGS)))
    }

    /** Settings in its own task, in the other half of the screen. */
    private fun besideIf(beside: Boolean, intent: Intent): Intent =
        if (beside) intent.addFlags(Intent.FLAG_ACTIVITY_LAUNCH_ADJACENT or Intent.FLAG_ACTIVITY_NEW_TASK) else intent

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
        startActivity(VendorSettings.appInfo(packageName))
    }

    /** This activity has focus, so it may read the clipboard directly. */
    private fun sendClipboard() {
        model.sendClipboard(getSystemService(ClipboardManager::class.java).primaryClip)
    }

    private companion object {
        const val SHIZUKU_PACKAGE = "moe.shizuku.privileged.api"
        const val SHIZUKU_DOWNLOAD = "https://shizuku.rikka.app/download/"

        /** Whose words the set-up uses: it doesn't change while the app runs. */
        val VENDOR = Vendor.current()

        // The navigation bar's scrims enableEdgeToEdge uses by default, for 3-button navigation.
        val LIGHT_SCRIM = Color.argb(0xe6, 0xFF, 0xFF, 0xFF)
        val DARK_SCRIM = Color.argb(0x80, 0x1b, 0x1b, 0x1b)
    }
}

/** What an intent that opens [MainActivity] asks for. */
sealed interface LaunchRequest {
    data object Pair : LaunchRequest

    data object History : LaunchRequest

    /** A link, maybe a pairing code. */
    data class Link(val uri: String) : LaunchRequest

    companion object {
        // The launcher shortcuts' actions (res/xml/shortcuts.xml).
        const val ACTION_PAIR = "io.github.nexoryx.pasteget.action.PAIR"
        const val ACTION_HISTORY = "io.github.nexoryx.pasteget.action.HISTORY"

        /**
         * `null` for nothing to do, and for the task's first intent given again: on Android 10 to
         * 12, Back closes the app when a shortcut or a link opened it, and reopening it from
         * recent apps brings back that intent, already handled.
         */
        fun of(intent: Intent?): LaunchRequest? {
            if (intent == null || intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY != 0) return null
            return when (intent.action) {
                ACTION_PAIR -> Pair
                ACTION_HISTORY -> History
                else -> intent.dataString?.let(::Link)
            }
        }
    }
}
