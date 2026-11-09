// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.annotation.DrawableRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.R
import io.github.nexoryx.pasteget.ui.theme.groupedShape

/**
 * "What paste-get sends": one row per feature, with its state in a few words. The row opens the
 * feature's screen, where the explanations are; its switch turns it on or off. Turning copies on
 * before the built-in mode is set up also opens its set-up (Back leads to the copies' screen).
 */
@Composable
fun FeatureRows(state: AppState, actions: AppActions, onOpen: (Route) -> Unit) {
    val copies = state.autoCopy
    val rows = listOf(
        FeatureRowState(
            icon = R.drawable.ic_content_copy,
            title = stringResource(R.string.copies_title),
            status = stringResource(featureStatus(copies.health)),
            checked = copies.on,
            onCheckedChange = { on ->
                actions.autoCopy.onChange(on)
                if (on && !copies.setUp) {
                    onOpen(Route.Copies)
                    onOpen(Route.CopiesSetUp)
                }
            },
            onClick = { onOpen(Route.Copies) },
        ),
        FeatureRowState(
            icon = R.drawable.ic_screenshot,
            title = stringResource(R.string.screenshots_title),
            status = stringResource(featureStatus(screenshotsHealth(state.screenshots.wanted, state.screenshots.access))),
            checked = state.screenshots.wanted,
            onCheckedChange = actions.onScreenshotsChange,
            onClick = { onOpen(Route.Screenshots) },
        ),
        FeatureRowState(
            icon = R.drawable.ic_password,
            title = stringResource(R.string.codes_title),
            status = stringResource(featureStatus(codesHealth(state.codes.on, state.codes.access, state.codes.shellReady))),
            checked = state.codes.on,
            onCheckedChange = actions.codes.onChange,
            onClick = { onOpen(Route.Codes) },
        ),
    )
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            stringResource(R.string.sends_title),
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.semantics { heading() },
        )
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            rows.forEachIndexed { index, row -> FeatureRow(row, Modifier.fillMaxWidth(), groupedShape(index, rows.size)) }
        }
    }
}

private class FeatureRowState(
    @param:DrawableRes val icon: Int,
    val title: String,
    val status: String,
    val checked: Boolean,
    val onCheckedChange: (Boolean) -> Unit,
    val onClick: () -> Unit,
)

@Composable
private fun FeatureRow(row: FeatureRowState, modifier: Modifier, shape: Shape) {
    Surface(modifier = modifier, shape = shape, color = MaterialTheme.colorScheme.surfaceContainer) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            // Read as "Send what you copy, On, automatic", and opens its screen.
            Row(
                modifier = Modifier
                    .weight(1f)
                    .heightIn(min = 72.dp)
                    .clickable(onClickLabel = stringResource(R.string.feature_open), onClick = row.onClick)
                    .padding(horizontal = 16.dp, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                TonalIcon(row.icon)
                Column {
                    Text(row.title, style = MaterialTheme.typography.bodyLarge)
                    Text(row.status, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            VerticalDivider(modifier = Modifier.height(32.dp))
            // Its own target, named like the row: "Send what you copy, switch, on".
            Box(
                modifier = Modifier
                    .toggleable(value = row.checked, role = Role.Switch, onValueChange = row.onCheckedChange)
                    .semantics { contentDescription = row.title }
                    .padding(16.dp),
            ) {
                Switch(checked = row.checked, onCheckedChange = null)
            }
        }
    }
}
