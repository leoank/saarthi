package me.leoank.saarthi

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsBottomHeight
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.TextButton
import androidx.compose.ui.text.style.TextOverflow
import uniffi.saarthi_core.ShellInfo
import uniffi.saarthi_core.NodeInfo

/** The Machines workspace's root (ADR-0076): the open shells, kept while the app runs
 *  (ADR-0068), each going back to where it was or closed here; then every machine, whether
 *  it is up, the tailnet its link came over, its version and sessions, and a shell on it
 *  (ADR-0054) or its health. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MachinesScreen(model: MachinesModel, openShell: (Shell) -> Unit, newShell: (NodeInfo) -> Unit, openHealth: (NodeInfo) -> Unit) {
    val conn = model.conn
    val nodes = conn.nodes.sortedWith(compareByDescending<NodeInfo> { it.connected }.thenBy { it.name })
    // The shells each machine keeps (ADR-0078), listed again as the link and the machines
    // change, and as shells open and close here.
    LaunchedEffect(conn.link, nodes.count { it.connected }, model.shells.size) { model.loadHeld() }
    var ending by remember { mutableStateOf<Pair<NodeInfo, ShellInfo>?>(null) }
    Scaffold(
        topBar = {
            TopAppBar(title = {
                val up = nodes.count { it.connected }
                val shells = model.shells.size
                PromptLine(
                    conn.hubName, "~/machines",
                    "● $up/${nodes.size} up · $shells ${if (shells == 1) "shell" else "shells"} open",
                    tailnet = conn.tailnet,
                )
            })
        },
    ) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding), contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp)) {
            if (model.shells.isNotEmpty()) {
                item(key = "open") { SectionTitle("open shells", count = model.shells.size) }
                items(model.shells, key = { "s${it.id}" }) { sh ->
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth().clickable { openShell(sh) }.padding(vertical = 8.dp),
                    ) {
                        Text("❯_", color = if (sh.ended == null) Flame.Machines else Flame.Dim, fontFamily = Mono, fontSize = 12.sp)
                        Spacer(Modifier.width(10.dp))
                        Column(Modifier.weight(1f)) {
                            Text(sh.nodeName, fontFamily = Mono, style = MaterialTheme.typography.bodyLarge)
                            Text(
                                sh.ended ?: listOfNotNull(
                                    if (sh.cols > 0) "${sh.cols}×${sh.rows}" else null,
                                    "opened ${relative(sh.startedMs)}",
                                ).joinToString(" · "),
                                style = MonoMeta,
                                color = Flame.Dim,
                            )
                        }
                        IconButton(onClick = { model.closeShell(sh) }) { Icon(Icons.Filled.Close, "Close the shell on ${sh.nodeName}", tint = Flame.Dim) }
                    }
                }
            }
            item(key = "machines") { SectionTitle("machines", count = nodes.size) }
            if (nodes.isEmpty()) {
                item { Empty("No machines yet", "Start saarthi node on a machine with this hub's address and a node token.") }
            }
            items(nodes, key = { it.id }) { n ->
                val sessions = conn.sessions.count { it.nodeId == n.id }
                Column(Modifier.fillMaxWidth().padding(vertical = 10.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        StatusDot(n.connected)
                        Spacer(Modifier.width(10.dp))
                        Text(n.name, fontFamily = Mono, style = MaterialTheme.typography.bodyLarge, color = if (n.connected) Flame.Text else Flame.Dim)
                        if (n.connected) {
                            Spacer(Modifier.width(8.dp))
                            NetTag(n.tailnet)
                        }
                    }
                    Text(
                        listOfNotNull(
                            if (n.connected) "v${n.version}" else "offline · last seen ${relative(n.lastSeenMs)}",
                            when (sessions) {
                                0 -> null
                                1 -> "1 session"
                                else -> "$sessions sessions"
                            },
                            if (n.connected && !n.terminal) "no shell: its node runs without --terminal" else null,
                        ).joinToString(" · "),
                        style = MonoMeta,
                        color = Flame.Dim,
                        modifier = Modifier.padding(start = 18.dp, top = 3.dp),
                    )
                    Row(Modifier.padding(start = 18.dp, top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (n.connected && n.terminal) MiniAction("❯_ new shell", Flame.Machines) { newShell(n) }
                        MiniAction("∿ health") { openHealth(n) }
                    }
                    // Its shells, kept until they exit or are ended: a tap attaches, here too.
                    for (info in model.held[n.id].orEmpty()) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier
                                .padding(start = 18.dp, top = 6.dp)
                                .fillMaxWidth()
                                .clickable { openShell(model.attach(n, info)) },
                        ) {
                            Text("❯_", fontFamily = Mono, fontSize = 12.sp, color = Flame.Machines)
                            Spacer(Modifier.width(8.dp))
                            Column(Modifier.weight(1f)) {
                                Text(
                                    info.title.ifEmpty { "shell ${info.id.take(6)}" },
                                    fontFamily = Mono,
                                    fontSize = 13.sp,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                Text(
                                    listOf(
                                        "started ${relative(info.startedMs)}",
                                        when (val c = info.clients.toInt()) {
                                            0 -> "detached"
                                            1 -> "1 attached"
                                            else -> "$c attached"
                                        },
                                    ).joinToString(" · "),
                                    style = MonoMeta,
                                    color = Flame.Dim,
                                )
                            }
                            IconButton(onClick = { ending = n to info }) { Icon(Icons.Filled.Close, "End this shell", tint = Flame.Dim) }
                        }
                    }
                }
            }
        }
    }
    ending?.let { (n, info) ->
        AlertDialog(
            onDismissRequest = { ending = null },
            title = { Text("End this shell?") },
            text = { Text("${info.title.ifEmpty { "The shell" }} on ${n.name} is killed, with whatever runs in it, for every client attached.") },
            confirmButton = {
                TextButton(onClick = {
                    ending = null
                    model.shells.filter { it.nodeId == n.id && it.shellId == info.id }.forEach(model::closeShell)
                    model.killShell(n.id, info.id)
                }) { Text("End it") }
            },
            dismissButton = { TextButton(onClick = { ending = null }) { Text("Keep it") } },
        )
    }
}
