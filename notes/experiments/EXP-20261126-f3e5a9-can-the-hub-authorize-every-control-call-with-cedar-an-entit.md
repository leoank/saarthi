---
id: EXP-20261126-f3e5a9
type: experiment
schema_version: 1
title: "Can the hub authorize every control call with Cedar: an entity model of clients, nodes, sessions and accounts, policies for an app limited to its own sessions, and the cost per call?"
project: PRJ-0001
status: concluded         # planned | active | paused | concluded | abandoned
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
- [[RUN-20261126-fjh58z]] the schema and 22 policies validate strictly; all 59 cases decide as intended; a policy set swapped at run time applies on the next call
- [[RUN-20261126-fjh6wk]] one decision about 14 µs at any size; a list filtered session by session 15 ms at 1 000 sessions, 144 ms at 10 000; entities kept live and upserted, never rebuilt per call
- [[RUN-20261126-fjh6d5]] builds and passes the table on aarch64-darwin, aarch64-linux and x86_64-linux

## Conclusion
Draft by agent:claude, for the owner to approve.

Positive: the hub can authorize every control call with Cedar (cedar-policy 4.15.0).

- H1 holds. One schema says all of it: people in roles; device clients *in* their person,
  so a phone has its person's rights; apps *not* in their person, held to a grant record
  (nodes, profiles, accounts, terminal); nodes owned, shared, or embedded in an app; the
  spawn's profile and account as entities in the request's context; labels as entity tags.
  The 22 policies validate against it in strict mode ([[RUN-20261126-fjh58z]]), and the
  crate builds and passes on all three of the hub's targets ([[RUN-20261126-fjh6d5]]).
- H2 holds: 59 cases across the owner, another person, apps, a monitor and nodes all
  decide as intended, with the deciding policies named ([[RUN-20261126-fjh58z]]). Nothing
  the phase needs was inexpressible. Sharing reads naturally (`principal in
  resource.shared_with`), and a `forbid` on a label overrides even the admin.
- H3 holds for a call (about 14 µs at p50, 18 µs at p99, flat from 100 to 10 000
  sessions), and filtering a list session by session is fine at today's sizes (15 ms at
  1 000) but not at 10 000 (144 ms) ([[RUN-20261126-fjh6wk]]). So lists narrow first by
  the hub's own query (the sessions an app launched, a person owns, on their nodes) and
  ask Cedar only about those; admins and monitors skip the filter. Partial evaluation
  was not tried.
- H4 holds: a policy set swapped behind a lock applies on the next call.

What it means for the design: the hub keeps one live entity set, built at start and
updated as sessions, clients and grants change (0.07 to 1.75 ms per update); rebuilding
entities from JSON per call costs about 35 times the decision. A request outside the
schema fails when it is built, before Cedar decides, which the hub reports as a refusal.
Grants as attributes on the app (from the hub's database) worked; Cedar's policy templates
were not tried, since a template's slots cannot express the context checks a spawn needs.
The ADR of phase [[PHS-20261126-f3e51h]] item 1 can rest on this.
