package me.leoank.saarthi

import androidx.compose.material3.FilterChip
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

private val ranges = listOf("1 h" to 1L, "6 h" to 6L, "24 h" to 24L)

/** One line of a chart: the metric's name, its value per sample, and how to show it. A
 *  [rate] series is a count since the previous sample, charted per second between the two. */
private class Series(
    val name: String,
    val unit: String,
    val max: Double?,
    val rate: Boolean = false,
    val pick: (Map<String, Double>) -> Double?,
) {
    fun label(v: Double) = when (unit) {
        "%" -> "%.0f%%".format(v)
        "GB" -> gigabytes(v * 1_073_741_824.0)
        "MB/s" -> "%.2f MB/s".format(v)
        else -> "%.2f".format(v)
    }
}

private val series = listOf(
    Series("CPU", "%", 100.0) { it["cpu.util_pct"] },
    Series("Memory", "%", 100.0) { m ->
        val used = m["mem.used_bytes"]; val total = m["mem.total_bytes"]
        if (used != null && total != null && total > 0) 100 * used / total else null
    },
    Series("GPU", "%", 100.0) { it["gpu.util_pct"] },
    Series("Disk", "%", 100.0) { m ->
        val used = m["disk.used_bytes"]; val total = m["disk.total_bytes"]
        if (used != null && total != null && total > 0) 100 * used / total else null
    },
    Series("Load, 1 min", "", null) { it["cpu.load_1m"] },
    // Bytes on all interfaces since the node's previous sample.
    Series("Network in", "MB/s", null, rate = true) { it["net.rx_bytes"]?.div(1_000_000) },
    Series("Network out", "MB/s", null, rate = true) { it["net.tx_bytes"]?.div(1_000_000) },
    // The account's rate limits, from the usage endpoint when the node polls it, else
    // Claude Code's own cached value, which only /usage refreshes.
    Series("Claude 5-hour limit", "%", 100.0) { it["usage.five_hour_pct"] },
    Series("Claude 7-day limit", "%", 100.0) { it["usage.seven_day_pct"] },
    Series("Claude 7-day Fable limit", "%", 100.0) { it["usage.seven_day_fable_pct"] },
)

/** The node's health over a chosen span as a chart per metric, refreshed every minute. */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun HealthScreen(model: HubModel, snackbar: SnackbarHostState, nodeId: String, back: () -> Unit) {
    val node = model.node(nodeId)
    var range by remember { mutableIntStateOf(0) }
    // The keep-awake switch as the latest sample reports it, or as just set from here until
    // a sample says so (ADR-0034).
    val reported = model.health.lastOrNull()?.metrics?.firstOrNull { it.name == "node.keep_awake_enabled" }?.value?.let { it == 1.0 }
    var switched by remember(nodeId) { mutableStateOf<Boolean?>(null) }
    LaunchedEffect(reported) { switched = null }
    val keepAwake = switched ?: reported
    LaunchedEffect(nodeId, range) {
        while (true) {
            model.loadHealth(nodeId, System.currentTimeMillis() - ranges[range].second * 3_600_000)
            delay(60_000)
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text(node?.name ?: "Node")
                        node?.let {
                            val awake = model.health.lastOrNull()?.metrics?.firstOrNull { m -> m.name == "node.keep_awake" }?.value
                            Text(
                                listOfNotNull(
                                    if (it.connected) "online · v${it.version}" else "offline · last seen ${relative(it.lastSeenMs)}",
                                    when {
                                        awake == 1.0 -> "keeping the machine awake"
                                        awake == 0.0 && keepAwake == true -> "letting it sleep"
                                        else -> null
                                    },
                                ).joinToString(" · "),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                },
                navigationIcon = { IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") } },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        val points = model.health.map { p -> p.timeMs to p.metrics.associate { it.name to it.value } }
        val memory = points.lastOrNull()?.second
        LazyColumn(
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.fillMaxSize().padding(padding),
        ) {
            item {
                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                    ranges.forEachIndexed { i, (label, _) ->
                        SegmentedButton(
                            selected = range == i,
                            onClick = { range = i },
                            shape = SegmentedButtonDefaults.itemShape(i, ranges.size),
                        ) { Text(label) }
                    }
                }
                if (model.healthLoading) LinearProgressIndicator(Modifier.fillMaxWidth().padding(top = 8.dp))
            }
            if (node?.connected == true && keepAwake != null) {
                item(key = "awake") {
                    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.weight(1f)) {
                            Text("Keep awake", style = MaterialTheme.typography.titleSmall)
                            Text(
                                "Hold the machine out of idle sleep while a session works (macOS)",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        Switch(
                            checked = keepAwake,
                            onCheckedChange = { on -> switched = on; model.setKeepAwake(nodeId, on) },
                        )
                    }
                }
            }
            if (node != null && model.claudeTokens.isNotEmpty()) {
                item(key = "token") {
                    Column(Modifier.fillMaxWidth()) {
                        Text("Default Claude account", style = MaterialTheme.typography.titleSmall)
                        Text(
                            "What sessions started from here run under when none is picked",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            FilterChip(
                                selected = node.defaultClaudeToken.isEmpty(),
                                onClick = { model.setDefaultClaudeToken(nodeId, "") },
                                label = { Text("Node's login") },
                            )
                            for (t in model.claudeTokens) {
                                FilterChip(
                                    selected = node.defaultClaudeToken == t.name,
                                    onClick = { model.setDefaultClaudeToken(nodeId, t.name) },
                                    label = { Text(t.name) },
                                )
                            }
                        }
                    }
                }
            }
            if (points.isEmpty() && !model.healthLoading) {
                item { Empty("No samples", "The node reports health once a minute while it runs.") }
            }
            for (s in series) {
                val values = if (s.rate) {
                    // Between samples that carry the metric: the usage poller's own samples
                    // fall in between and carry only the usage windows.
                    points.mapNotNull { (t, m) -> s.pick(m)?.let { t to it } }
                        .zipWithNext { (t0, _), (t, v) -> t to v / ((t - t0) / 1000.0).coerceAtLeast(1.0) }
                } else {
                    points.mapNotNull { (t, m) -> s.pick(m)?.let { t to it } }
                }
                if (values.isEmpty()) continue
                item(key = s.name) {
                    val detail = when {
                        s.name == "Memory" && memory != null -> {
                            val used = memory["mem.used_bytes"]; val total = memory["mem.total_bytes"]
                            if (used != null && total != null) "${gigabytes(used)} of ${gigabytes(total)}" else null
                        }
                        s.name.startsWith("Claude") && memory != null -> when (val age = memory["usage.age_s"]) {
                            null -> null
                            0.0 -> "live from the usage endpoint"
                            else -> "from /usage, ${relative(System.currentTimeMillis() - (age * 1000).toLong())}"
                        }
                        else -> null
                    }
                    ChartCard(s, values, detail)
                }
            }
        }
    }
}

@Composable
private fun ChartCard(s: Series, values: List<Pair<Long, Double>>, detail: String?) {
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer)) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.Bottom) {
                Column(Modifier.weight(1f)) {
                    Text(s.name, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    detail?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                }
                Text(s.label(values.last().second), style = MaterialTheme.typography.headlineSmall)
            }
            LineChart(values, s.max, Modifier.fillMaxWidth().height(110.dp).padding(top = 8.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                Text(clockTime(values.first().first), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                val peak = values.maxOf { it.second }
                Text("peak ${s.label(peak)}", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(clockTime(values.last().first), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

/** A line over time, from 0 to [max] (or the series' own peak), with quarter gridlines and
 *  the area beneath it tinted. */
@Composable
private fun LineChart(values: List<Pair<Long, Double>>, max: Double?, modifier: Modifier) {
    val color = MaterialTheme.colorScheme.primary
    val grid = MaterialTheme.colorScheme.outlineVariant
    val top = max ?: values.maxOf { it.second }.coerceAtLeast(1e-9) * 1.1
    Canvas(modifier) {
        for (q in 0..4) {
            val y = size.height - size.height * q / 4
            drawLine(grid, Offset(0f, y), Offset(size.width, y), strokeWidth = 1f)
        }
        if (values.size < 2) {
            values.firstOrNull()?.let { (_, v) ->
                val y = size.height - (v / top).coerceIn(0.0, 1.0).toFloat() * size.height
                drawCircle(color, radius = 4.dp.toPx(), center = Offset(size.width, y))
            }
            return@Canvas
        }
        val t0 = values.first().first
        val span = (values.last().first - t0).coerceAtLeast(1).toFloat()
        val line = Path()
        val area = Path()
        values.forEachIndexed { i, (t, v) ->
            val x = (t - t0) / span * size.width
            val y = size.height - (v / top).coerceIn(0.0, 1.0).toFloat() * size.height
            if (i == 0) {
                line.moveTo(x, y)
                area.moveTo(x, size.height)
                area.lineTo(x, y)
            } else {
                line.lineTo(x, y)
                area.lineTo(x, y)
            }
        }
        area.lineTo(size.width, size.height)
        area.close()
        drawPath(area, color.copy(alpha = 0.15f))
        drawPath(line, color, style = Stroke(width = 2.dp.toPx()))
    }
}
