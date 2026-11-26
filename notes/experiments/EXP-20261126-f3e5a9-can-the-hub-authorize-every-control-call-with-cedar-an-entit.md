---
id: EXP-20261126-f3e5a9
type: experiment
schema_version: 1
title: "Can the hub authorize every control call with Cedar: an entity model of clients, nodes, sessions and accounts, policies for an app limited to its own sessions, and the cost per call?"
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
Phase [[PHS-20261126-f3e51h]] replaces "any client token may do anything" (ADR-0021) with
Cedar policies over every control call. Before an ADR commits to it: can Cedar's schema
say what the hub needs, do the policies the owner wants decide correctly, and is an
authorization cheap enough to run per call and per listed session?

## Hypothesis and predictions
- H1: the `cedar-policy` crate builds in the project's flake for the hub's targets
  (aarch64 and x86_64 Linux, aarch64 Darwin) and a schema expresses: principals `Person`, and
  `Client` and `Node` each belonging to a person, in groups (`Admin`, `Member`, `App`), an action per control call grouped as read,
  operate and admin, resources `Node`, `Session`, `ClaudeAccount`, `Profile`, `Shell`,
  with a session's `launched_by`, `node` and labels as attributes. If false, the ADR
  weighs a hand-written role table instead.
- H2: four policy sets decide a table of requests as intended: the owner (an admin,
  everything), another person held to their own nodes, sessions and accounts and those
  shared with them, an app held to sessions it launched and spawns on its allowed nodes, profiles and
  accounts, never a terminal, shell or admin call; and a read-only monitor. If a case
  needs something the schema cannot say, that changes the entity model.
- H3: one authorization with entities built from the hub's state takes under 100 µs at
  1 000 sessions and 50 clients (p99), so a list can be filtered session by session. If
  not, lists filter by query (Cedar's partial evaluation, or the hub's own query from
  the policy) and the ADR says which.
- H4: a policy set replaced at run time takes effect on the next call with no restart.

## Plan
1. A spike under `spikes/16-cedar/`: the schema, the four policy sets, entities built as
   the hub would from its tables, a table-driven test of the requests.
2. A benchmark of H3 at 100, 1 000 and 10 000 sessions.
3. A build of the spike for each target through the flake (H1).
4. Record each step as a run with its script; conclude, then write the ADR (phase item 1).

## Success criteria
Every case in the table decides as intended, with the policies validated against the
schema; the benchmark's numbers recorded; the crate builds on all three targets. Any
case Cedar cannot express is named, with what the design does about it.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

## Conclusion
Written when status becomes `concluded`; an agent's draft waits for a human to approve it.
