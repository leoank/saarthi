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

/** Every node's notebooks, from its `yajna` service (ADR-0053). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NotebooksScreen(
    model: YajnaModel,
    snackbar: SnackbarHostState,
    open: (NotebookInfo) -> Unit,
    spawned: (nodeId: String, sessionId: String) -> Unit,
) {
    LaunchedEffect(model.conn.link) { if (model.conn.link == Link.Connected) model.loadNotebooks() }
    val offering = model.conn.nodes.filter { NOTEBOOK_SERVICE in it.services }.sortedBy { it.name }
    val labs = model.projectNotebooks()
    var newProject by remember { mutableStateOf(false) }
    if (newProject && labs.isNotEmpty()) {
        NewProjectDialog(model, labs, dismiss = { newProject = false }) { book, prompt, chosen, token ->
            newProject = false
            model.conn.spawn(book.nodeId, book.path, prompt, chosen, false, "", "", token) { id -> spawned(book.nodeId, id) }
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    val n = model.conn.nodes.count { NOTEBOOK_SERVICE in it.services }
                    PromptLine(model.conn.hubName, "~/yajna", if (n == 1) "▤ 1 machine with notebooks" else "▤ $n machines with notebooks", tailnet = model.conn.tailnet)
                },
                actions = {
                    IconButton(onClick = { model.loadNotebooks() }) { Icon(Icons.Filled.Refresh, "Refresh") }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
        floatingActionButton = {
            if (labs.isNotEmpty()) {
                ActionButton("new project", onClick = { newProject = true })
            }
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            if (model.notebooksLoading > 0) LinearProgressIndicator(Modifier.fillMaxWidth())
            LazyColumn(
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                if (offering.isEmpty()) {
                    item(key = "none") {
                        Empty(
                            "No notebooks",
                            "A node offers its machine's yajna notebooks when its services file lists yajna (see the node guide).",
                        )
                    }
                }
                for (node in offering) {
                    item(key = "n" + node.id) { SectionTitle(node.name) }
                    val books = model.notebooks[node.id]
                    val error = model.notebookErrors[node.id]
                    when {
                        !node.connected -> item(key = "o" + node.id) { Caption("Offline, last seen ${relative(node.lastSeenMs)}") }
                        error != null -> item(key = "x" + node.id) { Caption(error, color = MaterialTheme.colorScheme.error) }
                        books == null -> item(key = "l" + node.id) { Caption("Loading…") }
                        books.isEmpty() -> item(key = "e" + node.id) { Caption("No notebooks registered on this machine") }
                        else -> items(books, key = { "b" + node.id + it.namespace }) { b -> NotebookCard(b) { open(b) } }
                    }
                }
            }
        }
    }
}

/** The yajna commands that start a project, by the kind of project (yajna's plugin). */
private val projectKinds = listOf(
    Triple("General", "project", "A question to study, with its first experiments"),
    Triple("Software", "code-project", "Code with its design, decisions and first spikes"),
    Triple("Wet lab", "bio-project", "System, readouts, protocols, materials and safety"),
    Triple("Machine learning", "ml-project", "Data, baseline, metric and the sweep plan"),
    Triple("Hardware", "hardware-project", "A build: parts, design sources and characterization"),
)

/** A new project in a lab notebook (ADR-0053, ADR-0060): a session spawned in the notebook
 *  on `/yajna:<kind> <brief>`, which reads the notebook and then interviews the person, its
 *  questions held as question cards through the node's question tool (ADR-0061). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun NewProjectDialog(
    model: YajnaModel,
    labs: List<NotebookInfo>,
    dismiss: () -> Unit,
    start: (book: NotebookInfo, prompt: String, model: String, token: String?) -> Unit,
) {
    val key = { b: NotebookInfo -> b.nodeId + "/" + b.namespace }
    var bookKey by remember { mutableStateOf(key(labs.first())) }
    val book = labs.firstOrNull { key(it) == bookKey } ?: labs.first()
    val node = model.conn.node(book.nodeId)
    var kind by remember { mutableStateOf(projectKinds.first().second) }
    var brief by remember { mutableStateOf("") }
    var chosen by remember { mutableStateOf("") }
    var token by remember(book.nodeId) { mutableStateOf<String?>(null) }
    val effective = token ?: node?.defaultClaudeToken.orEmpty()
    Dialog(
        onDismissRequest = dismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Text("New project") },
                    navigationIcon = { IconButton(onClick = dismiss) { Icon(Icons.Filled.Close, "Cancel") } },
                )
            },
            bottomBar = {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .background(MaterialTheme.colorScheme.surface)
                        .navigationBarsPadding()
                        .imePadding()
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                ) {
                    Button(
                        enabled = brief.isNotBlank(),
                        onClick = {
                            // One line: the command takes the rest of the line as its brief.
                            val text = brief.trim().replace(Regex("\\s+"), " ")
                            start(book, "/yajna:$kind $text", chosen, token)
                        },
                        contentPadding = PaddingValues(vertical = 14.dp),
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text("Start the interview") }
                }
            },
        ) { padding ->
            Column(
                Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp),
            ) {
                OutlinedTextField(
                    value = brief,
                    onValueChange = { brief = it },
                    minLines = 4,
                    maxLines = 10,
                    placeholder = { Text("What is the project about?") },
                    supportingText = { Text("A sentence or two; the interview asks for the rest.") },
                    shape = RoundedCornerShape(16.dp),
                    colors = OutlinedTextFieldDefaults.colors(
                        unfocusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
                        focusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
                        unfocusedBorderColor = MaterialTheme.colorScheme.surfaceContainerLow,
                    ),
                    modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
                )
                Heading("Where")
                Group {
                    Choice(
                        "Notebook",
                        book.namespace,
                        labs.map { "${it.namespace} on ${model.conn.node(it.nodeId)?.name ?: "?"}" to key(it) },
                        bookKey,
                        details = labs.associate { key(it) to short(it.path) },
                    ) { bookKey = it }
                }
                Heading("Kind")
                Group {
                    Choice(
                        "Project",
                        projectKinds.first { it.second == kind }.first,
                        projectKinds.map { it.first to it.second },
                        kind,
                        details = projectKinds.associate { it.second to it.third },
                    ) { kind = it }
                }
                Heading("Options")
                Group {
                    Choice("Model", models.first { it.second == chosen }.first, models, chosen) { chosen = it }
                    if (model.conn.claudeTokens.isNotEmpty() && node != null) {
                        Divider()
                        val default = "Node default (${node.defaultClaudeToken.ifEmpty { "own login" }})"
                        val options = listOf(default to "\u0000") + model.conn.claudeTokens.map { it.name to it.name } +
                            if (node.defaultClaudeToken.isEmpty()) emptyList() else listOf("Node's own login" to "")
                        Choice(
                            "Claude account",
                            when (val t = token) {
                                null -> node.defaultClaudeToken.ifEmpty { "Node's login" }
                                "" -> "Node's login"
                                else -> t
                            },
                            options,
                            token ?: "\u0000",
                        ) { token = if (it == "\u0000") null else it }
                    }
                }
                Text(
                    "Claude reads ${book.namespace} first, then interviews you about the project one " +
                        "question at a time, as questions to answer here; nothing is created " +
                        "until you agree. Runs on ${node?.name ?: "the node"}" +
                        (if (effective.isNotEmpty()) " under the $effective account." else " with its own Claude login."),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 12.dp, start = 4.dp, end = 4.dp),
                )
                Spacer(Modifier.height(24.dp))
            }
        }
    }
}

@Composable
private fun NotebookCard(b: NotebookInfo, onOpen: () -> Unit) {
    Card(
        Modifier.fillMaxWidth().clickable(onClick = onOpen),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Column(Modifier.padding(horizontal = 16.dp, vertical = 12.dp)) {
            Text(b.namespace, style = MaterialTheme.typography.titleMedium)
            Text(
                listOf(b.shape, b.path).filter { it.isNotEmpty() }.joinToString(" · "),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.StartEllipsis,
            )
        }
    }
}

/** One notebook's pages, as its node's `yajna serve` renders them, in a web view fed
 *  through the hub (ADR-0056). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NotebookScreen(
    model: YajnaModel,
    snackbar: SnackbarHostState,
    nodeId: String,
    namespace: String,
    url: String,
    back: () -> Unit,
) {
    val context = LocalContext.current
    var canGoBack by remember { mutableStateOf(false) }
    var loading by remember { mutableStateOf(true) }
    val relay = remember(nodeId) { Relay(nodeId, NOTEBOOK_SERVICE) { model.conn.connection } }
    val view = remember(nodeId, url) {
        relay.webView(
            context,
            onHistory = { canGoBack = it },
            onLoading = { loading = it },
        ).apply { loadUrl(relay.origin + url) }
    }
    DisposableEffect(view) {
        onDispose {
            relay.close()
            view.destroy()
        }
    }
    BackHandler(enabled = canGoBack) { view.goBack() }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text(namespace)
                        Text(
                            model.conn.node(nodeId)?.name ?: "",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                },
                navigationIcon = { IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") } },
                actions = {
                    IconButton(onClick = { view.loadUrl(relay.origin + url + "files/") }) {
                        Icon(Icons.Filled.Folder, "Files")
                    }
                    IconButton(onClick = { view.reload() }) { Icon(Icons.Filled.Refresh, "Reload") }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            if (loading) LinearProgressIndicator(Modifier.fillMaxWidth())
            AndroidView(factory = { view }, modifier = Modifier.fillMaxSize())
        }
    }
}
