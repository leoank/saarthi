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
                                Link.Reconnecting -> "Reconnecting to ${model.hubName}…"
                                Link.Disconnected -> "Not connected to ${model.hubName}"
                            },
                            style = MaterialTheme.typography.bodySmall,
                            color = if (model.link == Link.Connected || model.link == Link.Connecting)
                                MaterialTheme.colorScheme.onSurfaceVariant
                            else MaterialTheme.colorScheme.error,
                            modifier = Modifier.clickable(enabled = model.link == Link.Disconnected, onClick = openSettings),
                        )
                    }
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
            LazyColumn(
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                if (model.pending.isNotEmpty()) {
                    item(key = "waiting") { SectionTitle("Waiting for you", Modifier.padding(top = 0.dp)) }
                    items(model.pending, key = { "p" + it.id }) { p ->
                        val s = model.sessions.firstOrNull { it.id == p.sessionId && it.nodeId == p.nodeId }
                        val where = listOfNotNull(model.node(p.nodeId)?.name, s?.name()).joinToString(" / ")
                        PendingCard(model, p, where, Modifier.clickable(enabled = s != null) { s?.let(openSession) })
                    }
                }
                if (model.nodes.isEmpty()) {
                    item(key = "none") {
                        Empty("No nodes yet", "Start saarthi node on a machine with this hub's address and a node token.")
                    }
                }
                for (node in model.nodes.sortedWith(compareByDescending<NodeInfo> { it.connected }.thenBy { it.name })) {
                    val sessions = model.sessions.filter { it.nodeId == node.id }.sortedByDescending { it.startedMs }
                    val folded = node.id in model.collapsed
                    item(key = "n" + node.id) {
                        NodeHeader(
                            node,
                            sessions.size,
                            sessions.count { s -> model.pendingFor(s).isNotEmpty() },
                            folded,
                            toggle = { model.toggleCollapsed(node.id) },
                        ) { openHealth(node) }
                    }
                    if (folded) continue
                    if (sessions.isEmpty()) {
                        item(key = "e" + node.id) {
                            Caption(if (node.connected) "No live sessions" else "Offline, last seen ${relative(node.lastSeenMs)}")
                        }
                    }
                    items(sessions, key = { "s" + it.nodeId + it.id }) { s ->
                        SessionCard(s, model.pendingFor(s).size) { openSession(s) }
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
private fun NodeHeader(node: NodeInfo, count: Int, waiting: Int, folded: Boolean, toggle: () -> Unit, onHealth: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().padding(top = 12.dp).clickable(onClick = toggle),
    ) {
        Icon(
            if (folded) Icons.Filled.ChevronRight else Icons.Filled.ExpandMore,
            if (folded) "Show ${node.name}'s sessions" else "Hide ${node.name}'s sessions",
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(20.dp),
        )
        Spacer(Modifier.width(6.dp))
        StatusDot(node.connected)
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(node.name, style = MaterialTheme.typography.titleMedium)
            Text(
                listOfNotNull(
                    if (node.connected) "online" else "offline",
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

@Composable
private fun SessionCard(s: SessionInfo, waiting: Int, onOpen: () -> Unit) {
    Card(
        Modifier.fillMaxWidth().clickable(onClick = onOpen),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp)) {
            if (s.busy) CircularProgressIndicator(Modifier.size(14.dp), strokeWidth = 2.dp)
            else StatusDot(false, Modifier.size(14.dp).padding(2.dp))
            Spacer(Modifier.width(14.dp))
            Column(Modifier.weight(1f)) {
                Text(s.name(), style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    s.parentPath(),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.StartEllipsis,
                )
                Text(
                    listOf(
                        when {
                            waiting > 0 -> "waiting for you"
                            s.busy -> "working"
                            else -> "idle"
                        },
                        "started ${relative(s.startedMs)}",
                        s.profile.ifEmpty { null },
                    ).filterNotNull().joinToString(" · "),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
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
