---
id: EXP-20261121-3k3cxv
type: experiment
schema_version: 1
title: Does a held notifications stream from the hub survive Doze and network changes on the phone, how quickly does a notification arrive with the screen off, and what does it cost in battery
project: PRJ-0001
status: active            # planned | active | paused | concluded | abandoned
outcome: null             # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-21
author: agent:claude
protocols: []
materials: []
tags: [push, app, tailnet]
review: pending
---

## Question
Phase [[PHS-20261117-f22kxe]] item 20 and ADR-0075 replace the push server with a stream
the app holds to the hub itself, in a foreground service, over its own tailnet identity.
Does that stream stay open, or come back by itself, through Doze, Samsung's battery
management, Wi-Fi and mobile data handing over, and the hub restarting? How long does a
notification take to arrive with the screen off, and what does holding it cost in battery?

## Hypothesis and predictions
H1: a foreground service keeps its network through Doze (the power-save firewall spares
processes in the foreground-service state), the 45 s beat keeps NATs and the radio from
dropping a quiet stream, and the service's reconnect covers network changes; a
notification arrives within a few seconds of the hub sending it, and the battery cost is a
few percent a day. If H1 is false we expect notifications delayed until the screen comes
on, or a stream that stays down after a network change, and the battery exemption (or
UnifiedPush) is needed.

## Plan
1. On the emulator, against a test hub: the stream held; a question's notification with the
   app in the background; the same in forced deep idle without the battery exemption; the
   network cut (airplane mode) and back.
2. On the owner's Fold, once ghost001's hub runs the release with `Notifications`: a day
   with the link on and the ntfy app's subscription paused; note each notification's delay
   (the hub's log has when it sent it) with the screen off for an hour or more, and after
   moving between Wi-Fi and mobile data.
3. The same with and without *Let it run in the background* (the battery exemption), and
   Samsung's *Unrestricted* battery setting.
4. The battery: Android's battery use for saarthi over the day, against a day with the link
   off.

## Success criteria
- Notifications arrive within 10 s of the hub sending them, screen off, over a day.
- The stream comes back by itself within a minute of any network change.
- Holding it costs under 5 % of the battery a day.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

Emulator, 2026-11-21 (step 1, recorded by hand, no run page): with the app in the
background a question's notification came over the stream 13 s after its session started
(the hub holds an alert 10 s); in forced deep idle with the screen off and no battery
exemption it still came in 14 s; with airplane mode on the service showed *Reconnecting*,
and with it off it held the stream again by itself. The hub logged the hold, the drop and
the hold again, and sent nothing to the phone's UnifiedPush endpoint while it was held.
Steps 2 to 4 wait for the phone.

## Conclusion
Written once, by a human, when status becomes `concluded`.
