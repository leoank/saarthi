---
id: EXP-20261025-25f4bf
type: experiment
schema_version: 1
title: Does a prompt typed into the wrapper's PTY arrive as a plain user turn, and does one typed during a running turn queue rather than interrupt?
project: PRJ-0001
status: concluded            # planned | active | paused | concluded | abandoned
outcome: positive             # null | positive | negative | inconclusive | infra
influenced_by: [EXP-20261016-rq1403, EXP-20261019-fgm60e]
created: 2026-10-25
author: agent:claude
protocols: []
materials: []
tags: [prompts, pty, wrapper, phase-1.5]
review: pending
---

## Question
ADR-0027 moves a wrapped session's prompt from the messaging socket to the PTY, so the
model sees a typed prompt and not a peer message. The wrapper already types a follow-up
after an interrupt ([[EXP-20261016-rq1403]], [[RUN-20261019-mvz0r3]]), always into an idle input box.
Two things are unchecked: that a prompt typed while the session is idle reaches the model
with nothing added (no banner, the usual UserPromptSubmit, the usual permission mode),
and what happens to text typed while a turn is running: Claude Code queues typed input
behind the turn in recent versions, but whether the wrapper's text and Enter arrive as one
queued prompt, get split, or are lost has not been observed under rc2s2.

## Hypothesis and predictions
H1: a prompt typed into the PTY of an idle wrapped session appears in the transcript as a
plain user line equal to the text, UserPromptSubmit fires with that prompt, and the reply
answers it. H2: text and Enter typed during a running turn are queued by Claude Code and
become the next user turn, unchanged, after the running turn's Stop, with no interrupt.
If H2 is false we expect the prompt to be lost, truncated, or merged with the input box's
content, and the daemon must then wait for Stop before typing (ADR-0027, consequences).

## Plan
1. A scratch hub and node as in [[RUN-20261025-166zqd]], a haiku session under `rc2s2 run`
   in a scratch directory under `target/live-work-*`, and `rc2s2 ctl prompt` (or the app)
   against the daemon's new PTY path.
2. Idle: prompt "Reply with just the word pelican." Record the transcript's user line, the
   UserPromptSubmit payload, and the reply.
3. Busy: start a count to 3000 at the terminal, prompt "Reply with just the word heron."
   from the client after 3 s, and record whether the count finishes, whether a Stop fires
   before the follow-up's UserPromptSubmit, and what the follow-up's user line contains.
4. Bypass mode: the same idle prompt in a `--dangerously-skip-permissions` session, to see
   that nothing is held (ADR-0022 applied to the socket only).

## Success criteria
- The idle prompt's transcript line is exactly the text, with no peer banner, and the
  reply answers it.
- The busy prompt arrives as its own user turn after the running turn, unchanged; or the
  failure mode is recorded and the daemon's rule (wait for Stop) is chosen from it.
- The bypass-mode prompt is answered without a hold.

## Runs
- [[RUN-20261025-2aymst]] idle, busy and bypass-mode prompts typed through the wrapper: plain user turns, queued when busy with the hook at submission, never held

## Findings
- A prompt typed into the PTY of an idle wrapped session reaches the model as exactly the
  text: the transcript's user line equals it, UserPromptSubmit carries it, no peer banner
  ([[RUN-20261025-2aymst]]).
- Typed during a running turn, text and Enter are queued by Claude Code: UserPromptSubmit
  fires at submission (1 s after typing), no Stop and no interrupt before it, the running
  turn completes, and the queued prompt is answered next. The daemon need not wait for Stop
  before typing (ADR-0027's fallback is not needed) ([[RUN-20261025-2aymst]]).
- In bypass-permissions mode a typed prompt is not held; the hold of ADR-0022 was a property
  of the socket path only ([[RUN-20261025-2aymst]]).

## Conclusion
Draft by agent:claude, for the owner to approve.

Both hypotheses hold ([[RUN-20261025-2aymst]]). H1: a prompt typed into the PTY of an idle
wrapped session reaches the model as exactly the text, with UserPromptSubmit carrying it
and no peer banner. H2: typed during a running turn, the text and Enter are queued by
Claude Code, UserPromptSubmit fires at submission with no interrupt, and the prompt is
answered after the running turn, so the daemon need not wait for Stop before typing. In
bypass-permissions mode a typed prompt is not held; the hold was a property of the socket
path. This confirmed ADR-0027 without its fallback, built as phase 1.5 step 2.
