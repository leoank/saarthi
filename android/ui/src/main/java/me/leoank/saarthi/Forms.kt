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
import androidx.compose.foundation.border
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import uniffi.saarthi_core.ClaudeTokenInfo
import uniffi.saarthi_core.NodeInfo

/** Models offered for a new session: claude's `--model` aliases, empty for its default. */
val models = listOf("Default" to "", "Fable" to "fable", "Opus" to "opus", "Sonnet" to "sonnet", "Haiku" to "haiku")

/** A path with the home directory as `~`. */
fun short(path: String): String = path.replace(Regex("^/(Users|home)/[^/]+"), "~")

/** A settings section's heading, as a rule (ADR-0069). */
@Composable
fun Heading(text: String) = SectionTitle(text, Modifier.padding(top = 4.dp))

/** Rows on one surface, like a settings group, with a hairline round it. */
@Composable
fun Group(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(6.dp))
            .background(MaterialTheme.colorScheme.surfaceContainerLow)
            .border(1.dp, Flame.Line, RoundedCornerShape(6.dp)),
    ) { content() }
}

@Composable
fun Divider() {
    HorizontalDivider(Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.outlineVariant)
}

/** A row showing a setting and its value; a tap opens a menu of `options` (label to value),
 *  each with an optional line under it from `details`. */
@Composable
fun Choice(
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
