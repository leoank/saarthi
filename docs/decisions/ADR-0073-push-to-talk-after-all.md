# ADR-0073: Push to talk after all

Status: accepted
Date: 2026-11-21
Amends: ADR-0070

## Context
ADR-0070 started dictation with a tap and let a pause end it. Used on the phone, Android's
recognizer ended at the first short pause, so a sentence was cut off while the owner
thought (2026-11-21). The recognizer always ends at a pause; the silence lengths a caller
may ask for are hints most recognizers ignore.

## Decision
- **The mic is held to talk**, as ADR-0055 first had it: pressed, it listens; let go, it
  stops, and what was heard goes in at the cursor, never sent by itself.
- **A pause does not end it while held**: each time the recognizer ends, its words go into
  the prompt and it listens again at once; silence while held is not an error.
- Longer silences are still asked for, for the recognizers that heed them.

## Alternatives considered
- Tap to start and tap to stop, restarting through pauses the same way: hands-free between
  taps, but nothing tells the person it is still listening once the phone is lowered.

## Consequences
- A word said in the instant the recognizer restarts may be lost.
- A long dictation arrives in pieces, each put in as it ends.
