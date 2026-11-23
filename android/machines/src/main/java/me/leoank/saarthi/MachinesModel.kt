package me.leoank.saarthi

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import uniffi.saarthi_core.HealthPoint
import kotlinx.coroutines.launch
import uniffi.saarthi_core.NodeInfo
import uniffi.saarthi_core.ShellInfo
import uniffi.saarthi_core.TerminalHandle
import uniffi.saarthi_core.TerminalListener

/** The Machines workspace (ADR-0076, ADR-0077): the open shells, a machine's health, its
 *  keep-awake, and the terminal's key bar. */
class MachinesModel(val conn: Connection) {
    private val prefs = conn.prefs

    /** Open shells, kept while the app runs (ADR-0068), oldest first. */
    val shells = mutableStateListOf<Shell>()
    private var shellIds = 0

    /** A new shell on a node; it opens once its screen knows its size. */
    fun newShell(node: NodeInfo): Shell = Shell(++shellIds, node.id, node.name).also { shells.add(it) }

    /** The shells each machine's holder keeps (ADR-0078), by node id, as last listed. */
    val held = mutableStateMapOf<String, List<ShellInfo>>()

    /** Lists the shells of every connected machine that offers them. */
    fun loadHeld() {
        val h = conn.connection ?: return
        for (n in conn.nodes.filter { it.connected && it.terminal }) {
            conn.scope.launch {
                runCatching { h.shells(n.id) }.onSuccess { held[n.id] = it }
            }
        }
    }

    /** A shell the holder keeps, opened on this phone: the one already open here, else a new
     *  attachment to it. */
    fun attach(node: NodeInfo, info: ShellInfo): Shell =
        shells.firstOrNull { it.nodeId == node.id && it.shellId == info.id }
            ?: Shell(++shellIds, node.id, node.name, info.id).also { shells.add(it) }

    /** Ends a shell for good (ADR-0078): every client on it is told, here or elsewhere. */
    fun killShell(nodeId: String, shellId: String) = conn.withHub {
        it.killShell(nodeId, shellId)
        held[nodeId] = held[nodeId].orEmpty().filter { s -> s.id != shellId }
    }

    fun shell(id: Int): Shell? = shells.firstOrNull { it.id == id }

    /** Leaves a shell on this phone; it goes on in its machine's holder (ADR-0078). */
    fun closeShell(s: Shell) {
        s.close()
        shells.remove(s)
    }

    /** Opens a shell on a node (ADR-0054), a new one or [shellId] its holder keeps (ADR-0078);
     *  null, with the listener told why, when it cannot. */
    suspend fun openTerminal(nodeId: String, shellId: String, cols: Int, rows: Int, widthPx: Int, heightPx: Int, listener: TerminalListener): TerminalHandle? {
        // A held shell that cannot be reached now is only dropped: it goes on, to attach later.
        val gone: (String) -> Unit = if (shellId.isEmpty()) listener::onEnded else listener::onDropped
        val h = conn.connection ?: run {
            gone("Not connected to the hub")
            return null
        }
        return try {
            h.terminal(nodeId, shellId, cols.toUInt(), rows.toUInt(), widthPx.toUInt(), heightPx.toUInt(), listener)
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            gone(conn.describe(e))
            null
        }
    }

    /** The terminal's key bar, by key id, kept on this device (phase PHS-20261117-f22kxe
     *  item 11). */
    var keyBar by mutableStateOf(prefs.getString("key_bar", null)?.split(",")?.filter { it.isNotEmpty() } ?: KeyBar.default)
        private set

    fun changeKeyBar(ids: List<String>) {
        keyBar = ids
        prefs.edit().putString("key_bar", ids.joinToString(",")).apply()
    }

    val health = mutableStateListOf<HealthPoint>()
    var healthLoading by mutableStateOf(false)

    fun loadHealth(nodeId: String, sinceMs: Long) = conn.withHub {
        healthLoading = true
        try {
            health.replaceAll(it.health(nodeId, sinceMs))
        } finally {
            healthLoading = false
        }
    }

    /** Switches a node's keep-awake hold (ADR-0031); health reports it from the next
     *  sample, so the health screen shows the switch as set until then. */
    fun setKeepAwake(nodeId: String, on: Boolean) = conn.withHub {
        conn.done(it.setKeepAwake(nodeId, on), if (on) "Keep awake on: the machine stays awake while a session works" else "Keep awake off: the machine may sleep")
    }

    fun closeAll() = shells.forEach { it.close() }

    init {
        // Shells are the old hub's.
        conn.onSwitch { shells.toList().forEach(::closeShell) }
        conn.onDropped { held.clear() }
    }
}
