package me.leoank.saarthi

import android.text.format.DateUtils
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.WrapText
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.border
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.sp
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

/** The colour a state is said in (ADR-0069). */
fun SessionState.color(): Color = when (this) {
    SessionState.INPUT -> Flame.Hot
    SessionState.RUN -> Flame.Blue
    SessionState.READY -> Flame.Ok
    SessionState.DONE -> Flame.Dim
}

/** A session's state as a coloured lowercase word in mono, `input`, `run`, `ready`,
 *  `done` (ADR-0069). */
@Composable
fun StatePill(state: SessionState, modifier: Modifier = Modifier) {
    Text(
        state.label.lowercase(),
        fontFamily = Mono,
        fontSize = 11.sp,
        fontWeight = FontWeight.SemiBold,
        color = state.color(),
        modifier = modifier,
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

/** Up or down, as a terminal marks it: `●` green, `○` dim. */
@Composable
fun StatusDot(on: Boolean, modifier: Modifier = Modifier) {
    Text(if (on) "●" else "○", fontFamily = Mono, fontSize = 11.sp, color = if (on) Flame.Ok else Flame.Dim, modifier = modifier)
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

/** A section as a rule, `── needs you ──────── 2` (ADR-0069): its name lowercase in mono,
 *  a hairline to the edge and its count at the end; amber when it needs the owner. */
@Composable
fun SectionTitle(text: String, modifier: Modifier = Modifier, count: Int? = null, hot: Boolean = false) {
    val color = if (hot) Flame.Hot else Flame.Dim
    Row(verticalAlignment = Alignment.CenterVertically, modifier = modifier.fillMaxWidth().padding(top = 16.dp, bottom = 6.dp)) {
        Text("── ", fontFamily = Mono, fontSize = 11.sp, color = Flame.LineStrong)
        Text(text.lowercase(), fontFamily = Mono, fontSize = 11.sp, fontWeight = FontWeight.Medium, color = color)
        Spacer(Modifier.width(8.dp))
        Box(Modifier.weight(1f).height(1.dp).background(Flame.Line))
        if (count != null) {
            Spacer(Modifier.width(8.dp))
            Text("$count", fontFamily = Mono, fontSize = 11.sp, color = if (hot) Flame.Hot else Flame.Dim)
        }
    }
}

/** A workspace root's top line, a prompt saying where you are (ADR-0069): `hub:~/agents`,
 *  and under it the hub's state in one dim line, red when it is down. */
@Composable
fun PromptLine(hub: String, path: String, status: String, statusColor: Color = Flame.Dim, onStatus: (() -> Unit)? = null) {
    Column(Modifier.padding(start = 2.dp)) {
        Text(
            androidx.compose.ui.text.buildAnnotatedString {
                withStyle(androidx.compose.ui.text.SpanStyle(color = Flame.Agents)) { append(hub.lowercase().replace(' ', '-')) }
                withStyle(androidx.compose.ui.text.SpanStyle(color = Flame.Dim)) { append(":") }
                withStyle(androidx.compose.ui.text.SpanStyle(color = Flame.Text, fontWeight = FontWeight.SemiBold)) { append(path) }
            },
            fontFamily = Mono,
            fontSize = 15.sp,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            // Its leading mark green while the hub is up.
            if (status.startsWith("●")) androidx.compose.ui.text.buildAnnotatedString {
                withStyle(androidx.compose.ui.text.SpanStyle(color = Flame.Ok)) { append("●") }
                append(status.drop(1))
            } else androidx.compose.ui.text.AnnotatedString(status),
            fontFamily = Mono,
            fontSize = 11.sp,
            color = statusColor,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = if (onStatus != null) Modifier.clickable(onClick = onStatus) else Modifier,
        )
    }
}

/** Nothing to show, said as a terminal would: a `# comment` title and why. */
@Composable
fun Empty(title: String, detail: String, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth().padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Text("# " + title.lowercase(), fontFamily = Mono, fontSize = 14.sp, color = Flame.Soft, textAlign = TextAlign.Center)
        Text(
            detail,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(top = 8.dp),
        )
    }
}

/** A dashed hairline along the top, between rows (ADR-0069). */
fun Modifier.dashedTop(): Modifier = drawBehind {
    drawLine(
        Flame.Line,
        start = androidx.compose.ui.geometry.Offset(0f, 0f),
        end = androidx.compose.ui.geometry.Offset(size.width, 0f),
        strokeWidth = 1.dp.toPx(),
        pathEffect = androidx.compose.ui.graphics.PathEffect.dashPathEffect(floatArrayOf(4.dp.toPx(), 3.dp.toPx())),
    )
}

/** A braille spinner, as a terminal shows work under way. */
@Composable
fun Spinner(color: Color = Flame.Blue) {
    val frames = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
    var i by androidx.compose.runtime.remember { androidx.compose.runtime.mutableIntStateOf(0) }
    androidx.compose.runtime.LaunchedEffect(Unit) {
        while (true) {
            kotlinx.coroutines.delay(80)
            i = (i + 1) % frames.length
        }
    }
    Text(frames[i].toString(), fontFamily = Mono, fontSize = 13.sp, color = color)
}

/** A filter as a small mono tag (ADR-0069), filled in the accent when on. */
@Composable
fun Tag(label: String, on: Boolean, onClick: () -> Unit, mark: (@Composable () -> Unit)? = null) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .clip(androidx.compose.foundation.shape.RoundedCornerShape(4.dp))
            .background(if (on) Flame.Agents else Color.Transparent)
            .border(1.dp, if (on) Flame.Agents else Flame.LineStrong, androidx.compose.foundation.shape.RoundedCornerShape(4.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 4.dp),
    ) {
        if (mark != null) {
            mark()
            Spacer(Modifier.width(5.dp))
        }
        Text(label, fontFamily = Mono, fontSize = 12.sp, fontWeight = if (on) FontWeight.SemiBold else FontWeight.Normal, color = if (on) Flame.Bg else Flame.Soft)
    }
}

/** The one action a workspace root offers, `+ new session` (ADR-0069): outlined in the
 *  accent with a soft glow, in mono. */
@Composable
fun ActionButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val shape = androidx.compose.foundation.shape.RoundedCornerShape(6.dp)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = modifier
            .shadow(14.dp, shape, ambientColor = Flame.Blue, spotColor = Flame.Blue)
            .clip(shape)
            .background(Flame.S0)
            .background(Flame.soft(Flame.Blue, 0.14f))
            .border(1.dp, Flame.Blue, shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 11.dp),
    ) {
        Text("+ ", fontFamily = Mono, fontSize = 14.sp, fontWeight = FontWeight.Bold, color = Flame.Blue)
        Text(label, fontFamily = Mono, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = Flame.Core)
    }
}
