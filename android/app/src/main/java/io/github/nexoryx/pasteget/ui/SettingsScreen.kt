// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import io.github.nexoryx.pasteget.R

/** What is rarely changed, behind the gear. [onBack] goes back to the tab it was opened from. */
@Composable
fun SettingsScreen(onBack: () -> Unit) {
    ScreenScaffold(title = stringResource(R.string.settings_title), onBack = onBack) {
        Text(
            stringResource(R.string.settings_about),
            style = MaterialTheme.typography.titleSmall,
            modifier = Modifier.semantics { heading() },
        )
        val context = LocalContext.current
        val version = remember { context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty() }
        Text(stringResource(R.string.about_version, version), style = MaterialTheme.typography.bodyLarge)
        SupportCard()
    }
}
