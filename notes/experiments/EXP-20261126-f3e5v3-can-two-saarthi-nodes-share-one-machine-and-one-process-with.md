---
id: EXP-20261126-f3e5v3
type: experiment
schema_version: 1
title: "Can two saarthi nodes share one machine, and one process, without stepping on each other: hooks, the session registry, sockets, Claude settings and tsnet state?"
project: PRJ-0001
status: concluded          # planned | active | paused | concluded | abandoned
outcome: positive         # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-26
author: agent:claude
protocols: []
materials: []
tags: []
review: pending
---

## Question
Phase [[PHS-20261126-f3e51h]] makes the node usable as a library, so an app may embed one
on a machine where the owner's daemon already runs, or two in one process. What does the
daemon hold today that belongs to the machine or to the process, and which of it collides
when there are two?

## Hypothesis and predictions
- H1 (hooks): the hooks the node relies on for sessions it did not start live in the
  user's `~/.claude/settings.json` and point at one node; spawned sessions carry their
  own through `--settings` (ADR-0037). Two nodes: sessions each spawns report to it, but
  a session started by hand reports to whichever node the settings name. Prediction: an
  embedded node can only watch the sessions it starts, and should not touch the settings
  file.
- H2 (registry): each node scans Claude Code's session registry, so both report every
  bare session on the machine and the hub lists it twice. Prediction: an embedded node
  must not scan.
- H3 (files and ports): sockets, the state directory, the Claude API proxy and the
  wrapper socket collide unless each node has its own state directory; with one each,
  they do not.
- H4 (process): two nodes in one process trip over something process-wide: environment
  set with `set_var`, signal handlers, the current directory, a global subscriber for
  logs, or tsnet (one identity per process?). EXP-20261112-rq8kgc ran two tsnet
  identities in one process, so tsnet is predicted fine.

## Plan
1. Read `crates/node` for every use of the home directory, `set_var`, signals,
   `current_dir`, fixed paths and globals; list them.
2. Two `saarthi node serve` processes on one machine, separate state directories,
   against a scratch hub: spawn on each, start a bare session and a `saarthi run` one,
   compare what the hub lists and what `~/.claude/settings.json` holds (H1 to H3).
3. A test binary that starts two nodes through the library in one process and spawns a
   session on each (H4).
4. Record each as a run with its script; conclude, then write the ADR (phase item 2).

## Success criteria
A list of every collision found, each with the run that showed it, and for each either a
fix in the library's shape or a rule for an embedded node ("does not scan the registry").
Both nodes in one process spawn and finish a session each.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.
- [[RUN-20261126-hsed5n]] the node crate read for process- and machine-wide state: two statics, both the tailnet's (`link.rs`); no environment written; every default path overridable; signals, the log subscriber and the registry scan only in `main.rs`
- [[RUN-20261126-hsee10]] two daemons on one machine: with default paths the second silently takes the first's wrapper socket (and leaves it dead when it stops or fails), and its changes service will not start; with paths apart both run, but both list all 9 sessions and upload the same ~114,700 events, and a session spawned on one is listed by the other
- [[RUN-20261126-hseepp]] two library nodes in one process: each spawns and steers its own session and both survive a hub restart; once one node is set to dial through a tailnet, the other loses its plain link too (`not on the tailnet yet`)

## Conclusion
Draft by agent:claude, for the owner to approve.

Two nodes can share a machine and a process, once three things the daemon takes for granted
are made the node's own. Every collision found, with its run and its answer:

1. **The tailnet setting is the process's** ([[RUN-20261126-hseepp]]). `VIA_TAILNET` and
   `TAILNET` in `link.rs` are statics: one node set to dial through a tailnet makes every node
   in the process dial through it. H4 was right that tsnet is fine (EXP-20261112-rq8kgc) and
   wrong that nothing else is: the fix is saarthi's, the way to reach the hub moving into
   `link::Config`. Nothing else in the library is process-wide: no environment is written,
   and signals and the log subscriber are installed only by the binary
   ([[RUN-20261126-hsed5n]]).
2. **The default paths are the user's, not the node's** ([[RUN-20261126-hsee10]], A; H3
   right). A second node on the defaults deletes the first's wrapper socket and binds its own,
   silently, even when it then fails on the hook port, so the first is left deaf to
   `saarthi run`; its changes service will not start. Every path has a flag, and with them
   apart nothing collides. For the library: a node is given a state directory, and every
   socket, the hook port (or port 0) and the tailnet state derive from it; the `$HOME`
   defaults stay the daemon's. Separately, `Wrappers::bind` should refuse a socket another
   node answers on, as `changes::serve` already does, and that is a fix for the daemon too.
3. **A Claude dir is scanned by every node pointed at it** ([[RUN-20261126-hsee10]], B; H2
   right, and costlier than predicted). Two nodes on the user's `~/.claude` both list all 9
   sessions, and both upload all of their transcripts, about 114,700 events each, the same
   ones, which held the hub's database long enough for a spawn's reply to miss its 15 s
   deadline. The hub keys sessions by node and session, so it cannot merge them. A session
   one node spawns is listed by the other too, in another state. Rule: an embedded node does
   not scan the registry, does not serve wrappers or a terminal, and sees only the sessions it
   starts.
4. **Hooks** (H1 right). `~/.claude/settings.json` names one node, the machine's daemon; a
   spawned session carries its own node's hooks, and run 3's nodes each gave theirs a port of
   its own. But a session an embedded node starts in the user's Claude dir also reads that
   settings file, and is in the registry the daemon scans: the owner's daemon reports the
   app's sessions as bare ones, which is what b did with a's session in run 2.

So an embedded node should run its sessions under a Claude config dir of its own, as a
sandbox profile does (ADR-0038): neither the daemon's scan nor the user's hooks reach it.
Such a dir has no login, so its sessions run under the hub's Claude accounts through the
node's proxy (ADR-0051), which is also what the app's grant names (EXP-20261126-f3e5a9).
That last step follows from the code and the sandbox profiles, and was not run with a real
claude here: the example app's end-to-end test (phase item 17) should check it.
