package me.leoank.relayspike

// EXP-20261114-raqjjy: can a WebView be fed from the relay with streaming kept?
// mode=intercept: pages at http://yajna.relay/ are answered by shouldInterceptRequest from
//   a stream (here an HTTP connection to the relay's proxy on the host, standing in for
//   the core's ServiceRequest stream).
// mode=loopback: the WebView loads the proxy's port directly, as it would a port the core
//   serves on the phone.
// mode=bridge: as intercept, but every HTML page gets a script at its top that replaces
//   EventSource with one fed over a WebMessagePort; the app reads the event stream itself
//   and posts each event to the page. The page's own code is unchanged.
import android.app.Activity
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.webkit.ConsoleMessage
import android.webkit.WebChromeClient
import android.webkit.WebMessage
import android.webkit.WebMessagePort
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import org.json.JSONObject
import java.io.ByteArrayInputStream
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ConcurrentHashMap

/** Replaces EventSource; the app hands it a port with the message "saarthi-port". */
const val SHIM = """<script>(function(){
  var port = null, queue = [], subs = new Map(), next = 1;
  function send(m){ m = JSON.stringify(m); if (port) port.postMessage(m); else queue.push(m); }
  window.addEventListener('message', function(e){
    if (e.data !== 'saarthi-port' || !e.ports || !e.ports[0]) return;
    port = e.ports[0];
    port.onmessage = function(m){
      var d = JSON.parse(m.data), s = subs.get(d.id); if (!s) return;
      if (d.open) { s.readyState = 1; s._fire('open', null); }
      else if (d.error) { s._fire('error', null); }
      else s._fire(d.event || 'message', d.data);
    };
    queue.splice(0).forEach(function(q){ port.postMessage(q); });
  });
  function BridgeEventSource(url){
    this.url = new URL(url, location.href).toString(); this.readyState = 0;
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
})();</script>"""

class MainActivity : Activity() {
    private val main = Handler(Looper.getMainLooper())
    private val streams = ConcurrentHashMap<Int, HttpURLConnection>()

    override fun onCreate(b: Bundle?) {
        super.onCreate(b)
        val mode = intent.getStringExtra("mode") ?: "loopback"
        val path = intent.getStringExtra("path") ?: "/__sse.html"
        val upstream = intent.getStringExtra("upstream") ?: "http://10.0.2.2:18767"
        val wv = WebView(this)
        setContentView(wv)
        wv.settings.javaScriptEnabled = true
        wv.settings.domStorageEnabled = true
        wv.webChromeClient = object : WebChromeClient() {
            override fun onConsoleMessage(m: ConsoleMessage): Boolean {
                Log.i("SPIKE", m.message())
                return true
            }
        }
        Log.i("SPIKE", "mode $mode path $path")
        if (mode == "intercept" || mode == "bridge") {
            val bridge = mode == "bridge"
            wv.webViewClient = object : WebViewClient() {
                override fun shouldInterceptRequest(view: WebView, req: WebResourceRequest): WebResourceResponse? {
                    if (req.url.host != "yajna.relay") return null
                    val q = req.url.encodedQuery?.let { "?$it" } ?: ""
                    val c = URL(upstream + req.url.encodedPath + q).openConnection() as HttpURLConnection
                    req.requestHeaders.forEach { (k, v) -> if (!k.equals("host", true)) c.setRequestProperty(k, v) }
                    val status = c.responseCode
                    val type = c.contentType ?: "application/octet-stream"
                    val mime = type.substringBefore(';').trim()
                    val charset = type.substringAfter("charset=", "utf-8").trim()
                    val headers = c.headerFields.filterKeys { it != null }.mapValues { it.value.joinToString(",") }
                    var body = if (status >= 400) c.errorStream else c.inputStream
                    var outHeaders = headers
                    if (bridge && mime == "text/html") {
                        // The shim goes first, before any of the page's own scripts.
                        val html = body.readBytes().toString(Charsets.UTF_8)
                        val at = Regex("<head[^>]*>", RegexOption.IGNORE_CASE).find(html)?.range?.last?.plus(1) ?: 0
                        body = ByteArrayInputStream((html.substring(0, at) + SHIM + html.substring(at)).toByteArray())
                        outHeaders = headers.filterKeys { !it.equals("content-length", true) }
                    }
                    Log.i("SPIKE", "intercepted ${req.url.encodedPath} $status $mime")
                    return WebResourceResponse(mime, charset, status, c.responseMessage ?: "OK", outHeaders, body)
                }

                override fun onPageFinished(view: WebView, url: String) {
                    if (bridge) givePort(view, upstream)
                }
            }
            wv.loadUrl("http://yajna.relay$path")
        } else {
            wv.webViewClient = WebViewClient()
            wv.loadUrl(upstream + path)
        }
    }

    /** One channel per page: the page's end goes to it, ours reads its requests. */
    private fun givePort(view: WebView, upstream: String) {
        val (ours, theirs) = view.createWebMessageChannel()
        ours.setWebMessageCallback(object : WebMessagePort.WebMessageCallback() {
            override fun onMessage(port: WebMessagePort, message: WebMessage) {
                val m = JSONObject(message.data)
                val id = m.getInt("id")
                when (m.getString("op")) {
                    "open" -> Thread { stream(ours, id, m.getString("url"), upstream) }.start()
                    "close" -> streams.remove(id)?.disconnect()
                }
            }
        })
        view.postWebMessage(WebMessage("saarthi-port", arrayOf(theirs)), Uri.parse("http://yajna.relay"))
        Log.i("SPIKE", "bridge port given")
    }

    /** Reads one event stream and posts each event to the page as it arrives. */
    private fun stream(port: WebMessagePort, id: Int, url: String, upstream: String) {
        val u = Uri.parse(url)
        val q = u.encodedQuery?.let { "?$it" } ?: ""
        val c = URL(upstream + u.encodedPath + q).openConnection() as HttpURLConnection
        c.setRequestProperty("Accept", "text/event-stream")
        streams[id] = c
        fun post(o: JSONObject) {
            o.put("id", id)
            val s = o.toString()
            main.post { port.postMessage(WebMessage(s)) }
        }
        try {
            if (c.responseCode != 200) { post(JSONObject().put("error", true)); return }
            post(JSONObject().put("open", true))
            Log.i("SPIKE", "bridge stream $id open ${u.encodedPath}")
            var event = "message"
            val data = StringBuilder()
            c.inputStream.bufferedReader().forEachLine { line ->
                when {
                    line.isEmpty() -> {
                        if (data.isNotEmpty()) post(JSONObject().put("event", event).put("data", data.toString()))
                        event = "message"; data.setLength(0)
                    }
                    line.startsWith("event:") -> event = line.substring(6).trim()
                    line.startsWith("data:") -> {
                        if (data.isNotEmpty()) data.append('\n')
                        data.append(line.substring(5).removePrefix(" "))
                    }
                }
            }
        } catch (e: Exception) {
            Log.i("SPIKE", "bridge stream $id ended: $e")
            post(JSONObject().put("error", true))
        }
    }
}
