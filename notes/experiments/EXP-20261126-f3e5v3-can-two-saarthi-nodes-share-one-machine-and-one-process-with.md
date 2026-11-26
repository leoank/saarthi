---
id: EXP-20261126-f3e5v3
type: experiment
schema_version: 1
title: "Can two saarthi nodes share one machine, and one process, without stepping on each other: hooks, the session registry, sockets, Claude settings and tsnet state?"
project: PRJ-0001
status: planned           # planned | active | paused | concluded | abandoned
outcome: null             # null | positive | negative | inconclusive | infra
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

## Conclusion
Written when status becomes `concluded`; an agent's draft waits for a human to approve it.
