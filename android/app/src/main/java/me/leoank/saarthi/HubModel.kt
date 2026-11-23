package me.leoank.saarthi

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.launch

/** The app's state (ADR-0077): the shared connection to the hub and one model per
 *  workspace on it. Each workspace's screens take their own model, which reaches the
 *  connection, and never another workspace's. */
class HubModel(app: Application) : AndroidViewModel(app) {
    val conn = Connection(app, viewModelScope)
    val agents = AgentsModel(conn)
    val yajna = YajnaModel(conn)
    val machines = MachinesModel(conn)

    init {
        viewModelScope.launch { conn.bootLog.boot(conn, app) }
    }

    override fun onCleared() {
        machines.closeAll()
        agents.closeAll()
        conn.close()
    }
}
