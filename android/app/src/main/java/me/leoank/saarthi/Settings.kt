package me.leoank.saarthi

import android.os.Build
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import uniffi.saarthi_core.coreVersion

/** The app's settings, a page of groups: the hubs it knows and which one it connects to,
 *  how text shows, and what is running. New kinds of settings get a group here. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(model: HubModel, snackbar: SnackbarHostState, back: (() -> Unit)?) {
    var editing by remember { mutableStateOf<HubProfile?>(null) }
    var adding by remember { mutableStateOf(model.hubs.isEmpty()) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = {
                    if (back != null) IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
        ) {
            Heading("Hubs")
            Group {
                for (h in model.hubs) {
                    HubRow(model, h, edit = { editing = h })
                    Divider()
                }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { adding = true }
                        .padding(horizontal = 16.dp, vertical = 14.dp),
                ) {
                    Icon(Icons.Filled.Add, null, tint = MaterialTheme.colorScheme.primary)
                    Text("Add a hub", style = MaterialTheme.typography.bodyLarge, modifier = Modifier.padding(start = 16.dp))
                }
            }
            Text(
                "The app connects to the selected hub: through a tailnet device of its own when the hub " +
                    "has an auth key, otherwise through the phone's own network, such as the Tailscale app.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = 4.dp, end = 4.dp, top = 8.dp),
            )

            Heading("Display")
            Group {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { model.toggleWrap() }
                        .padding(start = 16.dp, end = 12.dp, top = 8.dp, bottom = 8.dp),
                ) {
                    Column(Modifier.weight(1f)) {
                        Text("Wrap long lines", style = MaterialTheme.typography.bodyLarge)
                        Text(
                            "Code, diffs and tool output wrap instead of scrolling sideways",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Switch(checked = model.wrap, onCheckedChange = { model.toggleWrap() })
                }
            }

            Heading("About")
            val context = LocalContext.current
            val pkg = remember { context.packageManager.getPackageInfo(context.packageName, 0) }
            val core = remember { runCatching { coreVersion() }.getOrDefault("not loaded") }
            Group {
                Fact("App", "saarthi ${pkg.versionName}")
                Divider()
                Fact("Core", "saarthi-core $core")
                Divider()
                Fact("Phone", "${Build.MANUFACTURER} ${Build.MODEL}, Android ${Build.VERSION.RELEASE}")
            }
            Spacer(Modifier.height(32.dp))
        }
    }
    if (adding || editing != null) {
        HubEditor(
            initial = editing,
            first = model.hubs.isEmpty(),
            dismiss = { adding = false; editing = null },
            save = { model.saveHub(it); adding = false; editing = null },
            delete = editing?.let { h -> { model.removeHub(h.id); editing = null } },
        )
    }
}

@Composable
private fun HubRow(model: HubModel, h: HubProfile, edit: () -> Unit) {
    val active = h.id == model.activeId
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .clickable(enabled = !active) { model.activate(h.id) }
            .padding(start = 4.dp, end = 4.dp, top = 6.dp, bottom = 6.dp),
    ) {
        RadioButton(selected = active, onClick = { if (!active) model.activate(h.id) })
        Column(Modifier.weight(1f).padding(start = 4.dp)) {
            Text(h.title, style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                h.address.trim(),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (active) {
                val (text, bad) = when (model.link) {
                    Link.Connected -> "Connected" to false
                    Link.Connecting -> "Connecting…" to false
                    Link.Reconnecting -> "Reconnecting…" to true
                    Link.Disconnected -> (model.connectError ?: "Not connected") to true
                }
                Text(
                    text,
                    style = MaterialTheme.typography.bodySmall,
                    color = if (bad) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
                )
            }
        }
        IconButton(onClick = edit) { Icon(Icons.Filled.Edit, "Edit ${h.title}") }
    }
}

@Composable
private fun Fact(label: String, value: String) {
    Row(
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp),
    ) {
        Text(label, style = MaterialTheme.typography.bodyLarge)
        Text(
            value,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(start = 16.dp),
        )
    }
}

/** Adds a hub or edits one: its name, address and client token. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun HubEditor(
    initial: HubProfile?,
    first: Boolean,
    dismiss: () -> Unit,
    save: (HubProfile) -> Unit,
    delete: (() -> Unit)?,
) {
    var name by remember { mutableStateOf(initial?.name ?: "") }
    var address by remember { mutableStateOf(initial?.address ?: "") }
    var token by remember { mutableStateOf(initial?.token ?: "") }
    var showToken by remember { mutableStateOf(false) }
    var tailnetKey by remember { mutableStateOf(initial?.tailnetKey ?: "") }
    var tailnetHost by remember { mutableStateOf(initial?.tailnetHost ?: "") }
    var showKey by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    val draft = HubProfile(
        id = initial?.id ?: java.util.UUID.randomUUID().toString(), name = name.trim(), address = address.trim(), token = token.trim(),
        tailnetKey = tailnetKey.trim(), tailnetHost = tailnetHost.trim(),
    )

    Dialog(
        onDismissRequest = dismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Text(if (initial == null) "Add a hub" else "Edit hub") },
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
                        enabled = address.isNotBlank() && token.isNotBlank(),
                        onClick = {
                            val a = draft.address
                            save(draft.copy(address = if ("://" in a) a else "http://$a"))
                        },
                        contentPadding = PaddingValues(vertical = 14.dp),
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(if (initial == null && first) "Save and connect" else "Save") }
                }
            },
        ) { padding ->
            Column(
                Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Spacer(Modifier.size(4.dp))
                OutlinedTextField(
                    address, { address = it },
                    label = { Text("Address") },
                    placeholder = { Text("http://hub:7700") },
                    supportingText = { Text("The hub's control service, as this phone reaches it") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                    modifier = Modifier.fillMaxWidth(),
                )
                OutlinedTextField(
                    token, { token = it },
                    label = { Text("Client token") },
                    supportingText = { Text("A line of the hub's client-token file") },
                    singleLine = true,
                    visualTransformation = if (showToken) VisualTransformation.None else PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                    trailingIcon = {
                        IconButton(onClick = { showToken = !showToken }) {
                            Icon(
                                if (showToken) Icons.Filled.VisibilityOff else Icons.Filled.Visibility,
                                if (showToken) "Hide token" else "Show token",
                            )
                        }
                    },
                    modifier = Modifier.fillMaxWidth(),
                )
                OutlinedTextField(
                    name, { name = it },
                    label = { Text("Name (optional)") },
                    placeholder = { Text(draft.host.ifBlank { "home" }) },
                    supportingText = { Text("What the app calls it; the address's host without one") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                Heading("Tailnet")
                OutlinedTextField(
                    tailnetKey, { tailnetKey = it },
                    label = { Text("Auth key (optional)") },
                    supportingText = {
                        Text(
                            "With a key the app joins the hub's tailnet as a device of its own, whatever " +
                                "network the Tailscale app is on; without one it uses the phone's network. " +
                                "Kept on this phone, encrypted."
                        )
                    },
                    singleLine = true,
                    visualTransformation = if (showKey) VisualTransformation.None else PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                    trailingIcon = {
                        IconButton(onClick = { showKey = !showKey }) {
                            Icon(
                                if (showKey) Icons.Filled.VisibilityOff else Icons.Filled.Visibility,
                                if (showKey) "Hide key" else "Show key",
                            )
                        }
                    },
                    modifier = Modifier.fillMaxWidth(),
                )
                if (tailnetKey.isNotBlank()) {
                    OutlinedTextField(
                        tailnetHost, { tailnetHost = it },
                        label = { Text("Device name (optional)") },
                        placeholder = { Text(HubProfile.defaultTailnetHost()) },
                        supportingText = { Text("This phone's machine name on the hub's tailnet") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
                if (delete != null) {
                    TextButton(onClick = { confirmDelete = true }) {
                        Text("Remove this hub", color = MaterialTheme.colorScheme.error)
                    }
                }
                Spacer(Modifier.height(24.dp))
            }
        }
    }
    if (confirmDelete && delete != null) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Remove ${initial?.title}?") },
            text = {
                Text(
                    "Its address, token and tailnet identity are forgotten on this phone. The hub itself is " +
                        "not touched; a tailnet device stays in the tailnet until you remove it there."
                )
            },
            confirmButton = {
                TextButton(onClick = { confirmDelete = false; delete() }) {
                    Text("Remove", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Cancel") } },
        )
    }
}
