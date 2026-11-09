// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.content.ActivityNotFoundException
import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
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

/** A link to donations, when the build has one (`donate_url`). */
@Composable
fun SupportCard() {
    val url = stringResource(R.string.donate_url)
    if (url.isBlank()) return
    val context = LocalContext.current
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                stringResource(R.string.support_title),
                style = MaterialTheme.typography.bodyLarge,
                modifier = Modifier.semantics { heading() },
            )
            Text(stringResource(R.string.support_text), style = MaterialTheme.typography.bodySmall)
            OutlinedButton(onClick = {
                try {
                    context.startActivity(Intent(Intent.ACTION_VIEW, url.toUri()))
                } catch (e: ActivityNotFoundException) {
                    // No browser: nothing to open it with.
                }
            }) { Text(stringResource(R.string.support_button)) }
        }
    }
}
