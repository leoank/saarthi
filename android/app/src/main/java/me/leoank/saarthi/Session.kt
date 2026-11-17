package me.leoank.saarthi

import kotlin.math.roundToInt
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
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
import androidx.compose.material.icons.filled.Done
import androidx.compose.material.icons.filled.DoneAll
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Language
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Schedule
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
    back: () -> Unit,
    openHealth: () -> Unit,
) {
    val live = model.sessions.firstOrNull { it.nodeId == nodeId && it.id == sessionId }
    val s = live ?: model.openSession ?: return
    val ended = live == null
    val node = model.node(nodeId)?.name ?: "node"
    var menu by remember { mutableStateOf(false) }
    var confirmKill by remember { mutableStateOf(false) }
    var showUsage by remember { mutableStateOf(false) }
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
                        Column {
                            Text(s.name(), maxLines = 1, overflow = TextOverflow.Ellipsis)
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
                        WrapButton()
                        Box {
                            IconButton(onClick = { menu = true }) { Icon(Icons.Filled.MoreVert, "More") }
                            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                                DropdownMenuItem(text = { Text("Usage") }, onClick = { menu = false; showUsage = true })
                                DropdownMenuItem(text = { Text("Node health") }, onClick = { menu = false; openHealth() })
                                if ((s.wrapped || s.spawned) && !ended) {
                                    DropdownMenuItem(
                                        text = { Text("Interrupt") },
                                        onClick = { menu = false; model.interrupt(s, null) },
                                    )
                                }
                                if (ended && model.resumable(s)) {
                                    DropdownMenuItem(
                                        text = { Text("Resume") },
                                        onClick = { menu = false; model.resume(s, "") },
                                    )
                                }
                                if (!ended) {
                                    DropdownMenuItem(
                                        text = { Text("Kill session", color = MaterialTheme.colorScheme.error) },
                                        onClick = { menu = false; confirmKill = true },
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
                    PromptBar(
                        text, { text = it },
                        stoppable = s.wrapped || s.spawned,
                        busy = s.busy,
                        waiting = model.pendingFor(s).isNotEmpty(),
                        send = { model.prompt(s, text.trim()); text = "" },
                        stop = { model.interrupt(s, text.trim()); text = "" },
                    )
                } else if (model.resumable(s)) {
                    // Sending resumes the session in place with the text as its next turn.
                    PromptBar(
                        text, { text = it },
                        stoppable = false,
                        busy = false,
                        waiting = false,
                        resuming = true,
                        send = { model.resume(s, text.trim()); text = "" },
                        stop = {},
                    )
                }
            },
        ) { padding ->
            Column(Modifier.fillMaxSize().padding(padding)) {
                if (model.link == Link.Reconnecting) {
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
                Conversation(model, s, Modifier.weight(1f))
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

@Composable
private fun Conversation(model: HubModel, s: SessionInfo, modifier: Modifier) {
    val list = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val items = model.conversation
    // Every row: the list is lazy, so only what is on screen is composed.
    val rows = remember(items.size) { entries(items) }
    val sent = model.sending
    val waiting = model.pendingFor(s)
    val ended = model.sessions.none { it.nodeId == s.nodeId && it.id == s.id }
    val working = s.busy && !ended && waiting.isEmpty()
    // The person's last prompt: one still being sent, else the last in the transcript.
    val lastPrompt = if (sent.isNotEmpty()) rows.size + sent.size - 1 else rows.indexOfLast { it.item.role == Role.USER }
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
    val count = rows.size + sent.size + waiting.size + (if (working) 1 else 0)
    LaunchedEffect(count) {
        if (count > 0 && stick) list.scrollToItem(count - 1, Int.MAX_VALUE)
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
            itemsIndexed(rows, key = { i, e -> "${e.item.seq}-$i" }) { i, e ->
                val before = rows.getOrNull(i - 1)?.item
                if (before == null || e.item.timeMs - before.timeMs > 10 * 60_000) {
                    Caption(whenLabel(e.item.timeMs), Modifier.padding(top = if (before == null) 0.dp else 8.dp))
                }
                ConversationRow(e, s.cwd, delivered = e.item.role == Role.USER && e.item.text in model.delivered)
            }
            itemsIndexed(sent, key = { _, x -> "sent" + x.timeMs }) { _, x ->
                SentTurn(x, retry = { model.retry(x) })
            }
            itemsIndexed(waiting, key = { _, p -> "p" + p.id }) { _, p ->
                PendingCard(model, p, null, Modifier.padding(top = 8.dp))
            }
            if (working) item(key = "working") { Working() }
        }
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

/** The terminal's shapes, loosely: the person's prompt as a boxed line, Claude's text as
 *  rendered markdown, a tool call as one line with its output folded beneath, thinking
 *  folded, notices as captions. */
@Composable
private fun ConversationRow(e: Entry, cwd: String, delivered: Boolean = false) {
    val item = e.item
    when (item.role) {
        Role.USER -> UserTurn(item.text, ticks = if (delivered) 2 else 0, timeMs = item.timeMs)
        Role.ASSISTANT -> Column(Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
            Markdown(item.text, Modifier.fillMaxWidth())
            Text(
                clockTime(item.timeMs),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                textAlign = TextAlign.End,
                modifier = Modifier.fillMaxWidth(),
            )
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

/** A prompt sent from here, before the transcript echoes it: one tick once the hub took it,
 *  the node's note or the failure under it. */
@Composable
private fun SentTurn(x: Sent, retry: () -> Unit) {
    Column(Modifier.fillMaxWidth().clickable(enabled = x.failed != null, onClick = retry)) {
        UserTurn(x.text, ticks = if (x.accepted) 1 else 0, pending = !x.accepted && x.failed == null, timeMs = x.timeMs)
        (x.failed?.let { "$it · tap to retry" } ?: x.note)?.let {
            Text(
                it,
                style = MaterialTheme.typography.labelSmall,
                color = if (x.failed != null) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.End,
                modifier = Modifier.fillMaxWidth().padding(end = 8.dp, bottom = 4.dp),
            )
        }
    }
}

@Composable
private fun UserTurn(text: String, ticks: Int = 0, pending: Boolean = false, timeMs: Long? = null) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .height(IntrinsicSize.Min)
            .background(MaterialTheme.colorScheme.surfaceContainer, RoundedCornerShape(10.dp)),
    ) {
        Box(
            Modifier
                .width(4.dp)
                .fillMaxHeight()
                .background(MaterialTheme.colorScheme.primary, RoundedCornerShape(topStart = 8.dp, bottomStart = 8.dp)),
        )
        Text(
            text,
            style = MaterialTheme.typography.bodyLarge,
            fontFamily = if (text.startsWith("! ")) FontFamily.Monospace else FontFamily.Default,
            color = if (pending) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f).padding(horizontal = 12.dp, vertical = 10.dp),
        )
        if (timeMs != null) {
            Text(
                clockTime(timeMs),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                modifier = Modifier.padding(end = if (ticks > 0 || pending) 4.dp else 10.dp, top = 14.dp),
            )
        }
        if (ticks > 0 || pending) {
            Icon(
                when {
                    pending -> Icons.Filled.Schedule
                    ticks >= 2 -> Icons.Filled.DoneAll
                    else -> Icons.Filled.Done
                },
                contentDescription = when {
                    pending -> "Sending"
                    ticks >= 2 -> "In the transcript"
                    else -> "Sent"
                },
                tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                modifier = Modifier.padding(end = 10.dp, top = 14.dp).size(14.dp),
            )
        }
    }
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
            Text(call.tool, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.secondary)
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
    send: () -> Unit,
    stop: () -> Unit,
) {
    // Only the rounded input floats over the screen; no bar behind it.
    Box {
        Column(Modifier.navigationBarsPadding().imePadding().padding(horizontal = 12.dp, vertical = 8.dp)) {
            // One line on the session's state, where it changes what Send does.
            val (state, tint) = when {
                resuming -> "Ended · Send resumes it headless on its node, in place" to MaterialTheme.colorScheme.onSurfaceVariant
                waiting -> "Waiting for you · answer the card above" to MaterialTheme.colorScheme.tertiary
                !stoppable -> "Started outside saarthi run · Claude sees this as a message from another session · no stop from here" to MaterialTheme.colorScheme.onSurfaceVariant
                busy -> "Claude is working · Send queues after this turn · the stop button interrupts" to MaterialTheme.colorScheme.onSurfaceVariant
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
                        container = MaterialTheme.colorScheme.secondaryContainer,
                        content = MaterialTheme.colorScheme.onSecondaryContainer,
                    ) {
                        Icon(Icons.Filled.Stop, if (text.isBlank()) "Interrupt" else "Interrupt, then send", Modifier.size(18.dp))
                    }
                    Spacer(Modifier.width(8.dp))
                }
                RoundButton(
                    onClick = send,
                    enabled = text.isNotBlank(),
                    container = MaterialTheme.colorScheme.primary,
                    content = MaterialTheme.colorScheme.onPrimary,
                ) {
                    Icon(Icons.Filled.ArrowUpward, "Send", Modifier.size(20.dp))
                }
            }
        }
    }
}

private val ROUND_BUTTON = 36.dp

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
