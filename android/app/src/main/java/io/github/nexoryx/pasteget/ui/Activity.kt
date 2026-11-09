// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.text.format.DateFormat
import android.text.format.Formatter
import androidx.annotation.DrawableRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.Activity
import io.github.nexoryx.pasteget.ActivityKind
import io.github.nexoryx.pasteget.R
import java.util.Date

/**
 * One entry of the activity log: what it was and how big, where it went and how fast, and when.
 * [names] gives the computers' names by device ID.
 */
@Composable
fun ActivityRow(entry: Activity, names: Map<String, String>, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    Row(
        // Read as one line by TalkBack.
        modifier = modifier.fillMaxWidth().semantics(mergeDescendants = true) {}.padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        TonalIcon(iconOf(entry))
        Column(modifier = Modifier.weight(1f)) {
            Text(activityTitle(entry), style = MaterialTheme.typography.bodyLarge)
            Text(
                activityOutcome(entry, names),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Text(
            DateFormat.getTimeFormat(context).format(Date(entry.atMillis)),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@DrawableRes
private fun iconOf(entry: Activity): Int = when (entry.kind) {
    ActivityKind.TEXT -> R.drawable.ic_content_copy
    ActivityKind.IMAGE -> R.drawable.ic_image
    ActivityKind.SCREENSHOT -> R.drawable.ic_screenshot
    ActivityKind.CODE -> R.drawable.ic_password
}

/** "Copied text, 1.2 kB", "Code from Messages", "Image from a computer, 340 kB"… */
@Composable
fun activityTitle(entry: Activity): String {
    val size = Formatter.formatShortFileSize(LocalContext.current, entry.bytes.toLong())
    val format = when {
        entry.kind == ActivityKind.CODE -> return stringResource(R.string.activity_code, entry.app.orEmpty())
        !entry.sent && entry.sensitive -> R.string.activity_received_secret
        !entry.sent && entry.kind == ActivityKind.IMAGE -> R.string.activity_received_image
        !entry.sent -> R.string.activity_received_text
        entry.kind == ActivityKind.SCREENSHOT -> R.string.activity_screenshot
        entry.kind == ActivityKind.IMAGE -> R.string.activity_image
        entry.sensitive -> R.string.activity_secret
        entry.automatic -> R.string.activity_copy
        else -> R.string.activity_text
    }
    return stringResource(format, size)
}

/** Where it went and how fast ("HP-AIO in 38 ms"), or that it waits, or what became of it here. */
@Composable
fun activityOutcome(entry: Activity, names: Map<String, String>): String {
    if (!entry.sent) {
        return stringResource(if (entry.onClipboard) R.string.activity_on_clipboard else R.string.activity_not_on_clipboard)
    }
    if (entry.deliveries.isNotEmpty()) {
        val unknown = stringResource(R.string.activity_a_computer)
        return entry.deliveries.map { delivery ->
            val name = names[delivery.deviceId] ?: unknown
            if (delivery.accepted) {
                stringResource(R.string.activity_delivered, name, delivery.millis)
            } else {
                stringResource(R.string.activity_refused, name)
            }
        }.joinToString(", ")
    }
    return if (entry.sentTo > 0) {
        pluralStringResource(R.plurals.sent_to, entry.sentTo, entry.sentTo)
    } else {
        stringResource(R.string.send_waits)
    }
}

/** How long the last clip [deviceId] acknowledged took, if any did since the app started. */
fun List<Activity>.lastLatency(deviceId: String): Long? =
    firstNotNullOfOrNull { entry -> entry.deliveries.firstOrNull { it.deviceId == deviceId && it.accepted }?.millis }
