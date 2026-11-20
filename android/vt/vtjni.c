// The app's terminal renderer (ADR-0054): libghostty-vt behind JNI, as EXP-20261114-raqjf0
// measured it. The shell runs on a node; its output arrives through the core and is fed in
// here, and what the terminal owes the program in reply (device attributes, cursor
// reports) is kept for the app to send back. Each frame the app asks for a snapshot, which
// copies the screen out of the render state into an int array: per cell its base code
// point, foreground and background (0xRRGGBB, or -1 for the default) and style bits.
// Statically linked with libghostty-vt.a (an APK installs only lib*.so files).
//
// Images (the Kitty graphics protocol, which yazi and others use): the terminal keeps them,
// PNGs decoded by Android's BitmapFactory through Vt.decodePng; each snapshot also lists the
// images on screen, and Vt.imagePixels gives one image's pixels. The terminal answers the
// queries programs probe with (device attributes, size, version), so they find it can.
// Touch: Vt.mouse encodes a tap or a wheel step the way the program asked for mouse
// reports; Vt.state says whether it asked, and whether it is on the alternate screen.

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
  GhosttyRenderStateImageIterator images;
  GhosttyMouseEncoder mouse;
  GhosttyMouseEvent event;
  pthread_mutex_t lock;
  int changed;
  // The size, for the size reports programs ask for.
  uint16_t cols, rows_n;
  uint32_t cell_w, cell_h;
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

// --- PNG decoding through Android, for Kitty graphics -------------------------------------

static JavaVM *jvm;
static jclass vt_class;
static jmethodID decode_png_mid;

// Vt.decodePng(bytes) gives {width, height, ARGB...} or null; the terminal wants RGBA.
static bool decode_png(void *ud, const GhosttyAllocator *alloc, const uint8_t *data, size_t len, GhosttySysImage *out) {
  (void)ud;
  JNIEnv *env = NULL;
  if ((*jvm)->GetEnv(jvm, (void **)&env, JNI_VERSION_1_6) != JNI_OK) return false;
  jbyteArray in = (*env)->NewByteArray(env, (jsize)len);
  if (!in) return false;
  (*env)->SetByteArrayRegion(env, in, 0, (jsize)len, (const jbyte *)data);
  jintArray px = (*env)->CallStaticObjectMethod(env, vt_class, decode_png_mid, in);
  (*env)->DeleteLocalRef(env, in);
  if ((*env)->ExceptionCheck(env)) {
    (*env)->ExceptionClear(env);
    return false;
  }
  if (!px) return false;
  jsize n = (*env)->GetArrayLength(env, px);
  jint *p = (*env)->GetIntArrayElements(env, px, NULL);
  bool ok = false;
  if (n >= 2 && p[0] > 0 && p[1] > 0 && (size_t)n == 2 + (size_t)p[0] * (size_t)p[1]) {
    size_t count = (size_t)p[0] * (size_t)p[1];
    uint8_t *rgba = ghostty_alloc(alloc, count * 4);
    if (rgba) {
      for (size_t i = 0; i < count; i++) {
        uint32_t c = (uint32_t)p[2 + i];
        rgba[i * 4] = (c >> 16) & 0xff;
        rgba[i * 4 + 1] = (c >> 8) & 0xff;
        rgba[i * 4 + 2] = c & 0xff;
        rgba[i * 4 + 3] = (c >> 24) & 0xff;
      }
      out->width = (uint32_t)p[0];
      out->height = (uint32_t)p[1];
      out->data = rgba;
      ok = true;
    }
  }
  (*env)->ReleaseIntArrayElements(env, px, p, JNI_ABORT);
  (*env)->DeleteLocalRef(env, px);
  return ok;
}

JNIEXPORT jint JNI_OnLoad(JavaVM *vm, void *reserved) {
  (void)reserved;
  jvm = vm;
  JNIEnv *env;
  if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_OK) return JNI_ERR;
  jclass c = (*env)->FindClass(env, "me/leoank/saarthi/Vt");
  if (!c) return JNI_ERR;
  vt_class = (*env)->NewGlobalRef(env, c);
  decode_png_mid = (*env)->GetStaticMethodID(env, c, "decodePng", "([B)[I");
  if (!decode_png_mid) return JNI_ERR;
  ghostty_sys_set(GHOSTTY_SYS_OPT_DECODE_PNG, (const void *)decode_png);
  return JNI_VERSION_1_6;
}

// --- What the terminal tells programs that ask -------------------------------------------

// The size in cells and pixels (CSI 14/16/18 t), which programs drawing images use.
static bool size_report(GhosttyTerminal t, void *ud, GhosttySizeReportSize *out) {
  (void)t;
  Vt *v = ud;
  out->rows = v->rows_n;
  out->columns = v->cols;
  out->cell_width = v->cell_w;
  out->cell_height = v->cell_h;
  return true;
}

// A VT220 with colours: without an answer to CSI c, programs that send it last to learn
// their other queries are answered (yazi does) wait for it.
static bool device_attributes(GhosttyTerminal t, void *ud, GhosttyDeviceAttributes *out) {
  (void)t;
  (void)ud;
  out->primary.conformance_level = GHOSTTY_DA_CONFORMANCE_VT220;
  out->primary.features[0] = GHOSTTY_DA_FEATURE_COLUMNS_132;
  out->primary.features[1] = GHOSTTY_DA_FEATURE_ANSI_COLOR;
  out->primary.num_features = 2;
  out->secondary.device_type = GHOSTTY_DA_DEVICE_TYPE_VT220;
  out->secondary.firmware_version = 1;
  out->secondary.rom_cartridge = 0;
  out->tertiary.unit_id = 0;
  return true;
}

// The library's own name for XTVERSION.
static GhosttyString xtversion(GhosttyTerminal t, void *ud) {
  (void)t;
  (void)ud;
  return (GhosttyString){(const uint8_t *)"", 0};
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
  v->cols = (uint16_t)cols, v->rows_n = (uint16_t)rows, v->cell_w = (uint32_t)cell_w, v->cell_h = (uint32_t)cell_h;
  ghostty_terminal_set(v->term, GHOSTTY_TERMINAL_OPT_USERDATA, v);
  ghostty_terminal_set(v->term, GHOSTTY_TERMINAL_OPT_WRITE_PTY, (const void *)write_pty);
  ghostty_terminal_set(v->term, GHOSTTY_TERMINAL_OPT_SIZE, (const void *)size_report);
  ghostty_terminal_set(v->term, GHOSTTY_TERMINAL_OPT_DEVICE_ATTRIBUTES, (const void *)device_attributes);
  ghostty_terminal_set(v->term, GHOSTTY_TERMINAL_OPT_XTVERSION, (const void *)xtversion);
  // Kitty graphics, up to 64 MB of images.
  uint64_t limit = 64u << 20;
  ghostty_terminal_set(v->term, GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_STORAGE_LIMIT, &limit);
  ghostty_render_state_new(NULL, &v->rs);
  ghostty_render_state_row_iterator_new(NULL, &v->rows);
  ghostty_render_state_row_cells_new(NULL, &v->cells);
  ghostty_render_state_image_iterator_new(NULL, &v->images);
  ghostty_mouse_encoder_new(NULL, &v->mouse);
  ghostty_mouse_event_new(NULL, &v->event);
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
  v->cols = (uint16_t)cols, v->rows_n = (uint16_t)rows, v->cell_w = (uint32_t)cell_w, v->cell_h = (uint32_t)cell_h;
  v->changed = 1;
  pthread_mutex_unlock(&v->lock);
}

static int rgb(GhosttyColorRgb c) { return (c.r << 16) | (c.g << 8) | c.b; }

enum { BOLD = 1, ITALIC = 2, UNDERLINE = 4, INVERSE = 8, FAINT = 16, STRIKE = 32, INVISIBLE = 64 };

// Copies the screen into `cells` (cols*rows*4 ints) and `meta`: cols, rows, cursor x, y,
// visible, default fg, default bg. False, touching nothing, when nothing changed since the
// last call and `force` is not set.
// Each image on screen, IMAGE_INTS ints: image id, layer, z, x and y in pixels from the
// viewport's top left, width and height to draw at, the source rectangle (x, y, width,
// height), and the low 32 bits of the image's generation.
#define IMAGE_INTS 12

static int list_images(Vt *v, jint *o, jsize cap) {
  int n = 0;
  if (ghostty_render_state_get(v->rs, GHOSTTY_RENDER_STATE_DATA_IMAGE_ITERATOR, &v->images) != GHOSTTY_SUCCESS) return 0;
  while (ghostty_render_state_image_iterator_next(v->images) && (n + 1) * IMAGE_INTS <= cap) {
    GhosttyImageProtocol protocol = 0;
    uint32_t id = 0, offx = 0, offy = 0, w = 0, hh = 0, sx = 0, sy = 0, sw = 0, sh = 0;
    uint64_t gen = 0;
    GhosttyRenderStateImageLayer layer = 0;
    int32_t z = 0, col = 0, row = 0;
    GhosttyRenderStateImageData keys[] = {
        GHOSTTY_RENDER_STATE_IMAGE_DATA_PROTOCOL,      GHOSTTY_RENDER_STATE_IMAGE_DATA_IMAGE_ID,
        GHOSTTY_RENDER_STATE_IMAGE_DATA_IMAGE_GENERATION, GHOSTTY_RENDER_STATE_IMAGE_DATA_LAYER,
        GHOSTTY_RENDER_STATE_IMAGE_DATA_Z,             GHOSTTY_RENDER_STATE_IMAGE_DATA_VIEWPORT_COL,
        GHOSTTY_RENDER_STATE_IMAGE_DATA_VIEWPORT_ROW,  GHOSTTY_RENDER_STATE_IMAGE_DATA_CELL_OFFSET_X,
        GHOSTTY_RENDER_STATE_IMAGE_DATA_CELL_OFFSET_Y, GHOSTTY_RENDER_STATE_IMAGE_DATA_PIXEL_WIDTH,
        GHOSTTY_RENDER_STATE_IMAGE_DATA_PIXEL_HEIGHT,  GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_X,
        GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_Y,      GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_WIDTH,
        GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_HEIGHT,
    };
    void *values[] = {&protocol, &id, &gen, &layer, &z, &col, &row, &offx, &offy, &w, &hh, &sx, &sy, &sw, &sh};
    if (ghostty_render_state_image_get_multi(v->images, 15, keys, values, NULL) != GHOSTTY_SUCCESS) continue;
    if (protocol != GHOSTTY_IMAGE_PROTOCOL_KITTY) continue;
    jint *e = o + n * IMAGE_INTS;
    e[0] = (jint)id, e[1] = (jint)layer, e[2] = z;
    e[3] = col * (int32_t)v->cell_w + (int32_t)offx, e[4] = row * (int32_t)v->cell_h + (int32_t)offy;
    e[5] = (jint)w, e[6] = (jint)hh, e[7] = (jint)sx, e[8] = (jint)sy, e[9] = (jint)sw, e[10] = (jint)sh;
    e[11] = (jint)(uint32_t)gen;
    n++;
  }
  return n;
}

JNIEXPORT jboolean JNICALL Java_me_leoank_saarthi_Vt_snapshot(JNIEnv *env, jclass cls, jlong h, jintArray out,
                                                              jintArray meta, jintArray images, jboolean force) {
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
  jsize icap = (*env)->GetArrayLength(env, images);
  jint *io = (*env)->GetIntArrayElements(env, images, NULL);
  int nimages = list_images(v, io, icap);
  (*env)->ReleaseIntArrayElements(env, images, io, 0);
  ghostty_render_state_clean(v->rs);

  jint m[8] = {cols, rows, cx, cy, has && visible, rgb(dfg), rgb(dbg), nimages};
  (*env)->SetIntArrayRegion(env, meta, 0, 8, m);
  return JNI_TRUE;
}

// One Kitty image's pixels: {width, height, ARGB...}, or null if the terminal no longer
// holds it.
JNIEXPORT jintArray JNICALL Java_me_leoank_saarthi_Vt_imagePixels(JNIEnv *env, jclass cls, jlong h, jint id) {
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  jintArray out = NULL;
  pthread_mutex_lock(&v->lock);
  GhosttyKittyGraphics g = NULL;
  if (ghostty_terminal_get(v->term, GHOSTTY_TERMINAL_DATA_KITTY_GRAPHICS, &g) == GHOSTTY_SUCCESS && g) {
    GhosttyKittyGraphicsImage img = ghostty_kitty_graphics_image(g, (uint32_t)id);
    uint32_t w = 0, hh = 0;
    GhosttyKittyImageFormat fmt = 0;
    const uint8_t *data = NULL;
    size_t len = 0;
    if (img && ghostty_kitty_graphics_image_get(img, GHOSTTY_KITTY_IMAGE_DATA_WIDTH, &w) == GHOSTTY_SUCCESS &&
        ghostty_kitty_graphics_image_get(img, GHOSTTY_KITTY_IMAGE_DATA_HEIGHT, &hh) == GHOSTTY_SUCCESS &&
        ghostty_kitty_graphics_image_get(img, GHOSTTY_KITTY_IMAGE_DATA_FORMAT, &fmt) == GHOSTTY_SUCCESS &&
        ghostty_kitty_graphics_image_get(img, GHOSTTY_KITTY_IMAGE_DATA_DATA_PTR, &data) == GHOSTTY_SUCCESS &&
        ghostty_kitty_graphics_image_get(img, GHOSTTY_KITTY_IMAGE_DATA_DATA_LEN, &len) == GHOSTTY_SUCCESS && data && w &&
        hh) {
      size_t count = (size_t)w * hh;
      int bpp = fmt == GHOSTTY_KITTY_IMAGE_FORMAT_RGBA ? 4 : fmt == GHOSTTY_KITTY_IMAGE_FORMAT_RGB ? 3
              : fmt == GHOSTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA ? 2 : 1;
      if (len >= count * bpp && (out = (*env)->NewIntArray(env, (jsize)(2 + count)))) {
        jint *p = (*env)->GetPrimitiveArrayCritical(env, out, NULL);
        p[0] = (jint)w, p[1] = (jint)hh;
        for (size_t i = 0; i < count; i++) {
          const uint8_t *s = data + i * bpp;
          uint32_t r, gg, b, a;
          if (bpp >= 3) r = s[0], gg = s[1], b = s[2], a = bpp == 4 ? s[3] : 255;
          else r = gg = b = s[0], a = bpp == 2 ? s[1] : 255;
          p[2 + i] = (jint)((a << 24) | (r << 16) | (gg << 8) | b);
        }
        (*env)->ReleasePrimitiveArrayCritical(env, out, p, 0);
      }
    }
  }
  pthread_mutex_unlock(&v->lock);
  return out;
}

// What the program asked of the screen: bit 1 mouse reports, bit 2 the alternate screen.
JNIEXPORT jint JNICALL Java_me_leoank_saarthi_Vt_state(JNIEnv *env, jclass cls, jlong h) {
  (void)env;
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  bool tracking = false;
  GhosttyTerminalScreen screen = 0;
  pthread_mutex_lock(&v->lock);
  ghostty_terminal_get(v->term, GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING, &tracking);
  ghostty_terminal_get(v->term, GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN, &screen);
  pthread_mutex_unlock(&v->lock);
  return (tracking ? 1 : 0) | (screen == GHOSTTY_TERMINAL_SCREEN_ALTERNATE ? 2 : 0);
}

// A mouse event at (x, y) pixels in a width x height screen, as the program asked for
// them: action 0 press, 1 release, 2 motion; button 0 none, 1 left, 4 wheel up, 5 wheel
// down. Null when the program asked for none.
JNIEXPORT jbyteArray JNICALL Java_me_leoank_saarthi_Vt_mouse(JNIEnv *env, jclass cls, jlong h, jint action, jint button,
                                                            jfloat x, jfloat y, jint width, jint height) {
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  char buf[64];
  size_t len = 0;
  pthread_mutex_lock(&v->lock);
  ghostty_mouse_encoder_setopt_from_terminal(v->mouse, v->term);
  GhosttyMouseEncoderSize size = {sizeof size, (uint32_t)width, (uint32_t)height, v->cell_w, v->cell_h, 0, 0, 0, 0};
  ghostty_mouse_encoder_setopt(v->mouse, GHOSTTY_MOUSE_ENCODER_OPT_SIZE, &size);
  ghostty_mouse_event_set_action(v->event, (GhosttyMouseAction)action);
  if (button > 0) ghostty_mouse_event_set_button(v->event, (GhosttyMouseButton)button);
  else ghostty_mouse_event_clear_button(v->event);
  ghostty_mouse_event_set_position(v->event, (GhosttyMousePosition){x, y});
  if (ghostty_mouse_encoder_encode(v->mouse, v->event, buf, sizeof buf, &len) != GHOSTTY_SUCCESS) len = 0;
  pthread_mutex_unlock(&v->lock);
  if (len == 0) return NULL;
  jbyteArray out = (*env)->NewByteArray(env, (jsize)len);
  if (out) (*env)->SetByteArrayRegion(env, out, 0, (jsize)len, (const jbyte *)buf);
  return out;
}

// Moves the view through the scrollback by rows (up is negative); 0 back to the bottom.
JNIEXPORT void JNICALL Java_me_leoank_saarthi_Vt_scroll(JNIEnv *env, jclass cls, jlong h, jint rows) {
  (void)env;
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  GhosttyTerminalScrollViewport s = {0};
  if (rows == 0) s.tag = GHOSTTY_SCROLL_VIEWPORT_BOTTOM;
  else s.tag = GHOSTTY_SCROLL_VIEWPORT_DELTA, s.value.delta = rows;
  pthread_mutex_lock(&v->lock);
  ghostty_terminal_scroll_viewport(v->term, s);
  v->changed = 1;
  pthread_mutex_unlock(&v->lock);
}

JNIEXPORT void JNICALL Java_me_leoank_saarthi_Vt_free(JNIEnv *env, jclass cls, jlong h) {
  (void)env;
  (void)cls;
  Vt *v = (Vt *)(intptr_t)h;
  if (!v) return;
  ghostty_mouse_event_free(v->event);
  ghostty_mouse_encoder_free(v->mouse);
  ghostty_render_state_image_iterator_free(v->images);
  ghostty_render_state_row_cells_free(v->cells);
  ghostty_render_state_row_iterator_free(v->rows);
  ghostty_render_state_free(v->rs);
  ghostty_terminal_free(v->term);
  free(v->reply);
  free(v);
}
