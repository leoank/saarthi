---
id: AGS-20261113-1h4tz8
type: agent_session
schema_version: 1
title: "Phone tsnet: the client core embeds libtailscale on Android (ADR-0016), one identity per hub"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-13T01:23+00:00
finished: null
created_entities: [EXP-20261113-1h4ztf, RUN-20261113-1v2rh6]
edited_entities: [PHS-20261106-3702q3]
outcome: pending          # done | failed | aborted | pending
created: 2026-11-13
author: agent:claude
---

## Task
The owner asked whether the app embeds tsnet yet, then chose to build the phone's tsnet
before the egress proxy (phase item 2.4). Their choices: one tailnet identity per hub, the
auth key kept on the phone encrypted, a hub without a key still reached over the phone's
own network, persistent devices.

## What happened
- Planned [[EXP-20261113-1h4ztf]] for the two unchecked assumptions (the Android build,
  tsnet inside the app sandbox) and ran it on the emulator ([[RUN-20261113-1v2rh6]]).
  Go builds no c-archive for Android, so the phone gets `libtailscale.so` beside the core.
  The sandbox's netlink ban stopped `tsnet.Up`, as predicted; a Go file of ours added to
  the Android build gives netmon the interfaces from `getifaddrs` and the default route
  from the kernel's outbound route. tsnet then panicked for want of a log-state directory;
  the same file exports a setter for `TS_LOGS_DIR`, called before each start.
- The core: `Hub.connectViaTailnet` and `joinTailnet`, one running identity per state
  directory for the process's life, a few dial retries for a fresh netmap. The app: the
  hub editor's auth key and device name, the key sealed with AES-GCM under an Android
  Keystore key, state in `files/tailnet/<hub id>` deleted with the hub, the boot log's
  join step, Push's background connect through the same path. ADR-0049, the guide's
  "On the hub's tailnet", the architecture.
- Decided without asking: the device name defaults to `saarthi-<phone model>`; the
  stored key is handed to tsnet on every start (tsnet uses it only when it must log in);
  no network-change hook into netmon yet, though on Android it polls only every ten
  minutes, until the phone shows whether the keepalive and tsnet's reconnects suffice.
- Unplanned fix: the scratch hub's identity had been logged out by the tailnet and the
  hub never came back, because ADR-0048's key file was read only on the first join. A key
  file is now read on every start while it exists; the flags, the modules' option
  descriptions and the guide say so.
- Versions to 0.5.0 (app versionCode 5): the hub and the node change too.
- Not done: the owner's phone (only an APK install is mine to do there), a switch between
  Wi-Fi and mobile data.
