// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.content.Context
import androidx.core.content.edit

/** The user's choices, in the app's private storage. */
object Preferences {
    private const val FILE = "settings"
    private const val SEND_SCREENSHOTS = "send_screenshots"
    private const val SEND_COPIES = "send_copies"
    private const val BUILT_IN_PAIRED = "built_in_paired"
    private const val SEND_CODES = "send_codes"
    private const val APPEARANCE = "appearance"
    private const val WALLPAPER_COLORS = "wallpaper_colors"
    private const val AUTOSTART_TICKED = "autostart_ticked"
    private const val BATTERY_SAVER_TICKED = "battery_saver_ticked"
    private const val ONBOARDING_DONE = "onboarding_done"

    /** Off until the user turns it on and grants photo access. */
    fun sendScreenshots(context: Context): Boolean =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).getBoolean(SEND_SCREENSHOTS, false)

    fun setSendScreenshots(context: Context, send: Boolean) {
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).edit { putBoolean(SEND_SCREENSHOTS, send) }
    }

    /** Off until the user turns it on: the app then sees everything they copy. */
    fun sendCopies(context: Context): Boolean =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).getBoolean(SEND_COPIES, false)

    fun setSendCopies(context: Context, send: Boolean) {
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).edit { putBoolean(SEND_COPIES, send) }
    }

    /** Off until the user turns it on: one-time codes from notifications ([OneTimeCodes]). */
    fun sendCodes(context: Context): Boolean =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).getBoolean(SEND_CODES, false)

    fun setSendCodes(context: Context, send: Boolean) {
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).edit { putBoolean(SEND_CODES, send) }
    }

    /** Whether the app's key is paired with wireless debugging ([BuiltInShell]). */
    fun builtInPaired(context: Context): Boolean =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).getBoolean(BUILT_IN_PAIRED, false)

    fun setBuiltInPaired(context: Context, paired: Boolean) {
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).edit { putBoolean(BUILT_IN_PAIRED, paired) }
    }

    /** Light, dark, or as the system is (the default). */
    fun appearance(context: Context): Appearance {
        val saved = context.getSharedPreferences(FILE, Context.MODE_PRIVATE).getString(APPEARANCE, null)
        return Appearance.entries.firstOrNull { it.name == saved } ?: Appearance.SYSTEM
    }

    fun setAppearance(context: Context, appearance: Appearance) {
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).edit { putString(APPEARANCE, appearance.name) }
    }

    /** Android 12+: the wallpaper's colors (the default), or paste-get's own. */
    fun wallpaperColors(context: Context): Boolean =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).getBoolean(WALLPAPER_COLORS, true)

    fun setWallpaperColors(context: Context, on: Boolean) {
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).edit { putBoolean(WALLPAPER_COLORS, on) }
    }

    /** The user said they turned on the vendor's Autostart switch, which no API reads. */
    fun autostartTicked(context: Context): Boolean =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).getBoolean(AUTOSTART_TICKED, false)

    fun setAutostartTicked(context: Context, ticked: Boolean) {
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).edit { putBoolean(AUTOSTART_TICKED, ticked) }
    }

    /** The first-run guide was finished or skipped, or the phone had pairings already. */
    fun onboardingDone(context: Context): Boolean =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).getBoolean(ONBOARDING_DONE, false)

    fun setOnboardingDone(context: Context) {
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).edit { putBoolean(ONBOARDING_DONE, true) }
    }

    /** The user said they set Xiaomi's own battery saver to No restrictions, which no API reads. */
    fun batterySaverTicked(context: Context): Boolean =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).getBoolean(BATTERY_SAVER_TICKED, false)

    fun setBatterySaverTicked(context: Context, ticked: Boolean) {
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).edit { putBoolean(BATTERY_SAVER_TICKED, ticked) }
    }
}

/** How the app looks, chosen in Settings. */
enum class Appearance { SYSTEM, LIGHT, DARK }
