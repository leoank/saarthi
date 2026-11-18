package me.leoank.saarthi

import kotlin.math.roundToInt
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.draw.drawBehind
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.foundation.layout.offset
import androidx.compose.runtime.withFrameNanos
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.material.icons.filled.ChevronRight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.Done
import androidx.compose.material.icons.filled.DoneAll
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Language
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material.icons.filled.Terminal
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SmallFloatingActionButton
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.foundation.border
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.layout.heightIn
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material3.LocalContentColor
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.saarthi_core.QueuedPrompt
import uniffi.saarthi_core.Item
import uniffi.saarthi_core.Role
import uniffi.saarthi_core.SessionInfo

/** One session: its requests waiting at the top, the conversation live, and a prompt
 *  box; interrupt when the session runs under `saarthi run` or was spawned, kill for any. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SessionScreen(
    model: HubModel,
    snackbar: SnackbarHostState,
    nodeId: String,
    sessionId: String,
    findFirst: String?,
    back: () -> Unit,
    openHealth: () -> Unit,
    /** Opens another session of the same node, by id: one a resume forked into. */
    openSession: (String) -> Unit = {},
) {
    val live = model.sessions.firstOrNull { it.nodeId == nodeId && it.id == sessionId }
    val s = live ?: model.openSession ?: return
    val ended = live == null
    val node = model.node(nodeId)?.name ?: "node"
    var menu by remember { mutableStateOf(false) }
    var confirmKill by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    var showUsage by remember { mutableStateOf(false) }
    // Search in this session: the words, null while not searching; which match is shown.
    var find by remember(nodeId, sessionId) { mutableStateOf(findFirst) }
    // Matches count from the oldest; null is the newest, where a search starts, so lines
    // arriving in a live session do not move the one shown.
    var findAt by remember(nodeId, sessionId) { mutableStateOf<Int?>(null) }
    var findCount by remember(nodeId, sessionId) { mutableStateOf(0) }
    val clipboard = LocalClipboardManager.current
    // The session's unsent draft, kept by the model so it outlives this screen.
    var text by remember(nodeId, sessionId) { mutableStateOf(model.draft(nodeId, sessionId)) }
    LaunchedEffect(nodeId, sessionId, text) { model.setDraft(nodeId, sessionId, text) }
    val tokensIn = model.usage.sumOf { it.inputTokens }
    val tokensOut = model.usage.sumOf { it.outputTokens }
    val cost = model.usage.sumOf { it.costUsd }

    CompositionLocalProvider(LocalWrap provides Wrap(model.wrap, model::toggleWrap)) {
        Scaffold(
            topBar = {
                TopAppBar(
                    title = {
                        if (find != null) {
                            val focus = remember { FocusRequester() }
                            LaunchedEffect(Unit) { focus.requestFocus() }
                            BasicTextField(
                                value = find.orEmpty(),
                                onValueChange = { find = it; findAt = null },
                                singleLine = true,
                                textStyle = MaterialTheme.typography.titleMedium.copy(color = MaterialTheme.colorScheme.onSurface),
                                cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                                decorationBox = { field ->
                                    Box(contentAlignment = Alignment.CenterStart) {
                                        if (find.isNullOrEmpty()) {
                                            Text("Search this session", style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                        }
                                        field()
                                    }
                                },
                                modifier = Modifier.fillMaxWidth().focusRequester(focus),
                            )
                            return@TopAppBar
                        }
                        Column {
                            Text(s.title(), maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(
                                listOf(
                                    node,
                                    when {
                                        ended -> "ended"
                                        model.pendingFor(s).isNotEmpty() -> "waiting for you"
                                        s.busy -> "working"
                                        else -> "idle"
                                    },
                                    when {
                                        s.wrapped -> "saarthi run"
                                        s.spawned && s.profile.isNotEmpty() -> "spawned in ${s.profile}"
                                        s.spawned -> "spawned"
                                        else -> "started outside saarthi"
                                    },
                                    model.contextOf(s)?.takeIf { it.maxTokens > 0uL }?.let {
                                        "context ${(it.usedTokens.toDouble() * 100 / it.maxTokens.toDouble()).roundToInt()}%"
                                    },
                                    if (tokensIn + tokensOut > 0uL) "${thousands(tokensIn)} in · ${thousands(tokensOut)} out" + (if (cost > 0.0) " · ${dollars(cost)}" else "") else null,
                                ).filterNotNull().joinToString(" · "),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    },
                    navigationIcon = { IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") } },
                    actions = {
                        if (find != null) {
                            if (find.orEmpty().trim().length >= 2) {
                                Text(
                                    if (findCount == 0) "none" else "${(findAt ?: (findCount - 1)) + 1} of $findCount",
                                    style = MaterialTheme.typography.labelLarge,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                            // Up to the older match, down to the newer, around at the ends.
                            val at = findAt ?: (findCount - 1)
                            IconButton(onClick = { findAt = (at - 1).mod(findCount.coerceAtLeast(1)) }, enabled = findCount > 1) {
                                Icon(Icons.Filled.KeyboardArrowUp, "Older match")
                            }
                            IconButton(onClick = { findAt = (at + 1).mod(findCount.coerceAtLeast(1)) }, enabled = findCount > 1) {
                                Icon(Icons.Filled.KeyboardArrowDown, "Newer match")
                            }
                            IconButton(onClick = { find = null }) { Icon(Icons.Filled.Close, "Stop searching") }
                            return@TopAppBar
                        }
                        IconButton(onClick = { find = ""; findAt = null }) { Icon(Icons.Filled.Search, "Search this session") }
                        WrapButton()
                        Box {
                            IconButton(onClick = { menu = true }) { Icon(Icons.Filled.MoreVert, "More") }
                            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                                DropdownMenuItem(text = { Text("Usage") }, onClick = { menu = false; showUsage = true })
                                DropdownMenuItem(text = { Text("Node health") }, onClick = { menu = false; openHealth() })
                                // For `claude --resume <id>` at a terminal; the rest of the session's
                                // facts come with About (phase PHS-20261117-f22kxe item 19).
                                DropdownMenuItem(
                                    text = { Text("Copy session id") },
                                    onClick = {
                                        menu = false
                                        clipboard.setText(AnnotatedString(s.id))
                                        model.say("Session id copied: ${s.id}")
                                    },
                                )
                                if ((s.wrapped || s.spawned) && !ended) {
                                    DropdownMenuItem(
                                        text = { Text("Interrupt") },
                                        onClick = { menu = false; model.interrupt(s, null) },
                                    )
                                }
                                if (ended && model.resumable(s)) {
                                    DropdownMenuItem(
                                        text = { Text("Resume") },
                                        onClick = { menu = false; model.resume(s, "", openSession) },
                                    )
                                }
                                if (!ended) {
                                    DropdownMenuItem(
                                        text = { Text("Kill session", color = MaterialTheme.colorScheme.error) },
                                        onClick = { menu = false; confirmKill = true },
                                    )
                                } else {
                                    DropdownMenuItem(
                                        text = { Text("Delete session", color = MaterialTheme.colorScheme.error) },
                                        onClick = { menu = false; confirmDelete = true },
                                    )
                                }
                            }
                        }
                    },
                )
            },
            snackbarHost = { SnackbarHost(snackbar) },
            bottomBar = {
                if (!ended) {
                    val waiting = model.pendingFor(s)
                    // A single question waiting takes what is typed as its answer: an option's
                    // number, or the person's own words (phase PHS-20261117-f22kxe item 2).
                    val asked = waiting.singleOrNull()?.takeIf { typedAnswer(it, "1") != null }
                    Column {
                        // What waits on the person sits above the bar, not in the scroll.
                        val maxBody = (LocalConfiguration.current.screenHeightDp * 0.35f).dp
                        for (p in waiting) {
                            PendingCard(model, p, null, Modifier.padding(horizontal = 12.dp, vertical = 4.dp), maxBody = maxBody)
                        }
                        // Prompts the node holds until this turn ends, oldest first.
                        for (q in model.queueOf(s)) {
                            QueuedBubble(q, sendNow = { model.unqueue(s, q, true) }, discard = { model.unqueue(s, q, false) })
                        }
                        PromptBar(
                            text, { text = it },
                            stoppable = s.wrapped || s.spawned,
                            busy = s.busy,
                            waiting = waiting.isNotEmpty(),
                            answering = asked != null,
                            outgoing = "${s.nodeId}/${s.id}" in model.outgoing,
                            reconnecting = model.reconnecting,
                            notSent = model.notSent["${s.nodeId}/${s.id}"],
                            send = {
                                val answer = asked?.let { typedAnswer(it, text) }
                                if (asked != null && answer != null) {
                                    model.answer(asked, answer)
                                    text = ""
                                } else model.prompt(s, text.trim()) { text = "" }
                            },
                            stop = {
                                if (text.isBlank()) model.interrupt(s, null)
                                else model.interrupt(s, text.trim()) { text = "" }
                            },
                        )
                    }
                } else if (model.resumable(s)) {
                    // Sending resumes the session in place with the text as its next turn.
                    PromptBar(
                        text, { text = it },
                        stoppable = false,
                        busy = false,
                        waiting = false,
                        resuming = true,
                        send = { model.resume(s, text.trim(), openSession); text = "" },
                        stop = {},
                    )
                }
            },
        ) { padding ->
            Column(Modifier.fillMaxSize().padding(padding)) {
                if (model.reconnecting) {
                    Surface(color = MaterialTheme.colorScheme.errorContainer, modifier = Modifier.fillMaxWidth()) {
                        Caption("Reconnecting to ${model.hubName}…", color = MaterialTheme.colorScheme.onErrorContainer)
                    }
                }
                if (ended) {
                    Surface(color = MaterialTheme.colorScheme.surfaceVariant, modifier = Modifier.fillMaxWidth()) {
                        Caption(
                            if (model.resumable(s)) "This session has ended · send a prompt to resume it here"
                            else "This session has ended"
                        )
                    }
                }
                // *Edit and resend* puts a prompt back in the bar, in place of what was there.
                Conversation(
                    model, s, Modifier.weight(1f), edit = { text = it },
                    find = find, findAt = findAt, found = { n -> findCount = n; if ((findAt ?: 0) >= n) findAt = null },
                )
            }
        }
    }

    if (showUsage) {
        AlertDialog(
            onDismissRequest = { showUsage = false },
            title = { Text("Usage") },
            text = {
                Column {
                    if (model.usage.isEmpty()) Text("No usage counted yet.")
                    for (u in model.usage) {
                        Text(u.model, style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
                        Text(
                            "${thousands(u.inputTokens)} in · ${thousands(u.outputTokens)} out · " +
                                "${thousands(u.cacheReadTokens)} cache read · ${thousands(u.cacheWriteTokens)} cache write\n" +
                                "${u.messages} messages" + (if (u.costUsd > 0.0) " · ${dollars(u.costUsd)} estimated" else ""),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Text(
                        "Counted once per message by the daemon, subagents included; the cost is an estimate from list prices.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(top = 12.dp),
                    )
                }
            },
            confirmButton = { TextButton(onClick = { showUsage = false }) { Text("Close") } },
        )
    }

    if (confirmDelete) {
        DeleteDialog(dismiss = { confirmDelete = false }) {
            confirmDelete = false
            model.deleteSession(s) { back() }
        }
    }

    if (confirmKill) {
        AlertDialog(
            onDismissRequest = { confirmKill = false },
            title = { Text("Kill this session?") },
            text = {
                Text(
                    if (s.wrapped || s.spawned) "Claude Code in ${s.cwd} is stopped at once. Nothing is saved beyond its transcript."
                    else "Claude Code in ${s.cwd} was started outside saarthi: it gets SIGTERM, and if it runs in a terminal, that terminal's session ends. Nothing is saved beyond its transcript."
                )
            },
            confirmButton = {
                TextButton(onClick = { confirmKill = false; model.kill(s); back() }) {
                    Text("Kill", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { confirmKill = false }) { Text("Cancel") } },
        )
    }
}

/** One row of the conversation: an item, with a tool call's result attached to it. */
private data class Entry(val item: Item, val result: Item?)

private fun entries(items: List<Item>): List<Entry> {
    val out = ArrayList<Entry>(items.size)
    var i = 0
    while (i < items.size) {
        val it = items[i]
        if (it.role == Role.TOOL) {
            // The result follows its call, at most a notice or two in between.
            var j = i + 1
            while (j < items.size && j <= i + 3 && items[j].role == Role.SYSTEM) j++
            if (j < items.size && items[j].role == Role.TOOL_RESULT) {
                out += Entry(it, items[j])
                for (k in i + 1 until j) out += Entry(items[k], null)
                i = j + 1
                continue
            }
        }
        out += Entry(it, null); i++
    }
    return out
}

/** One prompt and what Claude did after it, up to the next prompt. [finished] once Claude
 *  Code wrote its turn line, or another prompt followed (an interrupted turn). */
private data class Turn(val prompt: Entry?, val body: List<Entry>, val finished: Boolean) {
    val start: ULong = (prompt ?: body.first()).item.seq
    /** Worth folding: a tool call, thinking, or more than one reply. */
    val foldable = body.any { it.item.role == Role.TOOL || it.item.role == Role.THINKING } ||
        body.count { it.item.role == Role.ASSISTANT } > 1
}

private fun turns(entries: List<Entry>): List<Turn> {
    val out = ArrayList<Turn>()
    var prompt: Entry? = null
    var body = ArrayList<Entry>()
    fun close(finished: Boolean) {
        if (prompt != null || body.isNotEmpty()) out += Turn(prompt, body, finished || body.any { it.item.role == Role.TURN })
    }
    for (e in entries) {
        if (e.item.role == Role.USER) {
            close(true)
            prompt = e
            body = ArrayList()
        } else {
            body += e
        }
    }
    close(false)
    return out
}

/** What the list shows: an entry, or the header a turn folds under. */
private sealed interface Line {
    data class Shown(val entry: Entry) : Line
    data class Header(val turn: Turn, val open: Boolean) : Line
}

/** A folded turn keeps its last reply, its turn line and its notices in view. */
private fun folded(turn: Turn): List<Entry> {
    val reply = turn.body.lastOrNull { it.item.role == Role.ASSISTANT }
    return turn.body.filter {
        it === reply || it.item.role in setOf(Role.TURN, Role.SYSTEM, Role.DAEMON)
    }
}

@Composable
private fun Conversation(
    model: HubModel,
    s: SessionInfo,
    modifier: Modifier,
    edit: (String) -> Unit,
    find: String? = null,
    findAt: Int? = null,
    found: (Int) -> Unit = {},
) {
    val list = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val items = model.conversation
    // Every row: the list is lazy, so only what is on screen is composed. A finished turn
    // folds under its header unless opened (phase PHS-20261117-f22kxe item 1); the running
    // one stays open.
    val turns = remember(items.size) { turns(entries(items)) }
    // Keys that stay with their rows when older pages come in above, so the list keeps its
    // place: an item by its seq and its place among that seq's items, a header by its turn.
    val rows: List<Line> = buildList {
        for (t in turns) {
            t.prompt?.let { add(Line.Shown(it)) }
            if (!t.foldable) {
                t.body.forEach { add(Line.Shown(it)) }
                continue
            }
            val open = model.turnOpen(s, t.start) ?: (!t.finished || model.openTurns)
            add(Line.Header(t, open))
            (if (open) t.body else folded(t)).forEach { add(Line.Shown(it)) }
        }
    }
    val sent = model.sending
    val waiting = model.pendingFor(s)
    val ended = model.sessions.none { it.nodeId == s.nodeId && it.id == s.id }
    val working = s.busy && !ended && waiting.isEmpty()
    // The person's last prompt: one still being sent, else the last in the transcript.
    val lastPrompt = if (sent.isNotEmpty()) rows.size + sent.size - 1 else rows.indexOfLast { it is Line.Shown && it.entry.item.role == Role.USER }
    val promptShown by remember(lastPrompt) {
        derivedStateOf { lastPrompt < 0 || list.layoutInfo.visibleItemsInfo.any { it.index == lastPrompt } }
    }
    // Follow new items while the person stays at the end. Only their own drag decides
    // otherwise (a programmatic scroll never does): once it settles, follow again if it
    // ended at the bottom, else stop until they come back down or tap the jump button.
    val atBottom by remember { derivedStateOf { !list.canScrollForward } }
    var stick by remember { mutableStateOf(true) }
    var dragged by remember { mutableStateOf(false) }
    LaunchedEffect(list) {
        list.interactionSource.interactions.collect { if (it is DragInteraction.Start) dragged = true }
    }
    LaunchedEffect(list.isScrollInProgress) {
        if (!list.isScrollInProgress && dragged) {
            dragged = false
            stick = !list.canScrollForward
        }
    }
    val count = rows.size + sent.size + (if (working) 1 else 0)
    // While searching, the match shown holds the view, not the end.
    val searching = (find?.trim()?.length ?: 0) >= 2
    LaunchedEffect(count) {
        if (count > 0 && stick && !searching) list.scrollToItem(count - 1, Int.MAX_VALUE)
    }
    // Search in the session: every entry whose text has the words, oldest first; the one
    // shown (the newest unless one was picked) opens its turn if folded, and is scrolled to
    // and marked.
    val query = find?.trim().orEmpty()
    val matches: List<Pair<Turn, Entry>> = remember(turns, query) {
        if (query.length < 2) emptyList()
        else turns.flatMap { t -> (listOfNotNull(t.prompt) + t.body).filter { it.item.text.contains(query, ignoreCase = true) }.map { t to it } }
    }
    LaunchedEffect(matches.size) { found(matches.size) }
    val at = findAt ?: matches.lastIndex
    val current = matches.getOrNull(at)?.second?.item?.seq
    val latestRows by rememberUpdatedState(rows)
    // Again once every older page is in: pages coming in above move what was found.
    LaunchedEffect(current, query, model.olderMore) {
        val (turn, entry) = matches.getOrNull(at) ?: return@LaunchedEffect
        if (turn.foldable && model.turnOpen(s, turn.start) != true) model.setTurnOpen(s, turn.start, true)
        stick = false
        // The opened turn's rows arrive with the next composition.
        repeat(30) {
            val i = latestRows.indexOfFirst { it is Line.Shown && it.entry === entry }
            if (i >= 0) {
                list.animateScrollToItem(i)
                return@LaunchedEffect
            }
            withFrameNanos { }
        }
    }
    val keys = remember(rows) {
        val seen = HashMap<ULong, Int>()
        rows.map { line ->
            when (line) {
                is Line.Shown -> { val k = seen.merge(line.entry.item.seq, 1, Int::plus)!!; "${line.entry.item.seq}-$k" }
                is Line.Header -> "turn-${line.turn.start}"
            }
        }
    }
    // Older pages: near the top, the next comes in; a search brings them all, so it finds
    // what was said before the session opened.
    val nearTop by remember { derivedStateOf { list.firstVisibleItemIndex <= 2 } }
    LaunchedEffect(nearTop, model.olderMore, model.olderLoading, query.length >= 2, rows.isEmpty()) {
        if ((nearTop || query.length >= 2) && rows.isNotEmpty() && model.olderMore && !model.olderLoading) model.loadOlder()
    }
    // The person's prompts, for the minimap at the edge.
    val prompts = rows.mapIndexedNotNull { i, line -> if (line is Line.Shown && line.entry.item.role == Role.USER) i to line.entry.item.text else null }
    val inView by remember {
        derivedStateOf { list.layoutInfo.visibleItemsInfo.let { v -> if (v.isEmpty()) IntRange.EMPTY else v.first().index..v.last().index } }
    }
    Box(modifier.fillMaxSize()) {
        if (rows.isEmpty() && waiting.isEmpty()) {
            Empty("Nothing here yet", "The conversation appears as the session's transcript is written.")
        }
        // A long press selects text to copy, as in any other app.
        SelectionContainer {
        LazyColumn(
            state = list,
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
            modifier = Modifier.fillMaxSize(),
        ) {
            itemsIndexed(rows, key = { i, _ -> keys[i] }) { i, line ->
                when (line) {
                    is Line.Header -> TurnHeader(
                        line.turn,
                        line.open,
                        working = working && !line.turn.finished,
                    ) { model.setTurnOpen(s, line.turn.start, !line.open) }
                    is Line.Shown -> {
                        val e = line.entry
                        val before = (rows.getOrNull(i - 1) as? Line.Shown)?.entry?.item
                        if (before != null && e.item.timeMs - before.timeMs > 10 * 60_000 || i == 0) {
                            Caption(whenLabel(e.item.timeMs), Modifier.padding(top = if (i == 0) 0.dp else 8.dp))
                        }
                        val marked = current != null && e.item.seq == current
                        Box(
                            if (marked) Modifier.background(MaterialTheme.colorScheme.tertiaryContainer.copy(alpha = 0.45f), RoundedCornerShape(10.dp))
                            else Modifier,
                        ) {
                            ConversationRow(e, s.cwd, delivered = e.item.role == Role.USER && e.item.text in model.delivered, edit = edit)
                        }
                    }
                }
            }
            itemsIndexed(sent, key = { _, x -> "sent" + x.timeMs }) { _, x ->
                SentTurn(x)
            }
            if (working) item(key = "working") { Working() }
        }
        }
        // An older page coming in: over the list, so its rows keep their places.
        if (model.olderLoading) {
            CircularProgressIndicator(
                Modifier.align(Alignment.TopCenter).padding(top = 48.dp).size(22.dp),
                strokeWidth = 2.dp,
            )
        }
        if (prompts.size >= 3 && (list.canScrollBackward || list.canScrollForward)) {
            PromptMap(
                prompts,
                inView,
                jump = { row -> stick = false; scope.launch { list.animateScrollToItem(row) } },
                modifier = Modifier.align(Alignment.CenterEnd).fillMaxHeight().padding(vertical = 56.dp),
            )
        }
        AnimatedVisibility(
            visible = !promptShown,
            modifier = Modifier.align(Alignment.TopCenter).padding(top = 8.dp),
        ) {
            Surface(
                onClick = {
                    stick = false
                    scope.launch { list.animateScrollToItem(lastPrompt) }
                },
                shape = RoundedCornerShape(16.dp),
                color = MaterialTheme.colorScheme.secondaryContainer,
                shadowElevation = 2.dp,
            ) {
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(start = 10.dp, end = 14.dp, top = 6.dp, bottom = 6.dp)) {
                    Icon(Icons.Filled.KeyboardArrowUp, null, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(4.dp))
                    Text("Your last prompt", style = MaterialTheme.typography.labelLarge)
                }
            }
        }
        AnimatedVisibility(
            visible = !atBottom && rows.isNotEmpty(),
            modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp),
        ) {
            SmallFloatingActionButton(onClick = {
                stick = true
                scope.launch { list.animateScrollToItem(count - 1, Int.MAX_VALUE) }
            }) {
                Icon(Icons.Filled.KeyboardArrowDown, "Jump to latest")
            }
        }
    }
}

/** The person's prompts as ticks down the right edge, those on screen bright: a touch or a
 *  drag shows the nearest prompt beside it, and letting go scrolls there. */
@Composable
private fun PromptMap(prompts: List<Pair<Int, String>>, inView: IntRange, jump: (Int) -> Unit, modifier: Modifier) {
    var touched by remember { mutableStateOf<Int?>(null) }
    val bright = MaterialTheme.colorScheme.primary
    val dim = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.45f)
    val latest by rememberUpdatedState(prompts)
    BoxWithConstraints(modifier.width(260.dp)) {
        val height = maxHeight
        fun at(k: Int): Float = if (prompts.size == 1) 0.5f else k.toFloat() / (prompts.size - 1)
        Box(
            Modifier
                .align(Alignment.TopEnd)
                .width(24.dp)
                .fillMaxHeight()
                .pointerInput(Unit) {
                    fun nearest(y: Float): Int {
                        val n = latest.size
                        return if (n <= 1) 0 else ((y / size.height) * (n - 1)).roundToInt().coerceIn(0, n - 1)
                    }
                    awaitEachGesture {
                        val down = awaitFirstDown()
                        down.consume()
                        touched = nearest(down.position.y)
                        while (true) {
                            val change = awaitPointerEvent().changes.firstOrNull() ?: break
                            if (!change.pressed) break
                            change.consume()
                            touched = nearest(change.position.y)
                        }
                        touched?.let { k -> latest.getOrNull(k)?.let { jump(it.first) } }
                        touched = null
                    }
                }
                .drawBehind {
                    prompts.forEachIndexed { k, (row, _) ->
                        val y = size.height * at(k)
                        val lit = row in inView
                        drawLine(
                            if (lit) bright else dim,
                            start = Offset(size.width - 16.dp.toPx(), y),
                            end = Offset(size.width - 4.dp.toPx(), y),
                            strokeWidth = (if (lit) 3 else 2).dp.toPx(),
                            cap = StrokeCap.Round,
                        )
                    }
                },
        )
        touched?.let { k ->
            Surface(
                shape = RoundedCornerShape(12.dp),
                color = MaterialTheme.colorScheme.inverseSurface,
                shadowElevation = 3.dp,
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .offset(x = (-30).dp, y = (height * at(k) - 18.dp).coerceAtLeast(0.dp))
                    .widthIn(max = 220.dp),
            ) {
                Text(
                    prompts.getOrNull(k)?.second.orEmpty(),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.inverseOnSurface,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                )
            }
        }
    }
}

/** A turn's one line: how long Claude worked and what it did, "Worked for 2m 36s · 2 edits ·
 *  3 reads · 1 command", failures in red; *Working…* with a breathing dot while it runs. A
 *  tap opens or folds it. */
@Composable
private fun TurnHeader(turn: Turn, open: Boolean, working: Boolean, toggle: () -> Unit) {
    val calls = turn.body.filter { it.item.role == Role.TOOL }
    val count = { names: Set<String> -> calls.count { it.item.tool in names } }
    val edits = count(editTools)
    val reads = count(setOf("Read"))
    val commands = count(setOf("Bash"))
    val searches = count(setOf("Grep", "Glob", "WebSearch", "WebFetch", "ToolSearch"))
    val other = calls.size - edits - reads - commands - searches
    val failed = calls.count { it.result?.error == true }
    fun n(k: Int, one: String, many: String) = if (k == 0) null else "$k ${if (k == 1) one else many}"
    val parts = listOfNotNull(
        n(edits, "edit", "edits"),
        n(reads, "read", "reads"),
        n(commands, "command", "commands"),
        n(searches, "search", "searches"),
        n(other, "other tool", "other tools"),
    )
    val first = turn.prompt?.item?.timeMs ?: turn.body.first().item.timeMs
    val last = (turn.body.lastOrNull { it.item.role == Role.TURN } ?: turn.body.last()).item.timeMs
    val label = if (working) "Working…" else "Worked for ${span(last - first)}"
    val pulse by rememberInfiniteTransition(label = "pulse").animateFloat(
        initialValue = 0.3f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(900), RepeatMode.Reverse),
        label = "alpha",
    )
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .clickable(onClick = toggle)
            .padding(vertical = 6.dp),
    ) {
        Icon(
            if (open) Icons.Filled.ExpandMore else Icons.Filled.ChevronRight,
            if (open) "Fold this turn" else "Show every step of this turn",
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(18.dp),
        )
        Spacer(Modifier.width(6.dp))
        if (working) {
            Box(
                Modifier
                    .size(8.dp)
                    .graphicsLayer { alpha = pulse }
                    .background(MaterialTheme.colorScheme.primary, CircleShape),
            )
            Spacer(Modifier.width(8.dp))
        }
        Text(
            buildAnnotatedString {
                withStyle(SpanStyle(color = if (working) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface)) {
                    append(label)
                }
                withStyle(SpanStyle(color = MaterialTheme.colorScheme.onSurfaceVariant)) {
                    parts.forEach { append(" · $it") }
                }
                if (failed > 0) {
                    withStyle(SpanStyle(color = MaterialTheme.colorScheme.error)) { append(" · $failed failed") }
                }
            },
            style = MaterialTheme.typography.bodyMedium,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** A duration as a turn header says it: "45s", "2m 36s", "1h 4m". */
private fun span(ms: Long): String {
    val s = (ms / 1000).coerceAtLeast(1)
    return when {
        s < 60 -> "${s}s"
        s < 3600 -> "${s / 60}m ${s % 60}s"
        else -> "${s / 3600}h ${(s % 3600) / 60}m"
    }
}

/** The terminal's shapes, loosely: the person's prompt as a boxed line, Claude's text as
 *  rendered markdown, a tool call as one line with its output folded beneath, thinking
 *  folded, notices as captions. */
@Composable
private fun ConversationRow(e: Entry, cwd: String, delivered: Boolean = false, edit: (String) -> Unit = {}) {
    val item = e.item
    val clipboard = LocalClipboardManager.current
    when (item.role) {
        Role.USER -> Column(horizontalAlignment = Alignment.End, modifier = Modifier.fillMaxWidth()) {
            UserTurn(item.text, ticks = if (delivered) 2 else 0, timeMs = item.timeMs)
            Row {
                TurnAction(Icons.Filled.ContentCopy, "Copy") { clipboard.setText(AnnotatedString(item.text)) }
                TurnAction(Icons.Filled.Edit, "Edit and resend") { edit(item.text) }
            }
        }
        // Claude's text as it reads, unboxed; its time and the copies under it.
        Role.ASSISTANT -> Column(Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
            Markdown(item.text, Modifier.fillMaxWidth())
            Row(verticalAlignment = Alignment.CenterVertically) {
                var copy by remember { mutableStateOf(false) }
                Box {
                    TurnAction(Icons.Filled.ContentCopy, "Copy") { copy = true }
                    DropdownMenu(expanded = copy, onDismissRequest = { copy = false }) {
                        DropdownMenuItem(text = { Text("Copy as Markdown") }, onClick = {
                            copy = false
                            clipboard.setText(AnnotatedString(item.text))
                        })
                        DropdownMenuItem(text = { Text("Copy as text") }, onClick = {
                            copy = false
                            clipboard.setText(AnnotatedString(plainText(item.text)))
                        })
                    }
                }
                Spacer(Modifier.weight(1f))
                Text(
                    clockTime(item.timeMs),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                )
            }
        }
        Role.THINKING -> Folded(
            label = "Thinking",
            summary = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        ) {
            Text(
                item.text,
                style = MaterialTheme.typography.bodyMedium,
                fontStyle = FontStyle.Italic,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = 24.dp, top = 4.dp),
            )
        }
        Role.TOOL -> ToolCall(item, e.result, cwd)
        Role.TOOL_RESULT -> {
            var open by remember(item.seq) { mutableStateOf(false) }
            Box(Modifier.fillMaxWidth().clickable { open = !open }) { Output(item, open) }
        }
        Role.SYSTEM -> Caption(item.text)
        Role.DAEMON -> Caption(item.text.uppercase(), color = MaterialTheme.colorScheme.error)
        Role.TURN -> Text(
            item.text,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.End,
            modifier = Modifier.fillMaxWidth().padding(top = 2.dp, bottom = 6.dp),
        )
    }
}

/** Claude is on a turn: a turning star and the seconds since this was shown, so a long
 *  turn never looks like a frozen screen. */
@Composable
private fun Working() {
    val turn by rememberInfiniteTransition(label = "working").animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(tween(2400, easing = LinearEasing)),
        label = "turn",
    )
    var seconds by remember { mutableStateOf(0) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(1000)
            seconds++
        }
    }
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(vertical = 8.dp)) {
        Text(
            "✻",
            color = MaterialTheme.colorScheme.primary,
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.graphicsLayer { rotationZ = turn },
        )
        Spacer(Modifier.width(8.dp))
        Text(
            if (seconds < 2) "Working…" else "Working… ${seconds}s",
            style = MaterialTheme.typography.bodyMedium,
            fontStyle = FontStyle.Italic,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** A prompt the hub took from here, before the transcript echoes it: one tick, and the
 *  node's note under it. */
@Composable
private fun SentTurn(x: Sent) {
    Column(Modifier.fillMaxWidth()) {
        UserTurn(x.text, ticks = 1, timeMs = x.timeMs)
        x.note?.let {
            Text(
                it,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.End,
                modifier = Modifier.fillMaxWidth().padding(end = 8.dp, bottom = 4.dp),
            )
        }
    }
}

/** A prompt: a neutral card on the right, its time and ticks inside. */
@Composable
private fun UserTurn(text: String, ticks: Int = 0, timeMs: Long? = null) {
    BoxWithConstraints(Modifier.fillMaxWidth().padding(vertical = 4.dp), contentAlignment = Alignment.CenterEnd) {
        Row(
            verticalAlignment = Alignment.Bottom,
            modifier = Modifier
                .widthIn(max = maxWidth * 0.85f)
                .background(MaterialTheme.colorScheme.surfaceContainerHigh, RoundedCornerShape(16.dp))
                .padding(start = 14.dp, end = 10.dp, top = 9.dp, bottom = 9.dp),
        ) {
            Text(
                text,
                style = MaterialTheme.typography.bodyLarge,
                fontFamily = if (text.startsWith("! ")) FontFamily.Monospace else FontFamily.Default,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f, fill = false),
            )
            if (timeMs != null) {
                Text(
                    clockTime(timeMs),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                    modifier = Modifier.padding(start = 10.dp, end = if (ticks > 0) 4.dp else 0.dp),
                )
            }
            if (ticks > 0) {
                Icon(
                    if (ticks >= 2) Icons.Filled.DoneAll else Icons.Filled.Done,
                    contentDescription = if (ticks >= 2) "In the transcript" else "Sent",
                    tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                    modifier = Modifier.padding(bottom = 2.dp).size(14.dp),
                )
            }
        }
    }
}

/** A small action under a turn: an icon and a word, quiet until tapped. */
@Composable
private fun TurnAction(icon: ImageVector, label: String, onClick: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.clip(RoundedCornerShape(8.dp)).clickable(onClick = onClick).padding(horizontal = 6.dp, vertical = 4.dp),
    ) {
        Icon(icon, null, Modifier.size(14.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(label, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(start = 4.dp))
    }
}

/** Markdown as plain text, for pasting where it would show as symbols: fences, emphasis,
 *  code ticks and heading marks go; a link keeps its text and its address. */
internal fun plainText(md: String): String = md.lines()
    .filterNot { it.trimStart().startsWith("```") }
    .joinToString("\n") { line ->
        line.replace(Regex("^#{1,6}\\s+"), "")
            .replace(Regex("\\[([^\\]]+)]\\(([^)]+)\\)"), "$1 ($2)")
            .replace(Regex("(\\*\\*|__)(.+?)\\1"), "$2")
            .replace(Regex("(?<![\\w*])[*_](\\S(?:.*?\\S)?)[*_](?![\\w*])"), "$1")
            .replace("`", "")
    }

private fun toolIcon(tool: String): ImageVector = when (tool) {
    "Bash", "Monitor" -> Icons.Filled.Terminal
    "Edit", "Write", "NotebookEdit", "MultiEdit" -> Icons.Filled.Edit
    "Read", "Grep", "Glob", "ToolSearch" -> Icons.Filled.Search
    "WebFetch", "WebSearch" -> Icons.Filled.Language
    else -> Icons.Filled.Build
}

/** "Bash  cargo test" with its output folded to one line: the count of lines, or the
 *  error's first line; a tap opens the output and the call's detail. */
private val editTools = setOf("Edit", "Write", "NotebookEdit", "MultiEdit")

/** A tool call said as what it did: "Read", "Edited", "Ran"; an MCP tool as its server and
 *  tool, `mcp__saarthi__ask_user_question` as "saarthi · ask_user_question". */
private fun verb(tool: String): String = when (tool) {
    "Edit", "MultiEdit" -> "Edited"
    "Write" -> "Wrote"
    "NotebookEdit" -> "Edited notebook"
    "Bash" -> "Ran"
    "Grep" -> "Searched"
    "Glob" -> "Listed"
    "WebSearch" -> "Searched the web"
    "WebFetch" -> "Fetched"
    "ToolSearch" -> "Loaded tools"
    "TodoWrite" -> "Updated todos"
    "Task", "Agent" -> "Agent"
    else -> if (tool.startsWith("mcp__")) tool.removePrefix("mcp__").replace("__", " · ") else tool
}

@Composable
private fun ToolCall(call: Item, result: Item?, cwd: String) {
    // The diff Claude Code computed for the files the call changed shows at once, as the
    // terminal does; without one, an edit's own old and new text or a multi-line command
    // (the file being written, usually) shows instead. Everything else folds until tapped.
    val diff = result?.diff
    val script = call.tool == "Bash" && call.detail?.contains('\n') == true
    var open by remember(call.seq) { mutableStateOf(diff == null && (call.tool in editTools || script)) }
    val more = result != null || call.detail != null
    // A path under the session's directory reads better without it.
    val headline = call.text.removePrefix(cwd.trimEnd('/') + "/")
    Column(
        Modifier
            .fillMaxWidth()
            .clickable(enabled = more) { open = !open }
            .padding(vertical = 2.dp),
    ) {
        Row(verticalAlignment = Alignment.Top) {
            Icon(
                toolIcon(call.tool), null,
                tint = MaterialTheme.colorScheme.secondary,
                modifier = Modifier.padding(top = 2.dp).size(16.dp),
            )
            Spacer(Modifier.width(8.dp))
            Text(verb(call.tool), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.secondary)
            Spacer(Modifier.width(8.dp))
            Text(
                headline,
                fontFamily = FontFamily.Monospace,
                style = MaterialTheme.typography.bodySmall,
                maxLines = if (open) Int.MAX_VALUE else 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).padding(top = 2.dp),
            )
            if (more) {
                Icon(
                    if (open) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore, null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
        if (diff != null) DiffBlock(diff, headline, Modifier.padding(start = 24.dp), cwd = cwd)
        if (open) call.detail?.let {
            when {
                call.tool in editTools && diff == null -> DiffBlock(it, headline, Modifier.padding(start = 24.dp))
                call.tool in editTools -> {}
                script -> ScriptBlock(it, headline, Modifier.padding(start = 24.dp))
                else -> CodeBlock(it, Modifier.padding(start = 24.dp))
            }
        }
        if (result != null) Output(result, open && call.tool !in editTools && diff == null)
    }
}

/** A long command shown as the terminal does: the first lines; a tap shows it whole. */
@Composable
private fun ScriptBlock(text: String, title: String, modifier: Modifier = Modifier) {
    var full by remember(text) { mutableStateOf(false) }
    val lines = remember(text) { text.lines() }
    val shown = if (lines.size <= SHORT_LINES) lines else lines.take(FIRST_LINES)
    Column(modifier.clickable { full = true }) {
        CodeBlock(shown.joinToString("\n"))
        if (shown.size < lines.size) MoreLines(lines.size - shown.size)
    }
    if (full) FullScreen(title, lines, close = { full = false }) { Mono(it, MaterialTheme.colorScheme.onSurface) }
}

/** A diff longer than this shows its first lines, the rest full screen. */
private const val SHORT_LINES = 18
private const val FIRST_LINES = 15

/** Removed lines red, added lines green, as the terminal shows an edit; a `=== path` line
 *  names the file (relative to the session's directory), an `@@` line a hunk. A long one
 *  shows its first lines; a tap opens it whole, full screen. */
@Composable
private fun DiffBlock(text: String, title: String, modifier: Modifier = Modifier, cwd: String = "") {
    var full by remember(text) { mutableStateOf(false) }
    val lines = remember(text) { text.lines() }
    val shown = if (lines.size <= SHORT_LINES) lines else lines.take(FIRST_LINES)
    val wrap = LocalWrap.current.on
    Column(
        modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .background(MaterialTheme.colorScheme.surfaceContainerHigh, RoundedCornerShape(8.dp))
            .clickable { full = true }
            .padding(10.dp),
    ) {
        Column(Modifier.scrollUnlessWrapped(wrap)) {
            for (line in shown) DiffLine(line, cwd)
        }
        if (shown.size < lines.size) MoreLines(lines.size - shown.size)
    }
    if (full) {
        // The file, when the diff covers one, else the call's own headline.
        val file = lines.filter { it.startsWith("=== ") }
            .map { it.removePrefix("=== ").removePrefix(cwd.trimEnd('/') + "/") }
        val name = when (file.size) {
            0 -> title
            1 -> file[0]
            else -> "${file[0]} and ${file.size - 1} more"
        }
        FullScreen(name, lines, close = { full = false }) { DiffLine(it, cwd) }
    }
}

@Composable
private fun DiffLine(line: String, cwd: String) {
    val header = line.startsWith("=== ")
    Mono(
        if (header) line.removePrefix("=== ").removePrefix(cwd.trimEnd('/') + "/") else line,
        when {
            header -> MaterialTheme.colorScheme.secondary
            line.startsWith("@@") -> MaterialTheme.colorScheme.onSurfaceVariant
            line.startsWith("-") -> MaterialTheme.colorScheme.error
            line.startsWith("+") -> Color(0xFF2E7D32)
            else -> MaterialTheme.colorScheme.onSurface
        },
        header,
    )
}

/** One line of code or diff, wrapped or not as the app-wide choice says. */
@Composable
private fun Mono(text: String, color: Color, header: Boolean = false) {
    Text(
        text,
        fontFamily = FontFamily.Monospace,
        style = if (header) MaterialTheme.typography.labelLarge else MaterialTheme.typography.bodySmall,
        softWrap = LocalWrap.current.on,
        color = color,
        modifier = if (header) Modifier.padding(top = 4.dp) else Modifier,
    )
}

@Composable
private fun MoreLines(n: Int) {
    Text(
        "… $n more lines · tap to see all",
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(top = 4.dp),
    )
}

/** A diff or a script whole, full screen, one list row per line so a long one scrolls
 *  smoothly; the wrap toggle is in its bar too. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun FullScreen(title: String, lines: List<String>, close: () -> Unit, line: @Composable (String) -> Unit) {
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                    navigationIcon = { IconButton(onClick = close) { Icon(Icons.Filled.Close, "Close") } },
                    actions = { WrapButton() },
                )
            },
        ) { padding ->
            Box(Modifier.fillMaxSize().padding(padding).scrollUnlessWrapped(LocalWrap.current.on)) {
                LazyColumn(Modifier.fillMaxHeight(), contentPadding = PaddingValues(16.dp)) {
                    items(lines) { line(it) }
                }
            }
        }
    }
}

/** A tool's output: folded to "⎿ 12 lines" (or the error's first line), open as a block. */
@Composable
private fun Output(result: Item, open: Boolean) {
    val text = result.text.trim()
    val lines = if (text.isEmpty()) 0 else text.count { it == '\n' } + 1
    val color = if (result.error) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant
    Column(Modifier.padding(start = 24.dp)) {
        if (!open) {
            val summary = when {
                text.isEmpty() -> "no output"
                result.error -> "error · " + text.lineSequence().first()
                lines == 1 && text.length <= 60 -> text
                lines == 1 -> "1 line"
                else -> "$lines lines"
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("⎿", style = MaterialTheme.typography.bodySmall, color = color)
                Spacer(Modifier.width(6.dp))
                Text(
                    summary,
                    fontFamily = FontFamily.Monospace,
                    style = MaterialTheme.typography.bodySmall,
                    color = color,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        } else if (text.isNotEmpty()) {
            Text(
                text,
                fontFamily = FontFamily.Monospace,
                style = MaterialTheme.typography.bodySmall,
                color = color,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 4.dp)
                    .background(MaterialTheme.colorScheme.surfaceContainerLow, RoundedCornerShape(8.dp))
                    .padding(8.dp)
                    .scrollUnlessWrapped(LocalWrap.current.on),
                softWrap = LocalWrap.current.on,
            )
        }
    }
}

/** A labelled line that opens on a tap. */
@Composable
private fun Folded(label: String, summary: String?, tint: androidx.compose.ui.graphics.Color, content: @Composable () -> Unit) {
    var open by remember(label) { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().clickable { open = !open }.padding(vertical = 2.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("✻", color = tint, style = MaterialTheme.typography.bodySmall)
            Spacer(Modifier.width(8.dp))
            Text(label, style = MaterialTheme.typography.labelLarge, color = tint)
            summary?.let {
                Spacer(Modifier.width(8.dp))
                Text(it, style = MaterialTheme.typography.bodySmall, color = tint, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Spacer(Modifier.weight(1f))
            Icon(if (open) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore, null, tint = tint, modifier = Modifier.size(18.dp))
        }
        if (open) content()
    }
}

@Composable
private fun PromptBar(
    text: String,
    onText: (String) -> Unit,
    stoppable: Boolean,
    busy: Boolean,
    waiting: Boolean,
    resuming: Boolean = false,
    answering: Boolean = false,
    outgoing: Boolean = false,
    reconnecting: Boolean = false,
    notSent: String? = null,
    send: () -> Unit,
    stop: () -> Unit,
) {
    // Only the rounded input floats over the screen; no bar behind it.
    Box {
        Column(Modifier.navigationBarsPadding().imePadding().padding(horizontal = 12.dp, vertical = 8.dp)) {
            // One line on the session's state, where it changes what Send does.
            val (state, tint) = when {
                outgoing && reconnecting -> "Waiting for the hub · sent once it is back, or left here after 45 s" to MaterialTheme.colorScheme.onSurfaceVariant
                outgoing -> "Sending…" to MaterialTheme.colorScheme.onSurfaceVariant
                notSent != null -> notSent to MaterialTheme.colorScheme.error
                resuming -> "Ended · Send resumes it headless on its node, in place" to MaterialTheme.colorScheme.onSurfaceVariant
                answering -> "Claude asks · tap an option, or type its number or your own answer" to MaterialTheme.colorScheme.tertiary
                waiting -> "Waiting for you · answer the card above" to MaterialTheme.colorScheme.tertiary
                !stoppable -> "Started outside saarthi run · Claude sees this as a message from another session · no stop from here" to MaterialTheme.colorScheme.onSurfaceVariant
                busy -> "Claude is working · Queue holds a prompt for when this turn ends · the stop button interrupts" to MaterialTheme.colorScheme.onSurfaceVariant
                else -> null to MaterialTheme.colorScheme.onSurfaceVariant
            }
            state?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.labelSmall,
                    color = tint,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(start = 16.dp, bottom = 6.dp),
                )
            }
            // The composer: a rounded card with the text and the round buttons on one centre
            // line while it is one line; as it grows, the buttons stay at the bottom.
            Row(
                verticalAlignment = Alignment.Bottom,
                modifier = Modifier
                    .fillMaxWidth()
                    .shadow(2.dp, RoundedCornerShape(26.dp))
                    .background(
                        // Raised off the page: white on ivory, a lighter grey on charcoal.
                        if (isSystemInDarkTheme()) MaterialTheme.colorScheme.surfaceContainerHigh else MaterialTheme.colorScheme.surfaceContainerLowest,
                        RoundedCornerShape(26.dp),
                    )
                    .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(26.dp))
                    .padding(start = 18.dp, end = 8.dp, top = 8.dp, bottom = 8.dp),
            ) {
                BasicTextField(
                    value = text,
                    onValueChange = onText,
                    readOnly = outgoing,
                    textStyle = MaterialTheme.typography.bodyLarge.copy(color = MaterialTheme.colorScheme.onSurface),
                    cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                    maxLines = 6,
                    modifier = Modifier.weight(1f).heightIn(min = ROUND_BUTTON),
                    decorationBox = { field ->
                        Box(Modifier.heightIn(min = ROUND_BUTTON).padding(vertical = 6.dp), contentAlignment = Alignment.CenterStart) {
                            if (text.isEmpty()) {
                                Text(
                                    when {
                                        resuming -> "Resume with a prompt"
                                        answering -> "An option's number, or your answer"
                                        busy -> "Queue a prompt"
                                        else -> "Prompt Claude"
                                    },
                                    style = MaterialTheme.typography.bodyLarge,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                            field()
                        }
                    },
                )
                if (stoppable) {
                    RoundButton(
                        onClick = stop,
                        enabled = true,
                        container = MaterialTheme.colorScheme.error,
                        content = MaterialTheme.colorScheme.onError,
                    ) {
                        Icon(Icons.Filled.Stop, if (text.isBlank()) "Interrupt" else "Interrupt, then send", Modifier.size(18.dp))
                    }
                    Spacer(Modifier.width(8.dp))
                }
                RoundButton(
                    onClick = send,
                    enabled = text.isNotBlank() && !outgoing,
                    container = MaterialTheme.colorScheme.primary,
                    content = MaterialTheme.colorScheme.onPrimary,
                ) {
                    if (busy && !resuming) Icon(Icons.Filled.Schedule, "Queue", Modifier.size(20.dp))
                    else Icon(Icons.Filled.ArrowUpward, "Send", Modifier.size(20.dp))
                }
            }
        }
    }
}

private val ROUND_BUTTON = 36.dp

/** A prompt the node holds until the turn ends: *Send now* hands it to Claude, which takes it
 *  at its next step; the cross drops it. */
@Composable
private fun QueuedBubble(q: QueuedPrompt, sendNow: () -> Unit, discard: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 3.dp)
            .background(MaterialTheme.colorScheme.surfaceContainer, RoundedCornerShape(14.dp))
            .padding(start = 14.dp, end = 2.dp),
    ) {
        Icon(Icons.Filled.Schedule, "Queued", Modifier.size(14.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(
            q.text,
            style = MaterialTheme.typography.bodyMedium,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f).padding(horizontal = 10.dp, vertical = 8.dp),
        )
        TextButton(onClick = sendNow) { Text("Send now") }
        IconButton(onClick = discard) { Icon(Icons.Filled.Close, "Discard", Modifier.size(18.dp)) }
    }
}

/** A round button whose icon sits at its exact centre; dimmed while disabled. */
@Composable
private fun RoundButton(
    onClick: () -> Unit,
    enabled: Boolean,
    container: Color,
    content: Color,
    icon: @Composable () -> Unit,
) {
    val bg = if (enabled) container else MaterialTheme.colorScheme.surfaceContainerHigh
    val fg = if (enabled) content else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f)
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier
            .size(ROUND_BUTTON)
            .clip(CircleShape)
            .background(bg)
            .clickable(enabled = enabled, role = androidx.compose.ui.semantics.Role.Button, onClick = onClick),
    ) {
        CompositionLocalProvider(LocalContentColor provides fg) { icon() }
    }
}

/** Asks before a session is deleted from the hub for good (ADR-0065). */
@Composable
fun DeleteDialog(dismiss: () -> Unit, delete: () -> Unit) {
    AlertDialog(
        onDismissRequest = dismiss,
        title = { Text("Delete this session?") },
        text = {
            Text(
                "Its conversation is removed from the hub for good, and its machine cannot send it back. " +
                    "Claude Code's own transcript on that machine stays, so `claude --resume` still works there."
            )
        },
        confirmButton = { TextButton(onClick = delete) { Text("Delete", color = MaterialTheme.colorScheme.error) } },
        dismissButton = { TextButton(onClick = dismiss) { Text("Cancel") } },
    )
}
