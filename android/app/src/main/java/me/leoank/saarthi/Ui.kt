package me.leoank.saarthi

import android.text.format.DateUtils
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.WrapText
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import uniffi.saarthi_core.SessionInfo
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

/** The last path segment, which is how a session is named everywhere. */
fun SessionInfo.name(): String = cwd.trimEnd('/').substringAfterLast('/').ifEmpty { cwd.ifEmpty { "session" } }

/** What the session is about: the title Claude Code gave it or the person named it, else its
 *  folder (phase PHS-20261117-f22kxe item 3). */
fun SessionInfo.title(): String = title.ifBlank { name() }

/** Where it runs, for the line under its title: "folder · branch" once it has a title, else
 *  the folder's parent, as before. */
fun SessionInfo.place(): String {
    if (title.isBlank()) return parentPath()
    return listOf(repo(), gitBranch).filter { it.isNotBlank() }.joinToString(" · ")
}

/** The repository it works in, by its folder's name: a worktree's folder is often named after
 *  its branch (or is `main`), and then the repository is the folder above. */
fun SessionInfo.repo(): String {
    val parts = cwd.trimEnd('/').split('/').filter { it.isNotEmpty() }
    val folder = parts.lastOrNull().orEmpty()
    val branchy = gitBranch.isNotBlank() && (folder == gitBranch || folder == gitBranch.substringAfterLast('/'))
    return if (branchy || folder in setOf("main", "master")) parts.getOrNull(parts.size - 2) ?: folder else folder
}

/** The state of a session at a glance: needs the person, Claude working, ready for a
 *  prompt, or done. */
enum class SessionState(val label: String) { INPUT("INPUT"), RUN("RUN"), READY("READY"), DONE("DONE") }

fun SessionInfo.state(waiting: Int): SessionState = when {
    endedMs != null -> SessionState.DONE
    waiting > 0 -> SessionState.INPUT
    busy -> SessionState.RUN
    else -> SessionState.READY
}

/** A session's state as a small pill: the only coloured thing on its row. */
@Composable
fun StatePill(state: SessionState, modifier: Modifier = Modifier) {
    val (bg, fg) = when (state) {
        SessionState.INPUT -> MaterialTheme.colorScheme.errorContainer to MaterialTheme.colorScheme.onErrorContainer
        SessionState.RUN -> MaterialTheme.colorScheme.secondaryContainer to MaterialTheme.colorScheme.onSecondaryContainer
        SessionState.READY, SessionState.DONE -> MaterialTheme.colorScheme.surfaceVariant to MaterialTheme.colorScheme.onSurfaceVariant
    }
    Text(
        state.label,
        style = MaterialTheme.typography.labelSmall,
        fontWeight = androidx.compose.ui.text.font.FontWeight.Bold,
        color = fg,
        modifier = modifier
            .background(bg, androidx.compose.foundation.shape.RoundedCornerShape(6.dp))
            .padding(horizontal = 6.dp, vertical = 1.dp),
    )
}

/** The directory above the name, with the home directory as `~`, for a second line. */
fun SessionInfo.parentPath(): String {
    val parent = cwd.trimEnd('/').substringBeforeLast('/', "")
    return parent.replace(Regex("^/(Users|home)/[^/]+"), "~")
}

fun relative(timeMs: Long): String {
    val now = System.currentTimeMillis()
    if (now - timeMs < DateUtils.MINUTE_IN_MILLIS) return "just now"
    return DateUtils.getRelativeTimeSpanString(timeMs, now, DateUtils.MINUTE_IN_MILLIS).toString()
}

private val clock: DateTimeFormatter = DateTimeFormatter.ofPattern("HH:mm")
private val dayClock: DateTimeFormatter = DateTimeFormatter.ofPattern("EEE HH:mm")

fun clockTime(timeMs: Long): String = clock.format(Instant.ofEpochMilli(timeMs).atZone(ZoneId.systemDefault()))

/** "14:05" today, "Tue 14:05" on another day. */
fun whenLabel(timeMs: Long): String {
    val t = Instant.ofEpochMilli(timeMs).atZone(ZoneId.systemDefault())
    val today = Instant.now().atZone(ZoneId.systemDefault()).toLocalDate()
    return if (t.toLocalDate() == today) clock.format(t) else dayClock.format(t)
}

fun gigabytes(bytes: Double): String = "%.1f GB".format(bytes / 1_073_741_824.0)

fun thousands(n: ULong): String = when {
    n < 1000uL -> n.toString()
    n < 1_000_000uL -> "%.1fk".format(n.toDouble() / 1000.0)
    else -> "%.2fM".format(n.toDouble() / 1e6)
}

fun dollars(usd: Double): String = if (usd < 0.01) "$%.3f".format(usd) else "$%.2f".format(usd)

/** Whether monospace text (code, diffs, a tool's output) wraps at the screen's edge or
 *  scrolls sideways: one app-wide choice, toggled from the session screen and the
 *  full-screen view, remembered across launches. */
class Wrap(val on: Boolean, val toggle: () -> Unit)

val LocalWrap = compositionLocalOf { Wrap(false) {} }

/** Sideways scrolling for text that does not wrap. */
@Composable
fun Modifier.scrollUnlessWrapped(wrap: Boolean): Modifier {
    val state = rememberScrollState()
    return if (wrap) this else horizontalScroll(state)
}

@Composable
fun WrapButton() {
    val wrap = LocalWrap.current
    IconButton(onClick = wrap.toggle) {
        Icon(
            Icons.AutoMirrored.Filled.WrapText,
            if (wrap.on) "Stop wrapping lines" else "Wrap lines",
            tint = if (wrap.on) MaterialTheme.colorScheme.primary else LocalContentColor.current,
        )
    }
}

@Composable
fun StatusDot(on: Boolean, modifier: Modifier = Modifier) {
    val color = if (on) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outlineVariant
    Box(modifier.size(10.dp).background(color, CircleShape))
}

/** A short centred line of small text, for "no sessions" and similar. */
@Composable
fun Caption(text: String, modifier: Modifier = Modifier, color: Color = MaterialTheme.colorScheme.onSurfaceVariant) {
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = color,
        textAlign = TextAlign.Center,
        modifier = modifier.fillMaxWidth().padding(vertical = 8.dp),
    )
}

@Composable
fun SectionTitle(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = modifier.padding(top = 16.dp, bottom = 4.dp),
    )
}

@Composable
fun Empty(title: String, detail: String, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth().padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Text(title, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
        Text(
            detail,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(top = 8.dp),
        )
    }
}
