---
id: AGS-20261116-gp98v6
type: agent_session
schema_version: 1
title: "battery health on nodes, message times, drafts kept, keep-awake signals agreeing"
model: "claude-opus-5-5"
harness: "claude-code"
started: 2026-11-16T14:40+00:00
finished: 2026-11-16T15:11+00:00
created_entities: []
edited_entities: []
outcome: done          # done | failed | aborted | pending
created: 2026-11-16
author: agent:claude
---

## Task
Four asks from the owner: battery health in the node's health metrics where there is a
battery; message times in the app if the messages have them; an unsent draft that
survives leaving a chat and opening another; and keep-awake's title, switch and toast,
which disagreed.

## What happened
- Battery: the node's sampler reads IOKit's `AppleSmartBattery` on macOS and
  `/sys/class/power_supply` on Linux (the system battery, not a peripheral's, and any
  mains supply) into `battery.charge_pct`, `battery.health_pct` (full-charge over design
  capacity, as macOS's *Maximum Capacity*), `battery.cycles`, `battery.charging`,
  `battery.on_power` and `battery.temp_c`; none without a battery. The app charts charge
  and health. On rogue: 51%, health 94%, 186 cycles.
- Message times: every item already had a time, but the node's read time, which after a
  restart is when it re-read the transcript. The core now takes the transcript line's own
  `timestamp`; the app shows it on prompts and replies. Conversations cached on a phone
  before this keep their read times.
- Drafts: kept per node and session in the model and the app's preferences, so they
  outlive the screen and the app.
- Keep-awake: the app took the latest health sample, which is often the usage poller's
  and carries no keep-awake metrics, so the switch and title came and went; the title's
  *letting it sleep* meant "on but idle"; the toast was the app's own text. Now the app
  reads each metric's latest value, the node sends the two keep-awake metrics the moment
  either changes, the screen reads them back five seconds after a switch, and the three
  say the same thing in the same words.
- Checked on the emulator against a hub and a node on rogue reading its own sessions, and
  the workspace built and tested on aarch64-linux (curiosity). There a keep-awake test
  failed before any of this: its stand-in `systemd-inhibit`, written and run at once, can
  fail with "Text file busy" while another test's thread forks; the test now retries it.
- Version 0.10.0 (app versionCode 11). Committed one change at a time, the version last.
