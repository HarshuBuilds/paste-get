// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import io.github.nexoryx.pasteget.Appearance
import io.github.nexoryx.pasteget.R

/** What Settings shows. [sdk] decides what this Android offers, so tests can pick one. */
data class SettingsState(
    val appearance: Appearance = Appearance.SYSTEM,
    val wallpaperColors: Boolean = true,
    val sdk: Int = Build.VERSION.SDK_INT,
)

/** Settings' buttons and switches. Each does nothing unless given, for tests. */
class SettingsActions(
    val onAppearanceChange: (Appearance) -> Unit = {},
    val onWallpaperColorsChange: (Boolean) -> Unit = {},
    /** Android 13+: Android's own language page for paste-get. */
    val onOpenLanguage: () -> Unit = {},
    /** Android 13+: asks to add the Send clipboard tile to Quick Settings. */
    val onAddTile: () -> Unit = {},
)

/**
 * What is rarely changed, behind the gear: keeping paste-get running, appearance, language, the
 * Quick Settings tile, and about paste-get. [onBack] goes back to the tab it was opened from;
 * [onOpenLicences] lists the open-source licences, [onOpenKeepAlive] opens the checklist that
 * [keepAlive] sums up.
 */
@Composable
fun SettingsScreen(
    state: SettingsState,
    actions: SettingsActions,
    onBack: () -> Unit,
    onOpenLicences: () -> Unit,
    keepAlive: KeepAliveState = KeepAliveState(),
    onOpenKeepAlive: () -> Unit = {},
) {
    ScreenScaffold(title = stringResource(R.string.settings_title), onBack = onBack) {
        // First: on most phones it decides whether copies keep going out.
        LinkRow(R.drawable.ic_check_circle, stringResource(R.string.keepalive_title), keepAliveProgress(keepAlive), onOpenKeepAlive)

        Section(R.string.settings_appearance)
        AppearanceChoice(state.appearance, actions.onAppearanceChange)
        // Material You's colors exist from Android 12.
        if (state.sdk >= Build.VERSION_CODES.S) {
            Card(modifier = Modifier.fillMaxWidth()) {
                Box(modifier = Modifier.padding(16.dp)) {
                    SwitchRow(
                        title = stringResource(R.string.wallpaper_colors),
                        text = stringResource(R.string.wallpaper_colors_text),
                        checked = state.wallpaperColors,
                        onCheckedChange = actions.onWallpaperColorsChange,
                        icon = R.drawable.ic_palette,
                    )
                }
            }
        }

        // Per-app languages exist from Android 13; before, paste-get follows the phone's.
        if (state.sdk >= Build.VERSION_CODES.TIRAMISU) {
            Section(R.string.settings_language)
            LinkRow(R.drawable.ic_language, stringResource(R.string.settings_language), stringResource(R.string.settings_language_text), actions.onOpenLanguage)
        }

        Section(R.string.settings_tile)
        Text(stringResource(R.string.tile_text), style = MaterialTheme.typography.bodyMedium)
        if (state.sdk >= Build.VERSION_CODES.TIRAMISU) {
            OutlinedButton(onClick = actions.onAddTile) { Text(stringResource(R.string.tile_add)) }
        } else {
            Text(stringResource(R.string.tile_how), style = MaterialTheme.typography.bodyMedium)
        }

        Section(R.string.settings_about)
        About(onOpenLicences)
        SupportCard()
    }
}

@Composable
private fun Section(@StringRes title: Int) {
    Text(
        stringResource(title),
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.semantics { heading() },
    )
}

/** System, Light or Dark, as one segmented control. */
@Composable
private fun AppearanceChoice(current: Appearance, onChange: (Appearance) -> Unit) {
    val labels = mapOf(
        Appearance.SYSTEM to R.string.appearance_system,
        Appearance.LIGHT to R.string.appearance_light,
        Appearance.DARK to R.string.appearance_dark,
    )
    SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
        Appearance.entries.forEachIndexed { index, appearance ->
            SegmentedButton(
                selected = appearance == current,
                onClick = { onChange(appearance) },
                shape = SegmentedButtonDefaults.itemShape(index, Appearance.entries.size),
            ) {
                Text(stringResource(labels.getValue(appearance)))
            }
        }
    }
}

/** A row that opens something else: an icon, a title, a line under it. */
@Composable
private fun LinkRow(@DrawableRes icon: Int, title: String, text: String, onClick: () -> Unit) {
    Card(modifier = Modifier.fillMaxWidth().clickable(onClick = onClick)) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            TonalIcon(icon)
            Column(modifier = Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.bodyLarge)
                Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null)
        }
    }
}

/** The version, the licence, the source code and the libraries' licences. */
@Composable
private fun About(onOpenLicences: () -> Unit) {
    val context = LocalContext.current
    val version = remember { context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty() }
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                TonalIcon(R.drawable.ic_info)
                Column {
                    Text(stringResource(R.string.app_name), style = MaterialTheme.typography.bodyLarge)
                    Text(stringResource(R.string.about_version, version), style = MaterialTheme.typography.bodyMedium)
                }
            }
            Text(stringResource(R.string.about_free), style = MaterialTheme.typography.bodyMedium)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = { openLink(context, SOURCE_URL) }) { Text(stringResource(R.string.about_source)) }
                TextButton(onClick = onOpenLicences) { Text(stringResource(R.string.licences_title)) }
            }
        }
    }
}

/** A web page, in the browser: paste-get itself makes no request. */
private fun openLink(context: Context, url: String) {
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, url.toUri()))
    } catch (e: ActivityNotFoundException) {
        // No browser: nothing to open it with.
    }
}

private const val SOURCE_URL = "https://github.com/Nexoryx-business/paste-get"

/** A link to donations, when the build has one (`donate_url`). */
@Composable
fun SupportCard() {
    val url = stringResource(R.string.donate_url)
    if (url.isBlank()) return
    val context = LocalContext.current
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                TonalIcon(R.drawable.ic_favorite)
                Text(
                    stringResource(R.string.support_title),
                    style = MaterialTheme.typography.bodyLarge,
                    modifier = Modifier.semantics { heading() },
                )
            }
            Text(stringResource(R.string.support_text), style = MaterialTheme.typography.bodyMedium)
            OutlinedButton(onClick = { openLink(context, url) }) { Text(stringResource(R.string.support_button)) }
        }
    }
}
