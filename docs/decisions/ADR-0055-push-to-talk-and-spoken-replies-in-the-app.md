# ADR-0055: Push to talk and spoken replies in the app

Status: accepted
Date: 2026-11-14

## Context
The owner wants to speak to sessions and hear them answer, at the desk and at the bench
with gloved hands ([[AGS-20261114-rapbh2]]). It is useful for every session, not only
yajna's, so it belongs to the app (ADR-0053). yajna's ADR-0012 chose server-side
transcription for voice notes captured into the notebook; live commands differ (a reply is
wanted in a second or two, and the bench may be out of reach).

## Decision
- **Push to talk** on a session's screen and its prompt bar: held, it records; released,
  the transcript goes into the prompt bar, sent at once or edited first (a setting).
- **Spoken replies**, per session, on or off: each finished assistant message is read by
  Android's text-to-speech, with code blocks, paths and long lists replaced by a short
  mention; a spoken reply stops when push to talk is pressed.
- **The recognizer** is chosen by [[EXP-20261114-raqk9b]], between Android's on-device
  recognizer, whisper.cpp on the phone and Whisper on the node; nothing outside the app
  depends on the choice.
- Hands-free (a wake word, voice activity) is left for later; yajna's bench work reuses
  these two pieces for its commands.

## Alternatives considered
- Only yajna's server-side transcription: slower for commands, and nothing at the bench
  out of reach.
- Voice in yajna's screens only: every session would want it.

## Consequences
- A recording leaves the phone only if the node's Whisper is chosen.
- Replies written for a screen read badly aloud; what is left out is decided in the
  experiment and tuned with use.
