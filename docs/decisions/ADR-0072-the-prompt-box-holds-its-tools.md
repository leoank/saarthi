# ADR-0072: The prompt box holds its tools

Status: accepted
Date: 2026-11-20
Amends: ADR-0069

## Context
ADR-0069 put the model chip at the right end of the state line above the prompt bar. A
state line of two lines (*Started outside saarthi run · …*) was cut short beside it, so
the chip hid what the line said. Voice (ADR-0070) and photos (ADR-0071) each add a button,
and the one-line bar has no room for them. Two layouts were shown on 2026-11-20: the tools
on a row inside the box, or on a row of their own above it; the owner chose inside.

## Decision
- **The state line** has the full width above the box, up to two lines, with nothing
  beside it.
- **The box**: `❯` and the text; attached photos as thumbnails under the text; then, under
  a dashed rule, a row of tools: attach and the mic on the left, the model chip, stop and
  send on the right.
- **What is put into the text** (dictation) lands at the cursor.

## Alternatives considered
- The tools on a row of their own above the box, the state line under it: the box stays
  one line, but the tools float apart from the text they act on.

## Consequences
- The box is a row taller when empty.
- The chip is no longer read beside the state line, only in the box.
