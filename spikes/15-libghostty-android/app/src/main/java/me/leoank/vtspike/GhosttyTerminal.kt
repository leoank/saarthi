package me.leoank.vtspike

import android.graphics.Paint
import android.graphics.Typeface
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity

/** A libghostty-vt terminal drawn on a Compose canvas: each frame, if the terminal changed,
 *  its screen is copied out and drawn in runs of one style. Reports what it measured. */
@Composable
fun GhosttyTerminal(command: String, stats: Stats, input: (ByteSink) -> Unit, modifier: Modifier = Modifier) {
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
    var handle by remember { mutableLongStateOf(0L) }
    var frame by remember { mutableIntStateOf(0) }
    val cols = (size ushr 32).toInt().let { if (it == 0) 0 else (it / cellW).toInt().coerceAtLeast(1) }
    val rows = (size and 0xffffffffL).toInt().let { if (it == 0) 0 else (it / cellH).toInt().coerceAtLeast(1) }
    val cells = remember(cols, rows) { IntArray(cols * rows * 4) }
    val meta = remember { IntArray(11) }

    // One terminal per command, sized to the canvas once it is laid out; resized after.
    LaunchedEffect(command, cols > 0) {
        if (cols == 0) return@LaunchedEffect
        stats.reset()
        handle = Vt.start(cols, rows, cellW.toInt(), cellH.toInt(), command)
        input(ByteSink { b -> if (handle != 0L) Vt.input(handle, b) })
    }
    LaunchedEffect(cols, rows) { if (handle != 0L && cols > 0) Vt.resize(handle, cols, rows, cellW.toInt(), cellH.toInt()) }
    DisposableEffect(command) { onDispose { if (handle != 0L) Vt.stop(handle); handle = 0L } }
    // Each display frame: copy the screen out if it changed, and draw it.
    LaunchedEffect(handle, cells) {
        if (handle == 0L) return@LaunchedEffect
        while (true) {
            withFrameNanos { }
            val t = System.nanoTime()
            if (Vt.snapshot(handle, cells, meta, false)) {
                stats.copied(System.nanoTime() - t)
                stats.fed(meta[7].toLong() or (meta[8].toLong() shl 31), if (meta[9] == 1) meta[10] else -1)
                frame++
            }
        }
    }

    // Clipped: drawColor would otherwise paint the whole window, over the chips above.
    Canvas(modifier.fillMaxSize().clipToBounds().onSizeChanged { size = (it.width.toLong() shl 32) or it.height.toLong() }) {
        frame // read, so a new frame redraws
        val t = System.nanoTime()
        val c = drawContext.canvas.nativeCanvas
        val dfg = 0xff000000.toInt() or meta[5]
        val dbg = 0xff000000.toInt() or meta[6]
        c.drawColor(if (meta[6] == 0) 0xff1e1e1e.toInt() else dbg)
        if (cols == 0 || meta[0] == 0) return@Canvas
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
        stats.drew(System.nanoTime() - t)
    }
}
