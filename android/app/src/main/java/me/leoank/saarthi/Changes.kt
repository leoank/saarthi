package me.leoank.saarthi

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.viewinterop.AndroidView

/** What a session changed (ADR-0067): yajna's changes view over the session's worktree,
 *  served by its node as `changes` and shown in the web view the notebooks use; files open
 *  from it in yajna's viewers. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChangesScreen(model: HubModel, nodeId: String, sessionId: String, back: () -> Unit) {
    val context = LocalContext.current
    var canGoBack by remember { mutableStateOf(false) }
    var loading by remember { mutableStateOf(true) }
    val relay = remember(nodeId) { Relay(nodeId, CHANGES_SERVICE) { model.connection } }
    val view = remember(nodeId, sessionId) {
        relay.webView(
            context,
            onHistory = { canGoBack = it },
            onLoading = { loading = it },
        ).apply { loadUrl("${relay.origin}/s/$sessionId/files/changes") }
    }
    DisposableEffect(view) {
        onDispose {
            relay.close()
            view.destroy()
        }
    }
    BackHandler(enabled = canGoBack) { view.goBack() }
    val session = model.sessions.firstOrNull { it.nodeId == nodeId && it.id == sessionId }
        ?: model.openSession?.takeIf { it.id == sessionId }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text("Changes")
                        Text(
                            session?.title() ?: (model.node(nodeId)?.name ?: ""),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                },
                navigationIcon = { IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") } },
                actions = { IconButton(onClick = { view.reload() }) { Icon(Icons.Filled.Refresh, "Reload") } },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            if (loading) LinearProgressIndicator(Modifier.fillMaxWidth())
            AndroidView(factory = { view }, modifier = Modifier.fillMaxSize())
        }
    }
}
