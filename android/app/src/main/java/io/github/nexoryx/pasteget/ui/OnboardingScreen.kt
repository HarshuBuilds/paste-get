// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget.ui

import android.content.Context
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.BackHandler
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import io.github.nexoryx.pasteget.Activity
import io.github.nexoryx.pasteget.ActivityKind
import io.github.nexoryx.pasteget.PhotoAccess
import io.github.nexoryx.pasteget.R
import kotlinx.coroutines.delay
import java.text.DateFormat
import java.util.Date

/** The first-run guide's pages, in order. */
enum class OnboardingStep { WELCOME, COMPUTER, PAIR, NOTIFICATIONS, CHOOSE, COPIES, SCREENSHOTS, CODES, KEEP_ALIVE, TRY }

/**
 * The pages for what the user chose to send: each chosen feature gets its set-up.
 * [asksForNotifications]: Android 13+ asks for them; before, they are on unless turned off.
 */
fun onboardingSteps(chosen: Set<Feature>, asksForNotifications: Boolean): List<OnboardingStep> = buildList {
    add(OnboardingStep.WELCOME)
    add(OnboardingStep.COMPUTER)
    add(OnboardingStep.PAIR)
    if (asksForNotifications) add(OnboardingStep.NOTIFICATIONS)
    add(OnboardingStep.CHOOSE)
    if (Feature.COPIES in chosen) add(OnboardingStep.COPIES)
    if (Feature.SCREENSHOTS in chosen) add(OnboardingStep.SCREENSHOTS)
    if (Feature.CODES in chosen) add(OnboardingStep.CODES)
    add(OnboardingStep.KEEP_ALIVE)
    add(OnboardingStep.TRY)
}

/**
 * Whether [step] has nothing left to do: its button then says Next, otherwise Later. Pages
 * that only explain are always done.
 */
fun onboardingStepDone(step: OnboardingStep, state: AppState): Boolean = when (step) {
    OnboardingStep.PAIR -> state.computers.isNotEmpty()
    OnboardingStep.NOTIFICATIONS -> state.keepAlive.checks.notifications
    OnboardingStep.COPIES -> state.autoCopy.setUp || state.autoCopy.health.health == Health.WORKING
    OnboardingStep.SCREENSHOTS -> state.screenshots.access == PhotoAccess.FULL
    OnboardingStep.CODES -> state.codes.access.ready
    OnboardingStep.KEEP_ALIVE -> state.keepAlive.done == state.keepAlive.total
    OnboardingStep.WELCOME, OnboardingStep.COMPUTER, OnboardingStep.CHOOSE, OnboardingStep.TRY -> true
}

/**
 * The first run, before the app's tabs: what paste-get is, the computer's side, pairing,
 * notifications, what to send and each one's set-up, keeping it running, then a real copy sent
 * to the computer. Back and Skip on every page; everything is in the app afterwards too.
 * [onFinish] leaves it for good. [first] and [sdk] are for tests.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OnboardingScreen(
    state: AppState,
    actions: AppActions,
    onFinish: () -> Unit,
    first: OnboardingStep = OnboardingStep.WELCOME,
    sdk: Int = Build.VERSION.SDK_INT,
) {
    var copies by rememberSaveable { mutableStateOf(true) }
    var screenshots by rememberSaveable { mutableStateOf(false) }
    var codes by rememberSaveable { mutableStateOf(false) }
    val chosen = buildSet {
        if (copies) add(Feature.COPIES)
        if (screenshots) add(Feature.SCREENSHOTS)
        if (codes) add(Feature.CODES)
    }
    val steps = onboardingSteps(chosen, asksForNotifications = sdk >= Build.VERSION_CODES.TIRAMISU)
    var index by rememberSaveable { mutableIntStateOf(steps.indexOf(first).coerceAtLeast(0)) }
    val step = steps[index.coerceIn(steps.indices)]
    val next: () -> Unit = {
        if (step == OnboardingStep.CHOOSE) {
            // What was chosen is turned on now, so its page can set it up. Screenshots wait for
            // their page to ask for photo access, unless Android gave it already.
            if (copies != state.autoCopy.on) actions.autoCopy.onChange(copies)
            if (codes != state.codes.on) actions.codes.onChange(codes)
            val photos = state.screenshots
            if (screenshots != photos.wanted && (!screenshots || photos.access == PhotoAccess.FULL)) {
                actions.onScreenshotsChange(screenshots)
            }
        }
        if (index < steps.lastIndex) index++ else onFinish()
    }
    BackHandler(enabled = index > 0) { index-- }
    val snackbar = rememberMessages(state.message, actions.onMessageShown)

    Scaffold(
        topBar = {
            TopAppBar(
                title = {},
                navigationIcon = {
                    if (index > 0) {
                        IconButton(onClick = { index-- }) {
                            Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = stringResource(R.string.back))
                        }
                    }
                },
                actions = {
                    if (step != OnboardingStep.TRY) TextButton(onClick = onFinish) { Text(stringResource(R.string.onboarding_skip)) }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.surface),
            )
        },
        bottomBar = { BottomBar(index, steps.size, step, onboardingStepDone(step, state), next) },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        // A scroll per page: each starts at its top.
        val scroll = key(step) { rememberScrollState() }
        Column(
            modifier = Modifier.fillMaxSize().padding(padding).verticalScroll(scroll),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(
                modifier = Modifier.widthIn(max = MAX_WIDTH).fillMaxWidth().padding(horizontal = 24.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                when (step) {
                    OnboardingStep.WELCOME -> Welcome()
                    OnboardingStep.COMPUTER -> OnTheComputer()
                    OnboardingStep.PAIR -> PairPage(state, actions)
                    OnboardingStep.NOTIFICATIONS -> Notifications(state, actions)
                    OnboardingStep.CHOOSE -> Choose(
                        copies = copies,
                        screenshots = screenshots,
                        codes = codes,
                        onCopies = { copies = it },
                        onScreenshots = { screenshots = it },
                        onCodes = { codes = it },
                    )
                    OnboardingStep.COPIES -> {
                        Title(R.string.setup_title)
                        SetUpContent(state, actions, onDone = null)
                    }
                    OnboardingStep.SCREENSHOTS -> {
                        Title(R.string.screenshots_title)
                        Body(stringResource(R.string.screenshots_explain))
                        PhotoAccessCard(state.screenshots, state.photoRequestDenied, actions)
                    }
                    OnboardingStep.CODES -> {
                        Title(R.string.codes_title)
                        Body(stringResource(R.string.codes_privacy))
                        CodesAccess(state.codes, actions.codes) {
                            // Android 15's hidden codes need the automatic copy's shell: its page first.
                            copies = true
                            if (!state.autoCopy.on) actions.autoCopy.onChange(true)
                            index = onboardingSteps(chosen + Feature.COPIES, sdk >= Build.VERSION_CODES.TIRAMISU).indexOf(OnboardingStep.COPIES)
                        }
                    }
                    OnboardingStep.KEEP_ALIVE -> {
                        Title(R.string.keepalive_title)
                        KeepAliveChecklist(state.keepAlive, actions.keepAlive)
                    }
                    OnboardingStep.TRY -> TryIt(state, actions)
                }
            }
        }
    }
    PairingFlow(state.pairing, state.computers, actions.onConfirmPairing, actions.onCancelPairing)
}

/** The dots that say where the guide is, and the one button that goes on. */
@Composable
private fun BottomBar(index: Int, count: Int, step: OnboardingStep, done: Boolean, onNext: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.surface) {
        Row(
            modifier = Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 24.dp, vertical = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val where = stringResource(R.string.setup_progress, index + 1, count)
            Row(
                modifier = Modifier.weight(1f).clearAndSetSemantics { contentDescription = where },
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                repeat(count) { dot ->
                    Box(
                        modifier = Modifier
                            .height(8.dp)
                            .width(if (dot == index) 20.dp else 8.dp)
                            .background(
                                if (dot == index) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outlineVariant,
                                CircleShape,
                            ),
                    )
                }
            }
            val label = when {
                step == OnboardingStep.WELCOME -> R.string.welcome_start
                step == OnboardingStep.COMPUTER -> R.string.computer_ready
                step == OnboardingStep.TRY -> R.string.onboarding_finish
                done -> R.string.onboarding_next
                else -> R.string.onboarding_later
            }
            // Something left to do on the page: going on without it is the quiet choice.
            if (done) {
                Button(onClick = onNext) { Text(stringResource(label)) }
            } else {
                TextButton(onClick = onNext) { Text(stringResource(label)) }
            }
        }
    }
}

@Composable
private fun Title(@StringRes text: Int) {
    Text(stringResource(text), style = MaterialTheme.typography.headlineMedium, modifier = Modifier.semantics { heading() })
}

@Composable
private fun Body(text: String) {
    Text(text, style = MaterialTheme.typography.bodyLarge)
}

/** What paste-get is, in three promises. */
@Composable
private fun ColumnScope.Welcome() {
    Illustration()
    Text(
        stringResource(R.string.welcome_title),
        style = MaterialTheme.typography.displaySmall,
        textAlign = TextAlign.Center,
        modifier = Modifier.fillMaxWidth().semantics { heading() },
    )
    Column(verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.align(Alignment.CenterHorizontally)) {
        Promise(R.string.welcome_wifi)
        Promise(R.string.welcome_encrypted)
        Promise(R.string.welcome_free)
    }
}

@Composable
private fun Promise(@StringRes text: Int) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Icon(painterResource(R.drawable.ic_check), contentDescription = null, tint = MaterialTheme.colorScheme.primary)
        Text(stringResource(text), style = MaterialTheme.typography.bodyLarge)
    }
}

/**
 * The phone, the computer, and a clip going from one to the other, once. Still when the phone's
 * animations are off.
 */
@Composable
private fun Illustration() {
    val context = LocalContext.current
    val still = remember { animationsOff(context) }
    val travel = remember { Animatable(if (still) 1f else 0f) }
    LaunchedEffect(Unit) {
        if (still) return@LaunchedEffect
        delay(400)
        travel.animateTo(1f, tween(durationMillis = 1_200))
    }
    val description = stringResource(R.string.welcome_illustration)
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 24.dp).clearAndSetSemantics { contentDescription = description },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Device(R.drawable.ic_smartphone)
        BoxWithConstraints(modifier = Modifier.weight(1f).height(16.dp).padding(horizontal = 12.dp)) {
            Box(
                modifier = Modifier
                    .align(Alignment.CenterStart)
                    .fillMaxWidth()
                    .height(2.dp)
                    .background(MaterialTheme.colorScheme.outlineVariant, CircleShape),
            )
            Box(
                modifier = Modifier
                    .align(Alignment.CenterStart)
                    // Read at layout time: the dot moves without recomposing.
                    .offset { IntOffset(((maxWidth - 16.dp) * travel.value).roundToPx(), 0) }
                    .size(16.dp)
                    .background(MaterialTheme.colorScheme.primary, CircleShape),
            )
        }
        Device(R.drawable.ic_computer)
    }
}

@Composable
private fun Device(@DrawableRes icon: Int) {
    Box(
        modifier = Modifier.size(88.dp).background(MaterialTheme.colorScheme.primaryContainer, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(icon), contentDescription = null, tint = MaterialTheme.colorScheme.onPrimaryContainer, modifier = Modifier.size(44.dp))
    }
}

/** Settings → Developer options → Remove animations (or animator scale off). */
private fun animationsOff(context: Context): Boolean =
    Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f

/** The computer's side: where to get paste-get, as text only (the app makes no request). */
@Composable
private fun OnTheComputer() {
    Title(R.string.computer_title)
    Body(stringResource(R.string.computer_text))
    Card(modifier = Modifier.fillMaxWidth()) {
        SelectionContainer {
            Text(
                stringResource(R.string.computer_url),
                style = MaterialTheme.typography.titleMedium,
                fontFamily = FontFamily.Monospace,
                modifier = Modifier.padding(16.dp),
            )
        }
    }
    Text(
        stringResource(R.string.computer_after),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** The scan, then the pairing sheet over the page; a check and a buzz once paired. */
@Composable
private fun PairPage(state: AppState, actions: AppActions) {
    val paired = state.computers.isNotEmpty()
    val haptics = LocalHapticFeedback.current
    var wasPaired by remember { mutableStateOf(paired) }
    LaunchedEffect(paired) {
        if (paired && !wasPaired) haptics.performHapticFeedback(HapticFeedbackType.Confirm)
        wasPaired = paired
    }
    Title(R.string.hero_pair_title)
    Body(stringResource(R.string.onboarding_pair_text))
    if (paired) {
        Done(stringResource(R.string.paired_with, state.computers.joinToString { it.name }))
    } else {
        Button(onClick = actions.onPair, enabled = state.pairing != PairingState.Busy) { Text(stringResource(R.string.hero_scan)) }
    }
}

/** Why notifications matter here, then Android's question; Settings once it stops asking. */
@Composable
private fun Notifications(state: AppState, actions: AppActions) {
    var asked by rememberSaveable { mutableStateOf(false) }
    val allowed = state.keepAlive.checks.notifications
    Title(R.string.setup_notifications)
    Body(stringResource(R.string.onboarding_notifications_text))
    when {
        allowed -> Done(stringResource(R.string.onboarding_notifications_ok))
        !asked -> Button(onClick = {
            asked = true
            actions.onRequestNotifications()
        }) { Text(stringResource(R.string.builtin_allow_notifications)) }
        else -> {
            // Android asks twice at most: after that, only Settings can.
            Text(stringResource(R.string.onboarding_notifications_denied), style = MaterialTheme.typography.bodyMedium)
            OutlinedButton(onClick = actions.autoCopy.onOpenNotificationSettings) { Text(stringResource(R.string.open_settings)) }
        }
    }
}

/** A check and what's done, read out when it appears. */
@Composable
private fun Done(text: String) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.primaryContainer,
            contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
        ),
    ) {
        Row(
            modifier = Modifier.padding(16.dp).semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(painterResource(R.drawable.ic_check_circle), contentDescription = null)
            Text(text, style = MaterialTheme.typography.titleMedium)
        }
    }
}

/** The three features, each with what it sends and keeps to itself. */
@Composable
private fun Choose(
    copies: Boolean,
    screenshots: Boolean,
    codes: Boolean,
    onCopies: (Boolean) -> Unit,
    onScreenshots: (Boolean) -> Unit,
    onCodes: (Boolean) -> Unit,
) {
    Title(R.string.choose_title)
    Body(stringResource(R.string.choose_text))
    Choice(R.string.copies_title, R.string.choose_copies_text, R.drawable.ic_content_copy, copies, onCopies, recommended = true)
    Choice(R.string.screenshots_title, R.string.choose_screenshots_text, R.drawable.ic_screenshot, screenshots, onScreenshots)
    Choice(R.string.codes_title, R.string.choose_codes_text, R.drawable.ic_password, codes, onCodes)
}

@Composable
private fun Choice(
    @StringRes title: Int,
    @StringRes text: Int,
    @DrawableRes icon: Int,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
    recommended: Boolean = false,
) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (recommended) {
                Surface(shape = CircleShape, color = MaterialTheme.colorScheme.tertiaryContainer) {
                    Text(
                        stringResource(R.string.choose_recommended),
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onTertiaryContainer,
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
                    )
                }
            }
            SwitchRow(stringResource(title), stringResource(text), checked, onChange, icon = icon)
        }
    }
}

/**
 * A sentence copied on the phone, and the proof it went: "Arrived on HP-AIO in 42 ms", from
 * the computer's acknowledgement. With the automatic copy working it goes out as any copy
 * would; otherwise it is sent by hand. After [TRY_TIMEOUT_MS] without it, what to check.
 */
@Composable
private fun TryIt(state: AppState, actions: AppActions) {
    Title(R.string.try_title)
    if (state.computers.isEmpty()) {
        Body(stringResource(R.string.try_no_computer))
        Button(onClick = actions.onPair, enabled = state.pairing != PairingState.Busy) { Text(stringResource(R.string.hero_scan)) }
        return
    }
    val context = LocalContext.current
    val haptics = LocalHapticFeedback.current
    var sentence by rememberSaveable { mutableStateOf(testSentence(context)) }
    // The newest entry of the activity log when the sentence went: what came after is its.
    var after by rememberSaveable { mutableStateOf<Long?>(null) }
    var attempt by rememberSaveable { mutableIntStateOf(0) }
    var timedOut by rememberSaveable { mutableStateOf(false) }
    val arrived = after?.let { last -> state.activity.firstOrNull { it.id > last && arrivedTest(it) } }
    LaunchedEffect(attempt) {
        if (attempt == 0) return@LaunchedEffect
        delay(TRY_TIMEOUT_MS)
        timedOut = true
    }
    LaunchedEffect(arrived?.id) { if (arrived != null) haptics.performHapticFeedback(HapticFeedbackType.Confirm) }
    val send: () -> Unit = {
        // A new sentence each time: the same text twice is only sent once.
        sentence = testSentence(context)
        after = state.activity.firstOrNull()?.id ?: 0L
        timedOut = false
        attempt++
        actions.onTryIt(sentence, state.autoCopy.health.health != Health.WORKING)
    }

    Body(stringResource(R.string.try_text))
    Surface(modifier = Modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surfaceContainerHigh) {
        Text(sentence, style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(16.dp))
    }
    when {
        arrived != null -> {
            val delivery = arrived.deliveries.first()
            val name = state.computers.firstOrNull { it.deviceId == delivery.deviceId }?.name ?: stringResource(R.string.activity_a_computer)
            Done(
                if (delivery.accepted) {
                    stringResource(R.string.try_arrived, name, delivery.millis)
                } else {
                    stringResource(R.string.try_refused, name)
                },
            )
        }
        after == null -> Button(onClick = send) { Text(stringResource(R.string.try_button)) }
        !timedOut -> Row(
            modifier = Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            CircularProgressIndicator(modifier = Modifier.size(24.dp), strokeWidth = 3.dp)
            Text(stringResource(R.string.try_waiting), style = MaterialTheme.typography.bodyLarge)
        }
        else -> Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.errorContainer,
                contentColor = MaterialTheme.colorScheme.onErrorContainer,
            ),
        ) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Column(modifier = Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }) {
                    Text(stringResource(R.string.try_failed_title), style = MaterialTheme.typography.titleMedium)
                    Text(stringResource(R.string.hero_looking_long), style = MaterialTheme.typography.bodyMedium)
                }
                Button(onClick = send) { Text(stringResource(R.string.hero_try_again)) }
            }
        }
    }
}

/** A text sent from the phone that a computer answered. */
private fun arrivedTest(entry: Activity): Boolean =
    entry.sent && entry.kind == ActivityKind.TEXT && entry.deliveries.isNotEmpty()

/** "Sent from my phone with paste-get at 14:02:33". */
private fun testSentence(context: Context): String =
    context.getString(R.string.try_sentence, DateFormat.getTimeInstance(DateFormat.MEDIUM).format(Date()))

/** How long the test sentence may take before the guide says what to check. */
const val TRY_TIMEOUT_MS = 5_000L
