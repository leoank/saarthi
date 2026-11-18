package me.leoank.saarthi

import android.content.Context
import android.graphics.Paint
import android.graphics.Typeface
import android.text.InputType
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import androidx.activity.findViewTreeOnBackPressedDispatcherOwner
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import uniffi.saarthi_core.TerminalHandle
import uniffi.saarthi_core.TerminalListener

/** An open shell (ADR-0054), kept in the model while the app runs (ADR-0068): its
 *  libghostty-vt state and its stream, which outlive the screen showing it. Output arrives
 *  on the core's thread and close comes from the main one; the lock keeps the terminal from
 *  being fed after it is freed. */
class Shell(val id: Int, val nodeId: String, val nodeName: String) {
    val startedMs = System.currentTimeMillis()
    /** The libghostty-vt terminal; 0 until the screen first knows its size. */
    @Volatile var vt = 0L
        private set
    @Volatile var handle: TerminalHandle? = null
        private set
    var cols by mutableIntStateOf(0)
        private set
    var rows by mutableIntStateOf(0)
        private set
    /** Why the shell ended, once it has; it stays listed until closed. */
    var ended by mutableStateOf<String?>(null)
    /** Bumped by *Open again*, which starts a new shell in its place. */
    var generation by mutableIntStateOf(0)
        private set

    /** Opens the shell at a size, once: the terminal first, so no output is lost. */
    suspend fun open(model: HubModel, cols: Int, rows: Int, cellW: Int, cellH: Int) {
        if (vt != 0L) return
        val v = Vt.create(cols, rows, cellW, cellH)
        synchronized(this) { vt = v }
        this.cols = cols
        this.rows = rows
        val h = model.openTerminal(nodeId, cols, rows, object : TerminalListener {
            override fun onOutput(bytes: ByteArray) {
                synchronized(this@Shell) {
                    if (vt != v) return
                    Vt.feed(v, bytes)
                    reply()
                }
            }

            override fun onEnded(reason: String) {
                ended = reason
            }
        })
        synchronized(this) {
            handle = h
            // Replies owed before the stream was ours.
            reply()
        }
    }

    /** What the terminal owes the program goes back at once, shown or not; it waits in the
     *  terminal until there is a stream to send it on. */
    private fun reply() {
        val h = handle ?: return
        if (vt != 0L) Vt.replies(vt)?.let { h.input(it) }
    }

    fun resize(cols: Int, rows: Int, cellW: Int, cellH: Int) {
        if (cols == this.cols && rows == this.rows) return
        synchronized(this) {
            if (vt == 0L) return
            Vt.resize(vt, cols, rows, cellW, cellH)
        }
        this.cols = cols
        this.rows = rows
        handle?.resize(cols.toUInt(), rows.toUInt())
    }

    fun input(bytes: ByteArray) = handle?.input(bytes)

    /** Copies the screen out for drawing; false when nothing changed. */
    fun snapshot(cells: IntArray, meta: IntArray, force: Boolean): Boolean = synchronized(this) {
        vt != 0L && Vt.snapshot(vt, cells, meta, force)
    }

    /** A new shell in place of an ended one. */
    fun reopen() {
        close()
        ended = null
        generation++
    }

    fun close() {
        handle?.hangUp()
        handle = null
        synchronized(this) {
            if (vt != 0L) Vt.free(vt)
            vt = 0L
        }
    }
}

/** A shell on a node (ADR-0054): the screen libghostty-vt keeps, drawn on a canvas; typing
 *  goes straight to the shell, and a bar of the keys a phone keyboard lacks, which the owner
 *  can edit, sits under it. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TerminalScreen(model: HubModel, shell: Shell, back: () -> Unit, close: () -> Unit) {
    var held by remember { mutableStateOf(emptySet<String>()) }
    var editing by remember { mutableStateOf(false) }
    var keys by remember { mutableStateOf<KeyView?>(null) }
    fun send(bytes: ByteArray) = shell.input(bytes)
    fun send(s: String) = send(s.toByteArray())
    // A key typed or tapped, with what Ctrl and Alt held for it; either lets go after it.
    fun key(s: String) {
        var out = s
        if (KeyBar.CTRL in held && s.length == 1 && (s[0].isLetter() || s[0] in "@[\\]^_ ")) {
            out = (if (s[0] == ' ') 0 else s[0].uppercaseChar().code and 0x1f).toChar().toString()
        }
        if (KeyBar.ALT in held) out = "\u001b" + out
        held = emptySet()
        send(out)
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(shell.nodeName) },
                navigationIcon = { IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") } },
                actions = { IconButton(onClick = close) { Icon(Icons.Filled.Close, "Close this shell") } },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).navigationBarsPadding().imePadding()) {
            shell.ended?.let { why ->
                Surface(color = MaterialTheme.colorScheme.surfaceVariant, modifier = Modifier.fillMaxWidth()) {
                    Row(Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) {
                        Text(why, style = MaterialTheme.typography.bodySmall, modifier = Modifier.weight(1f).padding(top = 12.dp))
                        TextButton(onClick = { shell.reopen() }) { Text("Open again") }
                    }
                }
            }
            Box(
                Modifier.weight(1f).fillMaxWidth().clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                ) { keys?.show() },
            ) {
                TerminalCanvas(model, shell)
                KeyInput({ keys = it }) { key(it) }
            }
            // The keys a phone keyboard lacks, as the owner arranged them.
            KeyBarRow(
                keys = KeyBar.keys(model.keyBar),
                held = held,
                press = { k ->
                    if (k.sends.isEmpty()) held = if (k.id in held) held - k.id else held + k.id else key(k.sends)
                },
                edit = { editing = true },
            )
        }
    }
    if (editing) KeyBarEditor(model.keyBar, model::changeKeyBar) { editing = false }
    LaunchedEffect(keys) { keys?.show() }
}

/** What the soft keyboard types, as a terminal takes it. A text field will not do: the
 *  keyboard keeps its own copy of the text, and resetting the field under it gets keys
 *  committed twice. With `TYPE_NULL` the keyboard keeps no text and sends keys and commits,
 *  each passed on at once, as Termux takes them. Enter is a carriage return, backspace DEL. */
class KeyView(context: Context, private val typed: (String) -> Unit) : View(context) {
    init {
        isFocusable = true
        isFocusableInTouchMode = true
    }

    fun show() {
        requestFocus()
        context.getSystemService(InputMethodManager::class.java).showSoftInput(this, 0)
    }

    override fun onCheckIsTextEditor() = true

    override fun onCreateInputConnection(out: EditorInfo): InputConnection {
        out.inputType = InputType.TYPE_NULL
        out.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN or EditorInfo.IME_FLAG_NO_EXTRACT_UI
        return object : BaseInputConnection(this, true) {
            private fun flush() {
                val e = editable ?: return
                if (e.isNotEmpty()) typed(e.toString().replace("\n", "\r"))
                e.clear()
            }

            override fun commitText(text: CharSequence?, newCursorPosition: Int): Boolean {
                super.commitText(text, newCursorPosition)
                flush()
                return true
            }

            override fun finishComposingText(): Boolean {
                super.finishComposingText()
                flush()
                return true
            }

            override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
                repeat(beforeLength) { typed("\u007f") }
                return true
            }
        }
    }

    // Back is the app's, not the shell's: as the focused view the key view would otherwise
    // let it fall past the app's back handlers and close the app.
    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BACK && event.isTracking && !event.isCanceled) {
            findViewTreeOnBackPressedDispatcherOwner()?.onBackPressedDispatcher?.onBackPressed()
            return true
        }
        return super.onKeyUp(keyCode, event)
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            event.startTracking()
            return true
        }
        val s = when (keyCode) {
            KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> "\r"
            KeyEvent.KEYCODE_DEL -> "\u007f"
            KeyEvent.KEYCODE_FORWARD_DEL -> "\u001b[3~"
            KeyEvent.KEYCODE_TAB -> "\t"
            KeyEvent.KEYCODE_ESCAPE -> "\u001b"
            KeyEvent.KEYCODE_DPAD_UP -> "\u001b[A"
            KeyEvent.KEYCODE_DPAD_DOWN -> "\u001b[B"
            KeyEvent.KEYCODE_DPAD_RIGHT -> "\u001b[C"
            KeyEvent.KEYCODE_DPAD_LEFT -> "\u001b[D"
            else -> {
                val c = event.getUnicodeChar(event.metaState and KeyEvent.META_CTRL_MASK.inv())
                when {
                    c == 0 -> return super.onKeyDown(keyCode, event)
                    event.isCtrlPressed && c.toChar().isLetter() -> (c and 0x1f).toChar().toString()
                    else -> String(Character.toChars(c))
                }
            }
        }
        typed(s)
        return true
    }
}

@Composable
private fun KeyInput(made: (KeyView) -> Unit, typed: (String) -> Unit) {
    val now by rememberUpdatedState(typed)
    AndroidView(
        factory = { KeyView(it) { s -> now(s) }.also(made) },
        modifier = Modifier.size(1.dp),
    )
}

/** The terminal itself: sized to the space it has, the shell opened at that size the first
 *  time, resized after, and drawn each frame its screen changed. Leaving it leaves the
 *  shell running. */
@Composable
private fun TerminalCanvas(model: HubModel, shell: Shell) {
    val density = LocalDensity.current
    val paint = remember {
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = Typeface.MONOSPACE
            textSize = with(density) { 12f * fontScale * this.density }
        }
    }
    val bold = remember { Paint(paint).apply { typeface = Typeface.create(Typeface.MONOSPACE, Typeface.BOLD) } }
    val fill = remember { Paint() }
    val cellW = remember { paint.measureText("M") }
    val metrics = remember { paint.fontMetrics }
    val cellH = remember { metrics.descent - metrics.ascent }
    var size by remember { mutableLongStateOf(0L) }
    val cols = (size ushr 32).toInt().let { if (it == 0) 0 else (it / cellW).toInt().coerceAtLeast(1) }
    val rows = (size and 0xffffffffL).toInt().let { if (it == 0) 0 else (it / cellH).toInt().coerceAtLeast(1) }
    var frame by remember { mutableIntStateOf(0) }
    val cells = remember(cols, rows) { IntArray(cols * rows * 4) }
    val meta = remember { IntArray(7) }

    // Opened the first time the canvas has its size (and again after Open again); resized
    // whenever the size changes.
    LaunchedEffect(cols > 0, shell.generation) {
        if (cols > 0) shell.open(model, cols, rows, cellW.toInt(), cellH.toInt())
    }
    LaunchedEffect(cols, rows) {
        if (cols > 0) shell.resize(cols, rows, cellW.toInt(), cellH.toInt())
    }
    // Each display frame: draw if the screen changed; the first frame draws whatever the
    // shell did while it was not shown.
    LaunchedEffect(cells, shell.generation) {
        var first = true
        while (true) {
            withFrameNanos { }
            if (shell.cols == cols && shell.rows == rows && shell.snapshot(cells, meta, first)) {
                first = false
                frame++
            }
        }
    }

    Canvas(Modifier.fillMaxSize().clipToBounds().background(Color(0xff1e1e1e)).onSizeChanged {
        size = (it.width.toLong() shl 32) or it.height.toLong()
    }) {
        frame // read, so a new frame redraws
        val c = drawContext.canvas.nativeCanvas
        if (cols == 0 || meta[0] == 0) return@Canvas
        val dfg = 0xff000000.toInt() or meta[5]
        val dbg = 0xff000000.toInt() or meta[6]
        c.drawColor(dbg)
        val sb = StringBuilder(cols)
        for (y in 0 until minOf(rows, meta[1])) {
            val top = y * cellH
            var x = 0
            while (x < cols) {
                val at = (y * cols + x) * 4
                val fg0 = cells[at + 1]
                val bg0 = cells[at + 2]
                val bits0 = cells[at + 3]
                // A run: the cells from x with the same colours and style.
                sb.setLength(0)
                var end = x
                while (end < cols) {
                    val a = (y * cols + end) * 4
                    if (cells[a + 1] != fg0 || cells[a + 2] != bg0 || cells[a + 3] != bits0) break
                    val cp = cells[a]
                    if (cp == 0) sb.append(' ') else sb.appendCodePoint(cp)
                    end++
                }
                var fg = if (fg0 == -1) dfg else 0xff000000.toInt() or fg0
                var bg = if (bg0 == -1) dbg else 0xff000000.toInt() or bg0
                if (bits0 and Vt.INVERSE != 0) fg = bg.also { bg = fg }
                if (bg0 != -1 || bits0 and Vt.INVERSE != 0) {
                    fill.color = bg
                    c.drawRect(x * cellW, top, end * cellW, top + cellH, fill)
                }
                if (bits0 and Vt.INVISIBLE == 0 && sb.isNotBlank()) {
                    val p = if (bits0 and Vt.BOLD != 0) bold else paint
                    p.color = if (bits0 and Vt.FAINT != 0) (fg and 0x00ffffff) or 0x99000000.toInt() else fg
                    p.isUnderlineText = bits0 and Vt.UNDERLINE != 0
                    p.isStrikeThruText = bits0 and Vt.STRIKE != 0
                    p.textSkewX = if (bits0 and Vt.ITALIC != 0) -0.2f else 0f
                    c.drawText(sb, 0, sb.length, x * cellW, top - metrics.ascent, p)
                }
                x = end
            }
        }
        if (meta[4] == 1) {
            fill.color = 0x99ffffff.toInt()
            c.drawRect(meta[2] * cellW, meta[3] * cellH, (meta[2] + 1) * cellW, (meta[3] + 1) * cellH, fill)
        }
    }
}
