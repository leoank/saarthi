package me.leoank.saarthi

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** A key the terminal's key bar can hold (phase PHS-20261117-f22kxe item 11): what it shows,
 *  its name in the editor, and what it sends; Ctrl and Alt send nothing themselves but are
 *  held for the next key. */
data class BarKey(val id: String, val label: String, val name: String, val sends: String = "")

object KeyBar {
    const val CTRL = "ctrl"
    const val ALT = "alt"

    /** Every key the bar can hold, in the editor's order. */
    val all = listOf(
        BarKey("esc", "Esc", "Escape", "\u001b"),
        BarKey("tab", "Tab", "Tab", "\t"),
        BarKey("stab", "⇤", "Shift-Tab", "\u001b[Z"),
        BarKey(CTRL, "Ctrl", "Ctrl, held for the next key"),
        BarKey(ALT, "Alt", "Alt, held for the next key"),
        BarKey("left", "←", "Left", "\u001b[D"),
        BarKey("up", "↑", "Up", "\u001b[A"),
        BarKey("down", "↓", "Down", "\u001b[B"),
        BarKey("right", "→", "Right", "\u001b[C"),
        BarKey("ctrl_c", "^C", "Interrupt (Ctrl-C)", "\u0003"),
        BarKey("ctrl_d", "^D", "End of input (Ctrl-D)", "\u0004"),
        BarKey("ctrl_z", "^Z", "Suspend (Ctrl-Z)", "\u001a"),
        BarKey("ctrl_l", "^L", "Clear (Ctrl-L)", "\u000c"),
        BarKey("ctrl_r", "^R", "Search history (Ctrl-R)", "\u0012"),
        BarKey("enter", "⏎", "Enter", "\r"),
        BarKey("home", "Home", "Home", "\u001b[H"),
        BarKey("end", "End", "End", "\u001b[F"),
        BarKey("pgup", "PgUp", "Page up", "\u001b[5~"),
        BarKey("pgdn", "PgDn", "Page down", "\u001b[6~"),
        BarKey("del", "Del", "Delete", "\u001b[3~"),
        BarKey("pipe", "|", "Pipe", "|"),
        BarKey("tilde", "~", "Tilde", "~"),
        BarKey("slash", "/", "Slash", "/"),
        BarKey("dash", "-", "Dash", "-"),
    )
    private val byId = all.associateBy { it.id }

    /** The bar before it is edited: the keys a phone keyboard lacks most. */
    val default = listOf("esc", "tab", CTRL, "left", "up", "down", "right", "ctrl_c")

    /** The saved bar's keys; ids no longer known are dropped. */
    fun keys(ids: List<String>): List<BarKey> = ids.mapNotNull { byId[it] }
}

/** The row under the terminal: the bar's keys, scrolling when they do not fit, and the
 *  editor's pencil pinned at its end. */
@Composable
fun KeyBarRow(keys: List<BarKey>, held: Set<String>, press: (BarKey) -> Unit, edit: () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Row(Modifier.weight(1f).horizontalScroll(rememberScrollState())) {
            for (k in keys) {
                TextButton(onClick = { press(k) }) {
                    Text(
                        k.label,
                        color = if (k.id in held) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
                    )
                }
            }
        }
        IconButton(onClick = edit) { Icon(Icons.Filled.Edit, "Edit key bar") }
    }
}

/** The key bar's editor: the bar's keys in order, each moved or removed, and the keys it
 *  does not hold yet to add at its end. */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun KeyBarEditor(ids: List<String>, change: (List<String>) -> Unit, dismiss: () -> Unit) {
    ModalBottomSheet(onDismissRequest = dismiss) {
        Column(
            Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(start = 20.dp, end = 12.dp, bottom = 24.dp),
        ) {
            Text("Key bar", style = MaterialTheme.typography.titleLarge)
            val keys = KeyBar.keys(ids)
            if (keys.isEmpty()) {
                Text(
                    "No keys: add some below.",
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(vertical = 12.dp),
                )
            }
            keys.forEachIndexed { i, k ->
                fun move(by: Int) = change(keys.map { it.id }.toMutableList().apply { add(i + by, removeAt(i)) })
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(k.label, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(0.25f))
                    Text(k.name, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(0.75f))
                    IconButton(onClick = { move(-1) }, enabled = i > 0) { Icon(Icons.Filled.KeyboardArrowUp, "Move ${k.name} left") }
                    IconButton(onClick = { move(1) }, enabled = i < keys.size - 1) {
                        Icon(Icons.Filled.KeyboardArrowDown, "Move ${k.name} right")
                    }
                    IconButton(onClick = { change(keys.map { it.id } - k.id) }) { Icon(Icons.Filled.Close, "Remove ${k.name}") }
                }
            }
            val rest = KeyBar.all.filter { it.id !in ids }
            if (rest.isNotEmpty()) {
                Text(
                    "Add",
                    style = MaterialTheme.typography.titleSmall,
                    color = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.padding(top = 18.dp, bottom = 4.dp),
                )
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (k in rest) {
                        OutlinedButton(onClick = { change(keys.map { it.id } + k.id) }) { Text(k.label) }
                    }
                }
            }
            TextButton(
                onClick = { change(KeyBar.default) },
                enabled = ids != KeyBar.default,
                modifier = Modifier.padding(top = 12.dp),
            ) { Text("Reset to the default keys") }
        }
    }
}
