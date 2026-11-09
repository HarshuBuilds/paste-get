// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp

/**
 * A title, a line under it and a switch, as one control: the whole row toggles, and TalkBack
 * reads "Send new screenshots, …, switch, off" instead of a bare "switch, off".
 */
@Composable
fun SwitchRow(
    title: String,
    text: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    titleStyle: TextStyle = MaterialTheme.typography.bodyLarge,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .toggleable(value = checked, role = Role.Switch, onValueChange = onCheckedChange),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(title, style = titleStyle)
            Text(text, style = MaterialTheme.typography.bodySmall)
        }
        // The row handles the tap.
        Switch(checked = checked, onCheckedChange = null)
    }
}
