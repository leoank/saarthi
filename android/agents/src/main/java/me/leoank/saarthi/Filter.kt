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
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.json.JSONObject
import uniffi.saarthi_core.SessionInfo

/** Which sessions Home and History show (phase PHS-20261126-f3e51h item 6): one machine, one
 *  repository (Home only: History pages through the hub, which knows no repositories), who
 *  launched them, and labels their launcher set. Empty fields keep everything. */
data class SessionsFilter(
    val nodeId: String = "",
    val repo: String = "",
    /** [YOU] for this phone, `node:` for every session started at a machine, `client:` for
     *  every one a client or app started, or one launcher exactly (`client:ctl`). */
    val launcher: String = "",
    val labels: Map<String, String> = emptyMap(),
) {
    /** How many parts are set; History leaves the repository out. */
    fun count(withRepo: Boolean = true): Int =
        listOf(nodeId, if (withRepo) repo else "", launcher).count { it.isNotEmpty() } + labels.size

    /** Whether `s` passes all but the machine, which the caller knows from the session's node. */
    fun matches(s: SessionInfo, you: String, withRepo: Boolean = true): Boolean {
        if (nodeId.isNotEmpty() && s.nodeId != nodeId) return false
        if (withRepo && repo.isNotEmpty() && s.repo() != repo) return false
        val by = if (launcher == YOU) you else launcher
        if (launcher.isNotEmpty() && !launchedBy(s.launchedBy, by)) return false
        return labels.all { (k, v) -> s.labels[k] == v }
    }

    fun toJson(): String = JSONObject()
        .put("node", nodeId)
        .put("repo", repo)
        .put("launcher", launcher)
        .put("labels", JSONObject(labels))
        .toString()

    companion object {
        const val YOU = "you"

        fun fromJson(text: String): SessionsFilter? = runCatching {
            val o = JSONObject(text)
            val l = o.optJSONObject("labels")
            SessionsFilter(
                o.optString("node"),
                o.optString("repo"),
                o.optString("launcher"),
                l?.keys()?.asSequence()?.associateWith { l.getString(it) }.orEmpty(),
            )
        }.getOrNull()

        /** Home's scope before the filter: `node:<id>` or `repo:<name>`. */
        fun fromScope(scope: String) = SessionsFilter(
            nodeId = scope.removePrefix("node:").takeIf { scope.startsWith("node:") }.orEmpty(),
            repo = scope.removePrefix("repo:").takeIf { scope.startsWith("repo:") }.orEmpty(),
        )
    }
}

/** Whether a session launched by `launcher` is one `by` names: exactly, or every launcher of a
 *  kind when `by` ends in `:`, as the hub matches them. An empty `by` (this phone, before the
 *  hub has said its name) matches nothing. */
fun launchedBy(launcher: String, by: String): Boolean =
    by.isNotEmpty() && if (by.endsWith(":")) launcher.startsWith(by) else launcher == by

/** A launcher in a few characters, for a row and the filter: `you`, `ctl`, `at machine`. */
fun launcherLabel(launcher: String, you: String): String = when {
    launcher.isEmpty() -> ""
    launcher == you -> "you"
    launcher.startsWith("node:") -> "at ${launcher.removePrefix("node:")}"
    else -> launcher.substringAfter(':')
}

/** The filter as one line under a list's title: what it keeps, in mono, and a way to clear
 *  it; a tap opens [FilterSheet]. `live` counts the sessions it keeps on Home; History has
 *  pages, not a count. */
@Composable
fun FilterBar(model: AgentsModel, withRepo: Boolean, live: Pair<Int, Int>?, modifier: Modifier = Modifier) {
    var open by rememberSaveable { mutableStateOf(false) }
    val f = model.filter
    val n = f.count(withRepo)
    val shape = RoundedCornerShape(6.dp)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(if (n > 0) Flame.soft(Flame.Agents, 0.08f) else Flame.S0)
            .border(1.dp, if (n > 0) Flame.Agents.copy(alpha = 0.45f) else Flame.Line, shape)
            .clickable { open = true }
            .padding(start = 10.dp, end = 2.dp, top = 2.dp, bottom = 2.dp),
    ) {
        Icon(Icons.Filled.Tune, null, tint = if (n > 0) Flame.Agents else Flame.Dim, modifier = Modifier.size(16.dp))
        Spacer(Modifier.width(8.dp))
        Text(
            if (n == 0) "all sessions" else summary(model, withRepo).joinToString(" · "),
            fontFamily = Mono,
            fontSize = 12.sp,
            color = if (n > 0) Flame.Text else Flame.Dim,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f).padding(vertical = 10.dp),
        )
        if (live != null && n > 0) {
            Text("${live.first}/${live.second}", fontFamily = Mono, fontSize = 11.sp, color = Flame.Dim, modifier = Modifier.padding(horizontal = 6.dp))
        }
        if (n > 0) {
            IconButton(onClick = { model.changeFilter(SessionsFilter()) }, modifier = Modifier.size(36.dp)) {
                Icon(Icons.Filled.Close, "Clear the filter", tint = Flame.Soft, modifier = Modifier.size(16.dp))
            }
        } else {
            Text("filter ›", fontFamily = Mono, fontSize = 11.sp, color = Flame.Dim, modifier = Modifier.padding(horizontal = 8.dp, vertical = 10.dp))
        }
    }
    if (open) FilterSheet(model, withRepo) { open = false }
}

/** The filter's parts as short phrases: `ghost001`, `app`, `by you`, `job=1234`. */
private fun summary(model: AgentsModel, withRepo: Boolean): List<String> {
    val f = model.filter
    return listOfNotNull(
        f.nodeId.ifEmpty { null }?.let { model.conn.node(it)?.name ?: "a gone machine" },
        f.repo.takeIf { withRepo && it.isNotEmpty() },
        when (f.launcher) {
            "" -> null
            SessionsFilter.YOU -> "by you"
            "node:" -> "by hand"
            "client:" -> "by clients"
            else -> "by " + launcherLabel(f.launcher, model.conn.you)
        },
    ) + f.labels.map { (k, v) -> "$k=$v" }
}

/** Every part of the filter, each a section of choices with how many sessions it keeps:
 *  who launched them, the machine, the repository (Home) and labels, typed or picked from
 *  those the sessions carry. Each choice applies at once, so the list behind shows it. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FilterSheet(model: AgentsModel, withRepo: Boolean, dismiss: () -> Unit) {
    val conn = model.conn
    val f = model.filter
    val you = conn.you
    // What the counts are of: the live sessions, and on History what it has loaded too.
    val known = (conn.sessions + if (withRepo) emptyList() else model.history.map { it.session })
        .distinctBy { it.nodeId to it.id }
    fun keeps(g: SessionsFilter) = known.count { g.matches(it, you, withRepo) }
    fun set(g: SessionsFilter) = model.changeFilter(g)
    var allRepos by rememberSaveable { mutableStateOf(false) }
    ModalBottomSheet(onDismissRequest = dismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true), containerColor = Flame.S1) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
            Text("filter sessions", fontFamily = Mono, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Flame.Text, modifier = Modifier.weight(1f))
            Text(
                "${keeps(f)} of ${known.size}" + if (withRepo) " live" else " loaded",
                fontFamily = Mono,
                fontSize = 11.sp,
                color = Flame.Dim,
            )
        }
        Column(
            Modifier
                .heightIn(max = 620.dp)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
        ) {
            SectionTitle("launched by")
            val seen = known.map { it.launchedBy }.filter { it.isNotEmpty() && it != you && !it.startsWith("node:") }.distinct().sorted()
            Option("anyone", null, f.launcher.isEmpty(), keeps(f.copy(launcher = ""))) { set(f.copy(launcher = "")) }
            Option("this phone", you.ifEmpty { null }, f.launcher == SessionsFilter.YOU, keeps(f.copy(launcher = SessionsFilter.YOU))) {
                set(f.copy(launcher = SessionsFilter.YOU))
            }
            Option("by hand at a machine", "in a terminal or under saarthi run", f.launcher == "node:", keeps(f.copy(launcher = "node:"))) {
                set(f.copy(launcher = "node:"))
            }
            Option("any client or app", "spawned through the hub", f.launcher == "client:", keeps(f.copy(launcher = "client:"))) {
                set(f.copy(launcher = "client:"))
            }
            for (l in seen) {
                Option(launcherLabel(l, you), l, f.launcher == l, keeps(f.copy(launcher = l)), indent = true) { set(f.copy(launcher = l)) }
            }

            SectionTitle("labels", count = f.labels.size.takeIf { it > 0 })
            for ((k, v) in f.labels.toSortedMap()) {
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().padding(start = 4.dp)) {
                    Text("$k = $v", fontFamily = Mono, fontSize = 13.sp, color = Flame.Text, modifier = Modifier.weight(1f))
                    IconButton(onClick = { set(f.copy(labels = f.labels - k)) }) {
                        Icon(Icons.Filled.Close, "Remove $k", tint = Flame.Dim, modifier = Modifier.size(16.dp))
                    }
                }
            }
            LabelForm { k, v -> set(f.copy(labels = f.labels + (k to v))) }
            // Labels the sessions carry, with how many carry each, to pick rather than type.
            val carried = known.flatMap { it.labels.entries.map { e -> e.key to e.value } }
                .filter { (k, v) -> f.labels[k] != v }
                .groupingBy { it }.eachCount()
                .entries.sortedWith(compareByDescending<Map.Entry<Pair<String, String>, Int>> { it.value }.thenBy { it.key.first })
                .take(12)
            if (carried.isNotEmpty()) {
                Text("on these sessions", fontFamily = Mono, fontSize = 11.sp, color = Flame.Dim, modifier = Modifier.padding(start = 4.dp, top = 10.dp, bottom = 2.dp))
                for ((kv, n) in carried) {
                    val (k, v) = kv
                    Option("+ $k = $v", null, false, n) { set(f.copy(labels = f.labels + (k to v))) }
                }
            } else if (f.labels.isEmpty()) {
                Caption("No session here carries a label. An app sets them on a spawn, such as job = 1234.")
            }

            if (conn.nodes.size > 1 || f.nodeId.isNotEmpty()) {
                SectionTitle("machine")
                Option("any machine", null, f.nodeId.isEmpty(), keeps(f.copy(nodeId = ""))) { set(f.copy(nodeId = "")) }
                for (n in conn.nodes.sortedBy { it.name }) {
                    Option(n.name, netLabel(n.tailnet), f.nodeId == n.id, keeps(f.copy(nodeId = n.id)), mark = { StatusDot(n.connected) }) {
                        set(f.copy(nodeId = n.id))
                    }
                }
            }

            if (withRepo) {
                // The busiest first; a long list folds after a few, the chosen one always shown.
                val repos = known.map { it.repo() }.filter { it.isNotBlank() }.groupingBy { it }.eachCount()
                    .entries.sortedWith(compareByDescending<Map.Entry<String, Int>> { it.value }.thenBy { it.key }).map { it.key }
                if (repos.size > 1 || f.repo.isNotEmpty()) {
                    SectionTitle("repository", count = repos.size)
                    Option("any repository", null, f.repo.isEmpty(), keeps(f.copy(repo = ""))) { set(f.copy(repo = "")) }
                    val shown = if (allRepos || repos.size <= REPOS_FOLDED + 1) repos else (repos.take(REPOS_FOLDED) + listOf(f.repo).filter { it.isNotEmpty() }).distinct()
                    for (r in shown) {
                        Option(r, null, f.repo == r, keeps(f.copy(repo = r))) { set(f.copy(repo = r)) }
                    }
                    if (shown.size < repos.size) {
                        Text(
                            "show all ${repos.size} ›",
                            fontFamily = Mono,
                            fontSize = 12.sp,
                            color = Flame.Agents,
                            modifier = Modifier.clip(RoundedCornerShape(4.dp)).clickable { allRepos = true }.padding(start = 28.dp, top = 8.dp, bottom = 8.dp, end = 8.dp),
                        )
                    }
                }
            }

            Row(Modifier.fillMaxWidth().padding(top = 12.dp, bottom = 8.dp), horizontalArrangement = Arrangement.End) {
                if (f.count(withRepo) > 0) TextButton(onClick = { set(SessionsFilter()) }) { Text("clear all", fontFamily = Mono, color = Flame.Soft) }
                Spacer(Modifier.width(8.dp))
                TextButton(onClick = dismiss) { Text("done", fontFamily = Mono, color = Flame.Agents, fontWeight = FontWeight.SemiBold) }
            }
            Spacer(Modifier.navigationBarsPadding())
        }
    }
}

/** Repositories listed before the rest fold under "show all". */
private const val REPOS_FOLDED = 6

/** One choice of a section: a radio mark, its name and a dim line under it, and how many
 *  sessions it would keep at the end. */
@Composable
private fun Option(
    title: String,
    detail: String?,
    on: Boolean,
    keeps: Int,
    indent: Boolean = false,
    mark: (@Composable () -> Unit)? = null,
    pick: () -> Unit,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(4.dp))
            .background(if (on) Flame.soft(Flame.Agents, 0.12f) else Color.Transparent)
            .clickable(onClick = pick)
            .padding(start = if (indent) 22.dp else 4.dp, end = 4.dp, top = 8.dp, bottom = 8.dp),
    ) {
        Text(if (on) "◉" else "○", fontFamily = Mono, fontSize = 13.sp, color = if (on) Flame.Agents else Flame.Dim)
        Spacer(Modifier.width(10.dp))
        if (mark != null) {
            mark()
            Spacer(Modifier.width(6.dp))
        }
        Column(Modifier.weight(1f)) {
            Text(
                title,
                style = MaterialTheme.typography.bodyMedium,
                color = if (on) Flame.Text else Flame.Soft,
                fontWeight = if (on) FontWeight.SemiBold else FontWeight.Normal,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (!detail.isNullOrEmpty()) Text(detail, style = MonoMeta, color = Flame.Dim, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Box(Modifier.padding(start = 8.dp)) {
            Text("$keeps", fontFamily = Mono, fontSize = 11.sp, color = if (keeps == 0) Flame.LineStrong else Flame.Dim)
        }
    }
}

/** A key and a value to add as a label, checked as the hub checks them. */
@Composable
private fun LabelForm(add: (String, String) -> Unit) {
    var key by rememberSaveable { mutableStateOf("") }
    var value by rememberSaveable { mutableStateOf("") }
    val keyOk = key.length in 1..64 && key.all { it.isLetterOrDigit() && it.code < 128 || it in "_.-/" }
    fun submit() {
        if (!keyOk) return
        add(key, value)
        key = ""
        value = ""
    }
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().padding(top = 4.dp)) {
        OutlinedTextField(
            value = key,
            onValueChange = { key = it.trim() },
            placeholder = { Text("key", fontFamily = Mono) },
            isError = key.isNotEmpty() && !keyOk,
            singleLine = true,
            textStyle = MonoMeta.copy(fontSize = 13.sp),
            shape = RoundedCornerShape(6.dp),
            modifier = Modifier.weight(1f),
        )
        Text(" = ", fontFamily = Mono, color = Flame.Dim)
        OutlinedTextField(
            value = value,
            onValueChange = { value = it.take(256) },
            placeholder = { Text("value", fontFamily = Mono) },
            singleLine = true,
            textStyle = MonoMeta.copy(fontSize = 13.sp),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { submit() }),
            shape = RoundedCornerShape(6.dp),
            modifier = Modifier.weight(1f),
        )
        Spacer(Modifier.width(8.dp))
        MiniAction("add", color = if (keyOk) Flame.Agents else Flame.Dim) { submit() }
    }
}
