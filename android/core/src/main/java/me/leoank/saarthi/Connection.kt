package me.leoank.saarthi

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.saarthi_core.Change
import uniffi.saarthi_core.ClaudeTokenInfo
import uniffi.saarthi_core.CoreException
import uniffi.saarthi_core.Demo
import uniffi.saarthi_core.Hub
import uniffi.saarthi_core.NodeInfo
import uniffi.saarthi_core.PendingInfo
import uniffi.saarthi_core.SessionInfo
import uniffi.saarthi_core.Subscription
import uniffi.saarthi_core.WatchListener
import uniffi.saarthi_core.demo as recordedDemo

/** How long a dropped link goes unmentioned while nothing waits on it. */
const val QUIET_RECONNECT_MS = 4_000L

/** Where the connection to the hub stands. */
enum class Link { Disconnected, Connecting, Connected, Reconnecting }

/** What every workspace shares (ADR-0077): the hubs set up on this phone, the connection to
 *  the active one, and what its watch lists, the machines, the sessions and the requests
 *  waiting on the owner; the demo, pairing, the Claude tokens and the notices shown. The
 *  core's listeners call from its own threads; every change is applied on the main thread,
 *  where Compose reads it. A broken watch reconnects by itself with backoff. */
class Connection(val app: Application, val scope: CoroutineScope) {
    val prefs = app.getSharedPreferences("saarthi", 0)

    private val store = HubStore(prefs)
    /** The hubs set up on this phone, and the one the app connects to. */
    val hubs = mutableStateListOf<HubProfile>().apply { addAll(store.load()) }
    var activeId by mutableStateOf(store.active(hubs))
        private set
    val active: HubProfile?
        get() = hubs.firstOrNull { it.id == activeId }

    var link by mutableStateOf(Link.Disconnected)
        private set
    /** Why the last connection attempt failed, for the settings screen. */
    var connectError by mutableStateOf<String?>(null)
        private set

    /** One-off outcomes to show briefly: a refusal's reason, a confirmation, an error. */
    private val notices = Channel<String>(Channel.BUFFERED)
    val messages = notices.receiveAsFlow()

    /** The tailnet this phone reaches the hub over, by the hub's name for it, empty for
     *  plain TCP (ADR-0076); null while not connected, or in the demo. */
    var tailnet by mutableStateOf<String?>(null)
        private set

    val nodes = mutableStateListOf<NodeInfo>()
    val sessions = mutableStateListOf<SessionInfo>()
    val pending = mutableStateListOf<PendingInfo>()

    /** The named Claude tokens the hub holds (ADR-0039), with their expiry and the usage
     *  last seen under each; empty when it holds none or is older than this app. */
    val claudeTokens = mutableStateListOf<ClaudeTokenInfo>()

    /** A reconnect that has gone on long enough to mention. */
    private var reconnectLong by mutableStateOf(false)

    /** Whether something waits on the link (a prompt on its way), set by the workspaces. */
    var waiting: () -> Boolean = { false }

    /** Whether to say the link is down: a drop is often over within a second or two, so
     *  only after [QUIET_RECONNECT_MS], or at once while something waits on the link. */
    val reconnecting: Boolean
        get() = link == Link.Reconnecting && (reconnectLong || waiting())

    private var hub: Hub? = null
    /** The live connection, for what calls the core itself (the notebooks' web view);
     *  null while not connected. */
    val connection: Hub?
        get() = hub
    private var watch: Subscription? = null
    private var reconnect: Job? = null

    /** What the workspaces do when the connection is made again (streams resumed), when
     *  it goes (what it listed dropped), and when another hub becomes active. */
    private val connected = ArrayList<suspend (Hub) -> Unit>()
    private val dropped = ArrayList<() -> Unit>()
    private val switching = ArrayList<() -> Unit>()
    fun onConnected(block: suspend (Hub) -> Unit) { connected += block }
    fun onDropped(block: () -> Unit) { dropped += block }
    fun onSwitch(block: () -> Unit) { switching += block }

    /** The active hub's name, for titles. */
    val hubName: String
        get() = if (demo != null) "Demo · add a hub in Settings" else active?.title ?: "no hub"

    /** A hub from a pairing link, waiting for the person to look at it and add it. */
    var pairing by mutableStateOf<HubProfile?>(null)

    /** Reads a `saarthi://pair` link from `saarthi ctl pair` into [pairing]; false when it
     *  is not one. */
    fun pairFrom(link: String): Boolean {
        val uri = runCatching { android.net.Uri.parse(link.trim()) }.getOrNull() ?: return false
        if (uri.scheme != "saarthi" || uri.host != "pair") return false
        val address = uri.getQueryParameter("address").orEmpty()
        val token = uri.getQueryParameter("token").orEmpty()
        if (address.isBlank() || token.isBlank()) return false
        pairing = HubProfile(
            name = uri.getQueryParameter("name").orEmpty(),
            address = address,
            token = token,
            tailnetKey = uri.getQueryParameter("tailnet_key").orEmpty(),
        )
        return true
    }

    /** The demo (phase PHS-20261117-f22kxe item 13): a recorded session to look around in,
     *  with no hub; null outside it. */
    var demo by mutableStateOf<Demo?>(null)
        private set

    fun startDemo() {
        val recording = runCatching { app.assets.open("demo.json").bufferedReader().use { it.readText() } }
        val d = recording.mapCatching { recordedDemo(it, System.currentTimeMillis()) }.getOrElse { say("The demo did not load: ${it.message}"); return }
        disconnect()
        demo = d
        nodes.replaceAll(listOf(d.node))
        sessions.replaceAll(listOf(d.session))
        pending.clear()
        link = Link.Connected
    }

    private fun leaveDemo() {
        if (demo == null) return
        demo = null
        nodes.clear()
        sessions.clear()
        dropped.forEach { it() }
        link = Link.Disconnected
    }

    /** Adds or replaces a hub. The first one becomes active; a change to the active one
     *  reconnects with it. */
    fun saveHub(saved: HubProfile) {
        val at = hubs.indexOfFirst { it.id == saved.id }
        // A new key is for a new login: with its state, the old identity would never use it.
        val was = hubs.getOrNull(at)
        val h = if (was != null && was.viaTailnet && saved.viaTailnet && was.tailnetKey != saved.tailnetKey) {
            was.tailnetDir(app).deleteRecursively()
            saved.freshTailnet()
        } else saved
        if (at >= 0) hubs[at] = h else hubs.add(h)
        if (activeId == null || activeId == h.id) activate(h.id) else store.save(hubs, activeId)
    }

    /** Forgets this phone's tailnet identity for a hub: the next connect joins as a new
     *  device with the hub's key. The old device stays in the tailnet until removed there. */
    fun forgetTailnet(id: String) {
        val h = hubs.firstOrNull { it.id == id } ?: return
        h.tailnetDir(app).deleteRecursively()
        val fresh = h.freshTailnet()
        hubs[hubs.indexOf(h)] = fresh
        if (activeId == id) activate(id) else store.save(hubs, activeId)
        say("A new tailnet identity for ${fresh.title}: it joins with the hub's key on the next connect")
    }

    fun removeHub(id: String) {
        hubs.firstOrNull { it.id == id }?.tailnetDirs(app)?.forEach { it.deleteRecursively() }
        hubs.removeAll { it.id == id }
        if (activeId == id) activate(hubs.firstOrNull()?.id) else store.save(hubs, activeId)
    }

    /** Makes a hub the one the app connects to, and connects to it. */
    fun activate(id: String?) {
        switching.forEach { it() }
        leaveDemo()
        activeId = id
        store.save(hubs, id)
        disconnect()
        connectError = null
        if (id != null) connect()
    }

    fun connect() {
        if (active == null) return
        reconnect?.cancel()
        scope.launch {
            link = Link.Connecting
            connectError = null
            try {
                establish()
            } catch (e: Exception) {
                connectError = describe(e)
                link = Link.Disconnected
            }
        }
    }

    /** [connect], awaited: null once connected, else why not. For the boot log. */
    suspend fun connectNow(): String? {
        reconnect?.cancel()
        link = Link.Connecting
        connectError = null
        return try {
            establish()
            null
        } catch (e: Exception) {
            describe(e).also {
                connectError = it
                link = Link.Disconnected
            }
        }
    }

    fun disconnect() {
        reconnect?.cancel()
        watch?.cancel(); watch = null
        dropped.forEach { it() }
        hub?.close(); hub = null
        link = Link.Disconnected
        tailnet = null
        nodes.clear(); sessions.clear(); pending.clear()
    }

    /** Opens the connection, lists everything, and starts the watch and what the
     *  workspaces stream. The old hub object goes first: a command or refresh while the new
     *  connection is being made must see "not connected", not a destroyed object. */
    suspend fun establish() {
        watch?.cancel()
        hub?.let { old -> hub = null; old.close() }
        val h = active!!.connect(app)
        hub = h
        reload(h)
        watch = h.watch(object : WatchListener {
            override fun onChange(change: Change) = onMain { apply(change) }
            override fun onError(message: String) = onMain { lost(message) }
        })
        connected.forEach { it(h) }
        link = Link.Connected
        reconnectLong = false
    }

    /** The watch ended: try again until it holds, with a short backoff. The core's
     *  keepalive pings end a dead link within half a minute, so this is also how a
     *  phone's silent drop (sleep, a network change) is recovered. */
    private fun lost(message: String) {
        if (link == Link.Disconnected || reconnect?.isActive == true) return
        link = Link.Reconnecting
        scope.launch {
            delay(QUIET_RECONNECT_MS)
            if (link == Link.Reconnecting) reconnectLong = true
        }
        reconnect = scope.launch {
            var wait = 1_000L
            while (isActive) {
                try {
                    establish()
                    return@launch
                } catch (e: Exception) {
                    connectError = describe(e)
                }
                delay(wait)
                wait = (wait * 2).coerceAtMost(10_000)
            }
        }
    }

    /** Called when the app comes to the front. A reconnect in progress retries at once
     *  instead of waiting out its backoff; a connected link is checked with a list, and a
     *  failure there starts the reconnect without waiting for the keepalive. */
    fun resumed() {
        when (link) {
            Link.Reconnecting -> {
                reconnect?.cancel()
                reconnect = null
                lost("resumed")
            }
            Link.Connected -> scope.launch {
                val h = hub ?: return@launch
                try {
                    reload(h)
                } catch (e: CoreException.Connection) {
                    lost(e.reason)
                } catch (e: Exception) {
                    say(describe(e))
                }
            }
            else -> {}
        }
    }

    fun refresh() = withHub { reload(it) }

    private suspend fun reload(h: Hub) {
        val n = h.nodes()
        val s = h.sessions(false)
        val p = h.pending()
        val t = tokensOf(h)
        claudeTokens.replaceAll(t)
        nodes.replaceAll(n)
        sessions.replaceAll(s)
        pending.replaceAll(p)
        tailnet = h.tailnet()
    }

    private fun apply(change: Change) {
        when (change) {
            is Change.Node -> nodes.upsert(change.node) { it.id == change.node.id }
            is Change.Session -> {
                val s = change.session
                if (s.endedMs != null) sessions.removeAll { it.id == s.id && it.nodeId == s.nodeId }
                else sessions.upsert(s) { it.id == s.id && it.nodeId == s.nodeId }
            }
            is Change.PendingOpened -> pending.upsert(change.pending) { it.id == change.pending.id }
            is Change.PendingClosed -> pending.removeAll { it.id == change.pendingId }
        }
    }

    fun node(id: String): NodeInfo? = nodes.firstOrNull { it.id == id }

    fun session(nodeId: String, id: String): SessionInfo? = sessions.firstOrNull { it.nodeId == nodeId && it.id == id }

    /** Starts a headless session on a node (ADR-0034), idle when [prompt] is blank and in a
     *  directory made first when [create] asks, in claude's permission [mode] (empty for the
     *  user's default), under the node's launch [profile] (empty for its own user); [opened]
     *  gets its id once the node has started it. A notebook starts one too, so it is here. */
    /** [claudeToken]: null for the node's default, empty for its own login (ADR-0039). */
    fun spawn(nodeId: String, cwd: String, prompt: String, model: String, create: Boolean, mode: String, profile: String, claudeToken: String?, opened: (String) -> Unit) =
        withHub { opened(it.spawn(nodeId, cwd.trim(), prompt.trim(), model, create, mode, profile, claudeToken)) }

    private suspend fun tokensOf(h: Hub): List<ClaudeTokenInfo> =
        try { h.claudeTokens() } catch (e: CoreException.Hub) { emptyList() }

    fun loadClaudeTokens() = withHub { claudeTokens.replaceAll(tokensOf(it)) }

    /** Sets a token's expiry by hand; 0 goes back to a year after the hub first saw it. */
    fun setClaudeTokenExpiry(name: String, expiresMs: Long) = withHub {
        claudeTokens.replaceAll(it.setClaudeTokenExpiry(name, expiresMs))
    }

    /** The token a node's spawned sessions run under when none is picked; empty for its own
     *  login. The node's change comes back through the watch. */
    fun setDefaultClaudeToken(nodeId: String, name: String) = withHub {
        it.setDefaultClaudeToken(nodeId, name)
        say(if (name.isEmpty()) "New sessions use the node's own login" else "New sessions use the $name token")
    }

    fun say(text: String) {
        notices.trySend(text)
    }

    /** A command's outcome: the node's note when it has one, else a short confirmation. */
    fun done(note: String, fallback: String) = say(note.ifEmpty { fallback })

    /** A command's outcome shown only when the node said something about it. */
    fun noted(note: String) {
        if (note.isNotEmpty()) say(note)
    }

    fun withHub(block: suspend (Hub) -> Unit) {
        scope.launch {
            val h = hub
            if (h == null && demo != null) {
                say("This is the demo: add a hub in Settings to act on real sessions")
                return@launch
            }
            if (h == null) {
                if (link == Link.Reconnecting) say("Reconnecting to the hub, try again in a moment")
                else if (link == Link.Connected) say("Not connected")
                return@launch
            }
            try {
                block(h)
            } catch (e: Exception) {
                say(describe(e))
            }
        }
    }

    fun describe(e: Exception): String = when (e) {
        is CoreException.Connection -> "Cannot reach the hub: ${e.reason}"
        is CoreException.Hub -> e.reason
        is CoreException.Refused -> "The node refused: ${e.reason}"
        else -> e.message ?: e.toString()
    }

    fun onMain(block: () -> Unit) {
        scope.launch(Dispatchers.Main) { block() }
    }

    fun close() {
        reconnect?.cancel()
        watch?.cancel()
        hub?.close()
    }
}

fun <T> MutableList<T>.replaceAll(items: List<T>) {
    clear()
    addAll(items)
}

fun <T> MutableList<T>.upsert(item: T, same: (T) -> Boolean) {
    val i = indexOfFirst(same)
    if (i >= 0) this[i] = item else add(item)
}
