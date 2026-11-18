package me.leoank.vtspike

import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Button
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import java.io.File

/** What a run feeds the terminal. `log` is a file of coloured lines written on first start,
 *  so both libraries read the same bytes. */
private fun workloads(files: File): List<Pair<String, String>> {
    val log = File(files, "log.ansi")
    if (!log.exists()) {
        log.bufferedWriter().use { w ->
            for (i in 0 until 50_000) {
                w.write("\u001b[3${i % 8}m%06d \u001b[1mbuild\u001b[0m \u001b[4${(i + 3) % 8}m step \u001b[0m ".format(i))
                w.write("\u001b[38;5;${i % 256}mcompiling crate ${i % 97} of 97\u001b[0m \u001b[2m(${i % 13}.${i % 10}s)\u001b[0m\r\n")
            }
        }
    }
    return listOf(
        "yes" to "yes 'the quick brown fox jumps over the lazy dog' | head -n 200000",
        // Output that does not stop for ten seconds: the frame rate while it streams.
        "flood" to "timeout 10 yes 'the quick brown fox jumps over the lazy dog'",
        "log" to "cat ${log.path}",
        "top" to "top -d 0.5",
        "sh" to "sh -i",
    )
}

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // `am start -e lib termux -e load log` runs one load without touching the screen.
        val lib0 = intent.getStringExtra("lib") ?: "ghostty"
        val load0 = intent.getStringExtra("load") ?: "sh"
        val loads = workloads(filesDir)
        setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                Spike(loads, lib0, load0)
            }
        }
    }
}

@Composable
private fun Spike(loads: List<Pair<String, String>>, lib0: String, load0: String) {
    var lib by remember { mutableStateOf(lib0) }
    var load by remember { mutableStateOf(load0) }
    var run by remember { mutableStateOf(0) }
    val stats = remember { Stats() }
    var line by remember { mutableStateOf("") }
    var sink by remember { mutableStateOf<ByteSink?>(null) }
    var ctrl by remember { mutableStateOf(false) }
    var text by remember { mutableStateOf("") }
    val command = loads.first { it.first == load }.second
    LaunchedEffect(lib, load, run) {
        while (true) {
            delay(250)
            line = stats.line()
            Log.i("vtspike", "$lib $load ${stats.line()}")
        }
    }
    fun send(bytes: ByteArray) = sink?.send(bytes)
    fun typed(s: String) = send(s.toByteArray())

    Column(Modifier.fillMaxSize().background(Color(0xff1e1e1e)).safeDrawingPadding().imePadding()) {
        Row(Modifier.horizontalScroll(rememberScrollState()).padding(4.dp)) {
            for (l in listOf("ghostty", "termux")) FilterChip(lib == l, { lib = l }, { Text(l) }, Modifier.padding(end = 4.dp))
            for ((name, _) in loads) FilterChip(load == name, { load = name }, { Text(name) }, Modifier.padding(end = 4.dp))
            TextButton(onClick = { run++ }) { Text("again") }
        }
        Text(line, color = Color(0xffaaaaaa), fontSize = 11.sp, modifier = Modifier.padding(horizontal = 8.dp))
        // A fresh terminal for each library, load and run.
        Column(Modifier.weight(1f).fillMaxWidth()) {
            key(lib, load, run) {
                if (lib == "ghostty") GhosttyTerminal(command, stats, { sink = it })
                else TermuxTerminal(command, stats, { sink = it })
            }
        }
        // The key row: what a phone keyboard lacks.
        Row(Modifier.horizontalScroll(rememberScrollState())) {
            @Composable
            fun k(label: String, onClick: () -> Unit) = TextButton(onClick = onClick) { Text(label) }
            k("Esc") { typed("\u001b") }
            k("Tab") { typed("\t") }
            k(if (ctrl) "CTRL" else "Ctrl") { ctrl = !ctrl }
            k("←") { typed("\u001b[D") }
            k("↑") { typed("\u001b[A") }
            k("↓") { typed("\u001b[B") }
            k("→") { typed("\u001b[C") }
            k("^C") { typed("\u0003") }
        }
        Row(Modifier.fillMaxWidth().padding(4.dp)) {
            OutlinedTextField(text, { text = it }, Modifier.weight(1f), singleLine = true)
            Button(onClick = {
                // A sticky Ctrl turns the first character into its control code.
                val out = if (ctrl && text.isNotEmpty()) {
                    ctrl = false
                    (text[0].uppercaseChar().code and 0x1f).toChar() + text.substring(1)
                } else text + "\r"
                typed(out)
                text = ""
            }, Modifier.padding(start = 4.dp)) { Text("Send") }
        }
    }
}
