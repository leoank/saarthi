---
id: EXP-20261019-fgm60e
previous_id: manthan/EXP-0014
type: experiment
schema_version: 1
title: Does intervention work in an interactive session under rc2s2 run?
project: PRJ-0001
status: concluded            # planned | active | paused | concluded | abandoned
outcome: positive             # null | positive | negative | inconclusive | infra
influenced_by: [EXP-20261016-rq1403, EXP-20261016-rq1404, EXP-20261016-rq1405, EXP-20261019-cqpw0d]
created: 2026-10-19
author: agent:claude
protocols: []
materials: []
tags: [intervention, phase-1, wrapper, questions, prompts, interrupt]
review: pending
---

## Question
[[EXP-20261019-cqpw0d]] checked allow, deny and fall-through in `-p` turns. The rest of step 4 needs an
interactive session: AskUserQuestion does not exist in `-p` mode, prompts go to the
messaging socket of a running session, and interrupt, kill and the keystroke release need
the `rc2s2 run` wrapper. Do they work against a real interactive Claude Code session?

## Hypothesis and predictions
If the phase 0 shapes carry over into the node ([[EXP-20261016-rq1403]], [[EXP-20261016-rq1404]], [[EXP-20261016-rq1405]]):
an AskUserQuestion held through PermissionRequest and answered from a client with free text
returns that text to the model; Enter typed at the wrapped terminal while a request is held
releases it at once and the queued Enter allows the dialog; a prompt from a client reaches
the idle session as a user turn; an interrupt with a follow-up prompt stops a long turn and
the follow-up is answered; kill ends the session. Also open: whether a socket prompt fires
UserPromptSubmit. If not, the turn shows which link failed: no pending request, a hold that
outlives the keystroke, no turn from the socket, or a follow-up that merges with the
interrupted prompt.

## Plan
1. A scratch hub and node on this machine (live state read only), and one throwaway
   interactive session on haiku started under `rc2s2 run` by a script that plays the
   person at the terminal through a PTY, in a scratch directory under the repo (so the
   workspace is trusted), hooks through `--settings`, `CLAUDE*` and `AI_AGENT` stripped.
   In order: a question answered with `rc2s2 ctl answer`; a command outside the working
   directory, answered by Enter at the terminal; a prompt with `rc2s2 ctl prompt`; a long
   turn interrupted with `rc2s2 ctl interrupt --then`; `rc2s2 ctl kill`.
2. Summarise from the hub's record of this session only: pending requests and how each
   closed, hook events, the model's replies, daemon events.
3. Added after the first runs, for what they left untested: a prompt from a client to a
   session started with `--dangerously-skip-permissions`, after one typed turn so the
   node knows its permission mode; and held requests answered at the terminal with
   Escape and with `2` ("yes, and don't ask again") instead of Enter.

## Success criteria
- The question closes as resolved and the model's reply carries the free-text answer.
- The terminal's Enter closes the held request as a keystroke within a second and the
  command runs.
- The socket prompt is answered as a user turn.
- The interrupt is recorded, the long turn stops, and the follow-up is answered alone.
- After kill, the session's process is gone and the wrapper exits.
- Bypass mode: the client is told the prompt is held, the model does not get it, and the
  terminal shows it as held for approval.
- Escape and `2` at the terminal each release the held request as a keystroke; Escape
  refuses the command and `2` runs it.

## Runs
- [[RUN-20261019-e63842]] question, keystroke release, socket prompt and kill in one wrapped session all worked; its interrupt reached an idle session
- [[RUN-20261019-e78rne]] interrupt of a running turn with a follow-up, then kill: stopped at 470 of 3000, follow-up answered alone
- [[RUN-20261019-fyy0e4]] a prompt to a bypass-mode session: the client was told it is held, and the terminal held it undelivered
- [[RUN-20261019-fzgrw0]] held requests answered at the terminal with Escape (refused) and 2 (allowed), each released in 2 ms

## Findings
- A wrapped session is recognised: the registry's pid is the wrapper's child, and the hub
  records the session as `LAUNCH_WRAPPER` ([[RUN-20261019-e63842]]).
- AskUserQuestion is held as a question and a free-text answer from `rc2s2 ctl` reaches the
  model as the tool result ("green, actually"; the model replied "green").
- Enter at the wrapped terminal releases a held request 2 ms after the keystroke and the
  queued Enter allows the dialog; a hold does not end by itself in the meantime.
- A prompt over the messaging socket is a user turn behind Claude Code's "Another Claude
  session sent a message" banner, and UserPromptSubmit fires for it, so prompts from the
  phone show up in the hook stream like typed ones.
- Interrupt stops a running turn mid-output (at 470 of 3000) with "[Request interrupted by
  user]"; no Stop hook fires, so the node's `interrupted` event is the only record; the
  follow-up typed after Ctrl-U is submitted alone ([[RUN-20261019-e78rne]]).
- Kill ends the session within about a second and the hub records the end.
- A prompt to a session started with `--dangerously-skip-permissions` is held by Claude
  Code ("Held peer message — from an unidentified session ... not delivered to Claude"),
  and the node's note from the hook's `permission_mode` says so to the client; a
  Notification hook fires for the approval prompt ([[RUN-20261019-fyy0e4]]).
- Any key at the wrapped terminal releases a hold, and the queued key answers the dialog:
  Escape refuses the tool and ends the turn without a Stop hook, `2` allows it
  ([[RUN-20261019-fzgrw0]]). A refusal at the terminal, like an interrupt, leaves no Stop.

## Conclusion
Draft by agent:claude, for the owner to approve.

Yes, every prediction held in a real interactive session under `rc2s2 run`. A question held
at PermissionRequest and answered with free text from a client reached the model as the
tool result; Enter at the wrapped terminal released a held request 2 ms after the
keystroke and the queued Enter allowed the dialog; a socket prompt was a user turn and
fired UserPromptSubmit; kill ended the session within about a second
([[RUN-20261019-e63842]]). An interrupt with a follow-up stopped a running count at 470 of
3000 and the follow-up was answered alone ([[RUN-20261019-e78rne]]). Escape and `2` at the
terminal also release a hold, refusing and allowing the tool ([[RUN-20261019-fzgrw0]]). In
a bypass-permissions session a socket prompt is held by Claude Code undelivered, and the
node's note tells the client ([[RUN-20261019-fyy0e4]]).

One thing to rely on elsewhere: an interrupt or a refusal at the terminal fires no Stop
hook, so the node's own `interrupted` event is the record. This closed phase 1 step 4.4.
