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
    // What just happened ("Paired with HP-AIO", "Sent to 1 computer"), for a few seconds.
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(state.message) {
        val message = state.message ?: return@LaunchedEffect
        snackbar.showSnackbar(message)
        actions.onMessageShown(message)
    }
    CompositionLocalProvider(LocalSnackbarHost provides snackbar) {
        Shell(state, actions, backStack, tab, settings)
    }
    PairingFlow(state.pairing, state.computers, actions.onConfirmPairing, actions.onCancelPairing)
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
                    HomeScreen(state, actions, onSettings = settings, onSeeAll = { selectTab(backStack, Tab.HISTORY) })
                }
                entry<Route.History> { HistoryScreen(state.activity, state.computers, onSettings = settings) }
                entry<Route.Devices> {
                    DevicesScreen(state, actions, onSettings = settings, onOpen = { open(backStack, Route.Device(it)) })
                }
                entry<Route.Device> { key -> DeviceScreen(state, key.deviceId, actions, onBack = { back(backStack) }) }
                entry<Route.Settings> { SettingsScreen(onBack = { back(backStack) }) }
            },
        )
    }
}
