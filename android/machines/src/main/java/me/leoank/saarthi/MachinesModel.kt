package me.leoank.saarthi

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import uniffi.saarthi_core.HealthPoint
import uniffi.saarthi_core.NodeInfo
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

    fun shell(id: Int): Shell? = shells.firstOrNull { it.id == id }

    fun closeShell(s: Shell) {
        s.close()
        shells.remove(s)
    }

    /** Opens a shell on a node (ADR-0054); null, with the listener told why, when it cannot. */
    suspend fun openTerminal(nodeId: String, cols: Int, rows: Int, widthPx: Int, heightPx: Int, listener: TerminalListener): TerminalHandle? {
        val h = conn.connection ?: run {
            listener.onEnded("Not connected to the hub")
            return null
        }
        return try {
            h.terminal(nodeId, cols.toUInt(), rows.toUInt(), widthPx.toUInt(), heightPx.toUInt(), listener)
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            listener.onEnded(conn.describe(e))
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
    }
}
