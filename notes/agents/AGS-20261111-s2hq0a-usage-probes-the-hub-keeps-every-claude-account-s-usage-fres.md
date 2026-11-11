---
id: AGS-20261111-s2hq0a
type: agent_session
schema_version: 1
title: "Usage probes: the hub keeps every Claude account's usage fresh with a throwaway turn every half hour"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-11T22:49+00:00
finished: 2026-11-11T22:49+00:00
created_entities: []
edited_entities: [PHS-20261106-3702ke]
outcome: done             # done | failed | aborted | pending
created: 2026-11-11
author: agent:claude
---

## Task
The owner asked whether the hub could launch throwaway sessions, with some randomness, to
keep each account's usage current, then to build it half-hourly; and to push the token
store and bump neusis, which was done first (saarthi 8a39128 pushed, neusis' input bumped,
left uncommitted for the owner).

## What happened
- Measured one probe turn first: about 18.7k input tokens, nearly all cached, 4 output,
  about $0.0015 at list price, a second.
- Built (ADR-0043): a node `Probe` command, run on its own task (the link handles commands
  inline, and a model turn would have held it up) and answered through the outbox; the
  node runs `claude -p` on Haiku through the login shell in a fresh temporary directory
  with an empty config directory, returns the `rate_limit_info`, removes the directory,
  and refuses without spawn roots. The hub's `--usage-probe-node` and
  `--usage-probe-minutes` (default 30): per token a wait drawn within 15%, skipped when a
  session reported lately, the first round within five minutes (or the interval, if
  shorter); `Live::command_within` for the longer wait. Hub module options
  `usageProbeNode`, `usageProbeMinutes`.
- Tests: an end-to-end test with a stand-in claude (both accounts probed under their own
  tokens in their own config directories, nothing listed, no directory left), and one
  that a node without spawn roots takes none; that one first passed even with the guard
  removed, because its node ran the real claude with a fake token, so it now runs the
  stand-in and fails without the guard.
- Live with the real Claude Code on a scratch hub at one-minute probes: the first probe
  after 55 s recorded work's windows (5h 3%, 7d 37%), no session listed, no directory
  left, no token in either log.
- Decided without asking: Haiku and the prompt, the 15% spread, the skip rule, probes on
  one named node only.
