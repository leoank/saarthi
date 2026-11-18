package me.leoank.vtspike

/** libghostty-vt behind the JNI shim (`jni/vtjni.c`): a command under a PTY, its output fed
 *  to the terminal, the screen copied out on demand. */
object Vt {
    init {
        System.loadLibrary("vtjni")
    }

    /** Runs `cmd` under `/system/bin/sh -c` in a `cols` by `rows` terminal; 0 on failure. */
    @JvmStatic external fun start(cols: Int, rows: Int, cellW: Int, cellH: Int, cmd: String): Long

    @JvmStatic external fun input(h: Long, bytes: ByteArray)

    @JvmStatic external fun resize(h: Long, cols: Int, rows: Int, cellW: Int, cellH: Int)

    /** Fills `cells` (four ints a cell: code point, fg, bg, style bits; -1 colours are the
     *  default) and `meta` (cols, rows, cursor x, y, visible, default fg, default bg, bytes
     *  low, high, exited, ms to exit); false when nothing changed since the last call. */
    @JvmStatic external fun snapshot(h: Long, cells: IntArray, meta: IntArray, force: Boolean): Boolean

    @JvmStatic external fun stop(h: Long)

    const val BOLD = 1
    const val ITALIC = 2
    const val UNDERLINE = 4
    const val INVERSE = 8
    const val FAINT = 16
    const val STRIKE = 32
    const val INVISIBLE = 64
}
