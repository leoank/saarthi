---
id: EXP-20261021-a2ty0f
previous_id: manthan/EXP-0015
type: experiment
schema_version: 1
title: Does the phone app intervene in a real Claude Code session?
project: PRJ-0001
status: concluded            # planned | active | paused | concluded | abandoned
outcome: positive             # null | positive | negative | inconclusive | infra
influenced_by: [EXP-20261019-cqpw0d, EXP-20261019-fgm60e]
created: 2026-10-19
author: agent:claude
protocols: []
materials: []
tags: [intervention, phase-1, android, app, questions, prompts, interrupt]
review: pending
---

## Question
[[EXP-20261019-cqpw0d]] and [[EXP-20261019-fgm60e]] checked intervention with `rc2s2 ctl` as the client. Step 5
built the Android app over the client core, and the emulator smoke test drove it only
against synthetic data. Does the app itself, on the emulator, intervene in a real
interactive Claude Code session through the hub: answer a question, deny with a message,
prompt, interrupt with a follow-up, and kill?

## Hypothesis and predictions
The app calls the same control service through the core that `rc2s2 ctl` used, so each
action should land as it did there: an option chosen in the question form reaches the model
as the answer; Deny with a message stops the command and the model quotes the message; Send
on the session screen queues a user turn over the messaging socket; Interrupt, then send
stops a running turn and the follow-up is answered alone; Kill ends the session. The
conversation on the session screen should show the turns as they happen. If not, the step
that fails shows which layer: the app's form, the core's call, or the hub.

## Plan
1. A scratch hub and node on this machine (live state read only, hooks through
   `--settings` from `rc2s2 node hooks`), the emulator with the app connected to the hub
   through `adb reverse`, and one throwaway interactive session on haiku under `rc2s2 run`
   in a scratch directory under the repo, typed into by a script through a PTY. The same
   script drives the app through `uiautomator` and `adb input`: the question's option chip
   and Answer; Deny with a message on a command outside the working directory; Send on the
   session screen; Interrupt, then send on a long count; Kill.
2. Summarise from the hub's record of this session only: pending requests and how each
   closed, hook events, the model's replies from the session's transcript; keep the
   screenshots out of git.

## Success criteria
- The question closes as resolved and the model's reply names the option tapped.
- The denied command does not run and the model quotes the message.
- The prompt sent from the app is answered, and appears in the app's conversation.
- The interrupt stops the running turn and the follow-up is answered alone.
- Kill from the app ends the session and its wrapper.

## Runs
- [[RUN-20261019-mvz0r3]] question, deny, prompt, interrupt and kill from the app in one wrapped session: all five landed
- [[RUN-20261025-166zqd]] the same five from the rebuilt app's screens (cards, dialogs, the stop button, the kill menu): all five landed, the conversation follows and shows prompts as typed

## Findings
- The app on the emulator intervenes in a real interactive session through the hub, as
  `rc2s2 ctl` did: an option tapped in the question form reaches the model as the answer
  ("Blue"); Deny with a message from the card stops the command and the model quotes it;
  Send on the session screen is a user turn over the messaging socket; "Interrupt, then
  send" stops a running count at 1325 of 3000 and the follow-up is answered alone; Kill
  ends the session within a second ([[RUN-20261019-mvz0r3]]).
- The hub records the app's resolutions as by `phone`, the client token's name.
- The session screen shows the conversation live: the long count, the interrupt, the
  follow-up and its reply.
- Android 13's notification permission dialog appears over the app right after Connect;
  a person answers it once, a test grants it up front.
- The rebuilt app (2026-10-25) intervenes the same way through its new screens: a question
  card at the end of the session's conversation, a permission card under "Waiting for you"
  on the home screen with the reason asked in a dialog, Send and a stop button that
  interrupts and sends, Kill behind a confirmation ([[RUN-20261025-166zqd]]).
- A prompt sent over the messaging socket reaches the transcript inside Claude Code's
  "Another Claude session sent a message" banner with a paragraph of advice; the core strips
  it so the app shows what the person typed ([[RUN-20261025-166zqd]]).
- A Stop hook was recorded for the interrupted turn in [[RUN-20261025-166zqd]] where
  [[RUN-20261019-mvz0r3]] saw none on the same Claude Code version; the daemon's own
  "interrupted" event is the one to rely on.

## Conclusion
Draft by agent:claude, for the owner to approve.

Yes. The app on the emulator, through the hub, answered a question (the tapped option
reached the model), denied a command with a message the model quoted, sent a prompt that
was answered, interrupted a running count at 1325 of 3000 with a follow-up answered alone,
and killed the session within a second ([[RUN-20261019-mvz0r3]]). The rebuilt app's screens
of 2026-10-25 did the same five ([[RUN-20261025-166zqd]]), with the core stripping Claude
Code's peer-message banner so the app shows prompts as typed. The hub records the app's
resolutions under the client token's name. This closed phase 1 step 5.6. Whether an
interrupted turn fires Stop differed between the two runs on the same Claude Code version,
so the daemon's `interrupted` event remains the one to rely on. Checked on the emulator
only, not on a phone.
