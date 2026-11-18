package me.leoank.vtspike

import android.content.Context
import android.graphics.Canvas
import android.view.KeyEvent
import android.view.MotionEvent
import android.widget.FrameLayout
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import com.termux.terminal.TerminalSession
import com.termux.terminal.TerminalSessionClient
import com.termux.view.TerminalView
import com.termux.view.TerminalViewClient

/** Termux's terminal-emulator and terminal-view, the comparison: the same command under the
 *  library's own PTY, its view timed in onDraw. */
@Composable
fun TermuxTerminal(command: String, stats: Stats, input: (ByteSink) -> Unit, modifier: Modifier = Modifier) {
    val holder = remember(command) { arrayOfNulls<TerminalSession>(1) }
    DisposableEffect(command) { onDispose { holder[0]?.finishIfRunning() } }
    AndroidView(
        modifier = modifier,
        factory = { ctx ->
            stats.reset()
            val view = TerminalView(ctx, null)
            val session = TerminalSession(
                "/system/bin/sh", ctx.filesDir.path, arrayOf("sh", "-c", command),
                arrayOf("TERM=xterm-256color", "COLORTERM=truecolor", "HOME=${ctx.filesDir.path}"),
                2000, SessionClient(view, stats),
            )
            holder[0] = session
            view.setTerminalViewClient(ViewClient())
            view.setTextSize((12 * ctx.resources.displayMetrics.scaledDensity).toInt())
            view.attachSession(session)
            input(ByteSink { b -> session.write(b, 0, b.size) })
            TimedFrame(ctx, stats).apply { addView(view) }
        },
    )
}

/** TerminalView is final: its drawing is timed from the frame around it. */
private class TimedFrame(ctx: Context, val stats: Stats) : FrameLayout(ctx) {
    override fun dispatchDraw(canvas: Canvas) {
        val t = System.nanoTime()
        super.dispatchDraw(canvas)
        stats.drew(System.nanoTime() - t)
    }
}

private class SessionClient(val view: TerminalView, val stats: Stats) : TerminalSessionClient {
    override fun onTextChanged(changedSession: TerminalSession) {
        stats.frame()
        view.onScreenUpdated()
    }
    override fun onTitleChanged(changedSession: TerminalSession) {}
    override fun onSessionFinished(finishedSession: TerminalSession) = stats.ended()
    override fun onCopyTextToClipboard(session: TerminalSession, text: String?) {}
    override fun onPasteTextFromClipboard(session: TerminalSession?) {}
    override fun onBell(session: TerminalSession) {}
    override fun onColorsChanged(session: TerminalSession) {}
    override fun onTerminalCursorStateChange(state: Boolean) {}
    override fun getTerminalCursorStyle(): Int? = null
    override fun logError(tag: String?, message: String?) {}
    override fun logWarn(tag: String?, message: String?) {}
    override fun logInfo(tag: String?, message: String?) {}
    override fun logDebug(tag: String?, message: String?) {}
    override fun logVerbose(tag: String?, message: String?) {}
    override fun logStackTraceWithMessage(tag: String?, message: String?, e: Exception?) {}
    override fun logStackTrace(tag: String?, e: Exception?) {}
}

private class ViewClient : TerminalViewClient {
    override fun onScale(scale: Float): Float = 1f
    override fun onSingleTapUp(e: MotionEvent?) {}
    override fun shouldBackButtonBeMappedToEscape(): Boolean = false
    override fun shouldEnforceCharBasedInput(): Boolean = true
    override fun shouldUseCtrlSpaceWorkaround(): Boolean = false
    override fun isTerminalViewSelected(): Boolean = true
    override fun copyModeChanged(copyMode: Boolean) {}
    override fun onKeyDown(keyCode: Int, e: KeyEvent?, session: TerminalSession?): Boolean = false
    override fun onKeyUp(keyCode: Int, e: KeyEvent?): Boolean = false
    override fun onLongPress(event: MotionEvent?): Boolean = false
    override fun readControlKey(): Boolean = false
    override fun readAltKey(): Boolean = false
    override fun readShiftKey(): Boolean = false
    override fun readFnKey(): Boolean = false
    override fun onCodePoint(codePoint: Int, ctrlDown: Boolean, session: TerminalSession?): Boolean = false
    override fun onEmulatorSet() {}
    override fun logError(tag: String?, message: String?) {}
    override fun logWarn(tag: String?, message: String?) {}
    override fun logInfo(tag: String?, message: String?) {}
    override fun logDebug(tag: String?, message: String?) {}
    override fun logVerbose(tag: String?, message: String?) {}
    override fun logStackTraceWithMessage(tag: String?, message: String?, e: Exception?) {}
    override fun logStackTrace(tag: String?, e: Exception?) {}
}
