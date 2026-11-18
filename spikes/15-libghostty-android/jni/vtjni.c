// EXP-20261114-raqjf0 step 2: libghostty-vt behind JNI for a throwaway Android app.
//
// A command runs under a PTY (forkpty, as the node's shell would); a reader thread feeds
// its output to a libghostty-vt terminal; the app asks for a snapshot each frame, which
// copies the screen out of the render state into an int array: per cell its base code
// point, foreground and background (0xRRGGBB, or -1 for the default) and style bits.
// Statically linked with libghostty-vt.a, as RUN-20261114-s2jk7j advises: an APK installs
// only lib*.so files, so the versioned shared library would not load.

#include <errno.h>
#include <jni.h>
#include <pthread.h>
#include <pty.h>
#include <signal.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#include <ghostty/vt.h>

typedef struct {
  GhosttyTerminal term;
  GhosttyRenderState rs;
  GhosttyRenderStateRowIterator rows;
  GhosttyRenderStateRowCells cells;
  pthread_mutex_t lock;
  pthread_t reader;
  int fd;
  pid_t pid;
  volatile long long bytes;     // bytes fed to the terminal so far
  volatile int changed;         // fed since the last snapshot
  volatile int exited;          // the command ended and its output is all fed
  volatile long long exited_ns; // when that happened, CLOCK_MONOTONIC
  long long started_ns;
} Vt;

static long long now_ns(void) {
  struct timespec t;
  clock_gettime(CLOCK_MONOTONIC, &t);
  return (long long)t.tv_sec * 1000000000LL + t.tv_nsec;
}

// Replies the terminal owes the program (device attributes, cursor reports) go back to it.
static void write_pty(GhosttyTerminal t, void *ud, const uint8_t *data, size_t len) {
  (void)t;
  Vt *v = ud;
  while (len > 0) {
    ssize_t n = write(v->fd, data, len);
    if (n <= 0) return;
    data += n;
    len -= (size_t)n;
  }
}

static void *read_loop(void *arg) {
  Vt *v = arg;
  uint8_t buf[65536];
  for (;;) {
    ssize_t n = read(v->fd, buf, sizeof buf);
    if (n <= 0) {
      if (n < 0 && errno == EINTR) continue;
      break;
    }
    pthread_mutex_lock(&v->lock);
    ghostty_terminal_vt_write(v->term, buf, (size_t)n);
    pthread_mutex_unlock(&v->lock);
    v->bytes += n;
    v->changed = 1;
  }
  v->exited_ns = now_ns();
  v->exited = 1;
  v->changed = 1;
  return NULL;
}

JNIEXPORT jlong JNICALL Java_me_leoank_vtspike_Vt_start(JNIEnv *env, jclass cls, jint cols, jint rows,
                                                        jint cell_w, jint cell_h, jstring cmd) {
  (void)cls;
  Vt *v = calloc(1, sizeof *v);
  if (!v) return 0;
  pthread_mutex_init(&v->lock, NULL);
  if (ghostty_terminal_new(NULL, &v->term, (uint16_t)cols, (uint16_t)rows) != GHOSTTY_SUCCESS) goto fail;
  ghostty_terminal_resize(v->term, (uint16_t)cols, (uint16_t)rows, (uint32_t)cell_w, (uint32_t)cell_h);
  ghostty_terminal_set(v->term, GHOSTTY_TERMINAL_OPT_USERDATA, v);
  ghostty_terminal_set(v->term, GHOSTTY_TERMINAL_OPT_WRITE_PTY, (const void *)write_pty);
  if (ghostty_render_state_new(NULL, &v->rs) != GHOSTTY_SUCCESS) goto fail;
  if (ghostty_render_state_row_iterator_new(NULL, &v->rows) != GHOSTTY_SUCCESS) goto fail;
  if (ghostty_render_state_row_cells_new(NULL, &v->cells) != GHOSTTY_SUCCESS) goto fail;

  struct winsize ws = {.ws_row = (unsigned short)rows, .ws_col = (unsigned short)cols,
                       .ws_xpixel = (unsigned short)(cols * cell_w), .ws_ypixel = (unsigned short)(rows * cell_h)};
  const char *c = (*env)->GetStringUTFChars(env, cmd, NULL);
  char *command = strdup(c);
  (*env)->ReleaseStringUTFChars(env, cmd, c);
  v->started_ns = now_ns();
  v->pid = forkpty(&v->fd, NULL, NULL, &ws);
  if (v->pid < 0) goto fail;
  if (v->pid == 0) {
    setenv("TERM", "xterm-256color", 1);
    setenv("COLORTERM", "truecolor", 1);
    execl("/system/bin/sh", "sh", "-c", command, (char *)NULL);
    _exit(127);
  }
  free(command);
  pthread_create(&v->reader, NULL, read_loop, v);
  return (jlong)(intptr_t)v;
fail:
  free(v);
  return 0;
}

JNIEXPORT void JNICALL Java_me_leoank_vtspike_Vt_input(JNIEnv *env, jclass cls, jlong h, jbyteArray bytes) {
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  jsize n = (*env)->GetArrayLength(env, bytes);
  jbyte *b = (*env)->GetByteArrayElements(env, bytes, NULL);
  write_pty(v->term, v, (const uint8_t *)b, (size_t)n);
  (*env)->ReleaseByteArrayElements(env, bytes, b, JNI_ABORT);
}

JNIEXPORT void JNICALL Java_me_leoank_vtspike_Vt_resize(JNIEnv *env, jclass cls, jlong h, jint cols, jint rows,
                                                        jint cell_w, jint cell_h) {
  (void)env;
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  pthread_mutex_lock(&v->lock);
  ghostty_terminal_resize(v->term, (uint16_t)cols, (uint16_t)rows, (uint32_t)cell_w, (uint32_t)cell_h);
  pthread_mutex_unlock(&v->lock);
  struct winsize ws = {.ws_row = (unsigned short)rows, .ws_col = (unsigned short)cols};
  ioctl(v->fd, TIOCSWINSZ, &ws);
  v->changed = 1;
}

static int rgb(GhosttyColorRgb c) { return (c.r << 16) | (c.g << 8) | c.b; }

// Style bits in the fourth int of a cell.
enum { BOLD = 1, ITALIC = 2, UNDERLINE = 4, INVERSE = 8, FAINT = 16, STRIKE = 32, INVISIBLE = 64 };

// Copies the screen into `cells` (cols*rows*4 ints) and `meta`: cols, rows, cursor x, y,
// visible, default fg, default bg, bytes fed (low, high), exited, elapsed ms to exit.
// Returns false, touching nothing, when nothing was fed since the last call.
JNIEXPORT jboolean JNICALL Java_me_leoank_vtspike_Vt_snapshot(JNIEnv *env, jclass cls, jlong h, jintArray out,
                                                              jintArray meta, jboolean force) {
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  if (!v->changed && !force) return JNI_FALSE;
  v->changed = 0;
  // Two phases: the terminal is held only while the update begins.
  pthread_mutex_lock(&v->lock);
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

  jint m[11] = {cols, rows, cx, cy, has && visible, rgb(dfg), rgb(dbg), (jint)(v->bytes & 0x7fffffff),
                (jint)(v->bytes >> 31), v->exited, v->exited ? (jint)((v->exited_ns - v->started_ns) / 1000000) : -1};
  (*env)->SetIntArrayRegion(env, meta, 0, 11, m);
  return JNI_TRUE;
}

JNIEXPORT void JNICALL Java_me_leoank_vtspike_Vt_stop(JNIEnv *env, jclass cls, jlong h) {
  (void)env;
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  if (!v) return;
  if (v->pid > 0) {
    kill(v->pid, SIGHUP);
    waitpid(v->pid, NULL, 0);
  }
  close(v->fd);
  pthread_join(v->reader, NULL);
  ghostty_render_state_row_cells_free(v->cells);
  ghostty_render_state_row_iterator_free(v->rows);
  ghostty_render_state_free(v->rs);
  ghostty_terminal_free(v->term);
  free(v);
}
