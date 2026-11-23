package me.leoank.saarthi

import android.annotation.SuppressLint
import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.webkit.WebMessage
import android.webkit.WebMessagePort
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import uniffi.saarthi_core.HeaderInfo
import uniffi.saarthi_core.Hub
import uniffi.saarthi_core.NotebookInfo
import uniffi.saarthi_core.ServiceResponse
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.util.concurrent.ConcurrentHashMap

/** The node service that answers for a machine's notebooks: `yajna serve --socket`. */
const val NOTEBOOK_SERVICE = "yajna"
/** The node's own service of what a session changed (ADR-0067). */
const val CHANGES_SERVICE = "changes"

/**
 * Feeds a web view from one node's local service, `yajna` or `changes`, through the hub
 * (ADR-0056, ADR-0067): every
 * request at [origin] is answered by `shouldInterceptRequest` over the core's
 * `service_request`; every HTML page gets [BRIDGE] at the top of its head, which carries
 * the page's event streams and its requests with a body, both of which interception
 * cannot, over a message port.
 */
class Relay(private val nodeId: String, private val service: String, private val hub: () -> Hub?) {
    /** A host of its own per node, so one machine's pages never share storage with another's. */
    val origin = "https://$nodeId.$service.saarthi"
    private val host = Uri.parse(origin).host
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    /** The current page's event streams, by the page's id for each. */
    private val streams = ConcurrentHashMap<Int, Pair<Job, ServiceResponse?>>()
    /** Whether the current page has been handed its port. */
    @Volatile private var ported = false

    @SuppressLint("SetJavaScriptEnabled")
    fun webView(context: android.content.Context, onHistory: (Boolean) -> Unit, onLoading: (Boolean) -> Unit): WebView {
        val view = WebView(context)
        // Its own size, not its content's: under wrap_content a page laid out at 100% of
        // the viewport (yajna's files viewer) gets no height at all.
        view.layoutParams = android.view.ViewGroup.LayoutParams(
            android.view.ViewGroup.LayoutParams.MATCH_PARENT,
            android.view.ViewGroup.LayoutParams.MATCH_PARENT,
        )
        view.settings.javaScriptEnabled = true
        // yajna's files viewer renders blank without it (ADR-0056).
        view.settings.domStorageEnabled = true
        view.webViewClient = object : WebViewClient() {
            override fun shouldInterceptRequest(view: WebView, req: WebResourceRequest): WebResourceResponse? =
                if (req.url.host == host) answer(req) else null

            override fun shouldOverrideUrlLoading(view: WebView, req: WebResourceRequest): Boolean {
                if (req.url.host == host) return false
                // A link out of the notebook opens in the browser.
                runCatching { view.context.startActivity(Intent(Intent.ACTION_VIEW, req.url)) }
                return true
            }

            override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
                // A new page: the last one's streams are gone with it.
                closeStreams()
                ported = false
                onLoading(true)
            }

            override fun onPageFinished(view: WebView, url: String) {
                onLoading(false)
                if (!ported && Uri.parse(url).host == host) {
                    ported = true
                    givePort(view)
                }
            }

            override fun doUpdateVisitedHistory(view: WebView, url: String, isReload: Boolean) {
                onHistory(view.canGoBack())
            }
        }
        return view
    }

    fun close() {
        closeStreams()
        scope.cancel()
    }

    private fun closeStreams() {
        for ((_, s) in streams) {
            s.second?.cancel()
            s.first.cancel()
        }
        streams.clear()
    }

    /** Answers one of the page's requests over the relay; runs on the web view's own
     *  request threads, so blocking on the core here holds up nothing else. */
    private fun answer(req: WebResourceRequest): WebResourceResponse {
        val h = hub() ?: return failure(req, "Not connected to the hub")
        var path = pathOf(req.url)
        val headers = req.requestHeaders
            .filterKeys { !it.equals("host", true) && !it.lowercase().startsWith("if-") }
            .map { (k, v) -> HeaderInfo(k, v) }
        try {
            // Interception cannot pass a redirect: one for the page itself becomes a page
            // that goes there, one for anything else is followed here.
            repeat(MAX_REDIRECTS) {
                val r = runBlocking { h.serviceRequest(nodeId, service, req.method, path, headers, null) }
                val status = r.status().toInt()
                val location = header(r, "location")
                if (status !in 300..399) return response(r, status)
                r.cancel(); r.close()
                if (location == null) return failure(req, "The notebook answered $status without a location")
                val to = Uri.parse(origin + path).let { base -> java.net.URI(base.toString()).resolve(location) }
                if (req.isForMainFrame) return redirectPage(to.toString())
                if (to.host != host) return failure(req, "The notebook redirected out of itself")
                path = (to.rawPath ?: "/") + (to.rawQuery?.let { "?$it" } ?: "")
            }
            return failure(req, "Too many redirects")
        } catch (e: Exception) {
            return failure(req, describe(e))
        }
    }

    private fun response(r: ServiceResponse, status: Int): WebResourceResponse {
        val type = header(r, "content-type") ?: "application/octet-stream"
        val mime = type.substringBefore(';').trim().lowercase()
        val charset = Regex("charset=\"?([^;\"]+)", RegexOption.IGNORE_CASE).find(type)?.groupValues?.get(1)
            ?: if (mime.startsWith("text/")) "utf-8" else null
        var headers = r.headers().associate { it.name to it.value }
        val body: InputStream = if (mime == "text/html") {
            // The bridge goes first, before any of the page's own scripts.
            val html = RelayBody(r).use { it.readBytes() }.toString(Charsets.UTF_8)
            val at = Regex("<head[^>]*>", RegexOption.IGNORE_CASE).find(html)?.range?.last?.plus(1) ?: 0
            headers = headers.filterKeys { !it.equals("content-length", true) }
            ByteArrayInputStream((html.substring(0, at) + BRIDGE + html.substring(at)).toByteArray())
        } else if (status == 206) {
            // The web view applies the request's range itself to what it is given, taking it
            // for the whole resource: the part goes at its place, behind a lead it skips.
            // It also takes `available()` for the resource's length and checks the range
            // against it, so that is the length up to the part's end.
            val range = Regex("""bytes (\d+)-(\d+)""").find(header(r, "content-range") ?: "")
            val start = range?.groupValues?.get(1)?.toLongOrNull() ?: 0L
            val end = range?.groupValues?.get(2)?.toLongOrNull()
            headers = headers.filterKeys { !it.equals("content-length", true) }
            RelayBody(r, lead = start, length = end?.plus(1))
        } else {
            // The web view sets the length from `available()` beside any it is given.
            val length = header(r, "content-length")?.trim()?.toLongOrNull()
            headers = headers.filterKeys { !it.equals("content-length", true) }
            RelayBody(r, length = length)
        }
        return WebResourceResponse(mime, charset, status, if (status < 400) "OK" else "Error", headers, body)
    }

    private fun redirectPage(to: String): WebResourceResponse {
        val q = to.replace("&", "&amp;").replace("\"", "&quot;")
        val html = "<!doctype html><meta http-equiv=\"refresh\" content=\"0;url=$q\">"
        return WebResourceResponse("text/html", "utf-8", ByteArrayInputStream(html.toByteArray()))
    }

    private fun failure(req: WebResourceRequest, why: String): WebResourceResponse {
        val body = if (req.isForMainFrame) {
            val t = why.replace("&", "&amp;").replace("<", "&lt;")
            "<!doctype html><meta name=viewport content=\"width=device-width\"><body style=\"font-family:sans-serif;padding:1em\"><h3>The notebook did not answer</h3><p>$t</p>"
        } else why
        return WebResourceResponse(
            if (req.isForMainFrame) "text/html" else "text/plain", "utf-8", 502, "Error", emptyMap(),
            ByteArrayInputStream(body.toByteArray()),
        )
    }

    /** One channel per page: the page's end goes to it, ours takes the page's requests. */
    private fun givePort(view: WebView) {
        val (ours, theirs) = view.createWebMessageChannel()
        ours.setWebMessageCallback(object : WebMessagePort.WebMessageCallback() {
            override fun onMessage(port: WebMessagePort, message: WebMessage) {
                val m = runCatching { JSONObject(message.data ?: return) }.getOrNull() ?: return
                val id = m.optInt("id")
                val url = Uri.parse(m.optString("url"))
                when (m.optString("op")) {
                    "open" -> if (url.host == host) openStream(view, ours, id, pathOf(url))
                    "close" -> streams.remove(id)?.let { (job, r) -> r?.cancel(); job.cancel() }
                    "fetch" -> if (url.host == host) fetch(view, ours, id, pathOf(url), m)
                }
            }
        })
        view.postWebMessage(WebMessage("saarthi-port", arrayOf(theirs)), Uri.parse(origin))
    }

    private fun post(view: WebView, port: WebMessagePort, o: JSONObject) {
        val s = o.toString()
        view.post { runCatching { port.postMessage(WebMessage(s)) } }
    }

    /** Reads one event stream and posts each event to the page as it arrives. An answer
     *  that is not a stream fails it for good, as with `EventSource`; a stream that ends
     *  or breaks lets the page's bridge reconnect. */
    private fun openStream(view: WebView, port: WebMessagePort, id: Int, path: String) {
        // Listed before it starts, so a close or the end of the page always finds it.
        val job = scope.launch(start = CoroutineStart.LAZY) {
            var r: ServiceResponse? = null
            try {
                val h = hub() ?: throw IOException("not connected")
                r = h.serviceRequest(
                    nodeId, service, "GET", path,
                    listOf(HeaderInfo("accept", "text/event-stream"), HeaderInfo("cache-control", "no-cache")),
                    null,
                )
                streams[id]?.let { streams[id] = it.first to r }
                val type = header(r, "content-type") ?: ""
                if (r.status().toInt() != 200 || !type.startsWith("text/event-stream")) {
                    post(view, port, JSONObject().put("id", id).put("error", true).put("fatal", true))
                    return@launch
                }
                post(view, port, JSONObject().put("id", id).put("open", true))
                val events = Sse { event, data ->
                    post(view, port, JSONObject().put("id", id).put("event", event).put("data", data))
                }
                while (true) events.feed(r.read() ?: break)
                if (streams.containsKey(id)) post(view, port, JSONObject().put("id", id).put("error", true))
            } catch (e: Exception) {
                if (streams.containsKey(id)) post(view, port, JSONObject().put("id", id).put("error", true))
            } finally {
                streams.remove(id)
                r?.close()
            }
        }
        streams[id] = job to null
        job.start()
    }

    /** A request with a body, which interception cannot see: sent here, answered whole. */
    private fun fetch(view: WebView, port: WebMessagePort, id: Int, path: String, m: JSONObject) {
        scope.launch {
            val reply = JSONObject().put("id", id).put("fetch", true)
            try {
                val h = hub() ?: throw IOException("not connected")
                val sent = m.optJSONObject("headers")
                val headers = sent?.keys()?.asSequence()?.map { HeaderInfo(it, sent.getString(it)) }?.toList() ?: emptyList()
                val r = h.serviceRequest(
                    nodeId, service, m.optString("method", "POST"), path, headers,
                    m.optString("body").toByteArray(),
                )
                val out = JSONObject()
                r.headers().forEach { out.put(it.name, it.value) }
                reply.put("status", r.status().toInt()).put("headers", out)
                // Reading the body to its end closes the answer.
                reply.put("body", RelayBody(r).use { it.readBytes() }.toString(Charsets.UTF_8))
            } catch (e: Exception) {
                reply.put("failed", describe(e))
            }
            post(view, port, reply)
        }
    }

    private fun pathOf(u: Uri): String = (u.encodedPath ?: "/").ifEmpty { "/" } + (u.encodedQuery?.let { "?$it" } ?: "")

    private fun header(r: ServiceResponse, name: String): String? =
        r.headers().firstOrNull { it.name.equals(name, true) }?.value

    private fun describe(e: Exception): String = when (e) {
        is uniffi.saarthi_core.CoreException.Connection -> "Cannot reach the hub: ${e.reason}"
        is uniffi.saarthi_core.CoreException.Hub -> e.reason
        is uniffi.saarthi_core.CoreException.Refused -> e.reason
        else -> e.message ?: e.toString()
    }
}

private const val MAX_REDIRECTS = 5

/** A service answer's body as a stream, read from the core as the web view asks, after
 *  [lead] bytes that stand for the part of the resource before it; [length], when known,
 *  is what `available()` says is left. */
private class RelayBody(
    private val r: ServiceResponse,
    private var lead: Long = 0,
    private var length: Long? = null,
) : InputStream() {
    private var piece = ByteArray(0)
    private var at = 0
    private var done = false

    private fun fill(): Boolean {
        while (at >= piece.size) {
            if (done) return false
            val next = try {
                runBlocking { r.read() }
            } catch (e: Exception) {
                done = true
                throw IOException(e.message, e)
            }
            if (next == null) {
                done = true
                return false
            }
            piece = next
            at = 0
        }
        return true
    }

    private fun consumed(n: Long) {
        length = length?.let { (it - n).coerceAtLeast(0) }
    }

    override fun read(): Int {
        if (lead > 0) {
            lead--
            consumed(1)
            return 0
        }
        if (!fill()) return -1
        consumed(1)
        return piece[at++].toInt() and 0xff
    }

    override fun read(b: ByteArray, off: Int, len: Int): Int {
        if (len == 0) return 0
        if (lead > 0) {
            val n = minOf(len.toLong(), lead).toInt()
            b.fill(0, off, off + n)
            lead -= n
            consumed(n.toLong())
            return n
        }
        if (!fill()) return -1
        val n = minOf(len, piece.size - at)
        System.arraycopy(piece, at, b, off, n)
        at += n
        consumed(n.toLong())
        return n
    }

    override fun skip(n: Long): Long {
        if (n <= 0) return 0
        if (lead > 0) {
            val k = minOf(n, lead)
            lead -= k
            consumed(k)
            return k
        }
        return super.skip(n)
    }

    override fun available(): Int =
        length?.coerceAtMost(Int.MAX_VALUE.toLong())?.toInt() ?: (piece.size - at)

    override fun close() {
        r.cancel()
        r.close()
    }
}

/** Server-sent events, parsed from pieces that may split a line or a character. */
private class Sse(private val emit: (event: String, data: String) -> Unit) {
    private val line = ByteArrayOutputStream()
    private var event = ""
    private val data = StringBuilder()

    fun feed(bytes: ByteArray) {
        for (b in bytes) {
            if (b == '\n'.code.toByte()) {
                take(String(line.toByteArray(), Charsets.UTF_8).removeSuffix("\r"))
                line.reset()
            } else {
                line.write(b.toInt())
            }
        }
    }

    private fun take(l: String) {
        if (l.isEmpty()) {
            if (data.isNotEmpty()) emit(event.ifEmpty { "message" }, data.removeSuffix("\n").toString())
            event = ""
            data.setLength(0)
            return
        }
        if (l.startsWith(":")) return
        val field = l.substringBefore(':')
        val value = if (':' in l) l.substringAfter(':').removePrefix(" ") else ""
        when (field) {
            "event" -> event = value
            "data" -> data.append(value).append('\n')
        }
    }
}

/**
 * Put at the top of every notebook page (ADR-0056). Replaces `EventSource` with one fed
 * over a `WebMessagePort` the app hands the page as "saarthi-port", reconnecting after a
 * dropped stream as `EventSource` does; and sends a same-origin `fetch` with a string body
 * (yajna's review writes) over the port, since interception never sees a request's body.
 */
private const val BRIDGE = """<script>(function(){
  var port = null, queue = [], subs = new Map(), fetches = new Map(), next = 1;
  function send(m){ m = JSON.stringify(m); if (port) port.postMessage(m); else queue.push(m); }
  window.addEventListener('message', function(e){
    if (e.data !== 'saarthi-port' || !e.ports || !e.ports[0]) return;
    port = e.ports[0];
    port.onmessage = function(m){
      var d = JSON.parse(m.data);
      if (d.fetch) {
        var f = fetches.get(d.id); if (!f) return; fetches.delete(d.id);
        if (d.failed !== undefined) { f.reject(new TypeError(d.failed)); return; }
        var empty = d.status === 204 || d.status === 205 || d.status === 304;
        f.resolve(new Response(empty ? null : d.body, {status: d.status, headers: d.headers}));
        return;
      }
      var s = subs.get(d.id); if (!s) return;
      if (d.open) { s.readyState = 1; s._fire('open', null); }
      else if (d.error) {
        if (d.fatal) { s.readyState = 2; subs.delete(s.id); s._fire('error', null); return; }
        s.readyState = 0; s._fire('error', null);
        setTimeout(function(){ if (s.readyState === 0) send({op: 'open', id: s.id, url: s.url}); }, 3000);
      }
      else s._fire(d.event || 'message', d.data);
    };
    queue.splice(0).forEach(function(q){ port.postMessage(q); });
  });
  function BridgeEventSource(url){
    this.url = new URL(url, location.href).toString(); this.readyState = 0; this.withCredentials = false;
    this._l = {}; this.id = next++; subs.set(this.id, this);
    send({op: 'open', id: this.id, url: this.url});
  }
  BridgeEventSource.prototype.addEventListener = function(t, f){ (this._l[t] = this._l[t] || []).push(f); };
  BridgeEventSource.prototype.removeEventListener = function(t, f){ this._l[t] = (this._l[t] || []).filter(function(g){ return g !== f; }); };
  BridgeEventSource.prototype.close = function(){ this.readyState = 2; subs.delete(this.id); send({op: 'close', id: this.id}); };
  BridgeEventSource.prototype._fire = function(t, data){
    var ev = new MessageEvent(t, {data: data}), h = this['on' + t];
    if (h) h.call(this, ev);
    (this._l[t] || []).forEach(function(f){ f.call(this, ev); }, this);
  };
  BridgeEventSource.CONNECTING = 0; BridgeEventSource.OPEN = 1; BridgeEventSource.CLOSED = 2;
  window.EventSource = BridgeEventSource;
  var plain = window.fetch.bind(window);
  window.fetch = function(input, init){
    var body = init && init.body;
    var url = new URL(typeof input === 'string' ? input : input.url, location.href);
    if (typeof body !== 'string' || url.origin !== location.origin) return plain(input, init);
    return new Promise(function(resolve, reject){
      var id = next++, headers = {};
      new Headers(init.headers || {}).forEach(function(v, k){ headers[k] = v; });
      fetches.set(id, {resolve: resolve, reject: reject});
      send({op: 'fetch', id: id, url: url.toString(), method: init.method || 'POST', headers: headers, body: body});
    });
  };
})();</script>"""
