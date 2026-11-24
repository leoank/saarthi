---
id: AGS-20261123-k10zjq
type: agent_session
schema_version: 1
title: investigate claude token fallback on rogue
model: "gemini-3.8-flash"
harness: "antigravity"
started: 2026-11-23T17:19+00:00
finished: 2026-11-23T17:28+00:00
created_entities: []
edited_entities: []
outcome: done
created: 2026-11-23
author: agent:antigravity
---

## Task
Investigate why starting a `saarthi run` session on rogue with a different Claude token always falls back to the node login account.

## What happened
Investigated `saarthi run` behavior, `crates/node/src/run.rs`, `crates/node/src/wrapper.rs`, daemon socket communication, and Home Manager configuration in neusis (`agent-harness`).

Found two core causes:
1. Double-wrapping in `claudeRunPkg`: In neusis, `claude` is wrapped such that if stdin/stdout are a terminal (`[ -t 0 ] && [ -t 1 ]`), it runs `exec saarthi run --command <bare-claude> -- "$@"`. When `saarthi run` is invoked with `--claude-token <name>`, its default `--command` is `"claude"`. Because `saarthi run` opens a PTY, the child `claude` (the wrapper) detects a terminal and launches a nested `saarthi run` without `--claude-token`. The inner `saarthi run` asks the daemon for `NodeDefault` (or falls back to the machine's own login if no default token is configured), stripping the proxy environment configured by the outer invocation.
2. Token spend limits and UI credential display: Both hub tokens (`bodygram` and `mrclmb`) have hit 100% of their 5-hour rate/spend limits (status 429). Furthermore, `saarthi-proxied` only relays `/v1/` inference calls; non-inference endpoints fail with the dummy token, causing Claude Code to display the cached identity and Keychain login (`kumar@mrclmb.ac.uk`, the node login account).
