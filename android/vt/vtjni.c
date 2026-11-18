// The app's terminal renderer (ADR-0054): libghostty-vt behind JNI, as EXP-20261114-raqjf0
// measured it. The shell runs on a node; its output arrives through the core and is fed in
// here, and what the terminal owes the program in reply (device attributes, cursor
// reports) is kept for the app to send back. Each frame the app asks for a snapshot, which
// copies the screen out of the render state into an int array: per cell its base code
// point, foreground and background (0xRRGGBB, or -1 for the default) and style bits.
// Statically linked with libghostty-vt.a (an APK installs only lib*.so files).

#include <jni.h>
#include <pthread.h>
#include <stdlib.h>
#include <string.h>

#include <ghostty/vt.h>

typedef struct {
  GhosttyTerminal term;
  GhosttyRenderState rs;
  GhosttyRenderStateRowIterator rows;
  GhosttyRenderStateRowCells cells;
  pthread_mutex_t lock;
  int changed;
  // Replies for the program, until the app takes them.
  uint8_t *reply;
  size_t reply_len, reply_cap;
} Vt;

static void write_pty(GhosttyTerminal t, void *ud, const uint8_t *data, size_t len) {
  (void)t;
  Vt *v = ud;
  if (v->reply_len + len > v->reply_cap) {
    size_t cap = (v->reply_len + len) * 2;
    uint8_t *r = realloc(v->reply, cap);
    if (!r) return;
    v->reply = r;
    v->reply_cap = cap;
  }
  memcpy(v->reply + v->reply_len, data, len);
  v->reply_len += len;
}

JNIEXPORT jlong JNICALL Java_me_leoank_saarthi_Vt_create(JNIEnv *env, jclass cls, jint cols, jint rows, jint cell_w,
                                                         jint cell_h) {
  (void)env;
  (void)cls;
  Vt *v = calloc(1, sizeof *v);
  if (!v) return 0;
  pthread_mutex_init(&v->lock, NULL);
  if (ghostty_terminal_new(NULL, &v->term, (uint16_t)cols, (uint16_t)rows) != GHOSTTY_SUCCESS) {
    free(v);
    return 0;
  }
  ghostty_terminal_resize(v->term, (uint16_t)cols, (uint16_t)rows, (uint32_t)cell_w, (uint32_t)cell_h);
  ghostty_terminal_set(v->term, GHOSTTY_TERMINAL_OPT_USERDATA, v);
  ghostty_terminal_set(v->term, GHOSTTY_TERMINAL_OPT_WRITE_PTY, (const void *)write_pty);
  ghostty_render_state_new(NULL, &v->rs);
  ghostty_render_state_row_iterator_new(NULL, &v->rows);
  ghostty_render_state_row_cells_new(NULL, &v->cells);
  return (jlong)(intptr_t)v;
}

// The shell's output, from any thread.
JNIEXPORT void JNICALL Java_me_leoank_saarthi_Vt_feed(JNIEnv *env, jclass cls, jlong h, jbyteArray bytes) {
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  jsize n = (*env)->GetArrayLength(env, bytes);
  jbyte *b = (*env)->GetByteArrayElements(env, bytes, NULL);
  pthread_mutex_lock(&v->lock);
  ghostty_terminal_vt_write(v->term, (const uint8_t *)b, (size_t)n);
  v->changed = 1;
  pthread_mutex_unlock(&v->lock);
  (*env)->ReleaseByteArrayElements(env, bytes, b, JNI_ABORT);
}

// What the terminal owes the program since the last call; null when nothing.
JNIEXPORT jbyteArray JNICALL Java_me_leoank_saarthi_Vt_replies(JNIEnv *env, jclass cls, jlong h) {
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  pthread_mutex_lock(&v->lock);
  jbyteArray out = NULL;
  if (v->reply_len > 0) {
    out = (*env)->NewByteArray(env, (jsize)v->reply_len);
    (*env)->SetByteArrayRegion(env, out, 0, (jsize)v->reply_len, (const jbyte *)v->reply);
    v->reply_len = 0;
  }
  pthread_mutex_unlock(&v->lock);
  return out;
}

JNIEXPORT void JNICALL Java_me_leoank_saarthi_Vt_resize(JNIEnv *env, jclass cls, jlong h, jint cols, jint rows,
                                                        jint cell_w, jint cell_h) {
  (void)env;
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  pthread_mutex_lock(&v->lock);
  ghostty_terminal_resize(v->term, (uint16_t)cols, (uint16_t)rows, (uint32_t)cell_w, (uint32_t)cell_h);
  v->changed = 1;
  pthread_mutex_unlock(&v->lock);
}

static int rgb(GhosttyColorRgb c) { return (c.r << 16) | (c.g << 8) | c.b; }

enum { BOLD = 1, ITALIC = 2, UNDERLINE = 4, INVERSE = 8, FAINT = 16, STRIKE = 32, INVISIBLE = 64 };

// Copies the screen into `cells` (cols*rows*4 ints) and `meta`: cols, rows, cursor x, y,
// visible, default fg, default bg. False, touching nothing, when nothing changed since the
// last call and `force` is not set.
JNIEXPORT jboolean JNICALL Java_me_leoank_saarthi_Vt_snapshot(JNIEnv *env, jclass cls, jlong h, jintArray out,
                                                              jintArray meta, jboolean force) {
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  pthread_mutex_lock(&v->lock);
  if (!v->changed && !force) {
    pthread_mutex_unlock(&v->lock);
    return JNI_FALSE;
  }
  v->changed = 0;
  // Two phases: the terminal is held only while the update begins.
  ghostty_render_state_begin_update(v->rs, v->term);
  pthread_mutex_unlock(&v->lock);
  ghostty_render_state_end_update(v->rs);

  uint16_t cols = 0, rows = 0;
  ghostty_render_state_get(v->rs, GHOSTTY_RENDER_STATE_DATA_COLS, &cols);
  ghostty_render_state_get(v->rs, GHOSTTY_RENDER_STATE_DATA_ROWS, &rows);
  jsize cap = (*env)->GetArrayLength(env, out);
  jint *o = (*env)->GetPrimitiveArrayCritical(env, out, NULL);
  ghostty_render_state_get(v->rs, GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR, &v->rows);
  int y = 0;
  while (ghostty_render_state_row_iterator_next(v->rows) && y < rows) {
    ghostty_render_state_row_get(v->rows, GHOSTTY_RENDER_STATE_ROW_DATA_CELLS, &v->cells);
    int x = 0;
    while (ghostty_render_state_row_cells_next(v->cells) && x < cols) {
      size_t at = ((size_t)y * cols + x) * 4;
      if ((jsize)(at + 4) > cap) break;
      uint32_t len = 0, cp = 0;
      ghostty_render_state_row_cells_get(v->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN, &len);
      if (len > 0) {
        uint32_t buf[8];
        if (len <= 8 &&
            ghostty_render_state_row_cells_get(v->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF, buf) ==
                GHOSTTY_SUCCESS)
          cp = buf[0];
      }
      GhosttyColorRgb c;
      int fg = -1, bg = -1, bits = 0;
      if (ghostty_render_state_row_cells_get(v->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR, &c) ==
          GHOSTTY_SUCCESS)
        fg = rgb(c);
      if (ghostty_render_state_row_cells_get(v->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR, &c) ==
          GHOSTTY_SUCCESS)
        bg = rgb(c);
      bool styled = false;
      ghostty_render_state_row_cells_get(v->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_HAS_STYLING, &styled);
      if (styled) {
        GhosttyStyle s = GHOSTTY_INIT_SIZED(GhosttyStyle);
        if (ghostty_render_state_row_cells_get(v->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE, &s) ==
            GHOSTTY_SUCCESS)
          bits = (s.bold ? BOLD : 0) | (s.italic ? ITALIC : 0) | (s.underline ? UNDERLINE : 0) |
                 (s.inverse ? INVERSE : 0) | (s.faint ? FAINT : 0) | (s.strikethrough ? STRIKE : 0) |
                 (s.invisible ? INVISIBLE : 0);
      }
      o[at] = (jint)cp;
      o[at + 1] = fg;
      o[at + 2] = bg;
      o[at + 3] = bits;
      x++;
    }
    for (; x < cols; x++) {
      size_t at = ((size_t)y * cols + x) * 4;
      if ((jsize)(at + 4) > cap) break;
      o[at] = 0, o[at + 1] = -1, o[at + 2] = -1, o[at + 3] = 0;
    }
    y++;
  }
  (*env)->ReleasePrimitiveArrayCritical(env, out, o, 0);

  bool has = false, visible = false;
  uint16_t cx = 0, cy = 0;
  ghostty_render_state_get(v->rs, GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE, &has);
  ghostty_render_state_get(v->rs, GHOSTTY_RENDER_STATE_DATA_CURSOR_VISIBLE, &visible);
  if (has) {
    ghostty_render_state_get(v->rs, GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X, &cx);
    ghostty_render_state_get(v->rs, GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y, &cy);
  }
  GhosttyColorRgb dfg = {0xdd, 0xdd, 0xdd}, dbg = {0x1e, 0x1e, 0x1e};
  ghostty_render_state_get(v->rs, GHOSTTY_RENDER_STATE_DATA_COLOR_FOREGROUND, &dfg);
  ghostty_render_state_get(v->rs, GHOSTTY_RENDER_STATE_DATA_COLOR_BACKGROUND, &dbg);
  ghostty_render_state_clean(v->rs);

  jint m[7] = {cols, rows, cx, cy, has && visible, rgb(dfg), rgb(dbg)};
  (*env)->SetIntArrayRegion(env, meta, 0, 7, m);
  return JNI_TRUE;
}

JNIEXPORT void JNICALL Java_me_leoank_saarthi_Vt_free(JNIEnv *env, jclass cls, jlong h) {
  (void)env;
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  if (!v) return;
  ghostty_render_state_row_cells_free(v->cells);
  ghostty_render_state_row_iterator_free(v->rows);
  ghostty_render_state_free(v->rs);
  ghostty_terminal_free(v->term);
  free(v->reply);
  free(v);
}
