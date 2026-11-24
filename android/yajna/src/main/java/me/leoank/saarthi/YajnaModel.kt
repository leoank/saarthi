package me.leoank.saarthi

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.launch
import uniffi.saarthi_core.NotebookInfo

/** The yajna workspace (ADR-0076, ADR-0077): each machine's notebooks. */
class YajnaModel(val conn: Connection) {
    /** Each node's notebooks, by node id, from its `yajna` service (ADR-0053), and why a
     *  node's did not load. */
    val notebooks = mutableStateMapOf<String, List<NotebookInfo>>()
    val notebookErrors = mutableStateMapOf<String, String>()
    var notebooksLoading by mutableStateOf(0)
        private set

    /** Lists the notebooks of every connected node that offers yajna, each on its own:
     *  a node whose service does not answer does not hold up the others. */
    fun loadNotebooks() {
        val h = conn.connection ?: return
        for (n in conn.nodes.filter { it.connected && NOTEBOOK_SERVICE in it.services }) {
            notebooksLoading++
            conn.scope.launch {
                try {
                    notebooks[n.id] = h.notebooks(n.id)
                    notebookErrors.remove(n.id)
                } catch (e: Exception) {
                    notebookErrors[n.id] = conn.describe(e)
                } finally {
                    notebooksLoading--
                }
            }
        }
    }

    /** The notebooks a new project can start in (ADR-0053): labs, since a project-shaped
     *  notebook is one project, on connected nodes that may spawn in them. */
    fun projectNotebooks(): List<NotebookInfo> = notebooks.values.flatten().filter { b ->
        val n = conn.node(b.nodeId)
        b.shape == "lab" && n != null && n.connected &&
            n.spawnRoots.any { b.path == it || b.path.startsWith(it.trimEnd('/') + "/") }
    }.sortedWith(compareBy({ it.namespace }, { conn.node(it.nodeId)?.name }))

    /** Notebook pages kept while the app runs, most recent last, so going back to one, from
     *  another workspace or the notebook list, finds it where it was instead of loading it
     *  again. At most [PAGES_KEPT]; each holds a web view. */
    private val pages = LinkedHashMap<String, Page>()

    /** The page for a notebook at [url] on [nodeId]: the one kept, else a new one loading. */
    fun page(context: android.content.Context, nodeId: String, url: String): Page {
        val key = "$nodeId $url"
        pages.remove(key)?.let { pages[key] = it; return it }
        val page = Page(context.applicationContext, nodeId, url) { conn.connection }
        pages[key] = page
        // Too many: the one longest unseen goes.
        while (pages.size > PAGES_KEPT) {
            val oldest = pages.entries.firstOrNull { !it.value.shown } ?: break
            pages.remove(oldest.key)
            oldest.value.destroy()
        }
        return page
    }

    private fun dropPages() {
        pages.values.forEach { it.destroy() }
        pages.clear()
    }

    init {
        conn.onDropped { notebooks.clear(); notebookErrors.clear() }
        // Another hub's machines: their pages go.
        conn.onSwitch { dropPages() }
    }

    companion object {
        const val PAGES_KEPT = 3
    }
}

/** A notebook's web view and its relay (ADR-0053), kept apart from the screen showing it.
 *  The view's context is the activity while shown and the application otherwise, so a kept
 *  page never holds on to an activity that has gone. */
class Page(app: android.content.Context, nodeId: String, val url: String, hub: () -> uniffi.saarthi_core.Hub?) {
    val relay = Relay(nodeId, NOTEBOOK_SERVICE, hub)
    private val context = android.content.MutableContextWrapper(app)
    var canGoBack by mutableStateOf(false)
    var loading by mutableStateOf(true)
    var shown = false
        private set
    val view: android.webkit.WebView =
        relay.webView(context, onHistory = { canGoBack = it }, onLoading = { loading = it })
            // Not `url` inside apply: that would be the web view's own, still null.
            .also { it.loadUrl(relay.origin + url) }

    /** Shown in [activity]'s screen; out of any parent it was in. */
    fun show(activity: android.content.Context) {
        (view.parent as? android.view.ViewGroup)?.removeView(view)
        context.baseContext = activity
        shown = true
    }

    fun hide() {
        (view.parent as? android.view.ViewGroup)?.removeView(view)
        context.baseContext = context.applicationContext
        shown = false
    }

    fun destroy() {
        hide()
        relay.close()
        view.destroy()
    }
}
