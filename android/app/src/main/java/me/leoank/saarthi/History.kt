package me.leoank.saarthi

import androidx.compose.foundation.clickable
import uniffi.saarthi_core.SearchHit
import kotlinx.coroutines.delay
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Search
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.RocketLaunch
import androidx.compose.material.icons.filled.Terminal
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import uniffi.saarthi_core.HistoryItem
import uniffi.saarthi_core.SessionInfo

private val dayFormat: DateTimeFormatter = DateTimeFormatter.ofPattern("EEEE d MMMM")

/** Past and live sessions from every node, newest first and grouped by day, with node
 *  filter chips (ADR-0036). A row opens the session; an ended one can be resumed there. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HistoryScreen(model: HubModel, snackbar: SnackbarHostState, back: () -> Unit, openSession: (SessionInfo, String?) -> Unit) {
    var nodeId by rememberSaveable { mutableStateOf("") }
    var query by rememberSaveable { mutableStateOf("") }
    LaunchedEffect(nodeId) { model.loadHistory(nodeId) }
    // Searched a moment after typing stops, on the node the chips pick (ADR-0063).
    LaunchedEffect(query, nodeId) {
        delay(250)
        model.search(query.trim(), nodeId)
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("History") },
                navigationIcon = { IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") } },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                placeholder = { Text("Search every session") },
                leadingIcon = { Icon(Icons.Filled.Search, null) },
                trailingIcon = {
                    if (query.isNotEmpty()) IconButton(onClick = { query = "" }) { Icon(Icons.Filled.Close, "Clear") }
                },
                singleLine = true,
                shape = RoundedCornerShape(28.dp),
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
            )
            if (model.nodes.size > 1) {
                LazyRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    contentPadding = PaddingValues(horizontal = 16.dp),
                ) {
                    item { FilterChip(selected = nodeId.isEmpty(), onClick = { nodeId = "" }, label = { Text("All") }) }
                    items(model.nodes.sortedBy { it.name }, key = { it.id }) { n ->
                        FilterChip(selected = nodeId == n.id, onClick = { nodeId = n.id }, label = { Text(n.name) })
                    }
                }
            }
            if (query.isNotBlank()) {
                if (model.searching) LinearProgressIndicator(Modifier.fillMaxWidth())
                LazyColumn(
                    contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                    modifier = Modifier.fillMaxSize(),
                ) {
                    if (model.searchHits.isEmpty() && !model.searching && model.searched == query.trim()) {
                        item { Empty("Nothing matches", "Search looks at what you typed and what Claude replied, and at session titles and folders.") }
                    }
                    items(model.searchHits, key = { "s" + it.session.nodeId + it.session.id }) { hit ->
                        SearchRow(hit, model.node(hit.session.nodeId)?.name ?: "node") { openSession(hit.session, query.trim()) }
                    }
                }
                return@Column
            }
            if (model.historyLoading && model.history.isEmpty()) LinearProgressIndicator(Modifier.fillMaxWidth())
            val days = model.history.groupBy { day(it.session.startedMs) }
            LazyColumn(
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                val error = model.historyError
                if (error != null && model.history.isEmpty() && !model.historyLoading) {
                    item { Empty("History did not load", error) }
                } else if (model.history.isEmpty() && !model.historyLoading) {
                    item { Empty("No sessions yet", "Sessions appear here once a node has reported them.") }
                }
                for ((d, rows) in days) {
                    item(key = "d$d") { SectionTitle(dayLabel(d)) }
                    items(rows, key = { "h" + it.session.nodeId + it.session.id }) { h ->
                        HistoryRow(h, model.node(h.session.nodeId)?.name ?: "node") { openSession(h.session, null) }
                    }
                }
                if (model.historyMore && model.history.isNotEmpty()) {
                    item(key = "more") {
                        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) {
                            if (model.historyLoading) CircularProgressIndicator(Modifier.size(24.dp))
                            else TextButton(onClick = { model.loadHistory(nodeId, more = true) }) { Text("Show older") }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun HistoryRow(h: HistoryItem, node: String, onOpen: () -> Unit) {
    val s = h.session
    Card(
        Modifier.fillMaxWidth().clickable(onClick = onOpen),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Column(Modifier.padding(horizontal = 16.dp, vertical = 12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    // A name or Claude Code's title, else the first prompt, else the folder.
                    s.title.ifBlank { h.title.ifEmpty { s.name() } },
                    style = MaterialTheme.typography.titleSmall,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                if (s.wrapped || s.spawned) {
                    Icon(
                        if (s.wrapped) Icons.Filled.Terminal else Icons.Filled.RocketLaunch,
                        contentDescription = if (s.wrapped) "Under saarthi run" else "Spawned from a client",
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(start = 8.dp).size(16.dp),
                    )
                }
            }
            Text(
                if (s.title.isBlank()) listOf(s.parentPath(), s.name()).filter { it.isNotEmpty() }.joinToString("/") else s.place(),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.StartEllipsis,
            )
            Text(
                listOfNotNull(
                    node,
                    s.profile.ifEmpty { null },
                    clockTime(s.startedMs),
                    s.endedMs?.let { e -> span(e - s.startedMs) } ?: "live",
                    if (h.inputTokens + h.outputTokens > 0uL) "${thousands(h.inputTokens)} in · ${thousands(h.outputTokens)} out" else null,
                    if (h.costUsd > 0.0) dollars(h.costUsd) else null,
                ).joinToString(" · "),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** A session a search found: its title or folder, where it ran, and the words around the
 *  match. */
@Composable
private fun SearchRow(hit: SearchHit, node: String, onOpen: () -> Unit) {
    val s = hit.session
    Card(
        Modifier.fillMaxWidth().clickable(onClick = onOpen),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Column(Modifier.padding(horizontal = 16.dp, vertical = 12.dp)) {
            Text(
                s.title.ifBlank { s.name() },
                style = MaterialTheme.typography.titleSmall,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                listOfNotNull(node, s.place().ifEmpty { null }, dayLabel(day(s.startedMs))).joinToString(" · "),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (hit.snippet.isNotEmpty()) {
                Text(
                    hit.snippet,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(top = 6.dp),
                )
            }
        }
    }
}

private fun day(ms: Long): LocalDate = Instant.ofEpochMilli(ms).atZone(ZoneId.systemDefault()).toLocalDate()

private fun dayLabel(d: LocalDate): String {
    val today = LocalDate.now()
    return when (d) {
        today -> "Today"
        today.minusDays(1) -> "Yesterday"
        else -> dayFormat.format(d)
    }
}

/** How long a session ran: "40 s", "12 min", "3 h 5 min". */
private fun span(ms: Long): String {
    val s = ms / 1000
    return when {
        s < 60 -> "$s s"
        s < 3600 -> "${s / 60} min"
        else -> "${s / 3600} h ${(s % 3600) / 60} min"
    }
}
