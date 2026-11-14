---
id: EXP-20261114-d82rcz
type: experiment
schema_version: 1
title: "Can every hub-token session reach the Claude API through the hub, which adds the token: does an interactive claude work over ANTHROPIC_UNIX_SOCKET with a dummy, what does a hub restart do to a turn mid-stream, and how many bytes does a turn move"
project: PRJ-0001
status: active            # planned | active | paused | concluded | abandoned
outcome: positive           # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-14
author: agent:claude
protocols: []
materials: []
tags: []
review: pending
---

## Question
ADR-0050 puts the Claude token in a proxy on the node; the owner wants every session that
runs under a hub token to reach the API through the hub instead, so the token never leaves
it and revoking a node's hub token cuts the node off at once (their choice of 2026-11-14,
[[AGS-20261114-d827jm]]: all hub tokens, host and terminal sessions included; the node's
own login stays direct; a hub restart may cut running turns). Three things decide whether
that works: does an interactive `claude`, as `saarthi run` and host spawns start it, work
over `ANTHROPIC_UNIX_SOCKET` with a dummy token; what does a hub that goes away mid-stream,
or is down when a turn starts, do to the turn; and how many bytes a turn moves through
the hub.

## Hypothesis and predictions
H1: the interactive TUI behaves as `-p` did in [[EXP-20261110-g5mh89]]: it starts with no
login prompt, answers through the socket, and the direct calls that fail with the dummy
(settings, policy limits, the account) cost nothing visible beyond, at most, the account
line. If it asks to log in or refuses the dummy, host and terminal sessions keep the token
and only sandboxed sessions go through the hub.

H2: Claude Code retries a failed or cut API request with backoff, so a hub back within
seconds costs one retried request and the turn completes; a long outage ends the turn
with an API error, the session still resumable. If a cut stream leaves the turn
truncated or the session broken, the hub needs a drain before it restarts.

H3: request bodies grow with the conversation (the whole context is sent each time,
caching does not shrink it on the wire): tens of KB early, hundreds of KB to a few MB in a
long session; answers are a few KB. If a turn moves several MB, the hub's uplink matters
and requests should be compressed between node and hub.

## Plan
1. A two-hop rig standing in for the design (`spikes/13-hub-proxy/`): a node hop on a unix
   socket that relays bytes to a hub hop over TCP and answers 502 when the hub is down; a
   hub hop that adds the token, forwards over TLS to `api.anthropic.com`, and logs per
   request the path, status, bytes each way, duration and the rate-limit headers it sees.
2. Interactive: drive the TUI in a PTY with a fresh config directory, a dummy token and
   the socket; two prompts, a look at what the screen shows, then exit.
3. Restart: a long streamed answer through the rig; the hub hop exits once the stream has
   relayed some bytes and comes back after 2 s, 15 s and 90 s; and a turn started while
   the hub is down for 20 s. Record whether the turn finishes, how many requests the hub
   saw, and whether a resume works afterwards.
4. Bytes: a realistic headless session with tools (reading code in a copy of the crates)
   and two resumes, recording each request's size.

## Success criteria
- H1 answered either way, with what the screen showed.
- For each outage: finished, failed with an error, or broken, and whether it resumes.
- Bytes per request and per turn for a session with tools, and whether the hub sees the
  rate-limit headers that ADR-0042 reads from sessions.

## Findings
All three held, with two differences for interactive sessions
([[RUN-20261114-gd7jeb]], [[RUN-20261114-gd816v]], [[RUN-20261114-gd8fn2]]).

- H1: the interactive TUI runs over the socket with only the dummy: no login prompt, answers
  through the hub hop. Two differences from the real token in its environment: a startup
  warning, `Remote managed settings failed to load (authentication rejected (401)) · no
  remote policy applied`, and the starting permission mode is manual where the account's
  default is now auto. Auto mode itself works with the dummy: shift-tab reaches it, and a
  headless turn with `--permission-mode auto` ran a shell command unasked as the baseline
  did. A node that wants auto by default can pass it.
- H2: a hub gone mid-stream for 2, 15 or 90 s cost one retried request each: Claude Code
  dropped the cut stream, resent the whole request with exponential backoff, and the
  answer came out whole and once; a resume worked after each, and so did a turn started
  while the hub was down. An outage of about 145 s exhausts the retries and ends the turn
  with `API Error: 502` and the node's message; `CLAUDE_CODE_MAX_RETRIES` exists to
  lengthen that (not tried).
- H3: requests are the whole context every call: 27 to 95 KB in a short session with tools
  (2.66 bytes per context token), answers a few KB. Applied to this machine's last 30 days
  of transcripts: 0.6 MB per request at the median, 1.3 MB at p90, 2.4 MB at most; a long
  session sends hundreds of MB up, the largest 2.7 GB; about 11 GB in 30 days. Through the
  hub that crosses the hub's link twice.
- The hub sees the unified rate-limit headers (5h, 7d, overage, status, resets) on every
  answer, the windows ADR-0042 reads from the sessions today.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

- [[RUN-20261114-gd7jeb]] the interactive TUI and headless auto mode with a dummy through a two-hop rig, against the real token as a baseline; works, starts in manual mode with a remote-settings warning
- [[RUN-20261114-gd816v]] the hub hop gone mid-stream for 2, 15 and 90 s, and down at a turn's start; every turn finished whole and resumed, about 145 s of outage ends a turn
- [[RUN-20261114-gd8fn2]] bytes per request in a session with tools, and an estimate for this machine's real sessions; 0.6 MB median, 2.4 MB max per request, about 11 GB in 30 days
- [[RUN-20261114-gd7jeb]] compute run, not yet described
- [[RUN-20261114-gd816v]] compute run, not yet described
- [[RUN-20261114-gd8fn2]] compute run, not yet described

## Conclusion
Written once, by a human, when status becomes `concluded`.
