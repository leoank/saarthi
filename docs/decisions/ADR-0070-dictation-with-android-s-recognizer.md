# ADR-0070: Dictation with Android's recognizer

Status: accepted
Date: 2026-11-20
Amends: ADR-0055

## Context
ADR-0055 chose push to talk, held to record, with the transcript sent at once or edited
first, and left the recognizer to [[EXP-20261114-raqk9b]], which compares Android's
on-device recognizer, whisper.cpp on the phone and Whisper on the node. The experiment has
not run. On 2026-11-20 the owner asked for voice input now, and chose among the three, and
between tapping and holding.

## Decision
- **Android's own recognizer** (`SpeechRecognizer`), the on-device one where the phone has
  it (Android 12+), else the phone's default, which may use the network.
- **Tap to start, tap or a pause to stop.** The words so far show beside the mic while it
  listens; the result goes into the prompt at the cursor, never sent by itself (phase
  [[PHS-20261117-f22kxe]] item 12).
- The microphone permission is asked for at the first tap.
- Spoken replies (ADR-0055) and the experiment stay to do; the experiment can still move
  recognition elsewhere, and nothing outside the app depends on the choice.

## Alternatives considered
- Whisper on the node: better at coding words, but slower over the hub, and needs a node
  online with Whisper installed.
- whisper.cpp on the phone: accurate offline, at 75 to 150 MB more in the APK.
- Push to talk, held: as ADR-0055 had it; a tap suits longer dictation and one hand.

## Consequences
- Where the phone has no on-device recognizer, the voice may go to Google's servers.
- Units and code words may come out mangled; the words are edited before sending.
