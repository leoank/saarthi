package me.leoank.saarthi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import uniffi.saarthi_core.ClaudeTokenInfo
import uniffi.saarthi_core.NodeInfo

/** Models offered for a new session: claude's `--model` aliases, empty for its default. */
private val models = listOf("Default" to "", "Fable" to "fable", "Opus" to "opus", "Sonnet" to "sonnet", "Haiku" to "haiku")

/** Permission modes offered: claude's `--permission-mode` values, empty for the user's default. */
private val modes = listOf("Ask" to "", "Accept edits" to "acceptEdits", "Plan" to "plan", "Bypass" to "bypassPermissions")

/** A new headless session (ADR-0034): a node that allows spawning, a directory under one of
 *  its roots (made if missing, unless switched off), an optional first prompt, a model, a
 *  permission mode and, when the node has launch profiles (ADR-0038), one of them or the
 *  node's own user, and, when the hub holds named Claude tokens (ADR-0039), the node's
 *  default, one of them, or the node's own login. [start] gets the node's id and the
 *  choices; its token is null for the default, empty for the own login. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun NewSessionDialog(
    model: HubModel,
    nodes: List<NodeInfo>,
    tokens: List<ClaudeTokenInfo>,
    dismiss: () -> Unit,
    start: (nodeId: String, cwd: String, prompt: String, model: String, create: Boolean, mode: String, profile: String, token: String?) -> Unit,
) {
    var nodeId by remember { mutableStateOf(nodes.first().id) }
    val node = nodes.firstOrNull { it.id == nodeId } ?: nodes.first()
    val dirs = model.spawnDirs(node)
    var cwd by remember(nodeId) { mutableStateOf(dirs.firstOrNull() ?: "") }
    var prompt by remember { mutableStateOf("") }
    var chosen by remember { mutableStateOf("") }
    var create by remember { mutableStateOf(true) }
    var mode by remember { mutableStateOf("") }
    var profileName by remember(nodeId) { mutableStateOf("") }
    val profile = node.profiles.firstOrNull { it.name == profileName }
    var token by remember(nodeId) { mutableStateOf<String?>(null) }
    val effective = token ?: node.defaultClaudeToken
    AlertDialog(
        onDismissRequest = dismiss,
        title = { Text("New session") },
        text = {
            Column(
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.verticalScroll(rememberScrollState()),
            ) {
                if (nodes.size > 1) {
                    Label("Node")
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        for (n in nodes) FilterChip(selected = n.id == nodeId, onClick = { nodeId = n.id }, label = { Text(n.name) })
                    }
                }
                Label("Directory")
                OutlinedTextField(
                    value = cwd,
                    onValueChange = { cwd = it },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    supportingText = { Text("Under ${node.spawnRoots.joinToString(" or ")}") },
                )
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (d in dirs) FilterChip(selected = d == cwd, onClick = { cwd = d }, label = { Text(d.substringAfterLast('/')) })
                }
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                    Text("Create it if it does not exist", style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
                    Switch(checked = create, onCheckedChange = { create = it })
                }
                Label("First prompt (optional)")
                OutlinedTextField(
                    value = prompt,
                    onValueChange = { prompt = it },
                    minLines = 3,
                    maxLines = 8,
                    placeholder = { Text("Leave empty to start idle and type in the session") },
                    modifier = Modifier.fillMaxWidth(),
                )
                Label("Model")
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for ((label, alias) in models) FilterChip(selected = chosen == alias, onClick = { chosen = alias }, label = { Text(label) })
                }
                if (node.profiles.isNotEmpty()) {
                    Label("Runs as")
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        FilterChip(selected = profile == null, onClick = { profileName = "" }, label = { Text("This node's user") })
                        for (p in node.profiles) FilterChip(selected = p == profile, onClick = { profileName = p.name }, label = { Text(p.name) })
                    }
                    if (profile != null && profile.description.isNotEmpty()) {
                        Text(profile.description, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                if (tokens.isNotEmpty()) {
                    Label("Claude account")
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        val default = node.defaultClaudeToken.ifEmpty { "node's login" }
                        FilterChip(selected = token == null, onClick = { token = null }, label = { Text("Default ($default)") })
                        for (t in tokens) FilterChip(selected = token == t.name, onClick = { token = t.name }, label = { Text(t.name) })
                        FilterChip(selected = token == "", onClick = { token = "" }, label = { Text("Node's login") })
                    }
                }
                Label("Permissions")
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for ((label, value) in modes) FilterChip(selected = mode == value, onClick = { mode = value }, label = { Text(label) })
                }
                val sandbox = if (profile?.sandboxed == true) " in the ${profile.name} sandbox" else ""
                val login = when {
                    effective.isNotEmpty() -> "$sandbox under the $effective Claude token".trimStart()
                    profile?.sandboxed == true -> "in the ${profile.name} sandbox, with its own Claude token"
                    else -> "with this node's own Claude login"
                }
                Text(
                    if (mode == "bypassPermissions") {
                        "Runs headless $login and every tool without asking, as with --dangerously-skip-permissions."
                    } else {
                        "Runs headless $login. It cannot ask questions; its permission requests come here."
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = if (mode == "bypassPermissions") MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 4.dp),
                )
            }
        },
        confirmButton = {
            TextButton(
                enabled = cwd.isNotBlank(),
                onClick = { start(node.id, cwd, prompt, chosen, create, mode, profile?.name ?: "", token) },
            ) { Text("Start") }
        },
        dismissButton = { TextButton(onClick = dismiss) { Text("Cancel") } },
    )
}

@Composable
private fun Label(text: String) {
    Text(text, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
}
