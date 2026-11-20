# ADR-0071: Photos saved on the node

Status: accepted
Date: 2026-11-20

## Context
The owner wants to add photos to a prompt from the phone's gallery or camera (2026-11-20).
Sessions take prompts three ways: a spawned one as a stream-json `user` line, a wrapped one
as text typed at its terminal, a bare one over its messaging socket. Only the first could
carry an image block; a terminal takes typed text only.

## Decision
- **The app shrinks** each photo to 2048 px on its long side, upright, as JPEG under about
  900 KB, and sends up to 3 MB of them with the prompt: `Prompt.attachments` at the node,
  `PromptRequest.attachments` at the hub. 3 MB keeps the call under gRPC's 4 MB a message
  on both links, so no limit changes.
- **The node saves them** where the session can read them, `~/.local/state/saarthi/uploads/
  <session>/`, or `tmp/uploads/<session>/` in a sandboxed profile's state directory, and
  adds a line `[Image: <path>]` per photo to the text before delivering or queuing it.
  Claude reads the file with `Read`, which shows it the image; this works for every kind of
  session.
- **No leave needed**: the path is outside the session's directory, so Claude Code asks
  before reading it. The node's hooks allow a `Read` of a file it saved for that same
  session at once (a plain file directly in that session's directory under one of the
  upload roots, nothing on the way a link); anything else is held as before.
- The app shows the `[Image: …]` lines of a prompt as `▣ name` tags.

## Alternatives considered
- Image blocks in the message, for spawned sessions only: Claude sees the image without a
  tool call, but terminal and bare sessions could not take photos.
- Saving into the session's own directory, which Claude reads without asking: it would
  leave files in the owner's repositories.

## Consequences
- Photos stay on the node after the session; nothing removes them yet.
- Each photo costs Claude a `Read` call.
- Two nodes on one machine (a test node beside the real one) both get a session's hooks;
  the one without the photo's root holds the read.
