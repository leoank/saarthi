package me.leoank.saarthi

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        take(intent)
        setContent {
            SaarthiTheme { App(requested.value, consumed = { requested.value = null }) }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        take(intent)
    }

    private fun take(intent: Intent?) {
        val node = intent?.getStringExtra("node_id") ?: return
        val session = intent.getStringExtra("session_id") ?: return
        requested.value = node to session
        intent.removeExtra("session_id")
    }
}

/** Where the person is; a small stack in place of a navigation library. A session is
 *  named by ids so the screen follows the live SessionInfo. */
sealed interface Screen {
    data object Home : Screen
    data class Session(val nodeId: String, val sessionId: String, val from: Screen = Home) : Screen
    data object History : Screen
    data object Tokens : Screen
    data class Health(val nodeId: String, val from: Screen) : Screen
}

@Composable
fun App(requested: Pair<String, String>?, consumed: () -> Unit, model: HubModel = viewModel()) {
    var screen by remember { mutableStateOf<Screen>(Screen.Home) }
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(Unit) { model.messages.collect { snackbar.showSnackbar(it) } }
    LifecycleResumeEffect(Unit) {
        model.resumed()
        onPauseOrDispose { }
    }

    if (model.link == Link.Disconnected || model.link == Link.Connecting) {
        ConnectScreen(model)
        return
    }
    // Once connected: register for notifications with the phone's distributor.
    val activity = LocalActivity.current
    LaunchedEffect(Unit) { activity?.let { Push.start(it) } }

    // A notification tap lands on its session once the lists are in; a usage alert, which
    // has none, on the node's health.
    LaunchedEffect(requested, model.sessions.size) {
        val (node, id) = requested ?: return@LaunchedEffect
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
    LaunchedEffect(spawned, model.sessions.size) {
        val (node, id) = spawned ?: return@LaunchedEffect
        val s = model.sessions.firstOrNull { it.nodeId == node && it.id == id } ?: return@LaunchedEffect
        model.open(s)
        screen = Screen.Session(node, id)
        spawned = null
    }

    BackHandler(enabled = screen != Screen.Home) {
        screen = when (val s = screen) {
            is Screen.Health -> s.from
            is Screen.Session -> { model.close(); s.from }
            else -> Screen.Home
        }
    }
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
        )
        Screen.History -> HistoryScreen(
            model, snackbar,
            back = { screen = Screen.Home },
            openSession = {
                model.open(it)
                screen = Screen.Session(it.nodeId, it.id, Screen.History)
            },
        )
        is Screen.Session -> SessionScreen(
            model, snackbar, s.nodeId, s.sessionId,
            back = { model.close(); screen = s.from },
            openHealth = { screen = Screen.Health(s.nodeId, s) },
        )
        is Screen.Health -> HealthScreen(model, snackbar, s.nodeId, back = { screen = s.from })
        Screen.Tokens -> TokensScreen(model, snackbar, back = { screen = Screen.Home })
    }
}
