---
id: PHS-20261106-3702q3
type: phase
schema_version: 1
title: "Phase 3: confinement and many networks"
project: PRJ-0001
status: active
since: 0428145cdafdefac83d1097e06c45ea55d6ff536
until: null               # the commit it finished at: yajna phase done
created: 2026-11-06
author: agent:claude
tags: []
---

## Goal
mxc sandbox profiles, embedded tsnet in hub, node and phone, PreToolUse policy rules, a terminal-level adapter, Postgres (`docs/architecture.md`, Phase 3).

## Progress
One item per piece of work, ticked when it lands; add items as the plan grows and keep the
done ones. The project page's *Progress* lists this phase on one line.
- [ ] Phase 3: confinement and many networks: mxc sandbox profiles, embedded tsnet in hub, node and phone, PreToolUse policy rules, a terminal-level adapter, Postgres
- [ ] 1. Confined spawned sessions on seatbelt (the owner's choices of 2026-11-07, [[AGS-20261107-413qp0]]):
  a `claude setup-token` token instead of leases, all egress, launch profiles in the node's
  config picked at spawn, and inside the nix daemon, git identity, the SSH agent, the
  owner's Claude files and tools
  - [x] 1.1 The token alone runs, resumes and leaves the keychain alone ([[EXP-20261107-413q6m]])
  - [x] 1.2 A stream-json session through mxc: stdin control, hooks over a unix socket,
    what is inside, resume; a kill must also end the tool process groups ([[EXP-20261107-413q82]])
  - [x] 1.3 Node: launch profiles from `--profiles`, host and seatbelt through mxc, the
    unix hook socket, the profiles' config dirs scanned beside `~/.claude`, spawn and
    resume under a profile, a hard kill that reaches the tool process groups; the profile
    name through the proto, hub and `ctl spawn --profile` (ADR-0038). Unit and end-to-end
    tests; checked live against a scratch hub with Claude Code 2.1.318: a held request
    allowed from `ctl`, git identity inside and the SSH key hidden, a kill mid-tool with no
    leftovers, a resume in the sandbox that kept the code word, History listing it
    (2026-11-07; [[AGS-20261107-413qp0]])
  - [x] 1.4 The app and the node's profiles: reported to the hub, picked on New session,
    shown on the session; `Session.profile` stored by the hub; `ctl nodes` lists them
    ([[AGS-20261107-7mpyze]])
  - [ ] 1.5 neusis: the profiles option and the token as an agenix secret
- [ ] 2. Embedded tsnet in the hub and the node, then the sandbox egress proxy (the owner's
  order and choices of 2026-11-12, [[AGS-20261112-rpj0rk]]): a first join by an auth key
  file, plain TCP kept, several tailnets on the hub, the peer's tailnet identity recorded
  - [x] 2.1 tonic over libtailscale on a real tailnet, two identities in one process,
    peers named, no descriptors leaked, a keyless restart ([[EXP-20261112-rq8kgc]])
  - [x] 2.2 `saarthi-tailnet`; the hub's `--tailnet` (repeatable) beside `--listen`; the
    node's `--tailnet`; the caller's tailnet and peer recorded and shown by `ctl nodes`;
    libtailscale `ac7ed43` in the flake; the modules' options; a live test gated on
    `SAARTHI_TAILNET_AUTHKEY_FILE`; checked live with the real binaries (ADR-0048)
  - [ ] 2.3 neusis: the hub's tailnets and a node's tailnet, with their auth keys as
    agenix secrets
  - [x] 2.4 The egress proxy that adds the Claude token, so a sandbox holds a dummy
    ([[EXP-20261110-g5mh89]]): the owner's choices of 2026-11-13 ([[AGS-20261113-28sfr0]]),
    egress kept open and only sandboxed sessions proxied; a proxy per sandboxed session on
    a unix socket in the profile's state directory, claude inside reaching it through
    `ANTHROPIC_UNIX_SOCKET` as plain HTTP, `/v1/` only (ADR-0050)
  - [x] 2.5 The phone's own identity per hub, ahead of 2.4 (the owner's order and choices
    of 2026-11-13, [[AGS-20261113-1h4tz8]]): one identity per hub, its auth key kept
    encrypted under the Android Keystore, a hub without a key on the phone's network,
    persistent devices
    - [x] 2.5.1 libtailscale for arm64 Android as a shared library beside the core, netmon
      interfaces from `getifaddrs`, a log directory for tsnet; `Hub.connectViaTailnet`;
      the hub editor's key and device name; the boot log's join; checked on the emulator
      with no Tailscale app (ADR-0049, [[EXP-20261113-1h4ztf]])
    - [x] 2.5.2 Checked on the owner's phone, including a switch between Wi-Fi and mobile
      data (the owner, 2026-11-13)
  - [x] 2.6 The hub holds every hub token and adds it (the owner's choice of 2026-11-14,
    [[AGS-20261114-d827jm]]): every session under a hub token, host, sandboxed or at a
    terminal, holds a dummy and reaches the API through the node's socket and the hub; the
    node's own login stays direct; a hub restart may cut running turns, zero-downtime
    restarts a later goal. De-risked by [[EXP-20261114-d82rcz]]. The owner's further
    choices of 2026-11-14: spawned and terminal sessions start in auto mode by default
    (the dummy loses the account's default), the hub records usage from the rate-limit
    headers it relays, compressing node-to-hub traffic is a later goal (ghost001 has
    10 TB a month). Built as ADR-0051 ([[AGS-20261114-kp7xq8]]): `ClaudeApi` on the node
    service, the node's proxy relaying for host, sandboxed, terminal and probe sessions,
    usage from the relayed headers; checked live against a scratch hub

## Notes
Migrated from PRJ-0001's Progress by `yajna phase migrate`.
