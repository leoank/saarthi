package me.leoank.saarthi

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.UnfoldMore
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.WarningAmber
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.SuggestionChip
import androidx.compose.material3.SuggestionChipDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import uniffi.saarthi_core.ClaudeTokenInfo
import uniffi.saarthi_core.NodeInfo

/** Models offered for a new session: claude's `--model` aliases, empty for its default. */
private val models = listOf("Default" to "", "Fable" to "fable", "Opus" to "opus", "Sonnet" to "sonnet", "Haiku" to "haiku")

/** Permission modes offered: claude's `--permission-mode` values, empty for the user's
 *  default, each with a line saying what it means. */
private val modes = listOf(
    Triple("Ask", "", "Permission requests come to this app"),
    Triple("Accept edits", "acceptEdits", "File edits go ahead; other tools ask"),
    Triple("Plan", "plan", "Reads and plans; changes nothing"),
    Triple("Bypass", "bypassPermissions", "Every tool runs without asking"),
)

/** A path with the home directory as `~`. */
private fun short(path: String): String = path.replace(Regex("^/(Users|home)/[^/]+"), "~")

/** A new headless session (ADR-0034), as a full-screen sheet: the first prompt on top, then
 *  where it runs (a node that allows spawning, a directory under one of its roots, made if
 *  missing unless unticked), then its options as rows that each open a menu: model,
 *  permission mode, the Claude account when the hub holds named tokens (ADR-0039) and the
 *  launch profile when the node has any (ADR-0038). [start] gets the node's id and the
 *  choices; its token is null for the node's default, empty for the node's own login. */
@OptIn(ExperimentalMaterial3Api::class)
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
    val bypass = mode == "bypassPermissions"

    Dialog(
        onDismissRequest = dismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Text("New session") },
                    navigationIcon = { IconButton(onClick = dismiss) { Icon(Icons.Filled.Close, "Cancel") } },
                )
            },
            bottomBar = {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .background(MaterialTheme.colorScheme.surface)
                        .navigationBarsPadding()
                        .imePadding()
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                ) {
                    Button(
                        enabled = cwd.isNotBlank(),
                        onClick = { start(node.id, cwd, prompt, chosen, create, mode, profile?.name ?: "", token) },
                        contentPadding = PaddingValues(vertical = 14.dp),
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(if (prompt.isBlank()) "Start idle" else "Start session") }
                }
            },
        ) { padding ->
            Column(
                Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp),
            ) {
                OutlinedTextField(
                    value = prompt,
                    onValueChange = { prompt = it },
                    minLines = 4,
                    maxLines = 10,
                    placeholder = { Text("What should Claude work on?") },
                    supportingText = { Text("Optional. Without it the session waits for your first message.") },
                    shape = RoundedCornerShape(16.dp),
                    colors = OutlinedTextFieldDefaults.colors(
                        unfocusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
                        focusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
                        unfocusedBorderColor = MaterialTheme.colorScheme.surfaceContainerLow,
                    ),
                    modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
                )

                Heading("Where")
                if (nodes.size > 1) {
                    if (nodes.size <= 3) {
                        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(bottom = 12.dp)) {
                            nodes.forEachIndexed { i, n ->
                                SegmentedButton(
                                    selected = n.id == nodeId,
                                    onClick = { nodeId = n.id },
                                    shape = SegmentedButtonDefaults.itemShape(i, nodes.size),
                                ) { Text(n.name, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                            }
                        }
                    } else {
                        Group(Modifier.padding(bottom = 12.dp)) {
                            Choice("Node", node.name, nodes.map { it.name to it.id }, nodeId) { nodeId = it }
                        }
                    }
                }
                OutlinedTextField(
                    value = cwd,
                    onValueChange = { cwd = it },
                    singleLine = true,
                    leadingIcon = { Icon(Icons.Outlined.Folder, null) },
                    supportingText = { Text("Under ${node.spawnRoots.joinToString(" or ") { short(it) }}") },
                    shape = RoundedCornerShape(12.dp),
                    modifier = Modifier.fillMaxWidth(),
                )
                if (dirs.size > 1) {
                    LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        items(dirs) { d ->
                            val on = d == cwd
                            SuggestionChip(
                                onClick = { cwd = d },
                                label = { Text(d.trimEnd('/').substringAfterLast('/'), maxLines = 1) },
                                colors = if (on) {
                                    SuggestionChipDefaults.suggestionChipColors(
                                        containerColor = MaterialTheme.colorScheme.secondaryContainer,
                                        labelColor = MaterialTheme.colorScheme.onSecondaryContainer,
                                    )
                                } else {
                                    SuggestionChipDefaults.suggestionChipColors()
                                },
                                border = if (on) null else SuggestionChipDefaults.suggestionChipBorder(true),
                            )
                        }
                    }
                }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).clickable { create = !create },
                ) {
                    Checkbox(checked = create, onCheckedChange = { create = it })
                    Text("Create the directory if it is missing", style = MaterialTheme.typography.bodyMedium)
                }

                Heading("Options")
                Group {
                    Choice("Model", models.first { it.second == chosen }.first, models, chosen) { chosen = it }
                    Divider()
                    Choice(
                        "Permissions",
                        modes.first { it.second == mode }.first,
                        modes.map { it.first to it.second },
                        mode,
                        details = modes.associate { it.second to it.third },
                    ) { mode = it }
                    if (tokens.isNotEmpty()) {
                        Divider()
                        val default = "Node default (${node.defaultClaudeToken.ifEmpty { "own login" }})"
                        val options = listOf(default to "\u0000") + tokens.map { it.name to it.name } +
                            // With no default the first option already is the node's own login.
                            if (node.defaultClaudeToken.isEmpty()) emptyList() else listOf("Node's own login" to "")
                        Choice(
                            "Claude account",
                            when (token) {
                                null -> node.defaultClaudeToken.ifEmpty { "Node's login" }
                                "" -> "Node's login"
                                else -> token!!
                            },
                            options,
                            token ?: "\u0000",
                        ) { token = if (it == "\u0000") null else it }
                    }
                    if (node.profiles.isNotEmpty()) {
                        Divider()
                        Choice(
                            "Runs as",
                            profile?.name ?: "This node's user",
                            listOf("This node's user" to "") + node.profiles.map { it.name to it.name },
                            profileName,
                            details = node.profiles.associate { it.name to it.description },
                        ) { profileName = it }
                    }
                }

                val sandbox = if (profile?.sandboxed == true) " in the ${profile.name} sandbox" else ""
                val login = when {
                    effective.isNotEmpty() -> "$sandbox under the $effective account".trimStart()
                    profile?.sandboxed == true -> "in the ${profile.name} sandbox, with its own Claude login"
                    else -> "with ${node.name}'s own Claude login"
                }
                if (bypass) {
                    Row(
                        verticalAlignment = Alignment.Top,
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(top = 16.dp)
                            .clip(RoundedCornerShape(12.dp))
                            .background(MaterialTheme.colorScheme.errorContainer)
                            .padding(12.dp),
                    ) {
                        Icon(
                            Icons.Outlined.WarningAmber,
                            null,
                            tint = MaterialTheme.colorScheme.onErrorContainer,
                            modifier = Modifier.size(20.dp),
                        )
                        Spacer(Modifier.width(10.dp))
                        Text(
                            "Runs headless $login, and every tool runs without asking, as with --dangerously-skip-permissions.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onErrorContainer,
                        )
                    }
                } else {
                    Text(
                        "Runs headless $login. It cannot ask you questions; its permission requests come here.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(top = 12.dp, start = 4.dp, end = 4.dp),
                    )
                }
                Spacer(Modifier.height(24.dp))
            }
        }
    }
}

@Composable
private fun Heading(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(top = 20.dp, bottom = 8.dp, start = 4.dp),
    )
}

/** Rows on one rounded surface, like a settings group. */
@Composable
private fun Group(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .background(MaterialTheme.colorScheme.surfaceContainerLow),
    ) { content() }
}

@Composable
private fun Divider() {
    HorizontalDivider(Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.outlineVariant)
}

/** A row showing a setting and its value; a tap opens a menu of `options` (label to value),
 *  each with an optional line under it from `details`. */
@Composable
private fun Choice(
    label: String,
    value: String,
    options: List<Pair<String, String>>,
    selected: String,
    details: Map<String, String> = emptyMap(),
    pick: (String) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    Box {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier
                .fillMaxWidth()
                .clickable { open = true }
                .padding(start = 16.dp, end = 12.dp, top = 14.dp, bottom = 14.dp),
        ) {
            Text(label, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
            Text(
                value,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Icon(
                Icons.Filled.UnfoldMore,
                null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = 4.dp).size(18.dp),
            )
        }
        // Anchored at the row's bottom right, under the value it changes.
        Box(Modifier.align(Alignment.BottomEnd)) {
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            for ((text, v) in options) {
                val detail = details[v].orEmpty()
                DropdownMenuItem(
                    text = {
                        Column {
                            Text(
                                text,
                                color = if (v == selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
                            )
                            if (detail.isNotEmpty()) {
                                Text(detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                    },
                    onClick = { open = false; pick(v) },
                )
            }
        }
        }
    }
}
