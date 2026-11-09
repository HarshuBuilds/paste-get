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
import uniffi.pasteget_ffi.PairedComputer

/*
 * Where the app stands, as values the screens (and later the notification and the tile) show:
 * one state for the whole app, [AppStatus], and one per feature, [FeatureHealth]. Each picks the
 * first case that applies and gives at most one next step. No Android calls: the JVM tests try
 * every combination.
 */

/** What paste-get sends by itself, once the user turns it on. */
enum class Feature { COPIES, SCREENSHOTS, CODES }

enum class Health {
    /** The user turned it off. */
    OFF,

    /** On, and working. */
    WORKING,

    /** On, and getting ready: nothing to do but wait. */
    STARTING,

    /** On, but not working until the user does [FeatureHealth.step]. */
    BLOCKED,
}

/** The one thing to do next for a feature that is blocked. */
enum class NextStep {
    /** Copies: set up the built-in mode (Android's wireless debugging). */
    SET_UP_BUILT_IN,

    /** Copies: the built-in mode's set-up asks for its code in a notification, and they are off. */
    ALLOW_NOTIFICATIONS,

    /** Copies: the built-in mode waits for the pairing code, typed in its notification. */
    FINISH_BUILT_IN_PAIRING,

    /** Copies: the built-in mode stopped. */
    RETRY_BUILT_IN,

    /** Copies: the built-in mode needs Wi-Fi. */
    CONNECT_WIFI,

    /** Copies: Shizuku runs, and paste-get isn't allowed to use it yet. */
    ALLOW_SHIZUKU,

    /** Copies, backup method: allow the log access Android asks for. */
    ALLOW_LOG_ACCESS,

    /** Copies, backup method: allow "display over other apps". */
    ALLOW_OVERLAY,

    /** Screenshots: no photo access. */
    ALLOW_PHOTOS,

    /** Screenshots: only the photos the user picked, which never include new screenshots. */
    ALLOW_ALL_PHOTOS,

    /** 2FA codes: one tap, through the built-in mode or Shizuku. */
    ALLOW_CODES,

    /** 2FA codes: notification access, in Settings. */
    OPEN_NOTIFICATION_ACCESS,

    /** 2FA codes: Android 15 hides them, and only the built-in mode or Shizuku can allow them. */
    SET_UP_SHELL_FOR_CODES,
}

/** How one feature is doing. [mode] is for copies only: how they are read right now. */
data class FeatureHealth(
    val feature: Feature,
    val health: Health,
    val step: NextStep? = null,
    val mode: CopyMode? = null,
)

/** The state the home screen leads with. */
sealed interface AppStatus {
    /** Nothing paired yet. */
    data object NoComputer : AppStatus

    /** [name] unpaired this phone, and no computer is left. */
    data class Unpaired(val name: String) : AppStatus

    /** Paired, but none is connected: the app is looking for [names]. */
    data class Looking(val names: List<String>) : AppStatus

    /** Connected, but [feature], turned on, waits for [step]. */
    data class NeedsStep(
        val connected: List<String>,
        val offline: List<String>,
        val feature: Feature,
        val step: NextStep,
    ) : AppStatus

    /** Connected, and everything turned on works or is starting. */
    data class Working(val connected: List<String>, val offline: List<String>) : AppStatus
}

/**
 * The automatic copy, from the state of each way it can read copies. [canNotify]: the built-in
 * mode's pairing notification can show.
 */
fun copiesHealth(
    on: Boolean,
    builtIn: BuiltInState,
    shizuku: ShizukuState,
    logs: LogStatus,
    fallback: FallbackAccess,
    canNotify: Boolean,
): FeatureHealth {
    if (!on) return FeatureHealth(Feature.COPIES, Health.OFF)
    val mode = CopyMode.of(builtIn, shizuku, logs)
    fun blocked(step: NextStep) = FeatureHealth(Feature.COPIES, Health.BLOCKED, step, mode)
    val starting = FeatureHealth(Feature.COPIES, Health.STARTING, mode = mode)
    return when {
        mode != CopyMode.MANUAL -> FeatureHealth(Feature.COPIES, Health.WORKING, mode = mode)
        // OFF: the service hasn't started the automatic copy yet.
        builtIn.status == BuiltInStatus.OFF -> starting
        builtIn.status == BuiltInStatus.STARTING -> starting
        shizuku.status == ShizukuStatus.CONNECTING -> starting
        builtIn.status == BuiltInStatus.NEEDS_WIFI -> blocked(NextStep.CONNECT_WIFI)
        builtIn.status == BuiltInStatus.FAILED -> blocked(NextStep.RETRY_BUILT_IN)
        builtIn.status == BuiltInStatus.PAIRING ->
            blocked(if (canNotify) NextStep.FINISH_BUILT_IN_PAIRING else NextStep.ALLOW_NOTIFICATIONS)
        // Installed and running: one tap is the shortest way.
        shizuku.status == ShizukuStatus.NO_PERMISSION -> blocked(NextStep.ALLOW_SHIZUKU)
        // Android may be asking right now.
        logs == LogStatus.STARTING -> blocked(NextStep.ALLOW_LOG_ACCESS)
        // The app is in front whenever this is shown: it starts by itself.
        logs == LogStatus.WAITING_FOR_APP -> starting
        fallback.readLogs && !fallback.overlay -> blocked(NextStep.ALLOW_OVERLAY)
        // The recommended way: it needs no other app and survives restarts.
        else -> blocked(if (canNotify) NextStep.SET_UP_BUILT_IN else NextStep.ALLOW_NOTIFICATIONS)
    }
}

/** New screenshots, which need access to all photos. */
fun screenshotsHealth(on: Boolean, photo: PhotoAccess): FeatureHealth = when {
    !on -> FeatureHealth(Feature.SCREENSHOTS, Health.OFF)
    photo == PhotoAccess.FULL -> FeatureHealth(Feature.SCREENSHOTS, Health.WORKING)
    photo == PhotoAccess.PARTIAL -> FeatureHealth(Feature.SCREENSHOTS, Health.BLOCKED, NextStep.ALLOW_ALL_PHOTOS)
    else -> FeatureHealth(Feature.SCREENSHOTS, Health.BLOCKED, NextStep.ALLOW_PHOTOS)
}

/** 2FA codes. [shellReady]: the built-in mode or Shizuku runs, so access is one tap away. */
fun codesHealth(on: Boolean, access: CodeAccess, shellReady: Boolean): FeatureHealth {
    fun blocked(step: NextStep) = FeatureHealth(Feature.CODES, Health.BLOCKED, step)
    return when {
        !on -> FeatureHealth(Feature.CODES, Health.OFF)
        !access.listening -> blocked(if (shellReady) NextStep.ALLOW_CODES else NextStep.OPEN_NOTIFICATION_ACCESS)
        access.seesCodes == false -> blocked(if (shellReady) NextStep.ALLOW_CODES else NextStep.SET_UP_SHELL_FOR_CODES)
        else -> FeatureHealth(Feature.CODES, Health.WORKING)
    }
}

/** Whether a service runs as the shell user, which grants the codes' access in one tap. */
fun shellReady(builtIn: BuiltInState, shizuku: ShizukuState): Boolean =
    builtIn.status == BuiltInStatus.WATCHING || shizuku.status == ShizukuStatus.WATCHING

/**
 * The app's state. [unpaired]: the computer that last unpaired this phone, if the user hasn't
 * been told yet. [features]: in the order their steps should come, the first blocked one wins.
 */
fun appStatus(
    computers: List<PairedComputer>,
    connected: Set<String>,
    unpaired: PairedComputer?,
    features: List<FeatureHealth>,
): AppStatus {
    if (computers.isEmpty()) return unpaired?.let { AppStatus.Unpaired(it.name) } ?: AppStatus.NoComputer
    val (online, offline) = computers.partition { it.deviceId in connected }
    if (online.isEmpty()) return AppStatus.Looking(offline.map { it.name })
    val onlineNames = online.map { it.name }
    val offlineNames = offline.map { it.name }
    val blocked = features.firstOrNull { it.health == Health.BLOCKED && it.step != null }
    return if (blocked?.step != null) {
        AppStatus.NeedsStep(onlineNames, offlineNames, blocked.feature, blocked.step)
    } else {
        AppStatus.Working(onlineNames, offlineNames)
    }
}
