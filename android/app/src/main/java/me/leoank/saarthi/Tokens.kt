package me.leoank.saarthi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import uniffi.saarthi_core.ClaudeTokenInfo

/** Within this many days of expiring, a token is shown as due (the hub reminds then too). */
private const val DUE_DAYS = 30L

private val date: DateTimeFormatter = DateTimeFormatter.ofPattern("d MMM yyyy")

/** The Claude accounts the hub holds as named `claude setup-token` tokens (ADR-0039): each
 *  one's expiry, which nodes start sessions under it by default, and the account's 5-hour
 *  and 7-day usage as a session under it last reported (ADR-0042). The expiry is a year
 *  after the hub first read the token unless set here by hand. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TokensScreen(model: HubModel, snackbar: SnackbarHostState, back: () -> Unit) {
    LaunchedEffect(Unit) { model.loadClaudeTokens() }
    var editing by remember { mutableStateOf<ClaudeTokenInfo?>(null) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Claude accounts") },
                navigationIcon = { IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") } },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        LazyColumn(
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.fillMaxSize().padding(padding),
        ) {
            if (model.claudeTokens.isEmpty()) {
                item { Empty("No Claude tokens", "The hub reads them from its --claude-tokens file.") }
            }
            items(model.claudeTokens, key = { it.name }) { t ->
                val defaultOn = model.nodes.filter { it.defaultClaudeToken == t.name }.map { it.name }
                TokenCard(t, defaultOn, edit = { editing = t }, clear = { model.setClaudeTokenExpiry(t.name, 0) })
            }
        }
    }
    editing?.let { t ->
        val state = rememberDatePickerState(initialSelectedDateMillis = t.expiresMs)
        DatePickerDialog(
            onDismissRequest = { editing = null },
            confirmButton = {
                TextButton(
                    enabled = state.selectedDateMillis != null,
                    onClick = {
                        state.selectedDateMillis?.let { model.setClaudeTokenExpiry(t.name, it) }
                        editing = null
                    },
                ) { Text("Set") }
            },
            dismissButton = { TextButton(onClick = { editing = null }) { Text("Cancel") } },
        ) {
            DatePicker(state, title = { Text("${t.name} expires on", Modifier.padding(start = 24.dp, top = 16.dp)) })
        }
    }
}

@Composable
private fun TokenCard(t: ClaudeTokenInfo, defaultOn: List<String>, edit: () -> Unit, clear: () -> Unit) {
    val now = System.currentTimeMillis()
    val days = (t.expiresMs - now) / 86_400_000
    val due = days < DUE_DAYS
    Card(
        Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Column(Modifier.padding(start = 16.dp, end = 8.dp, top = 12.dp, bottom = 4.dp)) {
            Text(t.name, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurface)
            val left = when {
                t.expiresMs <= now -> "expired"
                days == 0L -> "today"
                else -> "in $days days"
            }
            Text(
                "Expires ${date.format(Instant.ofEpochMilli(t.expiresMs).atZone(ZoneOffset.UTC))} · $left" +
                    if (t.expirySet) " · set by hand" else "",
                style = MaterialTheme.typography.bodyMedium,
                color = if (due) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface,
            )
            if (t.usageMs > 0) {
                val parts = listOfNotNull(
                    t.fiveHourPct?.let { "5-hour ${"%.0f".format(it)}%" },
                    t.sevenDayPct?.let { "7-day ${"%.0f".format(it)}%" },
                )
                Text(
                    "${parts.joinToString(" · ")} · ${relative(t.usageMs)}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } else {
                Text(
                    "No usage seen yet: it shows once a session runs under this token",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (defaultOn.isNotEmpty()) {
                Text(
                    "Default on ${defaultOn.joinToString(", ")}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                if (t.expirySet) TextButton(onClick = clear) { Text("Use a year from added") }
                TextButton(onClick = edit) { Text("Set expiry") }
            }
        }
    }
}
