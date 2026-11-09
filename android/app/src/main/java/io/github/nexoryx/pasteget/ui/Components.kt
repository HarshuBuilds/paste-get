// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.R

/**
 * A title, a line under it and a switch, as one control: the whole row toggles, and TalkBack
 * reads "Send new screenshots, …, switch, off" instead of a bare "switch, off". [icon] leads it.
 */
@Composable
fun SwitchRow(
    title: String,
    text: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    titleStyle: TextStyle = MaterialTheme.typography.bodyLarge,
    @DrawableRes icon: Int? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .toggleable(value = checked, role = Role.Switch, onValueChange = onCheckedChange),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        icon?.let { TonalIcon(it) }
        Column(modifier = Modifier.weight(1f)) {
            Text(title, style = titleStyle)
            Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        // The row handles the tap.
        Switch(checked = checked, onCheckedChange = null)
    }
}

/** A decorative icon in a 40 dp circle: what a row is about, at a glance. TalkBack skips it. */
@Composable
fun TonalIcon(
    @DrawableRes icon: Int,
    container: Color = MaterialTheme.colorScheme.secondaryContainer,
    content: Color = MaterialTheme.colorScheme.onSecondaryContainer,
) {
    Box(
        modifier = Modifier.size(40.dp).background(container, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(icon), contentDescription = null, tint = content, modifier = Modifier.size(24.dp))
    }
}

/**
 * One item of a checklist: a check when [done] (read as [doneLabel]), an empty circle
 * otherwise, then a title and a line under it. TalkBack reads it as one.
 */
@Composable
fun CheckedRow(title: String, text: String, done: Boolean, doneLabel: String) {
    Row(
        modifier = Modifier.fillMaxWidth().semantics(mergeDescendants = true) {},
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (done) {
            Icon(painterResource(R.drawable.ic_check_circle), contentDescription = doneLabel, tint = MaterialTheme.colorScheme.primary)
        } else {
            Icon(
                painterResource(R.drawable.ic_radio_button_unchecked),
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Column {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}
