package me.leoank.saarthi

import android.app.Activity
import android.app.Application
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.Build
import android.os.Process
import android.os.SystemClock
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.view.WindowCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.unifiedpush.android.connector.UnifiedPush
import uniffi.saarthi_core.coreVersion
import java.net.InetAddress

/** What the app does at a cold start, written out as it happens, in the manner of a Linux
 *  boot: the app, the phone and the core, the settings, the network, the hub and what it
 *  lists. Every line is a real step's outcome; nothing is staged. */
class BootLog {
    enum class Kind { Kernel, Banner, Ok, Failed, Info, Job }
    data class Line(val kind: Kind, val text: String, val atUs: Long)
    enum class State { Running, Done, Failed, NoHub }

    val lines = mutableStateListOf<Line>()
    var state by mutableStateOf(State.Running)
        private set

    private fun now(): Long = sinceStartUs()

    private fun add(kind: Kind, text: String): Int {
        lines.add(Line(kind, text, now()))
        return lines.lastIndex
    }

    private fun finish(at: Int, kind: Kind, text: String) {
        lines[at] = Line(kind, text, now())
    }

    suspend fun boot(model: HubModel, app: Application) {
        val pkg = app.packageManager.getPackageInfo(app.packageName, 0)
        @Suppress("DEPRECATION")
        val code = if (Build.VERSION.SDK_INT >= 28) pkg.longVersionCode else pkg.versionCode.toLong()
        add(Kind.Kernel, "saarthi ${pkg.versionName} ($code) on ${Build.MANUFACTURER} ${Build.MODEL}")
        add(
            Kind.Kernel,
            "Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT}), " +
                "Linux ${System.getProperty("os.version")}, ${Build.SUPPORTED_ABIS.firstOrNull() ?: "?"}",
        )
        val core = withContext(Dispatchers.IO) { runCatching { coreVersion() } }
        core.onFailure {
            add(Kind.Failed, "Failed to load saarthi-core: ${it.message}")
            state = State.Failed
            return
        }
        add(Kind.Kernel, "saarthi-core ${core.getOrNull()} loaded")
        add(Kind.Banner, "<<< saarthi >>>")

        val hubs = model.hubs.size
        val active = model.active
        add(
            Kind.Ok,
            "Loaded settings: $hubs ${if (hubs == 1) "hub" else "hubs"}" +
                (active?.let { ", ${it.title} active" } ?: ""),
        )
        if (active == null) {
            add(Kind.Info, "No hub configured; opening Settings")
            state = State.NoHub
            return
        }
        network(app)
        push(app)
        connect(model)
    }

    /** After a failed start: the connection again, logged below what came before. */
    suspend fun retry(model: HubModel) {
        state = State.Running
        add(Kind.Kernel, "retrying")
        connect(model)
    }

    private fun network(app: Application) {
        val cm = app.getSystemService(ConnectivityManager::class.java)
        val caps = cm?.activeNetwork?.let { cm.getNetworkCapabilities(it) }
        if (caps == null) {
            add(Kind.Failed, "Failed to find a network")
            return
        }
        val kinds = listOfNotNull(
            "Wi-Fi".takeIf { caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) },
            "mobile data".takeIf { caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) },
            "Ethernet".takeIf { caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) },
            "VPN".takeIf { caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN) },
        )
        add(Kind.Ok, "Reached network: ${kinds.ifEmpty { listOf("unknown") }.joinToString(", ")}")
    }

    private fun push(app: Application) {
        val distributor = UnifiedPush.getAckDistributor(app) ?: UnifiedPush.getDistributors(app).firstOrNull()
        if (distributor != null) add(Kind.Ok, "Found push distributor $distributor")
        else add(Kind.Info, "No UnifiedPush distributor; notifications off")
    }

    private suspend fun connect(model: HubModel) {
        val hub = model.active ?: return
        val host = hub.host
        // A name is looked up the way the core will; an address needs no lookup.
        if (!host.matches(Regex("[0-9.]+")) && ':' !in host) {
            val resolving = add(Kind.Job, "Resolving $host")
            val started = SystemClock.elapsedRealtime()
            val ips = withContext(Dispatchers.IO) {
                runCatching { withTimeout(6_000) { InetAddress.getAllByName(host).map { it.hostAddress } } }
            }
            val took = SystemClock.elapsedRealtime() - started
            ips.fold(
                { finish(resolving, Kind.Ok, "Resolved $host to ${it.joinToString(", ")} ($took ms)") },
                { finish(resolving, Kind.Failed, "Failed to resolve $host: ${it.message ?: it.javaClass.simpleName}") },
            )
        }

        val linking = add(Kind.Job, "A start job is running for hub ${hub.title}")
        val t = SystemClock.elapsedRealtime()
        val error = model.connectNow()
        if (error != null) {
            finish(linking, Kind.Failed, "Failed to connect to hub ${hub.title}: $error")
            add(Kind.Info, "Check the hub's address and token in Settings, or retry")
            state = State.Failed
            return
        }
        finish(linking, Kind.Ok, "Connected to hub ${hub.title} at ${hub.address.trim()} (${SystemClock.elapsedRealtime() - t} ms)")
        for (n in model.nodes.sortedWith(compareBy({ !it.connected }, { it.name }))) {
            if (n.connected) add(Kind.Ok, "Reached node ${n.name} (v${n.version})")
            else add(Kind.Info, "Node ${n.name} is offline, last seen ${relative(n.lastSeenMs)}")
        }
        val live = model.sessions.size
        val waiting = model.pending.size
        add(
            Kind.Ok,
            "Found $live live ${if (live == 1) "session" else "sessions"}" +
                if (waiting > 0) ", $waiting waiting for you" else "",
        )
        if (model.claudeTokens.isNotEmpty()) {
            val n = model.claudeTokens.size
            add(Kind.Ok, "Found $n Claude ${if (n == 1) "account" else "accounts"}")
        }
        add(Kind.Ok, "Started watch on ${hub.title}")
        add(Kind.Ok, "Reached target Sessions.")
        state = State.Done
    }
}

/** Microseconds since this process started, as a kernel counts from boot. */
private fun sinceStartUs(): Long =
    SystemClock.elapsedRealtimeNanos() / 1000 - Process.getStartElapsedRealtime() * 1000

private val Black = Color(0xFF000000)
private val Text = Color(0xFFC8C8C8)
private val Dim = Color(0xFF7A7A7A)
private val Green = Color(0xFF5FD35F)
private val Red = Color(0xFFFF5F5F)
private val Yellow = Color(0xFFE5C07B)
private val White = Color(0xFFFFFFFF)

/** systemd's start-job spinner: three stars sweeping a six-wide field. */
private val spinner = listOf("*     ", "**    ", "***   ", " ***  ", "  *** ", "   ***", "    **", "     *")

/** The boot log on black, white-on-black whatever the phone's theme; on failure, Retry
 *  and Settings. Lines are printed one after another, as a console scrolls, even when the
 *  steps behind them took no time; when the start is over and every line is shown, the
 *  log stays a moment and `finished` is called. */
@Composable
fun BootScreen(log: BootLog, retry: () -> Unit, settings: () -> Unit, finished: () -> Unit) {
    // Light system-bar icons over black, put back on leaving.
    val view = LocalView.current
    DisposableEffect(Unit) {
        val window = (view.context as? Activity)?.window
        val bars = window?.let { WindowCompat.getInsetsController(it, view) }
        val status = bars?.isAppearanceLightStatusBars
        val nav = bars?.isAppearanceLightNavigationBars
        bars?.isAppearanceLightStatusBars = false
        bars?.isAppearanceLightNavigationBars = false
        onDispose {
            status?.let { bars.isAppearanceLightStatusBars = it }
            nav?.let { bars.isAppearanceLightNavigationBars = it }
        }
    }
    val running = log.lines.any { it.kind == BootLog.Kind.Job }
    val clock by produceState(SystemClock.elapsedRealtime(), running) {
        while (running) {
            value = SystemClock.elapsedRealtime()
            delay(120)
        }
    }
    var shown by remember { mutableIntStateOf(0) }
    LaunchedEffect(log.lines.size) {
        while (shown < log.lines.size) {
            delay(45)
            shown++
        }
    }
    val over = shown >= log.lines.size && log.state != BootLog.State.Running
    LaunchedEffect(over, log.state) {
        if (over && log.state != BootLog.State.Failed) {
            delay(1_000)
            finished()
        }
    }
    val list = rememberLazyListState()
    LaunchedEffect(shown, over) {
        if (shown > 0) list.animateScrollToItem(shown)
    }
    val style = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 12.sp, lineHeight = 17.sp, color = Text)
    Box(Modifier.fillMaxSize().background(Black).systemBarsPadding()) {
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize().padding(horizontal = 10.dp, vertical = 8.dp),
        ) {
            itemsIndexed(log.lines.take(shown)) { _, line -> Text(render(line, clock), style = style) }
            item {
                if (over && log.state == BootLog.State.Failed) {
                    Row(Modifier.fillMaxWidth().padding(top = 20.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                        Text("[ Retry ]", style = style.copy(color = White), modifier = Modifier.clickable(onClick = retry).padding(4.dp))
                        Text("[ Settings ]", style = style.copy(color = White), modifier = Modifier.clickable(onClick = settings).padding(4.dp))
                    }
                } else {
                    Column(Modifier.padding(top = 2.dp)) { Text(if (running || !over) "" else "_", style = style.copy(color = Dim)) }
                }
            }
        }
    }
}

private fun render(line: BootLog.Line, clock: Long) = buildAnnotatedString {
    fun tag(color: Color, text: String) {
        append("[")
        withStyle(SpanStyle(color = color, fontWeight = FontWeight.Bold)) { append(text) }
        append("] ")
    }
    when (line.kind) {
        BootLog.Kind.Kernel -> {
            withStyle(SpanStyle(color = Dim)) { append("[%5d.%06d] ".format(line.atUs / 1_000_000, line.atUs % 1_000_000)) }
            append(line.text)
        }
        BootLog.Kind.Banner -> withStyle(SpanStyle(color = White, fontWeight = FontWeight.Bold)) { append("\n${line.text}\n") }
        BootLog.Kind.Ok -> { tag(Green, "  OK  "); append(line.text) }
        BootLog.Kind.Failed -> { tag(Red, "FAILED"); append(line.text) }
        BootLog.Kind.Info -> { tag(Yellow, " INFO "); append(line.text) }
        BootLog.Kind.Job -> {
            val since = (sinceStartUs() - line.atUs) / 1000
            tag(Red, spinner[((clock / 120) % spinner.size).toInt()])
            append("${line.text} (${since / 1000}s)")
        }
    }
}
