// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.R

/** A library paste-get ships, and its licence (SPDX). */
private class Library(val name: String, val licence: String)

/** What the APK contains besides paste-get's own code. Keep in step with app/build.gradle.kts. */
private val libraries = listOf(
    Library("Jetpack Compose, Material 3", "Apache-2.0"),
    Library("AndroidX Core, Activity, Lifecycle, SplashScreen, Navigation 3", "Apache-2.0"),
    Library("Kotlin, kotlinx.coroutines, kotlinx.serialization", "Apache-2.0"),
    Library("Material Symbols (icons, © Google LLC)", "Apache-2.0"),
    Library("ZXing Android Embedded (QR code scanner)", "Apache-2.0"),
    Library("Shizuku API", "MIT"),
    Library("JNA", "Apache-2.0 OR LGPL-2.1-or-later"),
)

/** The open-source licences of what paste-get is built with, and the Apache License's text. */
@Composable
fun LicencesScreen(onBack: () -> Unit) {
    ScreenScaffold(title = stringResource(R.string.licences_title), onBack = onBack) {
        Text(stringResource(R.string.licences_text), style = MaterialTheme.typography.bodyMedium)
        Card(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                libraries.forEach { Entry(it.name, it.licence) }
                // Many small crates: cargo-deny checks each licence against deny.toml.
                Entry(stringResource(R.string.licences_rust), stringResource(R.string.licences_rust_licence))
            }
        }
        ApacheLicence()
    }
}

@Composable
private fun Entry(name: String, licence: String) {
    Column {
        Text(name, style = MaterialTheme.typography.bodyLarge)
        Text(licence, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** The full text most of them use, shipped in the APK as the licence asks. */
@Composable
private fun ApacheLicence() {
    var shown by rememberSaveable { mutableStateOf(false) }
    TextButton(onClick = { shown = !shown }) {
        Text(stringResource(if (shown) R.string.licences_hide_apache else R.string.licences_show_apache))
    }
    if (shown) {
        val resources = LocalResources.current
        val text = remember(resources) { resources.openRawResource(R.raw.apache_license_2_0).bufferedReader().use { it.readText() } }
        SelectionContainer {
            Text(text, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
        }
    }
}
