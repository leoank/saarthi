package me.leoank.saarthi

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONArray
import org.json.JSONObject
import uniffi.saarthi_core.ContextUsage
import uniffi.saarthi_core.HistoryItem
import uniffi.saarthi_core.Hub
import uniffi.saarthi_core.Item
import uniffi.saarthi_core.ModelInUse
import uniffi.saarthi_core.ModelTotals
import uniffi.saarthi_core.NodeInfo
import uniffi.saarthi_core.PendingInfo
import uniffi.saarthi_core.Photo
import uniffi.saarthi_core.PromptSent
import uniffi.saarthi_core.QueuedPrompt
import uniffi.saarthi_core.Role
import uniffi.saarthi_core.SearchHit
import uniffi.saarthi_core.SessionAbout
import uniffi.saarthi_core.SessionInfo
import uniffi.saarthi_core.SessionListener
import uniffi.saarthi_core.Subscription
import java.io.File

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
    /** Photos sent with it, whose paths the node adds to the text (ADR-0071). */
    val photos: Int = 0,
)

/** How many transcript lines a session opens with, and each older page brings. */
private const val TAIL = 300u

/** How long a prompt waits in the bar for the hub to take it, the link's return included. */
private const val SEND_WAIT_MS = 45_000L

/** How long a session's stream outlives its last screen: a switch away and back, or a
 *  look at a notebook, finds it live (ADR-0076). */
private const val STREAM_LINGER_MS = 3 * 60_000L

/** Sessions kept open in memory at most; the oldest unseen one goes first. */
private const val OPEN_MOST = 8

/** Sessions per history page. */
private const val HISTORY_PAGE = 50

/** A session opened on this phone: its conversation as streamed, what was sent from here
 *  and not yet echoed, its usage, and the paging of older lines. Several can be open at
 *  once, one per pane (ADR-0076); each streams while a screen shows it and a few minutes
 *  after. */
class OpenSession(val nodeId: String, val id: String, first: SessionInfo) {
    /** The session as last seen: from the watch while it runs, as opened once it ended. */
    var session by mutableStateOf(first)
        internal set
    val conversation = mutableStateListOf<Item>()
    /** Prompts sent from here that the transcript has not echoed yet, newest last. */
    val sending = mutableStateListOf<Sent>()
    /** Texts the transcript echoed after being sent from here: shown with two ticks. */
    val delivered = mutableStateListOf<String>()
    /** The token totals by model, from the daemon's count. */
    val usage = mutableStateListOf<ModelTotals>()
    /** Whether older items may come before what is loaded, and one page of them is coming
     *  (phase PHS-20261117-f22kxe item 9): a session opens with its last [TAIL] lines. */
    var olderMore by mutableStateOf(true)
        internal set
    var olderLoading by mutableStateOf(false)
        internal set
    internal var olderBefore: ULong? = null
    internal var lastSeq = 0uL
    internal var stream: Subscription? = null
    /** Screens showing it now. */
    internal var shown = 0
    internal var linger: Job? = null
    /** When a screen last let it go, for which to drop first. */
    internal var seenMs = System.currentTimeMillis()
    /** While it opens, its stored items gather here until the stream goes quiet, then show
     *  in one piece, so the list lands at the end instead of scrolling through the history
     *  as it arrives. */
    internal var loading = false
    internal val backlog = ArrayList<Item>()
}

/** The Agents workspace (ADR-0077): the open sessions and their streams, prompts and photos
 *  on their way, the queues, models and context of each, history and search, and Home's
 *  and the session screen's preferences. */
class AgentsModel(val conn: Connection) {
    private val prefs = conn.prefs
    private val jobs = conn.scope

    /** Monospace text wraps instead of scrolling sideways. */
    var wrap by mutableStateOf(prefs.getBoolean("wrap", false))
        private set

    fun toggleWrap() {
        wrap = !wrap
        prefs.edit().putBoolean("wrap", wrap).apply()
    }

    /** What Home and History show, remembered across launches (phase PHS-20261126-f3e51h
     *  item 6; Home's old machine-or-repository scope carries over). */
    var filter by mutableStateOf(
        prefs.getString("filter", null)?.let(SessionsFilter::fromJson)
            ?: SessionsFilter.fromScope(prefs.getString("scope", "")!!),
    )
        private set

    fun changeFilter(to: SessionsFilter) {
        filter = to
        prefs.edit().putString("filter", to.toJson()).apply()
    }

    /** The filter's launcher as the hub names it: `you` is this phone's own name. */
    fun launchedBy(): String = if (filter.launcher == SessionsFilter.YOU) conn.you else filter.launcher

    /** Whether Home shows a session (null: a request whose session this phone does not
     *  know, kept unless the filter needs the session to say). */
    fun inScope(nodeId: String, s: SessionInfo?): Boolean {
        val f = filter
        if (f.nodeId.isNotEmpty() && nodeId != f.nodeId) return false
        if (s == null) return f.repo.isEmpty() && f.launcher.isEmpty() && f.labels.isEmpty()
        return f.matches(s, conn.you)
    }

    /** The hub's side of the filter, for the history and search; null while it names this
     *  phone and the hub has not said its name, when nothing can pass. */
    private fun hubFilter(): uniffi.saarthi_core.SessionFilter? =
        launchedBy().takeIf { it.isNotEmpty() || filter.launcher.isEmpty() }
            ?.let { uniffi.saarthi_core.SessionFilter(it, filter.labels) }

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

    /** The sessions open on this phone, by "node_session". */
    private val open = mutableStateMapOf<String, OpenSession>()
    /** What was seen of each session, kept in memory while the app lives and on disk
     *  between runs, so opening a session shows it at once and the stream resumes after
     *  the last item instead of replaying everything (phase 1.5 item 14). */
    private val caches = HashMap<String, SessionCache>()
    private val cacheDir = File(conn.app.filesDir, "sessions").apply { mkdirs() }

    /** A prompt on its way to the hub, by "node/session": it stays in the bar, and the bar
     *  takes no other, until the hub has it (phase PHS-20261117-f22kxe item 6). */
    val outgoing = mutableStateMapOf<String, String>()

    /** Why a session's last prompt was not taken, by "node/session", shown over its bar. */
    val notSent = mutableStateMapOf<String, String>()

    /** How full each spawned session's context was after its last turn, while the app runs. */
    private val contexts = mutableStateMapOf<String, ContextUsage>()
    fun contextOf(s: SessionInfo): ContextUsage? = contexts[key(s)]

    /** What the hub holds of the session *About this session* shows, once it has come. */
    var about by mutableStateOf<SessionAbout?>(null)
        private set

    fun loadAbout(s: SessionInfo) {
        about = null
        conn.withHub { about = it.sessionAbout(s.nodeId, s.id) }
    }

    /** Each session's model and effort as its transcript last said, or as just switched to. */
    private val models = mutableStateMapOf<String, ModelInUse>()
    fun modelOf(s: SessionInfo): ModelInUse? = models[key(s)]

    /** Switches a session's model and effort (phase PHS-20261117-f22kxe item 18); shown at
     *  once, and corrected by the session's next reply. */
    fun setModel(s: SessionInfo, model: String, effort: String) = conn.withHub {
        it.setModel(s.nodeId, s.id, model, effort)
        val was = models[key(s)]
        models[key(s)] = ModelInUse(
            label = model.ifEmpty { null }?.replaceFirstChar { c -> c.uppercase() } ?: was?.label.orEmpty(),
            effort = effort.ifEmpty { was?.effort.orEmpty() },
        )
    }

    /** The prompts the node holds for each session until it is next idle, as it last said. */
    private val queues = mutableStateMapOf<String, List<QueuedPrompt>>()
    fun queueOf(s: SessionInfo): List<QueuedPrompt> = queues[key(s)].orEmpty()

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
        searchJob = jobs.launch {
            val h = conn.connection ?: return@launch
            searching = true
            try {
                val hits = hubFilter()?.let { h.search(query, nodeId, it) }.orEmpty()
                searchHits.clear()
                searchHits.addAll(hits)
                searched = query
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                conn.say(conn.describe(e))
            } finally {
                searching = false
            }
        }
    }

    fun pendingFor(s: SessionInfo): List<PendingInfo> = conn.pending.filter { it.sessionId == s.id && it.nodeId == s.nodeId }

    // The card leaving is the confirmation; only the node's note, if any, is worth a toast.
    fun allow(p: PendingInfo) = conn.withHub { conn.noted(it.allow(p.nodeId, p.id)) }
    fun deny(p: PendingInfo, message: String) = conn.withHub { conn.noted(it.deny(p.nodeId, p.id, message)) }
    fun answer(p: PendingInfo, answers: Map<String, List<String>>) =
        conn.withHub { conn.noted(it.answer(p.nodeId, p.id, answers)) }

    /** [sent]: the hub took it, so the bar may let it go. */
    /** While Claude works the node holds the prompt until the turn ends, in the session's
     *  queue (phase PHS-20261117-f22kxe item 7); an older node delivers it at once. */
    fun prompt(s: SessionInfo, text: String, sent: () -> Unit) {
        val photos = photosOf(s)
        send(s, text, { this.photos.remove(key(s)); keepSent(s, photos); sent() }, photos.size) {
            it.prompt(s.nodeId, s.id, text, s.busy, photos.map { p -> Photo(p.name, p.bytes) })
        }
    }

    /** Where this phone keeps copies of the photos it sent to session [s], by the name the
     *  node saves each under, so its prompts can show them (ADR-0071). */
    fun photoDir(s: SessionInfo): File = File(conn.app.filesDir, "photos/" + key(s))

    private fun keepSent(s: SessionInfo, photos: List<Attached>) {
        if (photos.isEmpty()) return
        val dir = photoDir(s)
        jobs.launch(Dispatchers.IO) {
            runCatching {
                dir.mkdirs()
                for (p in photos) File(dir, savedName(p.name)).writeBytes(p.bytes)
            }
        }
    }

    /** Photos put in a session's prompt and not yet sent (ADR-0071), kept as the draft is. */
    private val photos = mutableStateMapOf<String, List<Attached>>()
    fun photosOf(s: SessionInfo): List<Attached> = photos[key(s)].orEmpty()
    fun attach(s: SessionInfo, more: List<Attached>) {
        val all = photosOf(s) + more
        if (all.sumOf { it.bytes.size } > PHOTOS_MOST_BYTES) conn.say("Too many photos for one prompt · send these first")
        else photos[key(s)] = all
    }
    fun detach(s: SessionInfo, photo: Attached) {
        photos[key(s)] = photosOf(s) - photo
    }
    fun interrupt(s: SessionInfo, then: String?, sent: () -> Unit = {}) {
        val follow = then?.takeIf { t -> t.isNotBlank() }
        if (follow == null) conn.withHub { conn.done(it.interrupt(s.nodeId, s.id, null), "Interrupted") }
        else send(s, follow, sent) { PromptSent(it.interrupt(s.nodeId, s.id, follow), false) }
    }

    /** Deletes an ended session from the hub for good (ADR-0065); [done] once it is gone. */
    fun deleteSession(s: SessionInfo, done: () -> Unit = {}) = conn.withHub {
        it.deleteSession(s.nodeId, s.id)
        history.removeAll { h -> h.session.nodeId == s.nodeId && h.session.id == s.id }
        searchHits.removeAll { h -> h.session.nodeId == s.nodeId && h.session.id == s.id }
        caches.remove(key(s))
        open.remove(key(s))?.let { v -> v.stream?.cancel(); v.linger?.cancel() }
        File(cacheDir, key(s) + ".json").delete()
        photoDir(s).deleteRecursively()
        conn.say("Deleted")
        done()
    }

    /** A queued prompt handed to Claude now, which takes it at its next step, or dropped.
     *  Sent, it shows with one tick until the transcript has it: the step Claude is in can
     *  run for minutes (a long build), and without it nothing would show it went. */
    fun unqueue(s: SessionInfo, q: QueuedPrompt, send: Boolean) = conn.withHub {
        val note = it.unqueue(s.nodeId, s.id, q.id, send)
        val gone = note == "it was no longer queued"
        if (send && !gone) open[key(s)]?.sending?.add(Sent(q.text, System.currentTimeMillis(), note.ifEmpty { "Sent · Claude takes it after the step it is in" }))
        else conn.noted(note)
    }

    /** A prompt stays in the bar until the hub takes it, waiting out a reconnect for up to
     *  [SEND_WAIT_MS]; then it shows with one tick, and the transcript's echo turns it into
     *  the real turn with two. It is sent once: a failure or no answer leaves it in the bar
     *  with the reason, for the person to send again, since a call that got no answer may
     *  still have arrived. */
    private fun send(s: SessionInfo, text: String, sent: () -> Unit, photos: Int = 0, call: suspend (Hub) -> PromptSent) {
        val key = "${s.nodeId}/${s.id}"
        if (key in outgoing) return
        if (conn.demo != null) {
            notSent[key] = "The demo sends nothing · add a hub in Settings to prompt real sessions"
            return
        }
        outgoing[key] = text
        notSent.remove(key)
        jobs.launch {
            var called = false
            val outcome: Result<PromptSent>? = withTimeoutOrNull(SEND_WAIT_MS) {
                snapshotFlow { conn.link }.first { it == Link.Connected || it == Link.Disconnected }
                val h = conn.connection ?: return@withTimeoutOrNull Result.failure(IllegalStateException("not connected to the hub"))
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
                error != null -> notSent[key] = "Not sent · ${if (error is Exception) conn.describe(error) else error.toString()}"
                else -> {
                    val r = outcome.getOrThrow()
                    // A queued prompt shows in the queue above the bar, not as sent.
                    if (!r.queued) open[key(s)]?.sending?.add(Sent(text, System.currentTimeMillis(), r.note.ifEmpty { null }, photos))
                    if (draft(s.nodeId, s.id) == text) setDraft(s.nodeId, s.id, "")
                    sent()
                }
            }
        }
    }
    fun kill(s: SessionInfo) = conn.withHub { conn.done(it.kill(s.nodeId, s.id), "Killed") }

    /** Loads the first page of history for a node (empty for every node), or with [more]
     *  the page after what is loaded. */
    fun loadHistory(nodeId: String, more: Boolean = false) = conn.withHub {
        if (historyLoading) return@withHub
        historyLoading = true
        try {
            val before = if (more) history.lastOrNull()?.session?.startedMs ?: 0L else 0L
            val page = hubFilter()?.let { f -> it.history(nodeId, before, HISTORY_PAGE.toUInt(), f) }.orEmpty()
            if (!more) history.clear()
            history.addAll(page)
            historyMore = page.size == HISTORY_PAGE
            historyError = null
        } catch (e: Exception) {
            // Shown on the screen itself, not as a toast that leaves "No sessions yet" behind.
            historyError = conn.describe(e)
        } finally {
            historyLoading = false
        }
    }

    /** Whether a finished session can be resumed: its node is online and its directory is
     *  under one of the node's spawn roots. */
    fun resumable(s: SessionInfo): Boolean =
        conn.node(s.nodeId)?.let { n -> n.connected && n.spawnRoots.any { s.cwd == it || s.cwd.startsWith("$it/") } } ?: false

    /** Continues a finished session in place (ADR-0036); its screen turns live when the
     *  watch reports it running. A blank prompt resumes it idle. */
    /** [forked]: the new session's id, when the node's copy and the hub's had diverged and
     *  the hub's resumed as a new session (ADR-0066). */
    fun resume(s: SessionInfo, prompt: String, forked: (String) -> Unit = {}) = conn.withHub {
        // Under the token the session last ran under.
        val r = it.resume(s.nodeId, s.id, prompt.trim(), "", null)
        conn.done(r.note, "Resumed")
        if (r.sessionId.isNotEmpty()) forked(r.sessionId)
    }

    /** Directories to offer for a new session on a node: its spawn roots, then where its
     *  live sessions under them run. */
    fun spawnDirs(node: NodeInfo): List<String> =
        (node.spawnRoots + conn.sessions.filter { it.nodeId == node.id }.map { it.cwd }
            .filter { cwd -> node.spawnRoots.any { cwd.startsWith(it) } }).distinct()

    /** A session opened on this phone, or null. */
    fun opened(nodeId: String, id: String): OpenSession? = open["${nodeId}_$id"]

    /** Opens a session: what the cache has at once; the stream starts when a screen shows
     *  it ([shown]). Opening one already open keeps what it has. */
    fun open(s: SessionInfo): OpenSession {
        open[key(s)]?.let { v -> v.session = s; return v }
        val v = OpenSession(s.nodeId, s.id, s)
        val cache = caches.getOrPut(key(s)) { SessionCache.load(File(cacheDir, key(s) + ".json")) }
        v.conversation.addAll(cache.items)
        v.usage.addAll(cache.usage)
        v.lastSeq = cache.lastSeq
        conn.demo?.let { d ->
            v.conversation.clear()
            v.conversation.addAll(d.items)
            v.usage.clear()
            v.usage.addAll(d.usage)
            v.olderMore = false
        }
        open[key(s)] = v
        // Too many open: the one longest unseen goes, its cache kept.
        if (open.size > OPEN_MOST) open.values.filter { it.shown == 0 && it !== v }.minByOrNull { it.seenMs }?.let { drop(it) }
        return v
    }

    /** A screen shows [v]: its stream runs. */
    fun shown(v: OpenSession) {
        v.shown++
        v.linger?.cancel()
        v.linger = null
        if (v.stream == null && conn.demo == null) conn.withHub { if (v.stream == null) startStream(it, v, v.lastSeq) }
    }

    /** A screen no longer shows [v]: its stream stays a few minutes for a quick return. */
    fun hidden(v: OpenSession) {
        v.shown = (v.shown - 1).coerceAtLeast(0)
        v.seenMs = System.currentTimeMillis()
        persist(v)
        if (v.shown > 0) return
        v.linger?.cancel()
        v.linger = jobs.launch {
            delay(STREAM_LINGER_MS)
            if (v.shown == 0) {
                v.stream?.cancel()
                v.stream = null
            }
        }
    }

    /** Closes a session for good on this phone: its stream ends, its cache stays. */
    fun close(v: OpenSession) {
        if (v.shown > 0) return
        drop(v)
    }

    private fun drop(v: OpenSession) {
        v.linger?.cancel()
        v.stream?.cancel()
        v.stream = null
        persist(v)
        open.remove("${v.nodeId}_${v.id}")
    }

    /** Brings in the page before what is loaded, at the top of the conversation. */
    fun loadOlder(v: OpenSession) {
        if (v.olderLoading || !v.olderMore || v.loading) return
        val before = v.olderBefore ?: v.conversation.firstOrNull()?.seq ?: return
        v.olderLoading = true
        jobs.launch {
            try {
                val h = conn.connection ?: return@launch
                val page = h.older(v.nodeId, v.id, before, TAIL)
                v.conversation.addAll(0, page.items)
                caches["${v.nodeId}_${v.id}"]?.items?.addAll(0, page.items)
                v.olderBefore = page.firstSeq
                v.olderMore = page.more && page.firstSeq < before
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                conn.say(conn.describe(e))
            } finally {
                v.olderLoading = false
            }
        }
    }

    private fun key(s: SessionInfo) = "${s.nodeId}_${s.id}"

    private suspend fun startStream(h: Hub, v: OpenSession, after: ULong) {
        v.stream?.cancel()
        val k = "${v.nodeId}_${v.id}"
        val cache = caches[k]
        if (after == 0uL) {
            v.loading = true
            v.backlog.clear()
            jobs.launch {
                var seen = -1
                while (v.loading) {
                    delay(150)
                    if (v.backlog.size == seen) break
                    seen = v.backlog.size
                }
                v.loading = false
                v.conversation.addAll(v.backlog)
                v.backlog.clear()
            }
        }
        // A first open brings the last TAIL lines; older ones page in as the person scrolls up.
        val tail = if (after == 0uL) TAIL else 0u
        var mine: Subscription? = null
        mine = h.streamSession(v.nodeId, v.id, after, tail, cache?.usage?.toList() ?: emptyList(), object : SessionListener {
            override fun onItems(items: List<Item>, lastSeq: ULong) = conn.onMain {
                cache?.let { c -> c.items.addAll(items); c.lastSeq = lastSeq }
                for (item in items) {
                    if (item.role == Role.USER) {
                        // With photos, the node added their paths after the text.
                        val i = v.sending.indexOfFirst { it.text == item.text || it.photos > 0 && item.text.startsWith(it.text) }
                        if (i >= 0) {
                            v.sending.removeAt(i)
                            v.delivered.add(item.text)
                        }
                    }
                }
                v.lastSeq = lastSeq
                if (v.loading) v.backlog.addAll(items) else v.conversation.addAll(items)
            }
            override fun onContext(context: ContextUsage) = conn.onMain { contexts[k] = context }
            override fun onQueue(queued: List<QueuedPrompt>) = conn.onMain { queues[k] = queued }
            override fun onModel(model: ModelInUse) = conn.onMain { models[k] = model }
            override fun onUsage(totals: List<ModelTotals>) = conn.onMain {
                cache?.let { c -> c.usage.clear(); c.usage.addAll(totals) }
                v.usage.clear()
                v.usage.addAll(totals)
            }
            override fun onError(message: String) = conn.onMain {
                // A stream that broke with the link comes back with it: say so only when,
                // a moment later, the link holds, the session is on screen and this stream
                // was not started again.
                jobs.launch {
                    delay(QUIET_RECONNECT_MS)
                    if (v.shown > 0 && conn.link == Link.Connected && v.stream === mine)
                        conn.say("Conversation stream ended: $message")
                }
            }
        })
        v.stream = mine
    }

    private fun persist(v: OpenSession) {
        val k = "${v.nodeId}_${v.id}"
        val snapshot = caches[k]?.snapshot() ?: return
        jobs.launch(Dispatchers.IO) {
            runCatching { File(cacheDir, "$k.json").writeText(snapshot) }
        }
    }

    /** Writes every open session's cache before the app's state goes. */
    fun closeAll() {
        for (v in open.values) {
            v.stream?.cancel()
            val k = "${v.nodeId}_${v.id}"
            caches[k]?.let { c -> runCatching { File(cacheDir, "$k.json").writeText(c.snapshot()) } }
        }
    }

    init {
        conn.waiting = { outgoing.isNotEmpty() }
        conn.onConnected { h -> open.values.filter { it.shown > 0 || it.linger?.isActive == true }.forEach { startStream(h, it, it.lastSeq) } }
        conn.onDropped {
            open.values.forEach { it.stream?.cancel(); it.stream = null; persist(it) }
            open.clear()
        }
        // The watch keeps an open session's row current while it runs.
        jobs.launch {
            snapshotFlow { conn.sessions.toList() }.collect { list ->
                for (v in open.values) list.firstOrNull { it.nodeId == v.nodeId && it.id == v.id }?.let { v.session = it }
            }
        }
    }
}
