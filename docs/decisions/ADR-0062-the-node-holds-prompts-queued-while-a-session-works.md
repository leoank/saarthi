# ADR-0062: The node holds prompts queued while a session works

Status: accepted
Date: 2026-11-18

## Context
A prompt sent while Claude works went straight to Claude Code, which keeps its own queue
of input typed during a turn (ADR-0027) and takes it at its next step. Nobody could see
that queue from the phone or take a prompt back. The phase *The app at a glance*
([[PHS-20261117-f22kxe]], item 7), after ovis, asks for a queue the person sees: queued
prompts as bubbles above the prompt bar, each with *Send now* and discard.

## Decision
- **The node holds the queue.** A prompt sent with `queue` set (`PromptRequest.queue`,
  node `Prompt.queue`) to a session the node sees busy is held in memory instead of
  delivered; the response says it was queued. The node delivers the oldest when the
  session is next idle, one per turn: after a delivery it waits to see the session busy,
  or 15 s, before the next, as a short turn can start and end between two looks at the
  registry's status.
- **The queue travels as a daemon `queue` event** on the session, the whole list each time
  it changes, as the context report does (ADR-0061). A client shows it from the
  conversation it streams; the hub stores and relays it like any event and needs nothing
  new.
- **`Unqueue` takes one off**: `send` delivers it at once, the way a prompt sent while
  Claude works always was (Claude Code takes it at its next step); otherwise it is
  dropped. A queued prompt the node no longer holds gets the note "it was no longer
  queued", and the queue as it is goes out again, which clears a client's stale list.
- **The app queues while the session is busy** and sends at once otherwise; `saarthi ctl`
  and the terminal UI send at once as before.

## Alternatives considered
- The queue in the app: lost when the app closes, unseen from another device, and nothing
  to deliver it when the phone is asleep.
- The queue in the hub: it knows a session's state only from the node, a step later, and
  would need storage of its own for something the node can deliver directly.
- Persisting the node's queue across restarts: the node keeps no other session state on
  disk, and spawned sessions do not outlive it; the stale list heals on the first touch.
- Interrupting for *Send now*: the stop button already does that; *Send now* keeps the
  turn going and lets Claude take the prompt at its next step.

## Consequences
- What waits for a turn is visible and can be changed until it goes.
- An older node ignores `queue` and delivers at once, as before; `Unqueue` to it fails as
  a command with no action.
- A node restart loses its queues; the app may show the last list until it is touched.
