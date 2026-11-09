// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteScaffold
import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.ui.NavDisplay

/**
 * The app: a navigation bar (a rail on wide screens) with Home, History and Devices, the screen
 * on top of [backStack], the pairing sheet over all of them, and [AppState.message] as a
 * Snackbar.
 */
@Composable
fun PasteGetUi(state: AppState, actions: AppActions, backStack: MutableList<NavKey>) {
    val tab = currentTab(backStack)
    val settings = { open(backStack, Route.Settings) }
    val snackbar = rememberMessages(state.message, actions.onMessageShown)
    CompositionLocalProvider(LocalSnackbarHost provides snackbar) {
        Shell(state, actions, backStack, tab, settings)
    }
    PairingFlow(state.pairing, state.computers, actions.onConfirmPairing, actions.onCancelPairing)
}

/**
 * What just happened ("Paired with HP-AIO", "Sent to 1 computer"), as a Snackbar for a few
 * seconds; then [onShown].
 */
@Composable
fun rememberMessages(message: String?, onShown: (String) -> Unit): SnackbarHostState {
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(message) {
        val shown = message ?: return@LaunchedEffect
        snackbar.showSnackbar(shown)
        onShown(shown)
    }
    return snackbar
}

@Composable
private fun Shell(state: AppState, actions: AppActions, backStack: MutableList<NavKey>, tab: Tab, settings: () -> Unit) {
    NavigationSuiteScaffold(
        navigationSuiteItems = {
            Tab.entries.forEach { item ->
                item(
                    selected = item == tab,
                    onClick = { selectTab(backStack, item) },
                    icon = { Icon(painterResource(item.icon), contentDescription = null) },
                    label = { Text(stringResource(item.label)) },
                )
            }
        },
    ) {
        NavDisplay(
            backStack = backStack,
            onBack = { back(backStack) },
            entryProvider = entryProvider {
                entry<Route.Home> {
                    HomeScreen(
                        state,
                        actions,
                        onSettings = settings,
                        onSeeAll = { selectTab(backStack, Tab.HISTORY) },
                        onOpen = { open(backStack, it) },
                    )
                }
                entry<Route.Copies> {
                    CopiesScreen(state.autoCopy, actions.autoCopy, onBack = { back(backStack) }, onOpenSetUp = { open(backStack, Route.CopiesSetUp) })
                }
                entry<Route.CopiesSetUp> { SetUpScreen(state, actions, onBack = { back(backStack) }) }
                entry<Route.Screenshots> { ScreenshotsScreen(state, actions, onBack = { back(backStack) }) }
                entry<Route.Codes> {
                    CodesScreen(
                        state.codes,
                        actions.codes,
                        names = state.computers.associate { it.deviceId to it.name },
                        onBack = { back(backStack) },
                        // Its set-up, with copies on (Back leads to the copies' screen).
                        onOpenCopies = {
                            if (!state.autoCopy.on) actions.autoCopy.onChange(true)
                            open(backStack, Route.Copies)
                            if (!state.autoCopy.setUp) open(backStack, Route.CopiesSetUp)
                        },
                    )
                }
                entry<Route.History> { HistoryScreen(state.activity, state.computers, onSettings = settings) }
                entry<Route.Devices> {
                    DevicesScreen(state, actions, onSettings = settings, onOpen = { open(backStack, Route.Device(it)) })
                }
                entry<Route.Device> { key -> DeviceScreen(state, key.deviceId, actions, onBack = { back(backStack) }) }
                entry<Route.Settings> {
                    SettingsScreen(
                        state.settings,
                        actions.settings,
                        onBack = { back(backStack) },
                        onOpenLicences = { open(backStack, Route.Licences) },
                        keepAlive = state.keepAlive,
                        onOpenKeepAlive = { open(backStack, Route.KeepAlive) },
                    )
                }
                entry<Route.Licences> { LicencesScreen(onBack = { back(backStack) }) }
                entry<Route.KeepAlive> { KeepAliveScreen(state.keepAlive, actions.keepAlive, onBack = { back(backStack) }) }
            },
        )
    }
}
