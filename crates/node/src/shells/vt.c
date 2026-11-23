// A shell's screen for the holder (ADR-0078): libghostty-vt fed the shell's output, and the
// screen written out as VT for a client that attaches, so its fresh terminal shows what the
// shell shows. EXP-20261123-j9wyqh found what this needs besides the formatter: the
// scrollback's byte cap off, so the line cap governs, and the primary screen kept as it was
// when a program entered the alternate one, since the formatter reaches only the active
// screen. The terminal answers no queries: the clients' terminals do. What libghostty-vt
// allocates goes back through ghostty_free; what this file allocates, through free.
#include <ghostty/vt.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    GhosttyTerminal t;
    // The primary screen with its scrollback, as VT, from just before the alternate screen
    // was entered; NULL outside a full-screen program.
    uint8_t *behind;
    size_t behind_len;
    // Bytes held back from the end of the last write: the start of a sequence that may
    // enter the alternate screen, whose rest comes with the next write.
    uint8_t carry[16];
    size_t carry_len;
} sh_term;

static const char *ENTERS[] = {"\x1b[?1049h", "\x1b[?1047h", "\x1b[?47h"};
#define N_ENTERS 3

static int alternate(GhosttyTerminal t) {
    GhosttyTerminalScreen s = GHOSTTY_TERMINAL_SCREEN_PRIMARY;
    ghostty_terminal_get(t, GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN, &s);
    return s == GHOSTTY_TERMINAL_SCREEN_ALTERNATE;
}

/** The active screen as VT with every extra; with [history] its scrollback too. */
static uint8_t *format(GhosttyTerminal t, int history, size_t *len) {
    GhosttyFormatterTerminalOptions o;
    memset(&o, 0, sizeof o);
    o.size = sizeof o;
    o.emit = GHOSTTY_FORMATTER_FORMAT_VT;
    o.extra.size = sizeof o.extra;
    o.extra.screen.size = sizeof o.extra.screen;
    o.extra.palette = o.extra.modes = o.extra.scrolling_region = o.extra.tabstops = o.extra.pwd = o.extra.keyboard = true;
    o.extra.screen.cursor = o.extra.screen.style = o.extra.screen.hyperlink = true;
    o.extra.screen.protection = o.extra.screen.kitty_keyboard = o.extra.screen.charsets = true;
    GhosttySelection sel;
    if (history) {
        size_t total = 0;
        uint16_t cols = 0;
        ghostty_terminal_get(t, GHOSTTY_TERMINAL_DATA_TOTAL_ROWS, &total);
        ghostty_terminal_get(t, GHOSTTY_TERMINAL_DATA_COLS, &cols);
        GhosttyPoint a = {.tag = GHOSTTY_POINT_TAG_SCREEN, .value.coordinate = {0, 0}};
        GhosttyPoint b = {.tag = GHOSTTY_POINT_TAG_SCREEN, .value.coordinate = {(uint16_t)(cols - 1), (uint32_t)(total - 1)}};
        memset(&sel, 0, sizeof sel);
        sel.size = sizeof sel;
        sel.start.size = sizeof sel.start;
        sel.end.size = sizeof sel.end;
        if (total > 0 && cols > 0 && ghostty_terminal_grid_ref(t, a, &sel.start) == GHOSTTY_SUCCESS &&
            ghostty_terminal_grid_ref(t, b, &sel.end) == GHOSTTY_SUCCESS)
            o.selection = &sel;
    }
    GhosttyFormatter f;
    uint8_t *out = NULL;
    *len = 0;
    if (ghostty_formatter_terminal_new(NULL, &f, t, o) != GHOSTTY_SUCCESS) return NULL;
    if (ghostty_formatter_format_alloc(f, NULL, &out, len) != GHOSTTY_SUCCESS) { out = NULL; *len = 0; }
    ghostty_formatter_free(f);
    return out;
}

static void drop_behind(sh_term *s) {
    if (s->behind) ghostty_free(NULL, s->behind, s->behind_len);
    s->behind = NULL;
    s->behind_len = 0;
}

sh_term *sh_new(uint16_t cols, uint16_t rows, size_t lines) {
    sh_term *s = calloc(1, sizeof *s);
    if (!s) return NULL;
    if (ghostty_terminal_new(NULL, &s->t, cols ? cols : 80, rows ? rows : 24) != GHOSTTY_SUCCESS) { free(s); return NULL; }
    size_t continuation = 4096;
    ghostty_terminal_set(s->t, GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES, NULL);
    ghostty_terminal_set(s->t, GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES, &lines);
    ghostty_terminal_set(s->t, GHOSTTY_TERMINAL_OPT_CONTINUATION_MAX_BYTES, &continuation);
    return s;
}

void sh_free(sh_term *s) {
    if (!s) return;
    ghostty_terminal_free(s->t);
    drop_behind(s);
    free(s);
}

/** Where in [p, p+n) one of the sequences entering the alternate screen starts, and its
 *  length in [*len]; NULL if none does. */
static const uint8_t *find_enter(const uint8_t *p, size_t n, size_t *len) {
    for (size_t i = 0; i < n; i++) {
        if (p[i] != 0x1b) continue;
        for (int k = 0; k < N_ENTERS; k++) {
            size_t m = strlen(ENTERS[k]);
            if (i + m <= n && memcmp(p + i, ENTERS[k], m) == 0) { *len = m; return p + i; }
        }
    }
    return NULL;
}

/** How many bytes at the end of [p, p+n) could begin a sequence entering the alternate
 *  screen whose rest has not come yet. */
static size_t partial_enter(const uint8_t *p, size_t n) {
    for (size_t back = 1; back < 9 && back <= n; back++) {
        const uint8_t *tail = p + n - back;
        if (*tail != 0x1b) continue;
        for (int k = 0; k < N_ENTERS; k++)
            if (back < strlen(ENTERS[k]) && memcmp(tail, ENTERS[k], back) == 0) return back;
        return 0;
    }
    return 0;
}

static void write_through(sh_term *s, const uint8_t *p, size_t n) {
    while (n > 0) {
        size_t m = 0;
        const uint8_t *at = find_enter(p, n, &m);
        if (!at) { ghostty_terminal_vt_write(s->t, p, n); return; }
        ghostty_terminal_vt_write(s->t, p, (size_t)(at - p));
        // The primary screen as it is, kept before the program takes the screen over.
        if (!alternate(s->t)) {
            drop_behind(s);
            s->behind = format(s->t, 1, &s->behind_len);
        }
        ghostty_terminal_vt_write(s->t, at, m);
        n -= (size_t)(at - p) + m;
        p = at + m;
    }
}

void sh_write(sh_term *s, const uint8_t *data, size_t len) {
    uint8_t joined[sizeof s->carry + 4096];
    // The bytes held back last time go first, joined with the start of these.
    while (s->carry_len > 0 && len > 0) {
        size_t take = len < 4096 ? len : 4096;
        memcpy(joined, s->carry, s->carry_len);
        memcpy(joined + s->carry_len, data, take);
        size_t n = s->carry_len + take;
        s->carry_len = 0;
        size_t hold = take == len ? partial_enter(joined, n) : 0;
        write_through(s, joined, n - hold);
        memcpy(s->carry, joined + n - hold, hold);
        s->carry_len = hold;
        data += take;
        len -= take;
    }
    if (len == 0) return;
    size_t hold = partial_enter(data, len);
    write_through(s, data, len - hold);
    memcpy(s->carry, data + len - hold, hold);
    s->carry_len = hold;
    if (!alternate(s->t)) drop_behind(s);
}

void sh_resize(sh_term *s, uint16_t cols, uint16_t rows, uint32_t cell_w, uint32_t cell_h) {
    if (cols && rows) ghostty_terminal_resize(s->t, cols, rows, cell_w, cell_h);
}

/** What a client attaching now is sent, malloc'd into [*out]: the screen with its
 *  scrollback; under a full-screen program, the kept primary screen, then the alternate
 *  screen, whose modes take the client into it; then any unfinished sequence, and the bytes
 *  held back, so the live output that follows lands as it did here. 0 on success. */
int sh_snapshot(sh_term *s, uint8_t **out, size_t *len) {
    size_t a_len = 0, c_len = 0, b_len = 0;
    uint8_t *a = format(s->t, !(alternate(s->t) && s->behind), &a_len);
    uint8_t *c = NULL;
    if (ghostty_terminal_continuation_alloc(s->t, NULL, &c, &c_len) != GHOSTTY_SUCCESS) { c = NULL; c_len = 0; }
    if (alternate(s->t) && s->behind) b_len = s->behind_len;
    *len = b_len + a_len + c_len + s->carry_len;
    *out = malloc(*len ? *len : 1);
    if (!*out) {
        if (a) ghostty_free(NULL, a, a_len);
        if (c) ghostty_free(NULL, c, c_len);
        return -1;
    }
    size_t at = 0;
    if (b_len) { memcpy(*out, s->behind, b_len); at += b_len; }
    if (a_len) { memcpy(*out + at, a, a_len); at += a_len; }
    if (c_len) { memcpy(*out + at, c, c_len); at += c_len; }
    memcpy(*out + at, s->carry, s->carry_len);
    if (a) ghostty_free(NULL, a, a_len);
    if (c) ghostty_free(NULL, c, c_len);
    return 0;
}

/** The title the shell or its program last set, copied into [buf]; its length. */
size_t sh_title(sh_term *s, uint8_t *buf, size_t cap) {
    GhosttyString title = {0};
    if (ghostty_terminal_get(s->t, GHOSTTY_TERMINAL_DATA_TITLE, &title) != GHOSTTY_SUCCESS || !title.ptr) return 0;
    size_t n = title.len < cap ? title.len : cap;
    memcpy(buf, title.ptr, n);
    return n;
}

void sh_buf_free(uint8_t *p) { free(p); }
