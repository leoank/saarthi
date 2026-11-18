package me.leoank.vtspike

import java.util.concurrent.atomic.AtomicLong

/** Where typed bytes go: the terminal's program. */
fun interface ByteSink {
    fun send(bytes: ByteArray)
}

/** What one run measured: frames that changed and were drawn, the time to copy the screen
 *  out (libghostty-vt only) and to draw it, the bytes fed, and when the command ended. */
class Stats {
    private val started = AtomicLong(System.nanoTime())
    private val frames = AtomicLong()
    private val copyNs = AtomicLong()
    private val copyMax = AtomicLong()
    private val drawNs = AtomicLong()
    private val drawMax = AtomicLong()
    private val draws = AtomicLong()
    @Volatile var bytes = 0L
    @Volatile var exitMs = -1
    // The last second's frames, for a rate that moves.
    private var windowAt = System.nanoTime()
    private var windowFrames = 0L
    @Volatile var fps = 0.0

    fun reset() {
        started.set(System.nanoTime())
        listOf(frames, copyNs, copyMax, drawNs, drawMax, draws).forEach { it.set(0) }
        bytes = 0
        exitMs = -1
        windowAt = System.nanoTime()
        windowFrames = 0
        fps = 0.0
    }

    fun copied(ns: Long) {
        frames.incrementAndGet()
        copyNs.addAndGet(ns)
        copyMax.accumulateAndGet(ns, ::maxOf)
    }

    fun drew(ns: Long) {
        draws.incrementAndGet()
        drawNs.addAndGet(ns)
        drawMax.accumulateAndGet(ns, ::maxOf)
    }

    /** A frame that changed, for a library that copies nothing out (Termux). */
    fun frame() {
        frames.incrementAndGet()
    }

    fun fed(total: Long, exit: Int) {
        bytes = total
        if (exit >= 0 && exitMs < 0) exitMs = exit
    }

    fun ended() {
        if (exitMs < 0) exitMs = ((System.nanoTime() - started.get()) / 1_000_000).toInt()
    }

    /** One line for the screen and the log. */
    fun line(): String {
        val now = System.nanoTime()
        val f = frames.get()
        if (now - windowAt > 1_000_000_000) {
            fps = (f - windowFrames) * 1e9 / (now - windowAt)
            windowAt = now
            windowFrames = f
        }
        val d = draws.get().coerceAtLeast(1)
        val n = f.coerceAtLeast(1)
        return "%.0f fps · %d frames · copy %.2f/%.1f ms · draw %.2f/%.1f ms · %.1f MB · %s".format(
            fps, f, copyNs.get() / 1e6 / n, copyMax.get() / 1e6, drawNs.get() / 1e6 / d, drawMax.get() / 1e6,
            bytes / 1e6, if (exitMs >= 0) "done in $exitMs ms" else "running",
        )
    }
}
