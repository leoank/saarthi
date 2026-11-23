# ADR-0078: Shells that persist

Status: accepted
Date: 2026-11-23
Amends: ADR-0054

## Context
A shell from the app (ADR-0054) is a login shell the node starts under a PTY when the
app's `Terminal` stream opens, and kills when that stream closes: ADR-0054 left keeping it
for a reconnect for later. In use that is the trouble. The phone sleeps, changes network,
or the hub or node restarts, the stream ends, and the shell and everything in it are gone.
The owner asked (2026-11-23) whether a proper SSH protocol would do better, pointing at
koh and a "tunnel shell", and asked for shells that persist until killed by hand.

SSH would not: an SSH session dies with its connection just the same. Every tool that
keeps a shell alive keeps it on the machine, apart from any connection: tmux, shpool (a
per-user daemon owning named shells), zmx (attach and detach, the screen restored on
reattach, several clients at once, built on libghostty-vt), mosh, Eternal Terminal, and
koh (mosh rewritten over QUIC, the server holding the terminal's state and sending screen
updates, the client reconnecting with backoff and buffering keystrokes meanwhile). SSH
would also bring back what ADR-0054 declined: keys on the phone and a second way into
every machine. The transport is not the problem; the shell's lifetime is.

The owner chose (2026-11-23): shells survive a node restart; several clients see the
same shell at once, as zmx does; 10,000 lines of scrollback to start.

## Decision
- **A shell holder per user on each machine**, `saarthi shells`, owns the shells: each a
  login shell under a PTY with an id, kept until it exits or is killed. It runs apart from
  the node, under its own launchd agent or systemd user unit from the node's module, so a
  node restart or upgrade leaves the shells running; a node run by hand starts one itself
  when none answers. The node is the holder's client over a unix socket.
- **The holder keeps each shell's screen** in libghostty-vt, with 10,000 lines of
  scrollback. Attaching first sends the screen as it is, written out by libghostty-vt's
  VT formatter (contents, cursor, style, modes, scrolling region), then the live output;
  a client starts from a fresh terminal each time, so a reattach is a redraw, not a replay
  of everything the shell ever printed. Full-screen programs come back as they were.
  [[EXP-20261123-j9wyqh]] found two details: the scrollback's byte cap is turned off so the
  line cap governs, and since the formatter reaches only the active screen, the holder
  formats the primary screen just before a program enters the alternate one and keeps it;
  under a full-screen program the snapshot is that, then the alternate screen.
- **Several clients at once.** Every attached client gets the output and may type; the
  shell takes the size of the client that attached or resized last.
- **Attach, list and kill are separate.** The `Terminal` stream attaches to a shell by id
  or starts a new one; closing it detaches. The hub can list a node's shells and kill
  one. Machines lists the shells each machine holds, not the ones the app remembers, so
  any client can find and reattach them.
- **The app reconnects by itself**: a dropped stream is attached again with backoff, at
  once on a new network, as the notification link does (ADR-0075), and what is typed
  meanwhile is held (up to 64 KiB) and sent once attached.
- [[EXP-20261123-j9wyqh]] checked first that the formatter's output, fed to a fresh
  terminal, restores the screen, the scrollback and a full-screen program's alternate
  screen: it does, about 300 KB for a full scrollback, formatted in a few milliseconds.

## Alternatives considered
- SSH from the phone, or mosh: see the context; neither keeps a shell by itself, and both
  need a way in to every machine besides the hub.
- Shells inside the node, kept across disconnects only: simpler, but a node restart (every
  rebuild) still loses them.
- tmux, shpool or zmx on each machine, the node attaching to them: another program to
  install and keep in step, and tmux takes over the screen's scrollback and keys.
- Replaying a byte log on reattach: grows without bound, and a full-screen program's
  screen comes back garbled when the log's start is cut.
- The newest client taking a shell over: the owner chose mirroring.

## Consequences
- A shell lives until it exits, is killed, or the machine reboots: forgotten shells use
  memory (each terminal and its scrollback) until killed from Machines.
- The node module gets a second service, and the holder links libghostty-vt for the
  machine's own platform, built in the flake as the Android one is.
- The `Terminal` stream and the hub gain shell ids, a list and a kill; an app older than
  this starts a new shell each time, as before.
