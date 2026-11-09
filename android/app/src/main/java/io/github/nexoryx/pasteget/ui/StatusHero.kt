// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.icu.text.ListFormatter
import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.R
import kotlinx.coroutines.delay

/**
 * The home screen's first card: how the app is doing, why, and the one thing to do next. Its
 * colors follow [AppStatus]; TalkBack reads it out when it changes. [copiesWork]: what's copied
 * goes out by itself, so the card says "copy anything" rather than how to send by hand. [onOpen]
 * opens the screen a step needs (the built-in mode's set-up).
 */
@Composable
fun StatusHero(status: AppStatus, copiesWork: Boolean, pairingBusy: Boolean, actions: AppActions, onOpen: (Route) -> Unit = {}) {
    AnimatedContent(
        targetState = status,
        // A new look for a new kind of state; within one, the text just changes.
        contentKey = { it::class },
        transitionSpec = { fadeIn() togetherWith fadeOut() },
        label = "status",
    ) { shown ->
        when (shown) {
            AppStatus.NoComputer -> HeroCard(
                container = MaterialTheme.colorScheme.secondaryContainer,
                content = MaterialTheme.colorScheme.onSecondaryContainer,
                icon = R.drawable.ic_qr_code_scanner,
                title = stringResource(R.string.hero_pair_title),
                text = stringResource(R.string.hero_pair_text),
            ) {
                Button(onClick = actions.onPair, enabled = !pairingBusy) { Text(stringResource(R.string.hero_scan)) }
            }
            is AppStatus.Unpaired -> HeroCard(
                container = MaterialTheme.colorScheme.errorContainer,
                content = MaterialTheme.colorScheme.onErrorContainer,
                icon = R.drawable.ic_link_off,
                title = stringResource(R.string.unpaired_title, shown.name),
                text = stringResource(R.string.unpaired_text),
            ) {
                Button(onClick = actions.onPair, enabled = !pairingBusy) { Text(stringResource(R.string.pair_again)) }
            }
            is AppStatus.Looking -> LookingCard(shown, actions)
            is AppStatus.NeedsStep -> HeroCard(
                container = MaterialTheme.colorScheme.tertiaryContainer,
                content = MaterialTheme.colorScheme.onTertiaryContainer,
                icon = R.drawable.ic_info,
                title = stringResource(R.string.hero_connected, names(shown.connected)),
                text = stringResource(needsStepText(shown.feature)),
                offline = shown.offline,
            ) {
                Button(onClick = { runStep(shown.step, actions, onOpen) }) { Text(stringResource(stepLabel(shown.step))) }
                OutlinedButton(onClick = actions.onSendClipboard) { Text(stringResource(R.string.send_clipboard_button)) }
            }
            is AppStatus.Working -> HeroCard(
                container = MaterialTheme.colorScheme.primaryContainer,
                content = MaterialTheme.colorScheme.onPrimaryContainer,
                icon = R.drawable.ic_check_circle,
                title = stringResource(R.string.hero_connected, names(shown.connected)),
                text = stringResource(if (copiesWork) R.string.hero_working_auto else R.string.hero_working_manual),
                offline = shown.offline,
            ) {
                FilledTonalButton(onClick = actions.onSendClipboard) { Text(stringResource(R.string.send_clipboard_button)) }
            }
        }
    }
}

/** Paired, none connected: still looking. After half a minute, what usually keeps them apart. */
@Composable
private fun LookingCard(status: AppStatus.Looking, actions: AppActions) {
    var long by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(LONG_LOOK_MS)
        long = true
    }
    HeroCard(
        container = MaterialTheme.colorScheme.surfaceContainerHigh,
        content = MaterialTheme.colorScheme.onSurface,
        icon = R.drawable.ic_wifi_find,
        title = stringResource(R.string.hero_looking, names(status.names)),
        text = if (long) stringResource(R.string.hero_looking_long) else null,
        progress = true,
    ) {
        // Sending works meanwhile: the clip waits for them.
        OutlinedButton(onClick = actions.onSendClipboard) { Text(stringResource(R.string.send_clipboard_button)) }
        if (long) TextButton(onClick = actions.onRefresh) { Text(stringResource(R.string.hero_try_again)) }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun HeroCard(
    container: Color,
    content: Color,
    @DrawableRes icon: Int,
    title: String,
    text: String?,
    offline: List<String> = emptyList(),
    progress: Boolean = false,
    buttons: @Composable () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.extraLarge,
        colors = CardDefaults.cardColors(containerColor = container, contentColor = content),
    ) {
        Column(modifier = Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            // Read out as one when the state changes, without moving the focus.
            Column(
                modifier = Modifier.fillMaxWidth().semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Icon(painterResource(icon), contentDescription = null)
                    Text(title, style = MaterialTheme.typography.headlineSmall)
                }
                if (text != null) Text(text, style = MaterialTheme.typography.bodyLarge)
                if (offline.isNotEmpty()) {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        // Said in words, never by color alone.
                        offline.forEach { name ->
                            Surface(shape = CircleShape, color = MaterialTheme.colorScheme.surfaceContainerHighest) {
                                Text(
                                    stringResource(R.string.hero_offline, name),
                                    style = MaterialTheme.typography.labelLarge,
                                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                                )
                            }
                        }
                    }
                }
            }
            if (progress) LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) { buttons() }
        }
    }
}

/** "HP-AIO", "HP-AIO and Arch-PC", in the phone's language. */
private fun names(names: List<String>): String = ListFormatter.getInstance().format(names)

/** How long to look before saying what usually keeps the phone and the computer apart. */
private const val LONG_LOOK_MS = 30_000L
