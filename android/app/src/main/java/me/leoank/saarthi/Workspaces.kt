package me.leoank.saarthi

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.material3.ExperimentalMaterial3Api
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
    Notebooks("notes", "▤", Flame.Notebooks),
    Terminals("term", "❯_", Flame.Terminals),
}

/** The workspace bar, a tmux status line (ADR-0069): each workspace's glyph and name in
 *  mono, the current one on the page's background under a glowing line in its hue, and
 *  what needs the owner counted on Agents. */
@Composable
fun WorkspaceBar(current: Workspace, needsYou: Int, select: (Workspace) -> Unit) {
    Column(Modifier.fillMaxWidth().background(Flame.S0)) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(Flame.Line))
        Row(Modifier.fillMaxWidth().height(46.dp)) {
            Workspace.entries.forEachIndexed { i, w ->
                if (i > 0) Box(Modifier.width(1.dp).fillMaxHeight().padding(vertical = 12.dp).background(Flame.Line))
                val on = w == current
                Box(
                    contentAlignment = Alignment.Center,
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .background(if (on) Flame.Bg else Color.Transparent)
                        .semantics { selected = on }
                        .clickable(role = Role.Tab) { select(w) },
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
                            Spacer(Modifier.width(5.dp))
                            Text("$needsYou", fontFamily = Mono, fontSize = 12.5.sp, fontWeight = FontWeight.Bold, color = Flame.Hot)
                        }
                    }
                }
            }
        }
        Spacer(Modifier.fillMaxWidth().windowInsetsBottomHeight(WindowInsets.navigationBars))
    }
}

/** The Terminals workspace's root: the machines that offer a shell (ADR-0054), each opening
 *  one; the others listed below with why not. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TerminalsScreen(model: HubModel, open: (NodeInfo) -> Unit) {
    val nodes = model.nodes.sortedWith(compareByDescending<NodeInfo> { it.connected && it.terminal }.thenBy { it.name })
    Scaffold(topBar = { TopAppBar(title = { Text("Terminals") }) }) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding), contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp)) {
            if (nodes.isEmpty()) {
                item { Text("No machines yet.", color = Flame.Dim, modifier = Modifier.padding(vertical = 16.dp)) }
            }
            items(nodes, key = { it.id }) { n ->
                val can = n.connected && n.terminal
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable(enabled = can) { open(n) }
                        .padding(vertical = 12.dp),
                ) {
                    Text(if (n.connected) "●" else "○", color = if (n.connected) Flame.Ok else Flame.Dim, fontFamily = Mono, fontSize = 12.sp)
                    Spacer(Modifier.width(10.dp))
                    Column(Modifier.weight(1f)) {
                        Text(n.name, fontFamily = Mono, style = MaterialTheme.typography.bodyLarge, color = if (can) Flame.Text else Flame.Dim)
                        Text(
                            when {
                                !n.connected -> "offline"
                                !n.terminal -> "no shell: its node runs without --terminal"
                                else -> "v${n.version}"
                            },
                            style = MonoMeta,
                            color = Flame.Dim,
                        )
                    }
                    if (can) Text("open a shell ›", style = MonoMeta, color = Flame.Terminals)
                }
            }
        }
    }
}
