// EXP-20261123-j9wyqh: does libghostty-vt's VT formatter restore a terminal in a fresh one?
// Feeds a terminal, formats it as VT, feeds that to a new terminal of the same size and
// compares their plain text (screen and scrollback) and cursor, for a long plain output, a
// styled prompt, and the alternate screen, before and after leaving it.
#include <ghostty/vt.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define COLS 80
#define ROWS 24

static GhosttyTerminal term(void) {
    GhosttyTerminal t;
    if (ghostty_terminal_new(NULL, &t, COLS, ROWS) != GHOSTTY_SUCCESS) { fprintf(stderr, "terminal_new\n"); exit(1); }
    // 10,000 lines (ADR-0078), and no byte cap besides, which would otherwise come first.
    size_t lines = 10000;
    ghostty_terminal_set(t, GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES, NULL);
    ghostty_terminal_set(t, GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES, &lines);
    return t;
}

static void put(GhosttyTerminal t, const char *s) { ghostty_terminal_vt_write(t, (const uint8_t *)s, strlen(s)); }

/** The terminal's active screen formatted: as VT with every extra, or as plain text; with
 *  [history], its scrollback too, through a selection from the first row to the last. */
static uint8_t *format_screen(GhosttyTerminal t, GhosttyFormatterFormat how, int history, int modes, size_t *len) {
    GhosttyFormatterTerminalOptions o;
    memset(&o, 0, sizeof o);
    o.size = sizeof o;
    o.emit = how;
    o.extra.size = sizeof o.extra;
    o.extra.screen.size = sizeof o.extra.screen;
    if (how == GHOSTTY_FORMATTER_FORMAT_VT) {
        o.extra.modes = modes;
        o.extra.palette = o.extra.scrolling_region = o.extra.tabstops = o.extra.pwd = o.extra.keyboard = true;
        o.extra.screen.cursor = o.extra.screen.style = o.extra.screen.hyperlink = true;
        o.extra.screen.protection = o.extra.screen.kitty_keyboard = o.extra.screen.charsets = true;
    }
    GhosttySelection sel;
    if (history) {
        size_t total = 0;
        ghostty_terminal_get(t, GHOSTTY_TERMINAL_DATA_TOTAL_ROWS, &total);
        GhosttyPoint a = {.tag = GHOSTTY_POINT_TAG_SCREEN, .value.coordinate = {0, 0}};
        GhosttyPoint b = {.tag = GHOSTTY_POINT_TAG_SCREEN, .value.coordinate = {COLS - 1, (uint32_t)(total - 1)}};
        memset(&sel, 0, sizeof sel);
        sel.size = sizeof sel;
        sel.start.size = sizeof sel.start;
        sel.end.size = sizeof sel.end;
        if (ghostty_terminal_grid_ref(t, a, &sel.start) != GHOSTTY_SUCCESS || ghostty_terminal_grid_ref(t, b, &sel.end) != GHOSTTY_SUCCESS) {
            fprintf(stderr, "grid_ref\n"); exit(1);
        }
        o.selection = &sel;
    }
    GhosttyFormatter f;
    if (ghostty_formatter_terminal_new(NULL, &f, t, o) != GHOSTTY_SUCCESS) { fprintf(stderr, "formatter_new\n"); exit(1); }
    uint8_t *out = NULL;
    if (ghostty_formatter_format_alloc(f, NULL, &out, len) != GHOSTTY_SUCCESS) { fprintf(stderr, "format\n"); exit(1); }
    ghostty_formatter_free(f);
    return out;
}

/** Plain text of the active screen with its scrollback, for comparing. */
static uint8_t *format(GhosttyTerminal t, GhosttyFormatterFormat how, size_t *len) { return format_screen(t, how, 1, 1, len); }

static int alternate(GhosttyTerminal t) {
    GhosttyTerminalScreen s = GHOSTTY_TERMINAL_SCREEN_PRIMARY;
    ghostty_terminal_get(t, GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN, &s);
    return s == GHOSTTY_TERMINAL_SCREEN_ALTERNATE;
}

/** The primary screen as it was when a full-screen program took over, kept by the holder
 *  (formatter output reaches only the active screen), and its length. */
static uint8_t *behind = NULL;
static size_t behind_len = 0;

/** Feeds output the way a holder would: before a sequence that enters the alternate
 *  screen, the primary screen is formatted, with its scrollback, while it is still the
 *  active one. The terminal itself is never changed to read it. */
static void feed(GhosttyTerminal t, const char *s) {
    static const char *enters[] = {"\x1b[?1049h", "\x1b[?1047h", "\x1b[?47h"};
    for (;;) {
        const char *first = NULL;
        for (int i = 0; i < 3; i++) {
            const char *at = strstr(s, enters[i]);
            if (at && (!first || at < first)) first = at;
        }
        if (!first) break;
        ghostty_terminal_vt_write(t, (const uint8_t *)s, (size_t)(first - s));
        if (!alternate(t)) {
            free(behind);
            behind = format_screen(t, GHOSTTY_FORMATTER_FORMAT_VT, 1, 1, &behind_len);
        }
        size_t n = strchr(first, 'h') - first + 1;
        ghostty_terminal_vt_write(t, (const uint8_t *)first, n);
        s = first + n;
    }
    put(t, s);
}

/** What a holder would send an attaching client: the screen with its scrollback; in a
 *  full-screen program, the primary screen it kept, then the alternate screen, whose
 *  modes take the client into it. */
static uint8_t *snapshot(GhosttyTerminal t, size_t *len) {
    if (!alternate(t) || !behind) return format_screen(t, GHOSTTY_FORMATTER_FORMAT_VT, 1, 1, len);
    size_t alen;
    uint8_t *a = format_screen(t, GHOSTTY_FORMATTER_FORMAT_VT, 0, 1, &alen);
    *len = behind_len + alen;
    uint8_t *out = malloc(*len);
    memcpy(out, behind, behind_len);
    memcpy(out + behind_len, a, alen);
    free(a);
    return out;
}

static void cursor(GhosttyTerminal t, uint16_t *x, uint16_t *y) {
    ghostty_terminal_get(t, GHOSTTY_TERMINAL_DATA_CURSOR_X, x);
    ghostty_terminal_get(t, GHOSTTY_TERMINAL_DATA_CURSOR_Y, y);
}

static int failures = 0;

/** Restores [a] into a fresh terminal and compares; returns the restored one. */
static GhosttyTerminal check(const char *name, GhosttyTerminal a) {
    size_t vlen, alen, blen;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    // The source as it was, to show the snapshot leaves it unchanged.
    size_t before_len;
    uint16_t bx0, by0;
    uint8_t *before = format(a, GHOSTTY_FORMATTER_FORMAT_PLAIN, &before_len);
    cursor(a, &bx0, &by0);
    uint8_t *vt = snapshot(a, &vlen);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    GhosttyTerminal b = term();
    ghostty_terminal_vt_write(b, vt, vlen);
    uint8_t *pa = format(a, GHOSTTY_FORMATTER_FORMAT_PLAIN, &alen);
    uint8_t *pb = format(b, GHOSTTY_FORMATTER_FORMAT_PLAIN, &blen);
    uint16_t ax, ay, bx, by;
    cursor(a, &ax, &ay);
    cursor(b, &bx, &by);
    int text = alen == blen && memcmp(pa, pb, alen) == 0;
    int cur = ax == bx && ay == by;
    int kept = before_len == alen && memcmp(before, pa, alen) == 0 && bx0 == ax && by0 == ay;
    if (!kept) { printf("%-28s the source CHANGED under the snapshot\n", name); failures++; }
    free(before);
    double ms = (t1.tv_sec - t0.tv_sec) * 1e3 + (t1.tv_nsec - t0.tv_nsec) / 1e6;
    printf("%-28s text %s (%zu vs %zu bytes)  cursor %s (%u,%u vs %u,%u)  vt %zu bytes in %.2f ms\n",
           name, text ? "same" : "DIFFERENT", alen, blen, cur ? "same" : "DIFFERENT", ax, ay, bx, by, vlen, ms);
    if (!text || !cur) failures++;
    free(vt); free(pa); free(pb);
    return b;
}

int main(void) {
    // 1. Long plain output past the scrollback cap, then a styled prompt, cursor mid-line.
    GhosttyTerminal a = term();
    char line[64];
    for (int i = 1; i <= 12000; i++) { snprintf(line, sizeof line, "line %05d of a long build log\r\n", i); put(a, line); }
    put(a, "\x1b[1;32mank@machine\x1b[0m:\x1b[34m~/src\x1b[0m$ git sta");
    size_t sb = 0;
    ghostty_terminal_get(a, GHOSTTY_TERMINAL_DATA_SCROLLBACK_ROWS, &sb);
    printf("scrollback rows kept: %zu of 12,000 lines written\n", sb);
    GhosttyTerminal b = check("scrollback and prompt", a);
    ghostty_terminal_free(b);

    // 2. A full-screen program: the alternate screen, a box, the cursor inside it.
    feed(a, "\x1b[?1049h\x1b[2J\x1b[H\x1b[7m top - 12:00:00 up 3 days \x1b[0m");
    for (int r = 3; r <= 20; r++) { snprintf(line, sizeof line, "\x1b[%d;5H\x1b[33m|\x1b[0m row %02d \x1b[33m|\x1b[0m", r, r); put(a, line); }
    put(a, "\x1b[10;12H");
    b = check("alternate screen", a);
    // What the restored terminal holds behind its alternate screen, looked at with mode 47.
    put(b, "\x1b[?47l");
    size_t behind;
    free(format(b, GHOSTTY_FORMATTER_FORMAT_PLAIN, &behind));
    put(b, "\x1b[?47h");
    printf("restored primary, behind the alternate screen: %zu bytes\n", behind);

    // 3. Leaving it, in both: the primary screen and its scrollback should come back.: the primary screen should come back the same.
    put(a, "\x1b[?1049l");
    put(b, "\x1b[?1049l");
    size_t alen, blen;
    uint8_t *pa = format(a, GHOSTTY_FORMATTER_FORMAT_PLAIN, &alen);
    uint8_t *pb = format(b, GHOSTTY_FORMATTER_FORMAT_PLAIN, &blen);
    uint16_t ax, ay, bx, by;
    cursor(a, &ax, &ay);
    cursor(b, &bx, &by);
    int text = alen == blen && memcmp(pa, pb, alen) == 0;
    printf("%-28s text %s (%zu vs %zu bytes)  cursor %s (%u,%u vs %u,%u)\n", "after leaving it", text ? "same" : "DIFFERENT",
           alen, blen, ax == bx && ay == by ? "same" : "DIFFERENT", ax, ay, bx, by);
    if (!text || ax != bx || ay != by) failures++;

    printf("===RESULT=== {\"failures\": %d}\n", failures);
    return failures ? 1 : 0;
}
