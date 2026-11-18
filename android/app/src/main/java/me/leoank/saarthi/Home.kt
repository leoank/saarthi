package me.leoank.saarthi

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.FilterChip
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ChevronRight
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.RocketLaunch
import androidx.compose.material.icons.filled.Terminal
import androidx.compose.material.icons.outlined.ShowChart
import androidx.compose.material3.Badge
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.foundation.background
import androidx.compose.ui.unit.sp
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Brush
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import uniffi.saarthi_core.NodeInfo
import uniffi.saarthi_core.SessionInfo

/** Everything waiting on the person first, then each node with its live sessions. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    model: HubModel,
    snackbar: SnackbarHostState,
    openSession: (SessionInfo) -> Unit,
    openHealth: (NodeInfo) -> Unit,
    spawned: (nodeId: String, sessionId: String) -> Unit,
    openHistory: () -> Unit,
    openTokens: () -> Unit,
    openSettings: () -> Unit,
) {
    var menu by remember { mutableStateOf(false) }
    var newSession by remember { mutableStateOf(false) }
    val spawnable = model.nodes.filter { it.connected && it.spawnRoots.isNotEmpty() }
    var refreshing by remember { mutableStateOf(false) }
    if (refreshing) {
        androidx.compose.runtime.LaunchedEffect(Unit) {
            model.refresh()
            delay(600)
            refreshing = false
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    val up = model.nodes.count { it.connected }
                    val ok = model.link == Link.Connected || model.link == Link.Connecting || !model.reconnecting && model.link == Link.Reconnecting
                    PromptLine(
                        model.hubName,
                        "~/agents",
                        when (model.link) {
                            Link.Connected -> "● $up/${model.nodes.size} machines up · ${model.sessions.size} sessions"
                            Link.Connecting -> "○ connecting…"
                            Link.Reconnecting -> if (model.reconnecting) "○ reconnecting…" else "● hub · $up machines up"
                            Link.Disconnected -> "✕ not connected · settings ›"
                        },
                        statusColor = if (ok) Flame.Dim else Flame.Bad,
                        onStatus = if (model.link == Link.Disconnected) openSettings else null,
                    )
                },
                actions = {
                    IconButton(onClick = openHistory) { Icon(Icons.Filled.History, "History") }
                    IconButton(onClick = { refreshing = true }) { Icon(Icons.Filled.Refresh, "Refresh") }
                    Box {
                        IconButton(onClick = { menu = true }) { Icon(Icons.Filled.MoreVert, "More") }
                        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                            if (model.claudeTokens.isNotEmpty()) {
                                DropdownMenuItem(
                                    text = { Text("Claude accounts") },
                                    onClick = { menu = false; openTokens() },
                                )
                            }
                            DropdownMenuItem(
                                text = { Text("Settings") },
                                onClick = { menu = false; openSettings() },
                            )
                        }
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
        floatingActionButton = {
            if (spawnable.isNotEmpty()) {
                ActionButton("new session", onClick = { newSession = true })
            }
        },
    ) { padding ->
        PullToRefreshBox(
            isRefreshing = refreshing,
            onRefresh = { refreshing = true },
            modifier = Modifier.fillMaxSize().padding(padding),
        ) {
            // One list across machines, by what each session needs (phase PHS-20261117-f22kxe
            // item 4): what waits on the person, then what works, then what is ready.
            val live = model.sessions.filter { model.inScope(it.nodeId, it) }
            val waiting = model.pending.filter { p ->
                model.inScope(p.nodeId, model.sessions.firstOrNull { it.id == p.sessionId && it.nodeId == p.nodeId })
            }
            val asking = live.filter { model.pendingFor(it).isNotEmpty() }
            val working = live.filter { it.busy && it !in asking }.sortedByDescending { it.startedMs }
            val ready = live.filter { !it.busy && it !in asking }.sortedByDescending { it.startedMs }
            val oneNode = model.scope.startsWith("node:") || model.nodes.size == 1
            LazyColumn(
                // Room under the last row for the New session button, which floats over the
                // list's end: its 56 dp, its 16 dp margin and a little air, so the last
                // machine's row and its health button scroll clear of it.
                contentPadding = PaddingValues(
                    start = 16.dp,
                    end = 16.dp,
                    top = 8.dp,
                    bottom = if (spawnable.isNotEmpty()) 96.dp else 8.dp,
                ),
                verticalArrangement = Arrangement.spacedBy(0.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                item(key = "scope") { ScopeChips(model) }
                if (waiting.isNotEmpty()) {
                    item(key = "waiting") { SectionTitle("Needs you", count = waiting.size, hot = true) }
                    items(waiting, key = { "p" + it.id }) { p ->
                        val s = model.sessions.firstOrNull { it.id == p.sessionId && it.nodeId == p.nodeId }
                        val where = listOfNotNull(model.node(p.nodeId)?.name, s?.title()).joinToString(" / ")
                        PendingCard(model, p, where, Modifier.padding(bottom = 8.dp).clickable(enabled = s != null) { s?.let(openSession) })
                    }
                }
                for ((name, list) in listOf("Working" to working, "Ready" to ready)) {
                    if (list.isEmpty()) continue
                    item(key = "t$name") { SectionTitle(name, count = list.size) }
                    itemsIndexed(list, key = { _, it -> "s" + it.nodeId + it.id }) { i, s ->
                        SessionRow(s, model.pendingFor(s).size, if (oneNode) null else model.node(s.nodeId)?.name, first = i == 0) { openSession(s) }
                    }
                }
                if (live.isEmpty() && waiting.isEmpty() && model.nodes.isNotEmpty()) {
                    item(key = "quiet") { Caption("No live sessions here") }
                }
                if (model.nodes.isEmpty()) {
                    item(key = "none") {
                        Empty("No nodes yet", "Start saarthi node on a machine with this hub's address and a node token.")
                    }
                }
                // The machines themselves: whether each is up, and its health.
                item(key = "machines") { SectionTitle("Machines") }
                for (node in model.nodes.sortedWith(compareByDescending<NodeInfo> { it.connected }.thenBy { it.name })) {
                    if (!model.inScope(node.id, null) && model.scope.startsWith("node:")) continue
                    val sessions = model.sessions.filter { it.nodeId == node.id }
                    item(key = "n" + node.id) {
                        NodeHeader(
                            node,
                            sessions.size,
                            sessions.count { s -> model.pendingFor(s).isNotEmpty() },
                            select = { model.changeScope(if (model.scope == "node:${node.id}") "" else "node:${node.id}") },
                        ) { openHealth(node) }
                    }
                }
            }
        }
    }
    if (newSession && spawnable.isNotEmpty()) {
        NewSessionDialog(model, spawnable, model.claudeTokens, dismiss = { newSession = false }) { nodeId, cwd, prompt, m, create, mode, profile, token ->
            newSession = false
            model.spawn(nodeId, cwd, prompt, m, create, mode, profile, token) { id -> spawned(nodeId, id) }
        }
    }
}

@Composable
private fun NodeHeader(
    node: NodeInfo,
    count: Int,
    waiting: Int,
    select: () -> Unit,
    onHealth: () -> Unit,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().clickable(onClick = select),
    ) {
        StatusDot(node.connected)
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(node.name, fontFamily = Mono, style = MaterialTheme.typography.bodyLarge)
            Text(
                listOfNotNull(
                    if (node.connected) "online" else "offline · last seen ${relative(node.lastSeenMs)}",
                    "v${node.version}",
                    when (count) {
                        0 -> null
                        1 -> "1 session"
                        else -> "$count sessions"
                    },
                    if (waiting > 0) "$waiting waiting" else null,
                ).joinToString(" · "),
                style = MonoMeta,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        IconButton(onClick = onHealth) { Icon(Icons.Outlined.ShowChart, "Health of ${node.name}") }
    }
}

/** What Home lists: all, a machine, or a repository with a live session. */
@Composable
private fun ScopeChips(model: HubModel) {
    val repos = model.sessions.map { it.repo() }.filter { it.isNotBlank() }.distinct().sorted()
    if (model.nodes.size < 2 && repos.size < 2) return
    LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.padding(bottom = 4.dp)) {
        item {
            Tag("all", model.scope.isEmpty(), onClick = { model.changeScope("") })
        }
        items(model.nodes.sortedBy { it.name }, key = { "n" + it.id }) { n ->
            Tag(
                n.name,
                model.scope == "node:${n.id}",
                onClick = { model.changeScope(if (model.scope == "node:${n.id}") "" else "node:${n.id}") },
                mark = { StatusDot(n.connected) },
            )
        }
        items(repos, key = { "r$it" }) { r ->
            Tag(r, model.scope == "repo:$r", onClick = { model.changeScope(if (model.scope == "repo:$r") "" else "repo:$r") })
        }
    }
}

/** A session as a terminal row (ADR-0069): a gutter mark, the title as Claude or the owner
 *  named it, its state as a word, `repo:branch · machine · age` in mono, and the last reply;
 *  rows are set apart by dashed hairlines, and one that needs the owner gets an amber wash. */
@Composable
private fun SessionRow(s: SessionInfo, waiting: Int, machine: String?, first: Boolean, onOpen: () -> Unit) {
    val state = s.state(waiting)
    Row(
        Modifier
            .fillMaxWidth()
            .then(if (first) Modifier else Modifier.dashedTop())
            .then(
                if (state == SessionState.INPUT) Modifier.background(Brush.horizontalGradient(listOf(Flame.soft(Flame.Hot, 0.08f), Color.Transparent)))
                else Modifier,
            )
            .clickable(onClick = onOpen)
            .padding(start = 2.dp, end = 4.dp, top = 10.dp, bottom = 10.dp),
    ) {
        Box(Modifier.width(16.dp).padding(top = 2.dp)) {
            when (state) {
                SessionState.INPUT -> Text("▌", fontFamily = Mono, fontSize = 13.sp, color = Flame.Hot)
                SessionState.RUN -> Spinner()
                else -> Text("·", fontFamily = Mono, fontSize = 13.sp, color = Flame.Dim)
            }
        }
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    s.title(),
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                if (s.wrapped || s.spawned) {
                    Text(if (s.wrapped) "run " else "spawn ", fontFamily = Mono, fontSize = 10.sp, color = Flame.Dim)
                }
                StatePill(state)
            }
            Text(
                androidx.compose.ui.text.buildAnnotatedString {
                    if (s.gitBranch.isNotBlank()) {
                        withStyle(SpanStyle(color = Flame.Soft)) { append(s.repo()) }
                        append(":")
                        withStyle(SpanStyle(color = Flame.Nitya)) { append(s.gitBranch) }
                    } else append(s.parentPath())
                    for (part in listOfNotNull(machine, relative(s.startedMs), s.profile.ifEmpty { null })) append(" · $part")
                },
                style = MonoMeta,
                color = Flame.Dim,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(top = 1.dp),
            )
            if (s.lastReply.isNotBlank()) {
                Text(
                    s.lastReply,
                    style = MaterialTheme.typography.bodySmall,
                    color = Flame.Soft,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(top = 4.dp),
                )
            }
        }
    }
}
