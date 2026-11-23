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
import uniffi.saarthi_core.NodeInfo

/** The app's parts, each one tap away on the bar (ADR-0068): its name on the bar, the glyph
 *  before it, and its hue. Nitya joins once it has anything to show. */
enum class Workspace(val label: String, val glyph: String, val hue: Color) {
    Agents("agents", "◆", Flame.Agents),
    Yajna("yajna", "▤", Flame.Yajna),
    Machines("machines", "❯_", Flame.Machines),
}

/** The workspace bar, a tmux status line (ADR-0069), on every screen (ADR-0076): each
 *  workspace's glyph and name in mono, the current one on the page's background under a
 *  glowing line in its hue, what needs the owner counted on Agents (a tap on the count
 *  answers it in a sheet), and the gear for Settings at its end. A drag up on the bar opens
 *  the switcher; a sideways swipe steps to the previous (left) or next (right) open thing.
 *  [panes] marks the workspace each pane holds when the screen is split (`L`, `R`). */
@Composable
fun WorkspaceBar(
    current: Workspace,
    needsYou: Int,
    select: (Workspace) -> Unit,
    settings: () -> Unit,
    settingsOn: Boolean = false,
    answer: () -> Unit = {},
    switcher: () -> Unit = {},
    step: (Int) -> Unit = {},
    hold: (Workspace) -> Unit = {},
    panes: Map<Workspace, String> = emptyMap(),
) {
    val density = androidx.compose.ui.platform.LocalDensity.current
    val far = with(density) { 48.dp.toPx() }
    val up = with(density) { 28.dp.toPx() }
    Column(Modifier.fillMaxWidth().background(Flame.S0)) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(Flame.Line))
        Row(
            Modifier
                .fillMaxWidth()
                .height(46.dp)
                .pointerInput(Unit) {
                    var dx = 0f
                    var dy = 0f
                    detectDragGestures(
                        onDragStart = { dx = 0f; dy = 0f },
                        onDragEnd = {
                            when {
                                -dy > up && -dy > kotlin.math.abs(dx) -> switcher()
                                kotlin.math.abs(dx) > far -> step(if (dx < 0) 1 else -1)
                            }
                        },
                    ) { change, drag ->
                        change.consume()
                        dx += drag.x
                        dy += drag.y
                    }
                },
        ) {
            Workspace.entries.forEachIndexed { i, w ->
                if (i > 0) Box(Modifier.width(1.dp).fillMaxHeight().padding(vertical = 12.dp).background(Flame.Line))
                val on = w == current && !settingsOn
                Box(
                    contentAlignment = Alignment.Center,
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .background(if (on) Flame.Bg else Color.Transparent)
                        .semantics { selected = on }
                        .combinedClickable(role = Role.Tab, onLongClick = { hold(w) }) { select(w) },
                ) {
                    if (on) {
                        // The line, and its glow falling into the tab.
                        Box(
                            Modifier
                                .align(Alignment.TopCenter)
                                .fillMaxWidth()
                                .height(14.dp)
                                .background(Brush.verticalGradient(listOf(w.hue.copy(alpha = 0.18f), Color.Transparent))),
                        )
                        Box(Modifier.align(Alignment.TopCenter).fillMaxWidth(0.72f).height(2.dp).background(w.hue))
                    }
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) {
                        Text(w.glyph, fontFamily = Mono, fontSize = 12.sp, color = if (on) w.hue else Flame.Dim.copy(alpha = 0.7f))
                        Spacer(Modifier.width(6.dp))
                        Text(w.label, fontFamily = Mono, fontSize = 12.5.sp, color = if (on) Flame.Text else Flame.Dim)
                        if (w == Workspace.Agents && needsYou > 0) {
                            Text(
                                "$needsYou",
                                fontFamily = Mono,
                                fontSize = 12.5.sp,
                                fontWeight = FontWeight.Bold,
                                color = Flame.Hot,
                                modifier = Modifier
                                    .clickable(onClickLabel = "Answer what needs you", onClick = answer)
                                    .padding(horizontal = 6.dp, vertical = 8.dp),
                            )
                        }
                        panes[w]?.let { side ->
                            Spacer(Modifier.width(4.dp))
                            Text(
                                side,
                                fontFamily = Mono,
                                fontSize = 9.sp,
                                fontWeight = FontWeight.Bold,
                                color = Flame.Bg,
                                modifier = Modifier.background(w.hue, MaterialTheme.shapes.extraSmall).padding(horizontal = 3.dp),
                            )
                        }
                    }
                }
            }
            Box(Modifier.width(1.dp).fillMaxHeight().background(Flame.Line))
            Box(
                contentAlignment = Alignment.Center,
                modifier = Modifier
                    .width(52.dp)
                    .fillMaxHeight()
                    .background(if (settingsOn) Flame.Bg else Color.Transparent)
                    .clickable(onClickLabel = "Settings", onClick = settings),
            ) {
                Icon(Icons.Outlined.Settings, "Settings", tint = if (settingsOn) Flame.Text else Flame.Dim, modifier = Modifier.size(20.dp))
            }
        }
        Spacer(Modifier.fillMaxWidth().windowInsetsBottomHeight(WindowInsets.navigationBars))
    }
}

/** The Machines workspace's root (ADR-0076): the open shells, kept while the app runs
 *  (ADR-0068), each going back to where it was or closed here; then every machine, whether
 *  it is up, the tailnet its link came over, its version and sessions, and a shell on it
 *  (ADR-0054) or its health. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MachinesScreen(model: MachinesModel, openShell: (Shell) -> Unit, newShell: (NodeInfo) -> Unit, openHealth: (NodeInfo) -> Unit) {
    val conn = model.conn
    val nodes = conn.nodes.sortedWith(compareByDescending<NodeInfo> { it.connected }.thenBy { it.name })
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
                        if (n.connected && n.terminal) MiniAction("❯_ shell", Flame.Machines) { newShell(n) }
                        MiniAction("∿ health") { openHealth(n) }
                    }
                }
            }
        }
    }
}
