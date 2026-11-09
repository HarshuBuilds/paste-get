// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import io.github.nexoryx.pasteget.BuiltInState
import io.github.nexoryx.pasteget.BuiltInStatus
import io.github.nexoryx.pasteget.CodeAccess
import io.github.nexoryx.pasteget.CopyMode
import io.github.nexoryx.pasteget.FallbackAccess
import io.github.nexoryx.pasteget.LogStatus
import io.github.nexoryx.pasteget.PhotoAccess
import io.github.nexoryx.pasteget.ShizukuState
import io.github.nexoryx.pasteget.ShizukuStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.pasteget_ffi.PairedComputer

/** The status model: every combination of inputs gives one state and at most one next step. */
class AppStatusTest {
    private val fallbacks = listOf(false, true).flatMap { logs -> listOf(false, true).map { FallbackAccess(logs, it) } }
    private val codeAccesses = listOf(false, true).flatMap { listening ->
        listOf(true, false, null).map { CodeAccess(listening, it) }
    }

    private val copySteps = setOf(
        NextStep.SET_UP_BUILT_IN,
        NextStep.ALLOW_NOTIFICATIONS,
        NextStep.FINISH_BUILT_IN_PAIRING,
        NextStep.RETRY_BUILT_IN,
        NextStep.CONNECT_WIFI,
        NextStep.ALLOW_SHIZUKU,
        NextStep.ALLOW_LOG_ACCESS,
        NextStep.ALLOW_OVERLAY,
    )

    @Test
    fun everyCopiesCombinationIsConsistent() {
        for (on in listOf(false, true)) {
            for (builtIn in BuiltInStatus.entries) {
                for (shizuku in ShizukuStatus.entries) {
                    for (logs in LogStatus.entries) {
                        for (fallback in fallbacks) {
                            for (canNotify in listOf(true, false)) {
                                val health = copiesHealth(on, BuiltInState(builtIn), ShizukuState(shizuku), logs, fallback, canNotify)
                                val case = "on=$on $builtIn $shizuku $logs $fallback notify=$canNotify: $health"
                                assertEquals(case, Feature.COPIES, health.feature)
                                assertConsistent(case, health)
                                if (health.step != null) assertTrue(case, health.step in copySteps)
                                if (!on) assertEquals(case, Health.OFF, health.health)
                                // Working means something reads copies, and nothing else.
                                val reading = on && CopyMode.of(BuiltInState(builtIn), ShizukuState(shizuku), logs) != CopyMode.MANUAL
                                assertEquals(case, reading, health.health == Health.WORKING)
                                // Never a step that needs the notification while it can't show.
                                if (!canNotify) {
                                    assertTrue(case, health.step != NextStep.SET_UP_BUILT_IN && health.step != NextStep.FINISH_BUILT_IN_PAIRING)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    @Test
    fun copiesTable() {
        fun health(
            builtIn: BuiltInStatus,
            shizuku: ShizukuStatus,
            logs: LogStatus,
            fallback: FallbackAccess = none,
            canNotify: Boolean = true,
        ) = copiesHealth(true, BuiltInState(builtIn), ShizukuState(shizuku), logs, fallback, canNotify)

        val builtIn = health(BuiltInStatus.WATCHING, ShizukuStatus.OFF, LogStatus.OFF)
        assertEquals(Health.WORKING, builtIn.health)
        assertEquals(CopyMode.BUILT_IN, builtIn.mode)
        assertEquals(CopyMode.SHIZUKU, health(BuiltInStatus.NOT_SET_UP, ShizukuStatus.WATCHING, LogStatus.OFF).mode)
        assertEquals(CopyMode.LOGS, health(BuiltInStatus.NOT_SET_UP, ShizukuStatus.NOT_RUNNING, LogStatus.RUNNING).mode)

        // A fresh phone: the built-in mode is the step to take.
        val fresh = health(BuiltInStatus.NOT_SET_UP, ShizukuStatus.NOT_INSTALLED, LogStatus.NEEDS_PERMISSIONS)
        assertEquals(Health.BLOCKED, fresh.health)
        assertEquals(NextStep.SET_UP_BUILT_IN, fresh.step)
        assertEquals(CopyMode.MANUAL, fresh.mode)
        // Its code is typed into a notification: those come first.
        val silenced = health(BuiltInStatus.NOT_SET_UP, ShizukuStatus.NOT_INSTALLED, LogStatus.NEEDS_PERMISSIONS, canNotify = false)
        assertEquals(NextStep.ALLOW_NOTIFICATIONS, silenced.step)
        assertEquals(
            NextStep.ALLOW_NOTIFICATIONS,
            health(BuiltInStatus.PAIRING, ShizukuStatus.OFF, LogStatus.NEEDS_PERMISSIONS, canNotify = false).step,
        )
        // Once set up, it runs without them.
        assertEquals(Health.WORKING, health(BuiltInStatus.WATCHING, ShizukuStatus.OFF, LogStatus.OFF, canNotify = false).health)

        assertEquals(NextStep.CONNECT_WIFI, health(BuiltInStatus.NEEDS_WIFI, ShizukuStatus.OFF, LogStatus.NEEDS_PERMISSIONS).step)
        assertEquals(NextStep.RETRY_BUILT_IN, health(BuiltInStatus.FAILED, ShizukuStatus.OFF, LogStatus.NEEDS_PERMISSIONS).step)
        assertEquals(
            NextStep.FINISH_BUILT_IN_PAIRING,
            health(BuiltInStatus.PAIRING, ShizukuStatus.NOT_INSTALLED, LogStatus.NEEDS_PERMISSIONS).step,
        )
        assertEquals(NextStep.ALLOW_SHIZUKU, health(BuiltInStatus.NOT_SET_UP, ShizukuStatus.NO_PERMISSION, LogStatus.STARTING).step)
        assertEquals(NextStep.ALLOW_LOG_ACCESS, health(BuiltInStatus.NOT_SET_UP, ShizukuStatus.NOT_RUNNING, LogStatus.STARTING).step)
        assertEquals(
            NextStep.ALLOW_OVERLAY,
            health(BuiltInStatus.NOT_SET_UP, ShizukuStatus.NOT_RUNNING, LogStatus.NEEDS_PERMISSIONS, FallbackAccess(true, false)).step,
        )

        // Getting ready: nothing to tap.
        assertEquals(Health.STARTING, health(BuiltInStatus.STARTING, ShizukuStatus.OFF, LogStatus.OFF).health)
        assertEquals(Health.STARTING, health(BuiltInStatus.NOT_SET_UP, ShizukuStatus.CONNECTING, LogStatus.OFF).health)
        assertEquals(Health.STARTING, health(BuiltInStatus.OFF, ShizukuStatus.OFF, LogStatus.OFF).health)
    }

    @Test
    fun everyScreenshotsCombinationIsConsistent() {
        for (on in listOf(false, true)) {
            for (photo in PhotoAccess.entries) {
                val health = screenshotsHealth(on, photo)
                val case = "on=$on $photo: $health"
                assertEquals(case, Feature.SCREENSHOTS, health.feature)
                assertConsistent(case, health)
            }
        }
        assertEquals(Health.OFF, screenshotsHealth(false, PhotoAccess.NONE).health)
        assertEquals(Health.WORKING, screenshotsHealth(true, PhotoAccess.FULL).health)
        assertEquals(NextStep.ALLOW_ALL_PHOTOS, screenshotsHealth(true, PhotoAccess.PARTIAL).step)
        assertEquals(NextStep.ALLOW_PHOTOS, screenshotsHealth(true, PhotoAccess.NONE).step)
    }

    @Test
    fun everyCodesCombinationIsConsistent() {
        for (on in listOf(false, true)) {
            for (access in codeAccesses) {
                for (shell in listOf(false, true)) {
                    val health = codesHealth(on, access, shell)
                    val case = "on=$on $access shell=$shell: $health"
                    assertEquals(case, Feature.CODES, health.feature)
                    assertConsistent(case, health)
                    // Working exactly when the access is complete (or can't be checked).
                    assertEquals(case, on && access.ready, health.health == Health.WORKING)
                }
            }
        }
        assertEquals(NextStep.ALLOW_CODES, codesHealth(true, CodeAccess(false, false), shellReady = true).step)
        assertEquals(NextStep.OPEN_NOTIFICATION_ACCESS, codesHealth(true, CodeAccess(false, false), shellReady = false).step)
        assertEquals(NextStep.SET_UP_SHELL_FOR_CODES, codesHealth(true, CodeAccess(true, false), shellReady = false).step)
        assertEquals(NextStep.ALLOW_CODES, codesHealth(true, CodeAccess(true, false), shellReady = true).step)
    }

    @Test
    fun theShellIsReadyWhenTheBuiltInModeOrShizukuWatches() {
        assertTrue(shellReady(BuiltInState(BuiltInStatus.WATCHING), ShizukuState(ShizukuStatus.OFF)))
        assertTrue(shellReady(BuiltInState(BuiltInStatus.NOT_SET_UP), ShizukuState(ShizukuStatus.WATCHING)))
        assertEquals(false, shellReady(BuiltInState(BuiltInStatus.STARTING), ShizukuState(ShizukuStatus.CONNECTING)))
    }

    @Test
    fun everyAppCombinationGivesTheFirstStateThatApplies() {
        val pcs = listOf(pc("a", "HP-AIO"), pc("b", "Arch-PC"))
        val computerSets = listOf(emptyList(), pcs.take(1), pcs)
        val connectedSets = listOf(emptySet(), setOf("a"), setOf("a", "b"))
        val featureSets = listOf(
            emptyList(),
            listOf(working(Feature.COPIES)),
            listOf(blocked(Feature.COPIES, NextStep.SET_UP_BUILT_IN), blocked(Feature.CODES, NextStep.ALLOW_CODES)),
            listOf(working(Feature.COPIES), blocked(Feature.SCREENSHOTS, NextStep.ALLOW_PHOTOS)),
            listOf(FeatureHealth(Feature.COPIES, Health.STARTING), FeatureHealth(Feature.CODES, Health.OFF)),
        )
        for (computers in computerSets) {
            for (connected in connectedSets) {
                for (unpaired in listOf(null, pc("c", "Old-PC"))) {
                    for (features in featureSets) {
                        val status = appStatus(computers, connected, unpaired, features)
                        val case = "${computers.map { it.name }} $connected ${unpaired?.name} $features: $status"
                        val online = computers.filter { it.deviceId in connected }
                        val firstBlocked = features.firstOrNull { it.health == Health.BLOCKED }
                        val expected = when {
                            computers.isEmpty() && unpaired != null -> AppStatus.Unpaired("Old-PC")
                            computers.isEmpty() -> AppStatus.NoComputer
                            online.isEmpty() -> AppStatus.Looking(computers.map { it.name })
                            firstBlocked != null -> AppStatus.NeedsStep(
                                online.map { it.name },
                                (computers - online.toSet()).map { it.name },
                                firstBlocked.feature,
                                firstBlocked.step!!,
                            )
                            else -> AppStatus.Working(online.map { it.name }, (computers - online.toSet()).map { it.name })
                        }
                        assertEquals(case, expected, status)
                    }
                }
            }
        }
    }

    @Test
    fun appTable() {
        val hp = pc("a", "HP-AIO")
        val arch = pc("b", "Arch-PC")
        assertEquals(AppStatus.NoComputer, appStatus(emptyList(), emptySet(), null, emptyList()))
        assertEquals(AppStatus.Unpaired("HP-AIO"), appStatus(emptyList(), emptySet(), hp, emptyList()))
        // Another computer is still there: it leads, not the one that left.
        assertEquals(AppStatus.Looking(listOf("Arch-PC")), appStatus(listOf(arch), emptySet(), hp, emptyList()))
        assertEquals(
            AppStatus.Working(listOf("HP-AIO"), listOf("Arch-PC")),
            appStatus(listOf(hp, arch), setOf("a"), null, listOf(working(Feature.COPIES))),
        )
        val needs = appStatus(listOf(hp), setOf("a"), null, listOf(blocked(Feature.COPIES, NextStep.ALLOW_SHIZUKU)))
        assertEquals(AppStatus.NeedsStep(listOf("HP-AIO"), emptyList(), Feature.COPIES, NextStep.ALLOW_SHIZUKU), needs)
    }

    /** Off and working have no step; blocked has exactly one; starting has none. */
    private fun assertConsistent(case: String, health: FeatureHealth) {
        when (health.health) {
            Health.OFF, Health.WORKING, Health.STARTING -> assertNull(case, health.step)
            Health.BLOCKED -> assertNotNull(case, health.step)
        }
    }

    private fun working(feature: Feature) = FeatureHealth(feature, Health.WORKING)

    private fun blocked(feature: Feature, step: NextStep) = FeatureHealth(feature, Health.BLOCKED, step)

    private fun pc(id: String, name: String) = PairedComputer(id, name, listOf("192.168.1.20:47290"))

    private companion object {
        val none = FallbackAccess(readLogs = false, overlay = false)
    }
}
