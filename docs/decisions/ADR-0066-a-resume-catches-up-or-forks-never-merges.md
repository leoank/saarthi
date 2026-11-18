# ADR-0066: A resume catches up or forks, never merges

Status: accepted
Date: 2026-11-18
Amends: ADR-0059

## Context
A resume from the app (ADR-0036) runs `claude --resume` on the session's node with the
node's own transcript; only a node with none asks the hub for the hub's copy (ADR-0059).
The two copies can differ. The hub's can be ahead: the node's disk was restored from a
backup, or its transcript was replaced by an older one. The node's can have lines the hub
never got and the hub lines the node lost: they diverged. Resuming then continues from
whichever copy the node has, which may not be the conversation the person sees in the app.
The phase *The app at a glance* ([[PHS-20261117-f22kxe]], item 17), after atuin's resume,
asks to catch up or fork, never merge, and never to write to a session that is open.

## Decision
- **The hub names its last line.** `Resume` carries the uuid of the last transcript line the
  hub holds for the session (`last_line_id`).
- **The node compares, on its own transcript:**
  - it has that line (level, or ahead: the hub will get the rest from its tail): resume as
    before;
  - it lacks it, and the hub's copy (`SessionTranscript`) has the node's last line: the node
    is behind; the hub's lines after it are appended to the node's transcript, and the
    session resumes in place;
  - neither has the other's last line: they diverged. The session forks: the hub's copy, the
    conversation the person sees, is written as a new session's transcript under a new id
    and resumed there; the node's transcript is left as it was. The response names the new
    session, and the new one's copied lines name the old as their parent (phase item 15).
- **Never write to an open session.** A running session is refused, as before, and the
  node checks again right before writing.
- **No merge.** Lines of two copies are never interleaved into one transcript.

## Alternatives considered
- Always resuming from the hub's copy: loses what the node had that the hub did not, and
  Claude Code's own session-state lines (titles, file-history snapshots), which the hub does
  not keep.
- Comparing whole copies on every resume: a transcript runs to tens of megabytes; the hub's
  last line decides most cases, and the hub's copy is fetched only when the node lacks it.
- Merging diverged copies by time: two conversations that went different ways do not make
  one; Claude would read a history that never happened.

## Consequences
- A resume of a level session costs one search of the node's transcript for one uuid.
- A diverged session comes back as a new session in the app, under the one it came from.
