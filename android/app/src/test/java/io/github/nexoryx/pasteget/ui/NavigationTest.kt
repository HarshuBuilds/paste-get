// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.navigation3.runtime.NavKey
import org.junit.Assert.assertEquals
import org.junit.Test

/** The back stack, as the tabs, the gear and Back change it. */
class NavigationTest {
    private val stack = mutableListOf<NavKey>(Route.Home)

    @Test
    fun aTabSitsOnTopOfHome() {
        selectTab(stack, Tab.DEVICES)
        assertEquals(listOf(Route.Home, Route.Devices), stack)
        assertEquals(Tab.DEVICES, currentTab(stack))

        // Another tab replaces it: Back from any tab goes home, never to the previous tab.
        selectTab(stack, Tab.HISTORY)
        assertEquals(listOf(Route.Home, Route.History), stack)

        selectTab(stack, Tab.HOME)
        assertEquals(listOf(Route.Home), stack)
        assertEquals(Tab.HOME, currentTab(stack))
    }

    @Test
    fun settingsBelongsToTheTabItWasOpenedFrom() {
        selectTab(stack, Tab.DEVICES)
        open(stack, Route.Settings)
        assertEquals(Tab.DEVICES, currentTab(stack))

        // A double tap opens it once.
        open(stack, Route.Settings)
        assertEquals(listOf(Route.Home, Route.Devices, Route.Settings), stack)

        // Tapping the tab again goes back to its start.
        selectTab(stack, Tab.DEVICES)
        assertEquals(listOf(Route.Home, Route.Devices), stack)
    }

    @Test
    fun backStopsAtHome() {
        selectTab(stack, Tab.HISTORY)
        open(stack, Route.Settings)

        back(stack)
        assertEquals(listOf(Route.Home, Route.History), stack)
        back(stack)
        back(stack)
        assertEquals(listOf<NavKey>(Route.Home), stack)
    }

    @Test
    fun anEmptyStackIsHome() {
        assertEquals(Tab.HOME, currentTab(emptyList()))
    }
}
