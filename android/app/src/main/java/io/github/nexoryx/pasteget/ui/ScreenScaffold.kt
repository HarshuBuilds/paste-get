// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.R

/**
 * Every screen's frame: a top bar with [title] (and a back arrow when [onBack] is given), the
 * [actions] at its end, and [content] in a column that scrolls ([scroll]). With [onRefresh],
 * pulling the column down calls it, and shows a spinner while [refreshing].
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ScreenScaffold(
    title: String,
    onBack: (() -> Unit)? = null,
    actions: @Composable RowScope.() -> Unit = {},
    floatingActionButton: @Composable () -> Unit = {},
    scroll: ScrollState = rememberScrollState(),
    refreshing: Boolean = false,
    onRefresh: (() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title, modifier = Modifier.semantics { heading() }) },
                navigationIcon = {
                    if (onBack != null) {
                        IconButton(onClick = onBack) {
                            Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = stringResource(R.string.back))
                        }
                    }
                },
                actions = actions,
            )
        },
        floatingActionButton = floatingActionButton,
        // Above the button, on whichever screen is open.
        snackbarHost = { LocalSnackbarHost.current?.let { SnackbarHost(it) } },
    ) { padding ->
        val column = @Composable {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(scroll)
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
                content = content,
            )
        }
        if (onRefresh == null) {
            Box(modifier = Modifier.padding(padding)) { column() }
        } else {
            PullToRefreshBox(isRefreshing = refreshing, onRefresh = onRefresh, modifier = Modifier.padding(padding)) { column() }
        }
    }
}

/** Where the shell shows its messages (Snackbars). `null` in tests that draw one screen. */
val LocalSnackbarHost = staticCompositionLocalOf<SnackbarHostState?> { null }

/** The gear that opens Settings, in the top bar of each tab. */
@Composable
fun SettingsButton(onClick: () -> Unit) {
    IconButton(onClick = onClick) {
        Icon(painterResource(R.drawable.ic_settings), contentDescription = stringResource(R.string.settings_title))
    }
}
