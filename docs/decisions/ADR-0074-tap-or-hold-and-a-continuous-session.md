# ADR-0074: Tap or hold, and a continuous session

Status: accepted
Date: 2026-11-21
Amends: ADR-0073

## Context
Push to talk (ADR-0073) still dropped words: restarting the recognizer at each pause
leaves a gap in which speech is lost. Holding the small mic through a long dictation was
tiresome, and the words being heard showed in a narrow strip beside it that filled at once
(the owner, 2026-11-21). Asked about a second engine (Whisper on a node, or whisper.cpp on
the phone, chosen in Settings), the owner chose neither for now.

## Decision
- **A continuous session** where the recognizer has one (Android 13+): one
  `SpeechRecognizer` session with `EXTRA_SEGMENTED_SESSION`, its stretches arriving as
  segment results, that does not end at a pause. Where it is turned down, the restarting
  way of ADR-0073 remains.
- **Tap or hold on one wide key**: a tap (under 350 ms) listens until the next tap; a press
  held longer listens until it is let go.
- **The words being heard show inside the prompt**, faint and italic at the cursor, until
  their stretch is put in; beside the mic only how to stop it, or why nothing came.

## Alternatives considered
- Whisper on a node or whisper.cpp on the phone as a second engine in Settings: no gaps and
  better with coding words, left for later.

## Consequences
- Android's recognizer stays the only engine.
- A session runs at most ten minutes before it is started again.
