---
id: AGS-20261112-t4fx4s
type: agent_session
schema_version: 1
title: "App: a settings screen with several hubs, and a boot log in place of the connect screen and splash"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-12T23:47+00:00
finished: 2026-11-12T23:47+00:00
created_entities: []
edited_entities: []
outcome: done             # done | failed | aborted | pending
created: 2026-11-12
author: agent:claude
---

## Task
The owner's request of 2026-11-12: a separate Settings screen that will hold more and more
settings (they imagine several hubs, each with its own tailnet auth key); no login screen
on a first run; and in place of the splash, a black boot log in the manner of a NixOS or
Linux boot, with real information.

## What happened
- The owner chose, asked in one round: a hub list now with the tailnet key per hub later
  (the phone has no tsnet yet, ADR-0016, so no field that does nothing), and the boot log
  on each cold start only.
- Built: `Hubs.kt` (profiles in the preferences, the old address and token migrated into
  one hub and mirrored for the notification actions), `Boot.kt` (the log and its screen:
  kernel-style lines with microseconds since the process started, `<<< saarthi >>>`,
  systemd-style `[  OK  ]`, `[FAILED]`, `[ INFO ]` and a start-job spinner, Retry and
  Settings on failure), `Settings.kt` (Hubs, Display, About; the hub editor), `Connect.kt`
  removed, Home's menu entry *Settings*, a black system splash with no icon, the core's
  `core_version()`, the `ACCESS_NETWORK_STATE` permission, the guide's app page and the
  architecture.
- Decided myself: lines are printed 45 ms apart and the finished log stays a second, since
  a fast connect otherwise shows it for a blink while the window fades in (timestamps and
  content stay real); an address that is an IP gets no resolve line; a first run returns
  to Home once its first hub connects; `http://` is added to an address without a scheme.
- Checked on the emulator against a scratch hub: the migration of the old settings, the
  failure path, adding and switching hubs, a cold start that connects, a first run, and
  removing the only hub. The phone was not touched.
- Version stays 0.4.0: nothing has shipped since the bump to it.
