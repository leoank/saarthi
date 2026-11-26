---
id: PHS-20261106-3701nh
type: phase
schema_version: 1
title: "Phase 1.5: fit for daily use"
project: PRJ-0001
status: done
since: 94b9a47b741e77138b56a6b0fdadf1f6970fd10d
until: d2088814420cdc9213a753b8bbc4d9203c24ab17
created: 2026-11-06
author: agent:claude
tags: []
---

## Goal
Phase 1.5: fit for daily use. From the owner's notes after using the app (`notes/tasks.md` of 2026-10-25, planned with their answers in [[AGS-20261025-25exxv]]); ADR-0027 to ADR-0030 in `docs/decisions/`

## Progress
One item per piece of work, ticked when it lands; add items as the plan grows and keep the
done ones. The project page's *Progress* lists this phase on one line.
- [x] 1. The app's reconnect bug: "Hub object has already been destroyed" shown many
  times. While reconnecting, the closed hub object stayed in place until the new one
  connected, so any command or refresh in between called it; the model now drops it first
  and says "reconnecting" instead (2026-10-25; [[AGS-20261025-25exxv]])
- [x] 2. Prompts through the PTY for wrapped sessions, the socket for the rest, with the
  app saying which the model will see (ADR-0027); de-risked by [[EXP-20261025-25f4bf]]:
  idle, busy and bypass-mode prompts all arrive as the person's own turn, a busy one queued
  (2026-10-25; [[RUN-20261025-2aymst]], [[AGS-20261025-27yxwz]])
- [x] 3. `rc2s2 node serve --ignore-cwd <glob>`: sessions under `*/target/live-work-*` are
  not tracked, so the run scripts stop reaching the live hub; `ignoreCwd` in the neusis
  node module, set for rogue, tests mirrored (T0 313/313); takes effect when the owner
  switches (2026-10-25; ADR-0028, [[AGS-20261025-27yxwz]])
- [x] 4. Notifications: an `ended` kind for a session that exited or was killed, and
  `--no-notify <kind>` on the hub so the policy is tuned by configuration once used;
  unit-tested, takes effect when the hub on ghost001 is rebuilt (2026-10-25; ADR-0029,
  [[AGS-20261025-27yxwz]])
- [x] 5. Usage in the app (ADR-0030): time and tokens under each turn from
  `turn_duration` and the daemon's usage deltas, the session's totals by model in its
  header and a Usage dialog, cost from a price table by model family kept in the core for
  now (the hub's table comes when the list needs cost); checked against `/cost` by
  [[EXP-20261025-25f8mm]]: the counts match the transcript, `/cost` adds interrupted
  requests (2026-10-25; [[RUN-20261025-2g2812]], [[AGS-20261025-27yxwz]])
- [x] 6. Rate-limit utilisation: the daemon reads the access token read only, calls the
  usage endpoint once a minute and reports `usage.five_hour_pct` and
  `usage.seven_day_pct` as health (the names the sampler already gives Claude Code's
  cached value, with `usage.age_s` 0 for a live one), charted per node; the Fable week
  (a `weekly_scoped` entry in the response's `limits`) too, as
  `usage.seven_day_fable_pct`, at the owner's request (2026-10-25). Built behind
  `rc2s2 node serve --claude-usage` with a `claudeUsage` option in the neusis module,
  off for rogue; the app charts both windows. Step 1 of [[EXP-20261025-25f6vm]] passed on a
  scratch item ([[RUN-20261025-9fytgj]]); steps 2 and 3, on the real credential, need the
  owner present, and the flag goes on after them (2026-10-25; [[AGS-20261025-92ty85]]).
  Steps 2 and 3 then passed on the real credential ([[RUN-20261025-c35jpc]],
  [[RUN-20261025-c5xjbh]]): no prompt, the windows as `/usage` shows them, the login
  intact across Claude Code's own refresh; but the endpoint refused once-a-minute polls
  with 429 for half an hour, so the poller now runs every five minutes and backs off on a
  429 (owner's choice). rc2s2 pushed and `claudeUsage = true` for rogue committed in neusis
  with the input bumped to `d52e5a4` (neusis `b309df4`, build-checked; the switch is the
  owner's) ([[AGS-20261025-c2ssaa]])
- [x] 7. The prompt bar and the person's own turns designed properly: a pill input with
  Send and Stop inside it, a state line above it (waiting for you, working with what Send
  and Stop do, started outside the wrapper), the person's turns tinted with the accent
  bar, line breaks kept in Claude's text (2026-10-25; [[AGS-20261025-2vf58e]])
- [x] 8. Delivery ticks instead of "Sent" toasts: a prompt from the phone shows in the
  conversation at once with one tick when the hub took it and two when the transcript
  echoes it, as messaging apps do; the node's note shows under it, not as a snackbar
  (2026-10-25; [[AGS-20261025-35g9vn]])
- [x] 9. Questions in bypass-permissions sessions reach the phone: [[EXP-20261025-35gght]]
  found PermissionRequest fires and the node holds the question in bypass mode too; the
  missed one fell in a window when the node was not forwarding (hooks not yet installed,
  or the crash loop after the first rogue switch). No change needed; retry on the phone
  and reopen if it recurs (2026-10-25; [[RUN-20261025-36bknd]], [[AGS-20261025-35g9vn]])
- [x] 10. A session opens at its most recent messages without scrolling through the
  history: the stored part lands in one piece, the list shows the last screenfuls and
  offers the earlier ones on demand (2026-10-25; [[AGS-20261025-35g9vn]])
- [x] 11. Diffs visible: an edit's old and new text shown under its row by default, with
  removed and added lines coloured, as the terminal shows them (2026-10-25;
  [[AGS-20261025-35g9vn]])
- [x] 12. Token labels as `/cost` uses them: "in" is uncached input only, cache read and
  write shown separately as "cached", so a turn does not read as 48k in when its context
  was cached; the header shows in and out (the counts themselves match the transcript,
  [[RUN-20261025-2g2812]]) (2026-10-25; [[AGS-20261025-35g9vn]])
- [x] 13. The hub link on the phone: a dead link was noticed only when the OS gave up on
  the TCP connection, a connect attempt could hang, and a reconnect backed off to 30 s
  even when the app came back to the front. Now the core's channel sends HTTP/2
  keepalive pings (15 s, 10 s timeout) and gives up connecting after 6 s; the app retries
  from 1 s up to 10 s, retries at once on resume, and checks a connected link with a list
  on resume, reconnecting if that fails; the session screen shows the state too. On the
  emulator the banner shows within 4 s of the hub dying and the link is back 2 s after
  the hub returns, the conversation kept (2026-10-25; [[AGS-20261025-8sxm34]])
- [x] 14. A session cache: opening a session replayed its whole event stream through the
  hub and the core each time (the usage events too, which is where the wait went); now
  the app keeps what it saw of each session in memory and on disk, shows it at once, and
  resumes the stream after the last item, with the usage totals seeded so the next turn's
  line counts only its own tokens; checked on the emulator on reopen and after a force
  stop (2026-10-25; [[AGS-20261025-8sxm34]])
- [x] 15. A failed send can be retried by tapping it, and the node's link to the hub and
  the hub's server send keepalive pings as the phone's does, so a machine that slept is
  noticed and its buffered events resent within half a minute of waking (the lost echo
  after rogue's screen turned off). Also from the same notes: the earlier-history control
  is a sticky header ("N earlier · show more"), since the edits the owner looked for sat
  behind it, and bold around code renders (2026-10-25; [[AGS-20261025-9rbnqz]])
- [x] 16. Keep awake: `rc2s2 node serve --keep-awake` holds `caffeinate -i` while a session
  is busy, `node.keep_awake` as health shown on the node's health screen, `keepAwake` in
  the neusis module, off until the owner turns it on; the switch from the app is a phase 2
  node command (ADR-0031; 2026-10-25; [[AGS-20261025-9rbnqz]])
- [x] 17. The file diffs of a session that writes files through Bash: the terminal shows
  the unified diff Claude Code computes for the files a command changed
  (`toolUseResult.bashEditDiff`; `structuredPatch` for an edit), so the core carries it
  on the result item and the app shows it under the row, red and green with hunk
  headers; without one, a command of several lines shows whole, the first 15 lines then
  "… N more lines" (2026-10-25;
  [[AGS-20261025-apwmfq]])
- [x] 18. Full diffs: a diff or a long script shows its first 15 lines under its row and,
  on a tap, opens whole in a full-screen view titled by its file; the core keeps an edit's
  text, a script and a call's diff up to 200,000 characters (a multi-line command had been
  cut at 4,000 by the generic detail limit, not the 16,000 meant). Checked on the emulator
  on a live session (2026-10-25; [[AGS-20261025-cnsmnq]])
- [x] 19. Wrapped text: one app-wide toggle in the session's bar and the full-screen view
  wraps diffs, code and tool output instead of scrolling them sideways, remembered across
  launches; checked on the emulator, a relaunch included (2026-10-25; [[AGS-20261025-cnsmnq]])
- [x] 20. Usage alerts (ADR-0032): the node reports each window's reset time, and the hub
  pushes once per window and reset when a week is on course to run out before it resets
  (from 20% used), when a window passes 80% or 95%, and when it reaches 100%; kind
  `usage`, a tap opens the node's health. Unit-tested and checked against a scratch hub
  with the real cached values (40% and 58% two days in: two "going fast" alerts, sent
  once); takes effect when the hub on ghost001 and rogue's node are rebuilt (2026-10-25;
  [[AGS-20261025-cnsmnq]])
- [x] 21. A user guide at <https://leoank.github.io/rc2s2/> (ADR-0033): mdBook in the yajna
  guide's theme under `docs/guide/`, built by `nix build .#docs` (also a flake check) and
  published by a Pages workflow; setup of the hub, a node, the app and notifications,
  using sessions, requests, health and `rc2s2 ctl`, troubleshooting, the command-line
  reference from `--help`, and the design section from `docs/`; README rewritten for
  users (2026-10-26; [[AGS-20261026-dedbsb]])
- [x] 22. `rc2s2 ctl` reads `hub` and `token_file` from `~/.config/rc2s2/ctl.toml`
  (or `$XDG_CONFIG_HOME`), flags and `RC2S2_*` variables still winning; unit-tested and
  checked against a scratch hub (2026-10-26; [[AGS-20261026-e9stb2]])
- [x] 23. Network in and out on the health screen, in MB/s from the bytes since the
  previous sample: the node always sent them, the app never charted them; checked on the
  emulator (2026-10-26; [[AGS-20261026-e9stb2]])
- Phase 1.5 is done when the owner has used the app for a week against ghost001 without
  reaching for `rc2s2 ctl` or the terminal for anything it covers

## Notes
Migrated from PRJ-0001's Progress by `yajna phase migrate`.
