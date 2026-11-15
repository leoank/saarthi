// EXP-20261114-raqjf0: feed a shell's kind of bytes into libghostty-vt and read the screen
// back as plain text. Prints one ===RESULT=== line; exits non-zero if the screen is wrong.
#include <stdio.h>
#include <string.h>
#include <ghostty/vt.h>

static char out[8192];
static size_t used;

static bool collect(void *u, const uint8_t *data, size_t len) {
  (void)u;
  if (used + len >= sizeof out) return false;
  memcpy(out + used, data, len);
  used += len;
  return true;
}

int main(void) {
  GhosttyTerminal t;
  if (ghostty_terminal_new(NULL, &t, 40, 6) != GHOSTTY_SUCCESS) return 2;
  const char *in[] = {
    "$ ls\r\n", "\033[1;34mdocs\033[0m  \033[32mrun.sh\033[0m\r\n",
    "placeholder\r\n", "\033[3;1H\033[2Kreplaced\r\n", "\033[5;10Hat 5,10",
  };
  for (size_t i = 0; i < sizeof in / sizeof *in; i++)
    ghostty_terminal_vt_write(t, (const uint8_t *)in[i], strlen(in[i]));
  GhosttyFormatterTerminalOptions o = GHOSTTY_INIT_SIZED(GhosttyFormatterTerminalOptions);
  o.emit = GHOSTTY_FORMATTER_FORMAT_PLAIN;
  o.trim = true;
  GhosttyFormatter f;
  if (ghostty_formatter_terminal_new(NULL, &f, t, o) != GHOSTTY_SUCCESS) return 3;
  GhosttyWriter w = {.write = collect, .userdata = NULL};
  if (ghostty_formatter_format(f, w) != GHOSTTY_SUCCESS) return 4;
  out[used] = 0;
  const char *want = "$ ls\ndocs  run.sh\nreplaced\n\n         at 5,10";
  int ok = strcmp(out, want) == 0;
  fputs(out, stderr);
  printf("\n===RESULT=== {\"screen_matches\": %s, \"bytes\": %zu}\n", ok ? "true" : "false", used);
  ghostty_formatter_free(f);
  ghostty_terminal_free(t);
  return ok ? 0 : 1;
}
