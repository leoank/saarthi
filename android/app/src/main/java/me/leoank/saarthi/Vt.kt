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
     *  default), `meta` (cols, rows, cursor x, y, visible, default fg, default bg, images)
     *  and `images` ([IMAGE_INTS] ints an image on screen: id, layer, z, x, y, width,
     *  height in pixels, source x, y, width, height, generation); false when nothing
     *  changed since the last call. */
    @JvmStatic external fun snapshot(h: Long, cells: IntArray, meta: IntArray, images: IntArray, force: Boolean): Boolean

    /** A Kitty image's pixels, `{width, height, ARGB...}`; null once the terminal dropped it. */
    @JvmStatic external fun imagePixels(h: Long, id: Int): IntArray?

    /** What the program asked of the screen: [MOUSE] reports, the [ALTERNATE] screen. */
    @JvmStatic external fun state(h: Long): Int

    /** A mouse event at pixels (x, y) of a `width` × `height` screen, encoded as the
     *  program asked: action 0 press, 1 release, 2 motion; button 0 none, 1 left, 4 wheel
     *  up, 5 wheel down. Null when the program asked for no mouse reports. */
    @JvmStatic external fun mouse(h: Long, action: Int, button: Int, x: Float, y: Float, width: Int, height: Int): ByteArray?

    /** Moves the view through the scrollback by rows, up negative; 0 back to the bottom. */
    @JvmStatic external fun scroll(h: Long, rows: Int)

    /** For the terminal's Kitty images: a PNG as `{width, height, ARGB...}`, or null when
     *  it is not one. Called from the shim while it parses the shell's output. */
    @JvmStatic
    fun decodePng(bytes: ByteArray): IntArray? = runCatching {
        val b = android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return null
        val out = IntArray(2 + b.width * b.height)
        out[0] = b.width
        out[1] = b.height
        b.getPixels(out, 2, b.width, 0, 0, b.width, b.height)
        b.recycle()
        out
    }.getOrNull()

    @JvmStatic external fun free(h: Long)

    const val BOLD = 1
    const val ITALIC = 2
    const val UNDERLINE = 4
    const val INVERSE = 8
    const val FAINT = 16
    const val STRIKE = 32
    const val INVISIBLE = 64

    const val IMAGE_INTS = 12
    /** Image layers, drawn in this order around the text. */
    const val BELOW_BACKGROUND = 0
    const val BELOW_TEXT = 1
    const val ABOVE_TEXT = 2

    const val MOUSE = 1
    const val ALTERNATE = 2
}
