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

    init {
        conn.onDropped { notebooks.clear(); notebookErrors.clear() }
    }
}
