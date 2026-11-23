package me.leoank.saarthi

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.activity.compose.setContent
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel

class MainActivity : ComponentActivity() {
    /** A session a notification asked to open: node id and session id. */
    private val requested = mutableStateOf<Pair<String, String>?>(null)
    /** A pairing link the app was opened with. */
    private val paired = mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Dark bars whatever the phone's setting: the app is dark only (ADR-0068).
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        take(intent)
        setContent {
            SaarthiTheme {
                App(requested.value, consumed = { requested.value = null }, paired.value, pairedSeen = { paired.value = null })
            }
        }
    }

    override fun onResume() {
        super.onResume()
        Push.inFront = true
    }

    override fun onPause() {
        Push.inFront = false
        super.onPause()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        take(intent)
    }

    private fun take(intent: Intent?) {
        // A hub handed over as a link (`saarthi ctl pair`): opened in Settings to confirm.
        if (intent?.data?.scheme == "saarthi") {
            paired.value = intent.dataString
            intent.data = null
            return
        }
        val node = intent?.getStringExtra("node_id") ?: return
        val session = intent.getStringExtra("session_id") ?: return
        requested.value = node to session
        intent.removeExtra("session_id")
    }
}

/** Where the person is in a workspace; each screen knows where back goes, in place of a
 *  navigation library. A session is named by ids so the screen follows the live SessionInfo. */
sealed interface Screen {
    data object Home : Screen
    /** The Machines workspace's root: the open shells and every machine (ADR-0076). */
    data object Machines : Screen
    /** [find]: words to search the session for as it opens, from a search's hit. */
    data class Session(val nodeId: String, val sessionId: String, val from: Screen = Home, val find: String? = null) : Screen
    data object History : Screen
    data object Tokens : Screen
    data class Health(val nodeId: String, val from: Screen) : Screen
    /** An open shell (ADR-0054), by its id in the model. */
    data class Terminal(val shellId: Int, val from: Screen) : Screen
    data class Changes(val nodeId: String, val sessionId: String, val from: Screen) : Screen
    data object Yajna : Screen
    data class Notebook(val nodeId: String, val namespace: String, val url: String) : Screen
}

/** Where a screen is, for the switcher: one key per thing open, whatever screen it was
 *  reached from. */
private fun Screen.key(): String = when (this) {
    is Screen.Session -> "s/$nodeId/$sessionId"
    is Screen.Notebook -> "n/$nodeId/$namespace/$url"
    is Screen.Terminal -> "t/$shellId"
    is Screen.Health -> "h/$nodeId"
    is Screen.Changes -> "c/$nodeId/$sessionId"
    else -> toString()
}

/** A pane (ADR-0076): the workspace it holds and that workspace's screens in it. */
private class Slot(w: Workspace) {
    var w by mutableStateOf(w)
    val screens = androidx.compose.runtime.mutableStateMapOf<Workspace, Screen>()
}

/** A pane's header on a wide screen: its workspace, its hue along the top while focused,
 *  and buttons to swap the panes and to give this one the whole width, or, while it has
 *  it ([swap] null), to split the screen again. */
@Composable
private fun PaneHeader(w: Workspace, focused: Boolean, swap: (() -> Unit)?, whole: () -> Unit) {
    androidx.compose.foundation.layout.Column(
        androidx.compose.ui.Modifier
            .fillMaxWidth()
            .background(Flame.S0)
            .windowInsetsPadding(androidx.compose.foundation.layout.WindowInsets.statusBars),
    ) {
        androidx.compose.foundation.layout.Box(
            androidx.compose.ui.Modifier.fillMaxWidth().height(2.dp).background(if (focused) w.hue else androidx.compose.ui.graphics.Color.Transparent),
        )
        androidx.compose.foundation.layout.Row(
            verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
            modifier = androidx.compose.ui.Modifier.fillMaxWidth().height(34.dp).padding(start = 12.dp),
        ) {
            androidx.compose.material3.Text(w.glyph, fontFamily = Mono, fontSize = 12.sp, color = if (focused) w.hue else Flame.Dim)
            androidx.compose.foundation.layout.Spacer(androidx.compose.ui.Modifier.width(8.dp))
            androidx.compose.material3.Text(w.label, fontFamily = Mono, fontSize = 12.sp, color = if (focused) Flame.Text else Flame.Dim, modifier = androidx.compose.ui.Modifier.weight(1f))
            if (swap != null) androidx.compose.material3.TextButton(onClick = swap) { androidx.compose.material3.Text("⇆", fontFamily = Mono, color = Flame.Dim) }
            androidx.compose.material3.TextButton(onClick = whole) { androidx.compose.material3.Text(if (swap != null) "▢" else "◫", fontFamily = Mono, color = Flame.Dim) }
        }
        androidx.compose.foundation.layout.Box(androidx.compose.ui.Modifier.fillMaxWidth().height(1.dp).background(Flame.Line))
    }
}

@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
fun App(
    requested: Pair<String, String>?,
    consumed: () -> Unit,
    paired: String? = null,
    pairedSeen: () -> Unit = {},
    model: HubModel = viewModel(),
) {
    val conn = model.conn
    val agents = model.agents
    // Two panes when the screen is wide enough (ADR-0076), each a slot with its workspace
    // and that workspace's screens, so a tab returns to where it was (ADR-0068) in each.
    // Folded, the focused one shows.
    val slots = remember { listOf(Slot(Workspace.Agents), Slot(Workspace.Yajna)) }
    var focus by remember { mutableStateOf(0) }
    val wide = androidx.compose.ui.platform.LocalConfiguration.current.screenWidthDp >= 600
    var full by remember { mutableStateOf(false) }
    val split = wide && !full
    var tab by object : kotlin.properties.ReadWriteProperty<Any?, Workspace> {
        override fun getValue(thisRef: Any?, property: kotlin.reflect.KProperty<*>) = slots[focus].w
        override fun setValue(thisRef: Any?, property: kotlin.reflect.KProperty<*>, value: Workspace) { slots[focus].w = value }
    }
    fun root(w: Workspace): Screen = when (w) {
        Workspace.Agents -> Screen.Home
        Workspace.Yajna -> Screen.Yajna
        Workspace.Machines -> Screen.Machines
    }
    fun screenOf(w: Workspace, i: Int = focus): Screen = slots[i].screens[w] ?: root(w)
    var screen by object : kotlin.properties.ReadWriteProperty<Any?, Screen> {
        override fun getValue(thisRef: Any?, property: kotlin.reflect.KProperty<*>) = screenOf(tab)
        override fun setValue(thisRef: Any?, property: kotlin.reflect.KProperty<*>, value: Screen) { slots[focus].screens[tab] = value }
    }
    // Settings over whatever is showing, on the tab of the workspace in front (ADR-0076).
    var settings by remember { mutableStateOf(false) }
    var settingsFrom by remember { mutableStateOf<Workspace?>(null) }
    fun openSettings(from: Workspace? = tab) {
        settingsFrom = from
        settings = true
    }
    // What is open across the workspaces, most recent first, and where a sideways swipe
    // along the bar has stepped to (ADR-0076).
    val places = remember { androidx.compose.runtime.mutableStateListOf<Place>() }
    var stepping by remember { mutableStateOf(false) }
    var switcher by remember { mutableStateOf(false) }
    var needs by remember { mutableStateOf(false) }

    /** Goes to [p] in its workspace, in the focused pane or with [other] the other one; a
     *  session is opened again if it was let go. */
    fun go(p: Place, step: Boolean = false, other: Boolean = false) {
        (p.screen as? Screen.Session)?.let { s ->
            if (agents.opened(s.nodeId, s.sessionId) == null) conn.session(s.nodeId, s.sessionId)?.let { agents.open(it) }
        }
        stepping = step
        settings = false
        val slot = slots[if (other && split) 1 - focus else focus]
        slot.w = p.w
        slot.screens[p.w] = p.screen
    }
    fun step(by: Int) {
        val list = places.filter { model.label(it) != null }
        if (list.isEmpty()) return
        val at = list.indexOfFirst { it.w == tab && it.screen.key() == screen.key() }
        val to = if (at < 0) (if (by > 0) 0 else return) else (at + by).coerceIn(0, list.size - 1)
        if (to != at) go(list[to], step = true)
    }
    fun drop(p: Place) {
        places.removeAll { it.screen.key() == p.screen.key() }
        when (val s = p.screen) {
            is Screen.Terminal -> model.machines.shell(s.shellId)?.let { model.machines.closeShell(it) }
            is Screen.Session -> agents.opened(s.nodeId, s.sessionId)?.let { agents.close(it) }
            else -> {}
        }
        for (slot in slots) if ((slot.screens[p.w] ?: root(p.w)).key() == p.screen.key()) slot.screens[p.w] = root(p.w)
    }
    // A machine's health is in Machines (ADR-0076), from wherever it is asked for.
    fun showHealth(nodeId: String) {
        settings = false
        tab = Workspace.Machines
        screen = Screen.Health(nodeId, Screen.Machines)
    }
    fun switchTo(w: Workspace) {
        // A second tap on the current tab goes to its root.
        if (w == tab && !settings) screen = root(w) else tab = w
        settings = false
    }
    // Each thing opened goes to the front of the switcher, unless a swipe stepped to it.
    LaunchedEffect(tab, screenOf(tab)) {
        val s = screenOf(tab)
        if (!s.isPlace()) return@LaunchedEffect
        if (stepping) { stepping = false; return@LaunchedEffect }
        places.removeAll { it.screen.key() == s.key() }
        places.add(0, Place(tab, s))
    }
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(Unit) { conn.messages.collect { snackbar.showSnackbar(it) } }
    LifecycleResumeEffect(Unit) {
        conn.resumed()
        onPauseOrDispose { }
    }

    // A cold start shows the boot log until the hub answers; with no hub set up, or when
    // the person leaves a failed start for Settings, the app goes on to Settings.
    var booted by remember { mutableStateOf(false) }
    // A first run goes from the boot log to Settings; once its first hub connects, Home.
    var firstRun by remember { mutableStateOf(false) }
    LaunchedEffect(firstRun, conn.link, conn.demo) {
        if (firstRun && conn.link == Link.Connected && conn.demo == null) {
            tab = Workspace.Agents
            screen = Screen.Home
            settings = false
            firstRun = false
        }
    }
    // A pairing link goes to Settings, where the hub it holds waits to be added.
    LaunchedEffect(paired) {
        val link = paired ?: return@LaunchedEffect
        if (conn.pairFrom(link)) {
            openSettings(null)
            booted = true
            firstRun = conn.hubs.isEmpty()
        } else conn.say("That link is not a saarthi pairing link")
        pairedSeen()
    }
    if (!booted) {
        BootScreen(
            model.bootLog,
            retry = { model.retryBoot() },
            settings = { openSettings(null); booted = true },
            finished = {
                if (model.bootLog.state == BootLog.State.NoHub) {
                    openSettings(null)
                    firstRun = true
                }
                booted = true
            },
        )
        return
    }
    // Once connected, and again with each hub switched to: register for notifications
    // with the phone's distributor.
    val activity = LocalActivity.current
    val connected = conn.link == Link.Connected && conn.demo == null
    LaunchedEffect(connected, conn.activeId) {
        if (connected) activity?.let {
            Push.start(it)
            // Notifications held from the hub itself, the push server the fallback (ADR-0075).
            LinkService.start(it)
        }
    }

    // A notification tap lands on its session once the lists are in; a usage alert, which
    // has none, on the node's health.
    LaunchedEffect(requested, conn.sessions.size) {
        val (node, id) = requested ?: return@LaunchedEffect
        if (id.isEmpty()) {
            showHealth(node)
            consumed()
            return@LaunchedEffect
        }
        conn.session(node, id) ?: return@LaunchedEffect
        go(Place(Workspace.Agents, Screen.Session(node, id)))
        consumed()
    }

    // A session just spawned from here opens once the watch has listed it.
    var spawned by remember { mutableStateOf<Pair<String, String>?>(null) }
    // Where back from it goes: home, or the notebooks for a new project.
    var spawnedFrom by remember { mutableStateOf<Screen>(Screen.Home) }
    LaunchedEffect(spawned, conn.sessions.size) {
        val (node, id) = spawned ?: return@LaunchedEffect
        val s = conn.session(node, id) ?: return@LaunchedEffect
        agents.open(s)
        val from = spawnedFrom
        // A notebook's new project opens in Agents, back going to the notebooks.
        go(Place(Workspace.Agents, Screen.Session(node, id, if (from == Screen.Yajna) Screen.Home else from)))
        spawned = null
        spawnedFrom = Screen.Home
    }

    // Back closes Settings, then leaves a screen for where it came from, a workspace's root
    // for Agents, and Agents for the phone; from Settings with no hub there is nowhere to go
    // back to.
    val atRoot = screen == root(tab)
    val noHub = conn.hubs.isEmpty() && conn.demo == null
    BackHandler(enabled = if (settings) !noHub else !(atRoot && tab == Workspace.Agents)) {
        if (settings) {
            settings = false
            return@BackHandler
        }
        if (atRoot) {
            switchTo(Workspace.Agents)
            return@BackHandler
        }
        screen = when (val s = screen) {
            is Screen.Health -> s.from
            is Screen.Terminal -> s.from
            is Screen.Changes -> s.from
            is Screen.Notebook -> Screen.Yajna
            is Screen.Session -> s.from
            else -> root(tab)
        }
    }

    /** Pane [i]'s screen, its workspace's own back stack moving as it is used. */
    @Composable
    fun Pane(i: Int) {
        val w = slots[i].w
        fun set(x: Screen) { slots[i].screens[w] = x }
        when (val s = screenOf(w, i)) {
            Screen.Home -> HomeScreen(
                agents, snackbar,
                openSession = {
                    agents.open(it)
                    set(Screen.Session(it.nodeId, it.id))
                },
                spawned = { node, id -> spawned = node to id },
                openHistory = { set(Screen.History) },
                openTokens = { set(Screen.Tokens) },
                openSettings = { openSettings(null) },
            )
            Screen.Machines -> MachinesScreen(
                model.machines,
                openShell = { set(Screen.Terminal(it.id, Screen.Machines)) },
                newShell = { set(Screen.Terminal(model.machines.newShell(it).id, Screen.Machines)) },
                openHealth = { set(Screen.Health(it.id, Screen.Machines)) },
            )
            Screen.Yajna -> NotebooksScreen(
                model.yajna, snackbar,
                open = { set(Screen.Notebook(it.nodeId, it.namespace, it.url)) },
                spawned = { node, id -> spawnedFrom = Screen.Yajna; spawned = node to id },
            )
            is Screen.Notebook -> NotebookScreen(
                model.yajna, snackbar, s.nodeId, s.namespace, s.url,
                back = { set(Screen.Yajna) },
            )
            Screen.History -> HistoryScreen(
                agents, snackbar,
                back = { set(root(w)) },
                openSession = { it, find ->
                    agents.open(it)
                    set(Screen.Session(it.nodeId, it.id, Screen.History, find))
                },
            )
            is Screen.Session -> if (agents.opened(s.nodeId, s.sessionId) == null) {
                // Dropped from memory while away (ADR-0076): opened again while it runs.
                LaunchedEffect(s) { conn.session(s.nodeId, s.sessionId)?.let { agents.open(it) } ?: run { set(s.from) } }
            } else SessionScreen(
                agents, snackbar, s.nodeId, s.sessionId, s.find,
                back = { set(s.from) },
                openHealth = { showHealth(s.nodeId) },
                openChanges = { set(Screen.Changes(s.nodeId, s.sessionId, s)) },
                openSession = { id -> spawnedFrom = s.from; spawned = s.nodeId to id },
            )
            is Screen.Health -> HealthScreen(model.machines, snackbar, s.nodeId, back = { set(s.from) })
            is Screen.Terminal -> model.machines.shell(s.shellId)?.let { shell ->
                TerminalScreen(model.machines, shell, back = { set(s.from) }, close = { model.machines.closeShell(shell); set(s.from) })
            } ?: LaunchedEffect(s) { set(s.from) }
            is Screen.Changes -> ChangesScreen(agents, s.nodeId, s.sessionId, back = { set(s.from) })
            Screen.Tokens -> TokensScreen(conn, snackbar, back = { set(root(w)) })
        }
    }

    // The bar is on every screen (ADR-0076), stepping aside only while the keyboard is up.
    val keyboard = androidx.compose.foundation.layout.WindowInsets.isImeVisible
    val bar = !keyboard
    androidx.compose.foundation.layout.Column(androidx.compose.ui.Modifier.fillMaxSize()) {
        androidx.compose.foundation.layout.Box(
            androidx.compose.ui.Modifier
                .weight(1f)
                .then(if (bar) androidx.compose.ui.Modifier.consumeWindowInsets(androidx.compose.foundation.layout.WindowInsets.navigationBars) else androidx.compose.ui.Modifier),
        ) {
            if (settings) SettingsScreen(
                model, snackbar,
                back = if (noHub) null else ({ settings = false }),
                tryDemo = { conn.startDemo(); tab = Workspace.Agents; screen = Screen.Home; settings = false },
                start = settingsFrom,
            ) else if (wide && full) androidx.compose.foundation.layout.Column(androidx.compose.ui.Modifier.fillMaxSize()) {
                PaneHeader(slots[focus].w, focused = true, swap = null, whole = { full = false })
                androidx.compose.foundation.layout.Box(androidx.compose.ui.Modifier.weight(1f).consumeWindowInsets(androidx.compose.foundation.layout.WindowInsets.statusBars)) { Pane(focus) }
            } else if (!split) Pane(focus) else androidx.compose.foundation.layout.Row(androidx.compose.ui.Modifier.fillMaxSize()) {
                for (i in slots.indices) {
                    if (i > 0) androidx.compose.foundation.layout.Box(androidx.compose.ui.Modifier.width(1.dp).fillMaxHeight().background(Flame.Line))
                    androidx.compose.foundation.layout.Column(
                        androidx.compose.ui.Modifier
                            .weight(1f)
                            .fillMaxHeight()
                            // A touch anywhere in a pane focuses it, the touch going on.
                            .pointerInput(i) {
                                awaitEachGesture {
                                    awaitFirstDown(requireUnconsumed = false, pass = androidx.compose.ui.input.pointer.PointerEventPass.Initial)
                                    focus = i
                                }
                            },
                    ) {
                        PaneHeader(
                            slots[i].w, focused = i == focus,
                            swap = {
                                val (a, b) = slots
                                val w = a.w; a.w = b.w; b.w = w
                                val m = a.screens.toMap(); a.screens.clear(); a.screens.putAll(b.screens); b.screens.clear(); b.screens.putAll(m)
                                focus = 1 - focus
                            },
                            whole = { focus = i; full = true },
                        )
                        androidx.compose.foundation.layout.Box(androidx.compose.ui.Modifier.weight(1f).consumeWindowInsets(androidx.compose.foundation.layout.WindowInsets.statusBars)) { Pane(i) }
                    }
                }
            }
        }
        if (bar) WorkspaceBar(
            tab,
            conn.pending.map { it.nodeId to it.sessionId }.distinct().size,
            ::switchTo,
            settings = { if (settings) settings = false else openSettings() },
            settingsOn = settings,
            answer = { needs = true },
            switcher = { switcher = true },
            step = ::step,
            // Held, a tab opens its workspace in the other pane.
            hold = { w -> if (split) slots[1 - focus].w = w },
            panes = if (split) slots.mapIndexed { i, sl -> sl.w to if (i == 0) "L" else "R" }.groupBy({ it.first }, { it.second }).mapValues { it.value.joinToString("") } else emptyMap(),
        )
    }
    if (switcher) SwitcherSheet(
        model, places, places.firstOrNull { it.w == tab && it.screen.key() == screen.key() },
        go = { switcher = false; go(it) },
        goOther = if (split) ({ switcher = false; go(it, other = true) }) else null,
        close = { drop(it) },
        dismiss = { switcher = false },
    )
    if (needs) NeedsYouSheet(
        model,
        open = { node, id -> needs = false; go(Place(Workspace.Agents, Screen.Session(node, id))) },
        dismiss = { needs = false },
    )
}
