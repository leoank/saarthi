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
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBars
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
    /** The Terminals workspace's root: the machines that offer a shell. */
    data object Terminals : Screen
    /** [find]: words to search the session for as it opens, from a search's hit. */
    data class Session(val nodeId: String, val sessionId: String, val from: Screen = Home, val find: String? = null) : Screen
    data object History : Screen
    data object Tokens : Screen
    data class Health(val nodeId: String, val from: Screen) : Screen
    /** An open shell (ADR-0054), by its id in the model. */
    data class Terminal(val shellId: Int, val from: Screen) : Screen
    data class Changes(val nodeId: String, val sessionId: String, val from: Screen) : Screen
    data object Settings : Screen
    data object Notebooks : Screen
    data class Notebook(val nodeId: String, val namespace: String, val url: String) : Screen
}

@Composable
fun App(
    requested: Pair<String, String>?,
    consumed: () -> Unit,
    paired: String? = null,
    pairedSeen: () -> Unit = {},
    model: HubModel = viewModel(),
) {
    // Each workspace keeps its own screen, so its tab returns to where it was (ADR-0068).
    var tab by remember { mutableStateOf(Workspace.Agents) }
    val screens = remember { androidx.compose.runtime.mutableStateMapOf<Workspace, Screen>() }
    fun root(w: Workspace): Screen = when (w) {
        Workspace.Agents -> Screen.Home
        Workspace.Notebooks -> Screen.Notebooks
        Workspace.Terminals -> Screen.Terminals
    }
    var screen by object : kotlin.properties.ReadWriteProperty<Any?, Screen> {
        override fun getValue(thisRef: Any?, property: kotlin.reflect.KProperty<*>) = screens[tab] ?: root(tab)
        override fun setValue(thisRef: Any?, property: kotlin.reflect.KProperty<*>, value: Screen) { screens[tab] = value }
    }
    // A tab coming back to a session reopens it: the model holds one open session.
    fun switchTo(w: Workspace) {
        if (w == tab) {
            // A second tap on the current tab goes to its root.
            if (screen is Screen.Session) model.close()
            screen = root(w)
            return
        }
        tab = w
        (screen as? Screen.Session)?.let { s ->
            model.sessions.firstOrNull { it.nodeId == s.nodeId && it.id == s.sessionId }?.let { model.open(it) }
        }
    }
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(Unit) { model.messages.collect { snackbar.showSnackbar(it) } }
    LifecycleResumeEffect(Unit) {
        model.resumed()
        onPauseOrDispose { }
    }

    // A cold start shows the boot log until the hub answers; with no hub set up, or when
    // the person leaves a failed start for Settings, the app goes on to Settings.
    var booted by remember { mutableStateOf(false) }
    // A first run goes from the boot log to Settings; once its first hub connects, Home.
    var firstRun by remember { mutableStateOf(false) }
    LaunchedEffect(firstRun, model.link, model.demo) {
        if (firstRun && model.link == Link.Connected && model.demo == null) {
            tab = Workspace.Agents
            screen = Screen.Home
            firstRun = false
        }
    }
    // A pairing link goes to Settings, where the hub it holds waits to be added.
    LaunchedEffect(paired) {
        val link = paired ?: return@LaunchedEffect
        if (model.pairFrom(link)) {
            screen = Screen.Settings
            booted = true
            firstRun = model.hubs.isEmpty()
        } else model.say("That link is not a saarthi pairing link")
        pairedSeen()
    }
    if (!booted) {
        BootScreen(
            model.bootLog,
            retry = { model.retryBoot() },
            settings = { screen = Screen.Settings; booted = true },
            finished = {
                if (model.bootLog.state == BootLog.State.NoHub) {
                    screen = Screen.Settings
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
    val connected = model.link == Link.Connected && model.demo == null
    LaunchedEffect(connected, model.activeId) { if (connected) activity?.let { Push.start(it) } }

    // A notification tap lands on its session once the lists are in; a usage alert, which
    // has none, on the node's health.
    LaunchedEffect(requested, model.sessions.size) {
        val (node, id) = requested ?: return@LaunchedEffect
        tab = Workspace.Agents
        if (id.isEmpty()) {
            screen = Screen.Health(node, Screen.Home)
            consumed()
            return@LaunchedEffect
        }
        val s = model.sessions.firstOrNull { it.nodeId == node && it.id == id } ?: return@LaunchedEffect
        model.open(s)
        screen = Screen.Session(node, id)
        consumed()
    }

    // A session just spawned from here opens once the watch has listed it.
    var spawned by remember { mutableStateOf<Pair<String, String>?>(null) }
    // Where back from it goes: home, or the notebooks for a new project.
    var spawnedFrom by remember { mutableStateOf<Screen>(Screen.Home) }
    LaunchedEffect(spawned, model.sessions.size) {
        val (node, id) = spawned ?: return@LaunchedEffect
        val s = model.sessions.firstOrNull { it.nodeId == node && it.id == id } ?: return@LaunchedEffect
        model.open(s)
        screen = Screen.Session(node, id, spawnedFrom)
        spawned = null
        spawnedFrom = Screen.Home
    }

    // From Settings with no hub there is nowhere to go back to: back leaves the app.
    // Back leaves a screen for its workspace's root, a workspace's root for Agents, and
    // Agents for the phone; from Settings with no hub there is nowhere to go back to.
    val atRoot = screen == root(tab)
    BackHandler(enabled = !(atRoot && tab == Workspace.Agents) && !(screen == Screen.Settings && model.hubs.isEmpty())) {
        if (atRoot) {
            switchTo(Workspace.Agents)
            return@BackHandler
        }
        screen = when (val s = screen) {
            is Screen.Health -> s.from
            is Screen.Terminal -> s.from
            is Screen.Changes -> s.from
            is Screen.Notebook -> Screen.Notebooks
            is Screen.Session -> { model.close(); s.from }
            else -> root(tab)
        }
    }
    // The bar shows on a workspace's root; inside a session, a notebook, a terminal and
    // the rest the screen has the whole height.
    androidx.compose.foundation.layout.Column(androidx.compose.ui.Modifier.fillMaxSize()) {
    androidx.compose.foundation.layout.Box(
        androidx.compose.ui.Modifier
            .weight(1f)
            .then(if (atRoot) androidx.compose.ui.Modifier.consumeWindowInsets(androidx.compose.foundation.layout.WindowInsets.navigationBars) else androidx.compose.ui.Modifier),
    ) {
    when (val s = screen) {
        Screen.Home -> HomeScreen(
            model, snackbar,
            openSession = {
                model.open(it)
                screen = Screen.Session(it.nodeId, it.id)
            },
            openHealth = { screen = Screen.Health(it.id, Screen.Home) },
            spawned = { node, id -> spawned = node to id },
            openHistory = { screen = Screen.History },
            openTokens = { screen = Screen.Tokens },
            openSettings = { screen = Screen.Settings },
        )
        Screen.Terminals -> TerminalsScreen(
            model,
            openShell = { screen = Screen.Terminal(it.id, Screen.Terminals) },
            newShell = { screen = Screen.Terminal(model.newShell(it).id, Screen.Terminals) },
        )
        Screen.Notebooks -> NotebooksScreen(
            model, snackbar,
            open = { screen = Screen.Notebook(it.nodeId, it.namespace, it.url) },
            spawned = { node, id -> spawnedFrom = Screen.Notebooks; spawned = node to id },
        )
        is Screen.Notebook -> NotebookScreen(
            model, snackbar, s.nodeId, s.namespace, s.url,
            back = { screen = Screen.Notebooks },
        )
        Screen.History -> HistoryScreen(
            model, snackbar,
            back = { screen = root(tab) },
            openSession = { it, find ->
                model.open(it)
                screen = Screen.Session(it.nodeId, it.id, Screen.History, find)
            },
        )
        is Screen.Session -> SessionScreen(
            model, snackbar, s.nodeId, s.sessionId, s.find,
            back = { model.close(); screen = s.from },
            openHealth = { screen = Screen.Health(s.nodeId, s) },
            openChanges = { screen = Screen.Changes(s.nodeId, s.sessionId, s) },
            openSession = { id -> spawnedFrom = s.from; spawned = s.nodeId to id },
        )
        is Screen.Health -> HealthScreen(model, snackbar, s.nodeId, back = { screen = s.from })
        is Screen.Terminal -> model.shell(s.shellId)?.let { shell ->
            TerminalScreen(model, shell, back = { screen = s.from }, close = { model.closeShell(shell); screen = s.from })
        } ?: LaunchedEffect(s) { screen = s.from }
        is Screen.Changes -> ChangesScreen(model, s.nodeId, s.sessionId, back = { screen = s.from })
        Screen.Tokens -> TokensScreen(model, snackbar, back = { screen = root(tab) })
        Screen.Settings -> SettingsScreen(
            model, snackbar,
            back = if (model.hubs.isEmpty() && model.demo == null) null else ({ screen = root(tab) }),
            tryDemo = { model.startDemo(); tab = Workspace.Agents; screen = Screen.Home },
        )
    }
    }
    if (atRoot) WorkspaceBar(tab, model.pending.map { it.nodeId to it.sessionId }.distinct().size, ::switchTo)
    }
}
