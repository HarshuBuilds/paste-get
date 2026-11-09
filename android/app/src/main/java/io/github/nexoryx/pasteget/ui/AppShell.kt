// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteScaffold
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.ui.NavDisplay

/**
 * The app: a navigation bar (a rail on wide screens) with Home, History and Devices, the screen
 * on top of [backStack], and the pairing sheet over all of them.
 */
@Composable
fun PasteGetUi(state: AppState, actions: AppActions, backStack: MutableList<NavKey>) {
    val tab = currentTab(backStack)
    val settings = { open(backStack, Route.Settings) }
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
    PairingFlow(state.pairing, state.computers, actions.onConfirmPairing, actions.onCancelPairing)
}
