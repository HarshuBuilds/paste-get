// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalLayoutDirection
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
 *
 * On a tablet or in landscape the column stays at most [MAX_WIDTH] wide, centred, so a switch
 * never ends up far from its label. It scrolls behind the gesture bar: the bottom inset is
 * space at the end of the column, not a border around it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ScreenScaffold(
    title: String,
    onBack: (() -> Unit)? = null,
    actions: @Composable RowScope.() -> Unit = {},
    floatingActionButton: (@Composable () -> Unit)? = null,
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
        floatingActionButton = floatingActionButton ?: {},
        // Above the button, on whichever screen is open.
        snackbarHost = { LocalSnackbarHost.current?.let { SnackbarHost(it) } },
    ) { padding ->
        val direction = LocalLayoutDirection.current
        val frame = Modifier.padding(
            start = padding.calculateStartPadding(direction),
            top = padding.calculateTopPadding(),
            end = padding.calculateEndPadding(direction),
        )
        // The button covers the end of the column otherwise.
        val end = padding.calculateBottomPadding() + if (floatingActionButton != null) FAB_SPACE else 8.dp
        val column = @Composable {
            Column(
                modifier = Modifier.fillMaxSize().verticalScroll(scroll),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Column(
                    modifier = Modifier
                        .widthIn(max = MAX_WIDTH)
                        .fillMaxWidth()
                        .padding(start = 16.dp, top = 8.dp, end = 16.dp, bottom = end),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                    content = content,
                )
            }
        }
        if (onRefresh == null) {
            Box(modifier = frame) { column() }
        } else {
            PullToRefreshBox(isRefreshing = refreshing, onRefresh = onRefresh, modifier = frame) { column() }
        }
    }
}

/** The widest a screen's column gets. */
val MAX_WIDTH = 640.dp

/** Room for the floating button under the last item. */
private val FAB_SPACE = 88.dp

/** Where the shell shows its messages (Snackbars). `null` in tests that draw one screen. */
val LocalSnackbarHost = staticCompositionLocalOf<SnackbarHostState?> { null }

/** The gear that opens Settings, in the top bar of each tab. */
@Composable
fun SettingsButton(onClick: () -> Unit) {
    IconButton(onClick = onClick) {
        Icon(painterResource(R.drawable.ic_settings), contentDescription = stringResource(R.string.settings_title))
    }
}
