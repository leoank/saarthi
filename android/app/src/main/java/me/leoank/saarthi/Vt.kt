package me.leoank.saarthi

/** libghostty-vt behind `android/vt/vtjni.c` (ADR-0054): a terminal's state, fed the shell's
 *  output from the node, its screen copied out each frame. */
object Vt {
    init {
        System.loadLibrary("vtjni")
    }

    @JvmStatic external fun create(cols: Int, rows: Int, cellW: Int, cellH: Int): Long

    /** The shell's output; any thread. */
    @JvmStatic external fun feed(h: Long, bytes: ByteArray)

    /** What the terminal owes the program (device attributes, cursor reports); null when
     *  nothing. */
    @JvmStatic external fun replies(h: Long): ByteArray?

    @JvmStatic external fun resize(h: Long, cols: Int, rows: Int, cellW: Int, cellH: Int)

    /** Fills `cells` (four ints a cell: code point, fg, bg, style bits; -1 colours are the
     *  default) and `meta` (cols, rows, cursor x, y, visible, default fg, default bg); false
     *  when nothing changed since the last call. */
    @JvmStatic external fun snapshot(h: Long, cells: IntArray, meta: IntArray, force: Boolean): Boolean

    @JvmStatic external fun free(h: Long)

    const val BOLD = 1
    const val ITALIC = 2
    const val UNDERLINE = 4
    const val INVERSE = 8
    const val FAINT = 16
    const val STRIKE = 32
    const val INVISIBLE = 64
}
