---
id: EXP-20261114-raqk9b
type: experiment
schema_version: 1
title: Which speech recognition transcribes short coding and lab commands well enough, on the phone or on the node, in a quiet room and at the bench
project: PRJ-0001
status: planned           # planned | active | paused | concluded | abandoned
outcome: null             # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-14
author: agent:claude
protocols: []
materials: []
tags: [voice, app]
review: pending
---

## Question
ADR-0055 adds push to talk and spoken replies to the app. Where should recognition run,
and with what: Android's on-device `SpeechRecognizer`, whisper.cpp on the phone, or
Whisper on the node (yajna's ADR-0012 chose server-side transcription for capture)? It
must handle short commands with coding and lab words ("rerun the failing test in saarthi
core", "spin at 13 000 g for ten minutes", "add 200 microlitres of TE") in a quiet room
and with the bench's noise (a centrifuge, a fume hood).

## Hypothesis and predictions
H1: on-device whisper.cpp (a small English model) has the lowest word error rate on the
jargon and answers within 1.5 s of release on the owner's phone; Android's recognizer is
faster but mangles units and names; the node's Whisper is the most accurate but its round
trip over the hub makes it slower, and it fails out of reach. If H1 holds, recognition
runs on the phone with whisper.cpp; if Android's recognizer is within a few points of it,
it wins on simplicity.

## Plan
1. Thirty commands written down, half coding, half bench, read by the owner into the phone
   twice: in a quiet room and beside a running centrifuge. Keep the recordings (they are
   the owner's voice: an `ART` page only if the owner agrees).
2. Transcribe each with the three engines; word error rate per engine and setting, with
   numbers and units scored exactly; time from release to text on the phone.
3. Android's text-to-speech reads ten assistant replies (with code and paths in them);
   note what it reads badly, to decide what a reply is stripped of before it is spoken.

## Success criteria
- An engine chosen from the error rates and times, with the noise's cost on each.
- A list of what a spoken reply must leave out or rewrite.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

## Conclusion
Written once, by a human, when status becomes `concluded`.
