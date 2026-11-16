# ADR-0059: A node restores a transcript from the hub to resume a session

Status: accepted
Date: 2026-11-16
Amends: ADR-0036

## Context
Resuming a finished session (ADR-0036) runs `claude --resume` on its node, which needs the
session's transcript in the node's Claude config directory. A node that was wiped (a
reinstall, a live disk after a reboot, a deleted `~/.claude`) has none, and resume was
refused, although the hub keeps the session's transcript lines. The owner asked that such
a session resume from the hub's data ([[AGS-20261116-hyer5d]]).

The hub kept only `user`, `assistant` and `system` lines. Claude Code chains a
conversation through each line's `parentUuid` and resumes by following it back from the
last line, and `attachment` lines (a mentioned file, a hook's output, reminders) are links
in that chain: in one of the owner's transcripts 2,798 kept lines had an attachment as
their parent, and the chain from the last line ran through 264 lines the hub did not have.

## Decision
- **The node also sends `attachment` lines.** Across the owner's transcripts they add
  about 15% to what the hub stores (76 MB against 499 MB).
- **A node asks the hub for a transcript it lacks.** On a resume with no transcript on the
  node, it calls `SessionTranscript`, which streams its own session's transcript lines in
  the order it read them, rebuilds the main conversation (subagents' lines and repeats
  left out) and writes it where Claude Code reads it, then resumes as before.
- **Where it goes:** the hub names the launch profile the session ran under in `Resume`,
  and the transcript goes into that profile's config directory, so a sandboxed session
  stays sandboxed; a node without that profile refuses.
- **Gaps are bridged.** A line whose parent is missing (an attachment the hub received
  before this) is chained to the line before it, so older sessions resume whole, without
  those attachments.

## Alternatives considered
- The hub sending the transcript with the `Resume` command: one message, but a transcript
  runs to tens of megabytes, past what a command should carry, and most resumes do not
  need it.
- Bridging gaps without storing attachments: no extra storage, but every restored session
  would lose the context they carried.
- Restoring subagents' transcripts too: `--resume` does not need them; left out until
  something does.

## Consequences
- The hub's copy of a transcript is now a record a node can be rebuilt from, so a machine
  can be wiped without losing its sessions.
- The session's directory must exist on the node; the transcript is restored, the project
  is not.
- A restored transcript has only the lines the hub kept: Claude Code's session-state lines
  (titles, modes, file-history snapshots) are not there, so `/rewind` to before the
  restore has no file snapshots.
