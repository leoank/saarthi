package me.leoank.saarthi

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Something open in a workspace (ADR-0076): a session, a notebook page, a shell, a
 *  machine's health or what a session changed, as the switcher lists it. */
data class Place(val w: Workspace, val screen: Screen)

/** Whether a screen is something open, for the switcher, rather than a workspace's root or
 *  one of its lists. */
fun Screen.isPlace(): Boolean =
    this is Screen.Session || this is Screen.Notebook || this is Screen.Terminal || this is Screen.Health || this is Screen.Changes

/** How the switcher names a place: its title and a line on where and what it is; null once
 *  it is gone (a shell closed, a session neither live nor open on this phone). */
class PlaceLabel(val glyph: String, val hue: Color, val title: String, val meta: String, val hot: Boolean = false)

fun HubModel.label(p: Place): PlaceLabel? {
    fun where(nodeId: String): String {
        val n = conn.node(nodeId) ?: return "machine gone"
        return "${n.name} ${netLabel(n.tailnet)}"
    }
    return when (val s = p.screen) {
        is Screen.Session -> {
            val info = conn.session(s.nodeId, s.sessionId) ?: agents.opened(s.nodeId, s.sessionId)?.session ?: return null
            val asks = agents.pendingFor(info).isNotEmpty()
            val state = when {
                asks -> "asks"
                info.endedMs != null -> "ended"
                info.busy -> "working"
                else -> "idle"
            }
            PlaceLabel("◆", Flame.Agents, info.title(), "${where(s.nodeId)} · $state", hot = asks)
        }
        is Screen.Notebook -> PlaceLabel(
            "▤", Flame.Yajna,
            s.namespace,
            "yajna · ${conn.node(s.nodeId)?.name ?: "machine gone"}",
        )
        is Screen.Terminal -> {
            val sh = machines.shell(s.shellId) ?: return null
            PlaceLabel("❯_", Flame.Machines, "${sh.nodeName} · shell ${sh.id}", sh.ended ?: "opened ${relative(sh.startedMs)}")
        }
        is Screen.Health -> PlaceLabel("∿", Flame.Machines, "${conn.node(s.nodeId)?.name ?: "machine"} · health", "machines")
        is Screen.Changes -> {
            val info = conn.session(s.nodeId, s.sessionId) ?: agents.opened(s.nodeId, s.sessionId)?.session
            PlaceLabel("±", Flame.Agents, "Changes · ${info?.title() ?: "session"}", where(s.nodeId))
        }
        else -> null
    }
}

/** Everything open across the workspaces, most recent first (ADR-0076): a tap goes there, a
 *  long press opens it in the other pane when the screen is split, ✕ drops it (and ends a
 *  shell). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SwitcherSheet(
    model: HubModel,
    places: List<Place>,
    current: Place?,
    go: (Place) -> Unit,
    close: (Place) -> Unit,
    dismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = dismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true), containerColor = Flame.S1) {
        val shown = places.mapNotNull { p -> model.label(p)?.let { p to it } }
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
            Text("open · ${shown.size}", fontFamily = Mono, fontSize = 12.sp, color = Flame.Soft, modifier = Modifier.weight(1f))
            Text("swipe the bar ← → to step", fontFamily = Mono, fontSize = 11.sp, color = Flame.Dim)
        }
        if (shown.isEmpty()) {
            Caption("Nothing open yet: sessions, notebook pages and shells you open gather here.", Modifier.padding(16.dp))
        }
        LazyColumn(Modifier.heightIn(max = 520.dp)) {
            items(shown, key = { it.first.toString() }) { (p, l) ->
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(if (p == current) Flame.S2 else Color.Transparent)
                        .clickable { go(p) }
                        .padding(start = 16.dp, end = 4.dp, top = 8.dp, bottom = 8.dp),
                ) {
                    Box(
                        contentAlignment = Alignment.Center,
                        modifier = Modifier
                            .size(32.dp)
                            .background(Flame.S2, MaterialTheme.shapes.small)
                            .border(1.dp, if (l.hot) Flame.Hot.copy(alpha = 0.5f) else Flame.LineStrong, MaterialTheme.shapes.small),
                    ) { Text(l.glyph, fontFamily = Mono, fontSize = 12.sp, color = if (l.hot) Flame.Hot else l.hue) }
                    Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f)) {
                        Text(l.title, style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(l.meta, style = MonoMeta, color = if (l.hot) Flame.Hot else Flame.Dim, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    IconButton(onClick = { close(p) }) { Icon(Icons.Filled.Close, "Close ${l.title}", tint = Flame.Dim, modifier = Modifier.size(18.dp)) }
                }
            }
        }
        Spacer(Modifier.padding(bottom = 16.dp))
    }
}

/** What waits on the owner, answered here from whatever workspace is in front (ADR-0076):
 *  each request as its card, with the session it is from and a way to open it. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NeedsYouSheet(model: HubModel, open: (nodeId: String, sessionId: String) -> Unit, dismiss: () -> Unit) {
    val conn = model.conn
    ModalBottomSheet(onDismissRequest = dismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true), containerColor = Flame.S1) {
        Text(
            "◆ needs you · ${conn.pending.size}",
            fontFamily = Mono,
            fontSize = 12.sp,
            color = Flame.Hot,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
        )
        if (conn.pending.isEmpty()) {
            Caption("Nothing waits on you.", Modifier.padding(16.dp))
        }
        LazyColumn(Modifier.heightIn(max = 600.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            items(conn.pending.sortedBy { it.createdMs }, key = { it.id }) { p ->
                val s = conn.session(p.nodeId, p.sessionId)
                Column(Modifier.padding(horizontal = 12.dp, vertical = 6.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(horizontal = 4.dp)) {
                        Column(Modifier.weight(1f)) {
                            Text(s?.title() ?: "A session", style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            val n = conn.node(p.nodeId)
                            Text(
                                listOfNotNull(s?.repo(), n?.let { "${it.name} ${netLabel(it.tailnet)}" }).joinToString(" · "),
                                style = MonoMeta,
                                color = Flame.Dim,
                            )
                        }
                        if (s != null) MiniAction("open ›") { open(p.nodeId, p.sessionId) }
                    }
                    PendingCard(model.agents, p, null, Modifier.padding(top = 6.dp))
                }
            }
        }
        Spacer(Modifier.padding(bottom = 16.dp))
    }
}
