// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.navigation3.runtime.NavKey
import io.github.nexoryx.pasteget.R
import kotlinx.serialization.Serializable

/**
 * The app's screens. The back stack is a plain list of these, saved with the activity (hence
 * `@Serializable`): Home is always at the bottom, so Back from any tab goes home first.
 */
@Serializable
sealed interface Route : NavKey {
    @Serializable
    data object Home : Route

    @Serializable
    data object History : Route

    @Serializable
    data object Devices : Route

    /** A paired computer's page, above Devices. */
    @Serializable
    data class Device(val deviceId: String) : Route

    /** Behind the gear in each tab's top bar. */
    @Serializable
    data object Settings : Route
}

/** The navigation bar's (or rail's) destinations, in order. */
enum class Tab(val route: Route, @param:StringRes val label: Int, @param:DrawableRes val icon: Int) {
    HOME(Route.Home, R.string.tab_home, R.drawable.ic_home),
    HISTORY(Route.History, R.string.tab_history, R.drawable.ic_history),
    DEVICES(Route.Devices, R.string.tab_devices, R.drawable.ic_devices),
}

/*
 * What the navigation does to the back stack: plain functions on a list, so the JVM tests try
 * them without a screen.
 */

/** The tab [stack] is in: the one under its top. Settings belongs to the tab it was opened from. */
fun currentTab(stack: List<NavKey>): Tab =
    stack.asReversed().firstNotNullOfOrNull { key -> Tab.entries.firstOrNull { it.route == key } } ?: Tab.HOME

/** Shows [tab], from its start: Home, then the tab on top of it. */
fun selectTab(stack: MutableList<NavKey>, tab: Tab) {
    stack.clear()
    stack.add(Route.Home)
    if (tab != Tab.HOME) stack.add(tab.route)
}

/** Opens [route] on top, unless it is there already (a double tap). */
fun open(stack: MutableList<NavKey>, route: Route) {
    if (stack.lastOrNull() != route) stack.add(route)
}

/** Back, never past Home: there, Back leaves the app. */
fun back(stack: MutableList<NavKey>) {
    if (stack.size > 1) stack.removeAt(stack.lastIndex)
}
