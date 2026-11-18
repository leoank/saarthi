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
import androidx.compose.material.icons.automirrored.filled.MenuBook
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
import androidx.compose.material3.ExtendedFloatingActionButton
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
    openNotebooks: () -> Unit,
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
                    Column {
                        Text("Sessions")
                        Text(
                            when (model.link) {
                                Link.Connected -> model.hubName
                                Link.Connecting -> "Connecting to ${model.hubName}…"
                                Link.Reconnecting -> if (model.reconnecting) "Reconnecting to ${model.hubName}…" else model.hubName
                                Link.Disconnected -> "Not connected to ${model.hubName}"
                            },
                            style = MaterialTheme.typography.bodySmall,
                            color = if (model.link == Link.Connected || model.link == Link.Connecting || !model.reconnecting && model.link == Link.Reconnecting)
                                MaterialTheme.colorScheme.onSurfaceVariant
                            else MaterialTheme.colorScheme.error,
                            modifier = Modifier.clickable(enabled = model.link == Link.Disconnected, onClick = openSettings),
                        )
                    }
                },
                actions = {
                    if (model.nodes.any { NOTEBOOK_SERVICE in it.services }) {
                        IconButton(onClick = openNotebooks) { Icon(Icons.AutoMirrored.Filled.MenuBook, "Notebooks") }
                    }
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
                ExtendedFloatingActionButton(
                    onClick = { newSession = true },
                    icon = { Icon(Icons.Filled.Add, null) },
                    text = { Text("New session") },
                )
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
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                item(key = "scope") { ScopeChips(model) }
                if (waiting.isNotEmpty()) {
                    item(key = "waiting") { SectionTitle("Needs you · ${waiting.size}", Modifier.padding(top = 0.dp)) }
                    items(waiting, key = { "p" + it.id }) { p ->
                        val s = model.sessions.firstOrNull { it.id == p.sessionId && it.nodeId == p.nodeId }
                        val where = listOfNotNull(model.node(p.nodeId)?.name, s?.title()).joinToString(" / ")
                        PendingCard(model, p, where, Modifier.clickable(enabled = s != null) { s?.let(openSession) })
                    }
                }
                for ((name, list) in listOf("Working" to working, "Ready" to ready)) {
                    if (list.isEmpty()) continue
                    item(key = "t$name") { SectionTitle("$name · ${list.size}") }
                    items(list, key = { "s" + it.nodeId + it.id }) { s ->
                        SessionCard(s, model.pendingFor(s).size, if (oneNode) null else model.node(s.nodeId)?.name) { openSession(s) }
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
private fun NodeHeader(node: NodeInfo, count: Int, waiting: Int, select: () -> Unit, onHealth: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().clickable(onClick = select),
    ) {
        StatusDot(node.connected)
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(node.name, style = MaterialTheme.typography.titleMedium)
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
                style = MaterialTheme.typography.bodySmall,
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
    LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        item {
            FilterChip(selected = model.scope.isEmpty(), onClick = { model.changeScope("") }, label = { Text("All") })
        }
        items(model.nodes.sortedBy { it.name }, key = { "n" + it.id }) { n ->
            FilterChip(
                selected = model.scope == "node:${n.id}",
                onClick = { model.changeScope(if (model.scope == "node:${n.id}") "" else "node:${n.id}") },
                label = { Text(n.name) },
                leadingIcon = { StatusDot(n.connected, Modifier.size(8.dp)) },
            )
        }
        items(repos, key = { "r$it" }) { r ->
            FilterChip(
                selected = model.scope == "repo:$r",
                onClick = { model.changeScope(if (model.scope == "repo:$r") "" else "repo:$r") },
                label = { Text(r) },
            )
        }
    }
}

@Composable
private fun SessionCard(s: SessionInfo, waiting: Int, machine: String?, onOpen: () -> Unit) {
    Card(
        Modifier.fillMaxWidth().clickable(onClick = onOpen),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp)) {
            Column(Modifier.weight(1f)) {
                Text(s.title(), style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    listOfNotNull(machine, s.place().ifBlank { null }).joinToString(" · "),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.StartEllipsis,
                )
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 2.dp)) {
                    StatePill(s.state(waiting))
                    Spacer(Modifier.width(8.dp))
                    Text(
                        listOfNotNull("started ${relative(s.startedMs)}", s.profile.ifEmpty { null }).joinToString(" · "),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                if (s.lastReply.isNotBlank()) {
                    Text(
                        s.lastReply,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(top = 4.dp),
                    )
                }
            }
            if (waiting > 0) Badge(Modifier.padding(end = 8.dp)) { Text("$waiting") }
            if (s.wrapped || s.spawned) {
                Icon(
                    if (s.wrapped) Icons.Filled.Terminal else Icons.Filled.RocketLaunch,
                    contentDescription = if (s.wrapped) "Under saarthi run" else "Spawned from a client",
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
    }
}
