package me.leoank.relayspike

// EXP-20261114-raqjjy: can a WebView be fed from the relay with streaming kept?
// mode=intercept: pages at http://yajna.relay/ are answered by shouldInterceptRequest from
//   a stream (here an HTTP connection to the relay's proxy on the host, standing in for
//   the core's ServiceRequest stream).
// mode=loopback: the WebView loads the proxy's port directly, as it would a port the core
//   serves on the phone.
import android.app.Activity
import android.os.Bundle
import android.util.Log
import android.webkit.ConsoleMessage
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import java.net.HttpURLConnection
import java.net.URL

class MainActivity : Activity() {
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
        if (mode == "intercept") {
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
                    val body = if (status >= 400) c.errorStream else c.inputStream
                    Log.i("SPIKE", "intercepted ${req.url.encodedPath} $status $mime")
                    return WebResourceResponse(mime, charset, status, c.responseMessage ?: "OK", headers, body)
                }
            }
            wv.loadUrl("http://yajna.relay$path")
        } else {
            wv.webViewClient = WebViewClient()
            wv.loadUrl(upstream + path)
        }
    }
}
