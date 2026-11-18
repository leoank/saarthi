package me.leoank.saarthi

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import androidx.compose.runtime.snapshotFlow
import kotlinx.coroutines.flow.receiveAsFlow
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.saarthi_core.Change
import uniffi.saarthi_core.Demo
import uniffi.saarthi_core.demo as recordedDemo
import uniffi.saarthi_core.ContextUsage
import uniffi.saarthi_core.ClaudeTokenInfo
import uniffi.saarthi_core.CoreException
import uniffi.saarthi_core.HealthPoint
import uniffi.saarthi_core.HistoryItem
import uniffi.saarthi_core.Hub
import uniffi.saarthi_core.Item
import uniffi.saarthi_core.ModelTotals
import uniffi.saarthi_core.PromptSent
import uniffi.saarthi_core.QueuedPrompt
import uniffi.saarthi_core.SearchHit
import uniffi.saarthi_core.NodeInfo
import uniffi.saarthi_core.NotebookInfo
import uniffi.saarthi_core.PendingInfo
import uniffi.saarthi_core.Role
import uniffi.saarthi_core.SessionInfo
import uniffi.saarthi_core.SessionListener
import uniffi.saarthi_core.Subscription
import uniffi.saarthi_core.WatchListener

/** One session's conversation as seen so far, with the usage totals and the last seq. */
class SessionCache(val items: ArrayList<Item>, val usage: ArrayList<ModelTotals>, var lastSeq: ULong) {
    /** The cache as JSON; only the last [KEPT] items go to disk. */
    fun snapshot(): String {
        val o = JSONObject()
        o.put("lastSeq", lastSeq.toString())
        o.put("items", JSONArray().apply {
            for (i in items.takeLast(KEPT)) put(JSONObject().apply {
                put("seq", i.seq.toString()); put("timeMs", i.timeMs); put("role", i.role.name)
                put("tool", i.tool); put("text", i.text); put("detail", i.detail ?: JSONObject.NULL); put("error", i.error)
                put("diff", i.diff ?: JSONObject.NULL)
            })
        })
        o.put("usage", JSONArray().apply {
            for (u in usage) put(JSONObject().apply {
                put("model", u.model); put("messages", u.messages.toString()); put("input", u.inputTokens.toString())
                put("output", u.outputTokens.toString()); put("cacheRead", u.cacheReadTokens.toString())
                put("cacheWrite", u.cacheWriteTokens.toString()); put("cost", u.costUsd)
            })
        })
        return o.toString()
    }

    companion object {
        const val KEPT = 2000

        fun load(file: File): SessionCache {
            val empty = SessionCache(ArrayList(), ArrayList(), 0uL)
            if (!file.exists()) return empty
            return runCatching {
                val o = JSONObject(file.readText())
                val items = ArrayList<Item>()
                val arr = o.getJSONArray("items")
                for (k in 0 until arr.length()) {
                    val i = arr.getJSONObject(k)
                    items.add(Item(
                        seq = i.getString("seq").toULong(), timeMs = i.getLong("timeMs"), role = Role.valueOf(i.getString("role")),
                        tool = i.optString("tool"), text = i.optString("text"),
                        detail = if (i.isNull("detail")) null else i.getString("detail"), error = i.optBoolean("error"),
                        diff = if (i.isNull("diff")) null else i.getString("diff"),
                    ))
                }
                val usage = ArrayList<ModelTotals>()
                val us = o.getJSONArray("usage")
                for (k in 0 until us.length()) {
                    val u = us.getJSONObject(k)
                    usage.add(ModelTotals(
                        model = u.getString("model"), messages = u.getString("messages").toULong(),
                        inputTokens = u.getString("input").toULong(), outputTokens = u.getString("output").toULong(),
                        cacheReadTokens = u.getString("cacheRead").toULong(), cacheWriteTokens = u.getString("cacheWrite").toULong(),
                        costUsd = u.getDouble("cost"),
                    ))
                }
                SessionCache(items, usage, o.getString("lastSeq").toULong())
            }.getOrDefault(empty)
        }
    }
}

/** A prompt the hub took from this phone, until the transcript echoes it. */
data class Sent(
    val text: String,
    val timeMs: Long,
    val note: String? = null,
)

/** How many transcript lines a session opens with, and each older page brings. */
private const val TAIL = 300u

/** How long a prompt waits in the bar for the hub to take it, the link's return included. */
private const val SEND_WAIT_MS = 45_000L

/** How long a dropped link goes unmentioned while nothing waits on it. */
private const val QUIET_RECONNECT_MS = 4_000L

/** Where the connection to the hub stands. */
enum class Link { Disconnected, Connecting, Connected, Reconnecting }

/** The app's whole state: the hub connection from the core, and what it lists and streams.
 *  The core's listeners call from its own threads; every change is applied on the main
 *  thread, where Compose reads it. A broken watch reconnects by itself with backoff, and
 *  an open session's stream resumes after the last item seen. */
class HubModel(app: Application) : AndroidViewModel(app) {
    private val prefs = app.getSharedPreferences("saarthi", 0)

    private val store = HubStore(prefs)
    /** The hubs set up on this phone, and the one the app connects to. */
    val hubs = mutableStateListOf<HubProfile>().apply { addAll(store.load()) }
    var activeId by mutableStateOf(store.active(hubs))
        private set
    val active: HubProfile?
        get() = hubs.firstOrNull { it.id == activeId }
    /** Monospace text wraps instead of scrolling sideways. */
    var wrap by mutableStateOf(prefs.getBoolean("wrap", false))
        private set

    fun toggleWrap() {
        wrap = !wrap
        prefs.edit().putBoolean("wrap", wrap).apply()
    }

    /** What Home lists: every machine (empty), one machine (`node:<id>`) or one repository
     *  (`repo:<name>`), remembered across launches (phase PHS-20261117-f22kxe item 4). */
    var scope by mutableStateOf(prefs.getString("scope", "")!!)
        private set

    fun changeScope(to: String) {
        scope = to
        prefs.edit().putString("scope", to).apply()
    }

    fun inScope(nodeId: String, s: SessionInfo?): Boolean = when {
        scope.startsWith("node:") -> nodeId == scope.removePrefix("node:")
        scope.startsWith("repo:") -> s?.repo() == scope.removePrefix("repo:")
        else -> true
    }

    /** Finished turns open with every step shown, instead of folded under their header. */
    var openTurns by mutableStateOf(prefs.getBoolean("open_turns", false))
        private set

    fun toggleOpenTurns() {
        openTurns = !openTurns
        prefs.edit().putBoolean("open_turns", openTurns).apply()
    }

    /** Turns the person opened or folded by hand, by session and the turn's first item,
     *  while the app runs; the rest follow [openTurns]. */
    private val turnsOpened = mutableStateMapOf<String, Boolean>()

    fun turnOpen(s: SessionInfo, turn: ULong): Boolean? = turnsOpened["${s.nodeId}/${s.id}/$turn"]

    fun setTurnOpen(s: SessionInfo, turn: ULong, open: Boolean) {
        turnsOpened["${s.nodeId}/${s.id}/$turn"] = open
    }

    /** Nodes whose live sessions are folded away on the home screen, by node id, remembered
     *  across launches. */
    var collapsed by mutableStateOf(prefs.getStringSet("collapsed", emptySet())!!.toSet())
        private set

    /** Unsent drafts by node and session, kept across screens and launches. */
    private val drafts: MutableMap<String, String> = runCatching {
        val o = JSONObject(prefs.getString("drafts", "{}")!!)
        o.keys().asSequence().associateWith { o.getString(it) }.toMutableMap()
    }.getOrDefault(mutableMapOf())

    fun draft(nodeId: String, sessionId: String): String = drafts["$nodeId/$sessionId"].orEmpty()

    fun setDraft(nodeId: String, sessionId: String, text: String) {
        val key = "$nodeId/$sessionId"
        if (drafts[key].orEmpty() == text) return
        if (text.isEmpty()) drafts.remove(key) else drafts[key] = text
        prefs.edit().putString("drafts", JSONObject(drafts.toMap()).toString()).apply()
    }

    fun toggleCollapsed(nodeId: String) {
        collapsed = if (nodeId in collapsed) collapsed - nodeId else collapsed + nodeId
        prefs.edit().putStringSet("collapsed", collapsed).apply()
    }
    var link by mutableStateOf(Link.Disconnected)
        private set
    /** Why the last connection attempt failed, for the settings screen. */
    var connectError by mutableStateOf<String?>(null)
        private set

    /** One-off outcomes to show briefly: a refusal's reason, a confirmation, an error. */
    private val notices = Channel<String>(Channel.BUFFERED)
    val messages = notices.receiveAsFlow()

    val nodes = mutableStateListOf<NodeInfo>()
    val sessions = mutableStateListOf<SessionInfo>()
    val pending = mutableStateListOf<PendingInfo>()

    /** The session whose conversation is streamed, and the conversation itself. */
    var openSession by mutableStateOf<SessionInfo?>(null)
        private set
    val conversation = mutableStateListOf<Item>()
    /** What was seen of each session, kept in memory while the app lives and on disk
     *  between runs, so opening a session shows it at once and the stream resumes after
     *  the last item instead of replaying everything (phase 1.5 item 14). */
    private val caches = HashMap<String, SessionCache>()
    private val cacheDir = File(app.filesDir, "sessions").apply { mkdirs() }
    /** Prompts sent from here that the transcript has not echoed yet, newest last. */
    val sending = mutableStateListOf<Sent>()

    /** A prompt on its way to the hub, by "node/session": it stays in the bar, and the bar
     *  takes no other, until the hub has it (phase PHS-20261117-f22kxe item 6). */
    val outgoing = mutableStateMapOf<String, String>()

    /** Why a session's last prompt was not taken, by "node/session", shown over its bar. */
    val notSent = mutableStateMapOf<String, String>()

    /** A reconnect that has gone on long enough to mention. */
    private var reconnectLong by mutableStateOf(false)

    /** Whether to say the link is down: a drop is often over within a second or two, so
     *  only after [QUIET_RECONNECT_MS], or at once while a prompt waits on the link. */
    val reconnecting: Boolean
        get() = link == Link.Reconnecting && (reconnectLong || outgoing.isNotEmpty())
    /** Texts the transcript echoed after being sent from here: shown with two ticks. */
    val delivered = mutableStateListOf<String>()
    /** The open session's token totals by model, from the daemon's count. */
    val usage = mutableStateListOf<ModelTotals>()
    /** How full each spawned session's context was after its last turn, while the app runs. */
    private val contexts = mutableStateMapOf<String, ContextUsage>()
    fun contextOf(s: SessionInfo): ContextUsage? = contexts[key(s)]

    /** The prompts the node holds for each session until it is next idle, as it last said. */
    private val queues = mutableStateMapOf<String, List<QueuedPrompt>>()
    fun queueOf(s: SessionInfo): List<QueuedPrompt> = queues[key(s)].orEmpty()
    private var lastSeq = 0uL

    val health = mutableStateListOf<HealthPoint>()
    var healthLoading by mutableStateOf(false)

    /** The named Claude tokens the hub holds (ADR-0039), with their expiry and the usage
     *  last seen under each; empty when it holds none or is older than this app. */
    val claudeTokens = mutableStateListOf<ClaudeTokenInfo>()

    /** Sessions newest first as the history screen pages them in (ADR-0036). */
    val history = mutableStateListOf<HistoryItem>()
    var historyLoading by mutableStateOf(false)
    /** Whether the hub may have older sessions than the ones loaded. */
    var historyMore by mutableStateOf(true)
        private set
    /** Why the last page of history did not load; null once one does. */
    var historyError by mutableStateOf<String?>(null)
        private set

    /** What the last search found (ADR-0063), for the query in [searched]. */
    val searchHits = mutableStateListOf<SearchHit>()
    var searched by mutableStateOf("")
        private set
    var searching by mutableStateOf(false)
        private set
    private var searchJob: Job? = null

    /** Searches every session's titles and transcripts; a newer query replaces an older one. */
    fun search(query: String, nodeId: String) {
        searchJob?.cancel()
        if (query.isBlank()) {
            searchHits.clear()
            searched = ""
            return
        }
        searchJob = viewModelScope.launch {
            val h = hub ?: return@launch
            searching = true
            try {
                val hits = h.search(query, nodeId)
                searchHits.clear()
                searchHits.addAll(hits)
                searched = query
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                say(describe(e))
            } finally {
                searching = false
            }
        }
    }

    private var hub: Hub? = null
    /** The live connection, for the notebook's web view, which calls the core itself;
     *  null while not connected. */
    internal val connection: Hub?
        get() = hub
    private var watch: Subscription? = null
    private var stream: Subscription? = null
    private var reconnect: Job? = null

    /** The boot log shown while the app starts (see Boot.kt). */
    val bootLog = BootLog()

    init {
        viewModelScope.launch { bootLog.boot(this@HubModel, app) }
    }

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
        val recording = runCatching { getApplication<Application>().assets.open("demo.json").bufferedReader().use { it.readText() } }
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
        conversation.clear()
        link = Link.Disconnected
    }

    /** Adds or replaces a hub. The first one becomes active; a change to the active one
     *  reconnects with it. */
    fun saveHub(h: HubProfile) {
        val at = hubs.indexOfFirst { it.id == h.id }
        if (at >= 0) hubs[at] = h else hubs.add(h)
        if (activeId == null || activeId == h.id) activate(h.id) else store.save(hubs, activeId)
    }

    fun removeHub(id: String) {
        hubs.firstOrNull { it.id == id }?.tailnetDir(getApplication())?.deleteRecursively()
        hubs.removeAll { it.id == id }
        if (activeId == id) activate(hubs.firstOrNull()?.id) else store.save(hubs, activeId)
    }

    /** Makes a hub the one the app connects to, and connects to it. */
    fun activate(id: String?) {
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
        viewModelScope.launch {
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

    /** After a failed start, the boot log's Retry. */
    fun retryBoot() {
        viewModelScope.launch { bootLog.retry(this@HubModel) }
    }

    /** [connect], awaited: null once connected, else why not. For the boot log. */
    internal suspend fun connectNow(): String? {
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
        stream?.cancel(); stream = null
        hub?.close(); hub = null
        link = Link.Disconnected
        nodes.clear(); sessions.clear(); pending.clear()
        notebooks.clear(); notebookErrors.clear()
        openSession = null
        conversation.clear()
    }

    /** Opens the connection, lists everything, and starts the watch and any open stream.
     *  The old hub object goes first: a command or refresh while the new connection is
     *  being made must see "not connected", not a destroyed object. */
    internal suspend fun establish() {
        watch?.cancel()
        stream?.cancel()
        hub?.let { old -> hub = null; old.close() }
        val h = active!!.connect(getApplication())
        hub = h
        reload(h)
        watch = h.watch(object : WatchListener {
            override fun onChange(change: Change) = onMain { apply(change) }
            override fun onError(message: String) = onMain { lost(message) }
        })
        openSession?.let { startStream(h, it, lastSeq) }
        link = Link.Connected
        reconnectLong = false
    }

    /** The watch ended: try again until it holds, with a short backoff. The core's
     *  keepalive pings end a dead link within half a minute, so this is also how a
     *  phone's silent drop (sleep, a network change) is recovered. */
    private fun lost(message: String) {
        if (link == Link.Disconnected || reconnect?.isActive == true) return
        link = Link.Reconnecting
        viewModelScope.launch {
            delay(QUIET_RECONNECT_MS)
            if (link == Link.Reconnecting) reconnectLong = true
        }
        reconnect = viewModelScope.launch {
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
            Link.Connected -> viewModelScope.launch {
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
        openSession?.let { open -> sessions.firstOrNull { it.id == open.id && it.nodeId == open.nodeId }?.let { openSession = it } }
    }

    private fun apply(change: Change) {
        when (change) {
            is Change.Node -> nodes.upsert(change.node) { it.id == change.node.id }
            is Change.Session -> {
                val s = change.session
                if (s.endedMs != null) sessions.removeAll { it.id == s.id && it.nodeId == s.nodeId }
                else sessions.upsert(s) { it.id == s.id && it.nodeId == s.nodeId }
                if (openSession?.id == s.id && openSession?.nodeId == s.nodeId) openSession = s
            }
            is Change.PendingOpened -> pending.upsert(change.pending) { it.id == change.pending.id }
            is Change.PendingClosed -> pending.removeAll { it.id == change.pendingId }
        }
    }

    fun pendingFor(s: SessionInfo): List<PendingInfo> = pending.filter { it.sessionId == s.id && it.nodeId == s.nodeId }

    fun node(id: String): NodeInfo? = nodes.firstOrNull { it.id == id }

    // The card leaving is the confirmation; only the node's note, if any, is worth a toast.
    fun allow(p: PendingInfo) = withHub { noted(it.allow(p.nodeId, p.id)) }
    fun deny(p: PendingInfo, message: String) = withHub { noted(it.deny(p.nodeId, p.id, message)) }
    fun answer(p: PendingInfo, answers: Map<String, List<String>>) =
        withHub { noted(it.answer(p.nodeId, p.id, answers)) }

    /** [sent]: the hub took it, so the bar may let it go. */
    /** While Claude works the node holds the prompt until the turn ends, in the session's
     *  queue (phase PHS-20261117-f22kxe item 7); an older node delivers it at once. */
    fun prompt(s: SessionInfo, text: String, sent: () -> Unit) =
        send(s, text, sent) { it.prompt(s.nodeId, s.id, text, s.busy) }
    fun interrupt(s: SessionInfo, then: String?, sent: () -> Unit = {}) {
        val follow = then?.takeIf { t -> t.isNotBlank() }
        if (follow == null) withHub { done(it.interrupt(s.nodeId, s.id, null), "Interrupted") }
        else send(s, follow, sent) { PromptSent(it.interrupt(s.nodeId, s.id, follow), false) }
    }

    /** Deletes an ended session from the hub for good (ADR-0065); [done] once it is gone. */
    fun deleteSession(s: SessionInfo, done: () -> Unit = {}) = withHub {
        it.deleteSession(s.nodeId, s.id)
        history.removeAll { h -> h.session.nodeId == s.nodeId && h.session.id == s.id }
        searchHits.removeAll { h -> h.session.nodeId == s.nodeId && h.session.id == s.id }
        caches.remove(key(s))
        File(cacheDir, key(s) + ".json").delete()
        say("Deleted")
        done()
    }

    /** A queued prompt handed to Claude now, which takes it at its next step, or dropped. */
    fun unqueue(s: SessionInfo, q: QueuedPrompt, send: Boolean) =
        withHub { noted(it.unqueue(s.nodeId, s.id, q.id, send)) }

    /** A prompt stays in the bar until the hub takes it, waiting out a reconnect for up to
     *  [SEND_WAIT_MS]; then it shows with one tick, and the transcript's echo turns it into
     *  the real turn with two. It is sent once: a failure or no answer leaves it in the bar
     *  with the reason, for the person to send again, since a call that got no answer may
     *  still have arrived. */
    private fun send(s: SessionInfo, text: String, sent: () -> Unit, call: suspend (Hub) -> PromptSent) {
        val key = "${s.nodeId}/${s.id}"
        if (key in outgoing) return
        if (demo != null) {
            notSent[key] = "The demo sends nothing · add a hub in Settings to prompt real sessions"
            return
        }
        outgoing[key] = text
        notSent.remove(key)
        viewModelScope.launch {
            var called = false
            val outcome: Result<PromptSent>? = withTimeoutOrNull(SEND_WAIT_MS) {
                snapshotFlow { link }.first { it == Link.Connected || it == Link.Disconnected }
                val h = hub ?: return@withTimeoutOrNull Result.failure(IllegalStateException("not connected to the hub"))
                called = true
                try {
                    Result.success(call(h))
                } catch (e: kotlinx.coroutines.CancellationException) {
                    throw e
                } catch (e: Exception) {
                    Result.failure(e)
                }
            }
            outgoing.remove(key)
            val error = outcome?.exceptionOrNull()
            when {
                outcome == null && called -> notSent[key] = "No answer from the hub in 45 s · it may still arrive, so look before sending again"
                outcome == null -> notSent[key] = "Not sent · no connection to the hub for 45 s"
                error != null -> notSent[key] = "Not sent · ${if (error is Exception) describe(error) else error.toString()}"
                else -> {
                    val r = outcome.getOrThrow()
                    // A queued prompt shows in the queue above the bar, not as sent.
                    if (!r.queued) sending.add(Sent(text, System.currentTimeMillis(), r.note.ifEmpty { null }))
                    if (draft(s.nodeId, s.id) == text) setDraft(s.nodeId, s.id, "")
                    sent()
                }
            }
        }
    }
    fun kill(s: SessionInfo) = withHub { done(it.kill(s.nodeId, s.id), "Killed") }

    /** Starts a headless session on a node (ADR-0034), idle when [prompt] is blank and in a
     *  directory made first when [create] asks, in claude's permission [mode] (empty for the
     *  user's default), under the node's launch [profile] (empty for its own user); [opened]
     *  gets its id once the node has started it. */
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

    /** Loads the first page of history for a node (empty for every node), or with [more]
     *  the page after what is loaded. */
    fun loadHistory(nodeId: String, more: Boolean = false) = withHub {
        if (historyLoading) return@withHub
        historyLoading = true
        try {
            val before = if (more) history.lastOrNull()?.session?.startedMs ?: 0L else 0L
            val page = it.history(nodeId, before, HISTORY_PAGE.toUInt())
            if (!more) history.clear()
            history.addAll(page)
            historyMore = page.size == HISTORY_PAGE
            historyError = null
        } catch (e: Exception) {
            // Shown on the screen itself, not as a toast that leaves "No sessions yet" behind.
            historyError = describe(e)
        } finally {
            historyLoading = false
        }
    }

    /** Whether a finished session can be resumed: its node is online and its directory is
     *  under one of the node's spawn roots. */
    fun resumable(s: SessionInfo): Boolean =
        node(s.nodeId)?.let { n -> n.connected && n.spawnRoots.any { s.cwd == it || s.cwd.startsWith("$it/") } } ?: false

    /** Continues a finished session in place (ADR-0036); its screen turns live when the
     *  watch reports it running. A blank prompt resumes it idle. */
    /** [forked]: the new session's id, when the node's copy and the hub's had diverged and
     *  the hub's resumed as a new session (ADR-0066). */
    fun resume(s: SessionInfo, prompt: String, forked: (String) -> Unit = {}) = withHub {
        // Under the token the session last ran under.
        val r = it.resume(s.nodeId, s.id, prompt.trim(), "", null)
        done(r.note, "Resumed")
        if (r.sessionId.isNotEmpty()) forked(r.sessionId)
    }

    /** Directories to offer for a new session on a node: its spawn roots, then where its
     *  live sessions under them run. */
    fun spawnDirs(node: NodeInfo): List<String> =
        (node.spawnRoots + sessions.filter { it.nodeId == node.id }.map { it.cwd }
            .filter { cwd -> node.spawnRoots.any { cwd.startsWith(it) } }).distinct()

    /** Switches a node's keep-awake hold (ADR-0031); health reports it from the next
     *  sample, so the health screen shows the switch as set until then. */
    fun setKeepAwake(nodeId: String, on: Boolean) = withHub {
        done(it.setKeepAwake(nodeId, on), if (on) "Keep awake on: the machine stays awake while a session works" else "Keep awake off: the machine may sleep")
    }

    /** Shows one session's conversation: what the cache has at once, then the stream from
     *  where the cache ends. */
    fun open(s: SessionInfo) {
        if (openSession?.id == s.id && openSession?.nodeId == s.nodeId && stream != null) return
        openSession = s
        conversation.clear()
        sending.clear()
        delivered.clear()
        usage.clear()
        val cache = caches.getOrPut(key(s)) { SessionCache.load(File(cacheDir, key(s) + ".json")) }
        conversation.addAll(cache.items)
        usage.addAll(cache.usage)
        lastSeq = cache.lastSeq
        olderMore = true
        olderBefore = null
        demo?.let { d ->
            conversation.clear()
            conversation.addAll(d.items)
            usage.clear()
            usage.addAll(d.usage)
            olderMore = false
            return
        }
        withHub { startStream(it, s, cache.lastSeq) }
    }

    /** Whether older items may come before what is loaded, and one page of them is coming
     *  (phase PHS-20261117-f22kxe item 9): a session opens with its last [TAIL] lines. */
    var olderMore by mutableStateOf(true)
        private set
    var olderLoading by mutableStateOf(false)
        private set
    private var olderBefore: ULong? = null

    /** Brings in the page before what is loaded, at the top of the conversation. */
    fun loadOlder() {
        val s = openSession ?: return
        if (olderLoading || !olderMore || loading) return
        val before = olderBefore ?: conversation.firstOrNull()?.seq ?: return
        olderLoading = true
        viewModelScope.launch {
            try {
                val h = hub ?: return@launch
                val page = h.older(s.nodeId, s.id, before, TAIL)
                if (openSession?.id != s.id || openSession?.nodeId != s.nodeId) return@launch
                conversation.addAll(0, page.items)
                caches[key(s)]?.items?.addAll(0, page.items)
                olderBefore = page.firstSeq
                olderMore = page.more && page.firstSeq < before
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                say(describe(e))
            } finally {
                olderLoading = false
            }
        }
    }

    private fun key(s: SessionInfo) = "${s.nodeId}_${s.id}"

    /** While a session opens, its stored items are gathered here until the stream goes
     *  quiet, then shown in one piece, so the list lands at the end instead of scrolling
     *  through the history as it arrives. */
    private var loading = false
    private val backlog = ArrayList<Item>()

    private suspend fun startStream(h: Hub, s: SessionInfo, after: ULong) {
        stream?.cancel()
        val cache = caches[key(s)]
        if (after == 0uL) {
            loading = true
            backlog.clear()
            viewModelScope.launch {
                var seen = -1
                while (loading && openSession?.id == s.id) {
                    delay(150)
                    if (backlog.size == seen) break
                    seen = backlog.size
                }
                loading = false
                if (openSession?.id == s.id) {
                    conversation.addAll(backlog)
                    backlog.clear()
                }
            }
        }
        // A first open brings the last TAIL lines; older ones page in as the person scrolls up.
        val tail = if (after == 0uL) TAIL else 0u
        stream = h.streamSession(s.nodeId, s.id, after, tail, cache?.usage?.toList() ?: emptyList(), object : SessionListener {
            override fun onItems(items: List<Item>, lastSeq: ULong) = onMain {
                cache?.let { c -> c.items.addAll(items); c.lastSeq = lastSeq }
                if (openSession?.id != s.id) return@onMain
                for (item in items) {
                    if (item.role == Role.USER) {
                        val i = sending.indexOfFirst { it.text == item.text }
                        if (i >= 0) {
                            sending.removeAt(i)
                            delivered.add(item.text)
                        }
                    }
                }
                this@HubModel.lastSeq = lastSeq
                if (loading) backlog.addAll(items) else conversation.addAll(items)
            }
            override fun onContext(context: ContextUsage) = onMain { contexts[key(s)] = context }
            override fun onQueue(queued: List<QueuedPrompt>) = onMain { queues[key(s)] = queued }
            override fun onUsage(totals: List<ModelTotals>) = onMain {
                cache?.let { c -> c.usage.clear(); c.usage.addAll(totals) }
                if (openSession?.id != s.id) return@onMain
                usage.clear()
                usage.addAll(totals)
            }
            override fun onError(message: String) = onMain {
                // A stream that broke with the link comes back with it: say so only when,
                // a moment later, the link holds and this stream was not started again.
                val mine = stream
                viewModelScope.launch {
                    delay(QUIET_RECONNECT_MS)
                    if (openSession?.id == s.id && link == Link.Connected && stream === mine)
                        say("Conversation stream ended: $message")
                }
            }
        })
    }

    fun close() {
        stream?.cancel()
        stream = null
        openSession?.let { s -> caches[key(s)]?.let { c -> persist(key(s), c) } }
        openSession = null
        loading = false
        backlog.clear()
        conversation.clear()
        sending.clear()
        delivered.clear()
        usage.clear()
    }

    /** Each node's notebooks, by node id, from its `yajna` service (ADR-0053), and why a
     *  node's did not load. */
    val notebooks = mutableStateMapOf<String, List<NotebookInfo>>()
    val notebookErrors = mutableStateMapOf<String, String>()
    var notebooksLoading by mutableStateOf(0)
        private set

    /** Lists the notebooks of every connected node that offers yajna, each on its own:
     *  a node whose service does not answer does not hold up the others. */
    fun loadNotebooks() {
        val h = hub ?: return
        for (n in nodes.filter { it.connected && NOTEBOOK_SERVICE in it.services }) {
            notebooksLoading++
            viewModelScope.launch {
                try {
                    notebooks[n.id] = h.notebooks(n.id)
                    notebookErrors.remove(n.id)
                } catch (e: Exception) {
                    notebookErrors[n.id] = describe(e)
                } finally {
                    notebooksLoading--
                }
            }
        }
    }

    /** The notebooks a new project can start in (ADR-0053): labs, since a project-shaped
     *  notebook is one project, on connected nodes that may spawn in them. */
    fun projectNotebooks(): List<NotebookInfo> = notebooks.values.flatten().filter { b ->
        val n = node(b.nodeId)
        b.shape == "lab" && n != null && n.connected &&
            n.spawnRoots.any { b.path == it || b.path.startsWith(it.trimEnd('/') + "/") }
    }.sortedWith(compareBy({ it.namespace }, { node(it.nodeId)?.name }))

    fun loadHealth(nodeId: String, sinceMs: Long) = withHub {
        healthLoading = true
        try {
            health.replaceAll(it.health(nodeId, sinceMs))
        } finally {
            healthLoading = false
        }
    }

    private fun persist(key: String, cache: SessionCache) {
        val snapshot = cache.snapshot()
        viewModelScope.launch(Dispatchers.IO) {
            runCatching { File(cacheDir, "$key.json").writeText(snapshot) }
        }
    }

    fun say(text: String) {
        notices.trySend(text)
    }

    /** A command's outcome: the node's note when it has one, else a short confirmation. */
    private fun done(note: String, fallback: String) = say(note.ifEmpty { fallback })

    /** A command's outcome shown only when the node said something about it. */
    private fun noted(note: String) {
        if (note.isNotEmpty()) say(note)
    }

    private fun withHub(block: suspend (Hub) -> Unit) {
        viewModelScope.launch {
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

    private fun describe(e: Exception): String = when (e) {
        is CoreException.Connection -> "Cannot reach the hub: ${e.reason}"
        is CoreException.Hub -> e.reason
        is CoreException.Refused -> "The node refused: ${e.reason}"
        else -> e.message ?: e.toString()
    }

    private fun onMain(block: () -> Unit) {
        viewModelScope.launch(Dispatchers.Main) { block() }
    }

    override fun onCleared() {
        openSession?.let { s -> caches[key(s)]?.let { c -> runCatching { File(cacheDir, key(s) + ".json").writeText(c.snapshot()) } } }
        reconnect?.cancel()
        watch?.cancel()
        stream?.cancel()
        hub?.close()
    }
}

private fun <T> MutableList<T>.replaceAll(items: List<T>) {
    clear()
    addAll(items)
}

private fun <T> MutableList<T>.upsert(item: T, same: (T) -> Boolean) {
    val i = indexOfFirst(same)
    if (i >= 0) this[i] = item else add(item)
}

/** The node service that answers for a machine's notebooks: `yajna serve --socket`. */
const val NOTEBOOK_SERVICE = "yajna"

/** Sessions per history page. */
private const val HISTORY_PAGE = 50
