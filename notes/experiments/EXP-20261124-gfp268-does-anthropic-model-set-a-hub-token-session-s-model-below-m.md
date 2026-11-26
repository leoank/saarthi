---
id: EXP-20261124-gfp268
type: experiment
schema_version: 1
title: Does ANTHROPIC_MODEL set a hub-token session's model, below --model and above the user's settings?
project: PRJ-0001
status: concluded         # planned | active | paused | concluded | abandoned
outcome: positive         # null | positive | negative | inconclusive | infra
influenced_by: [EXP-20261124-1pg2cq]
created: 2026-11-24
author: agent:claude
protocols: []
materials: []
tags: []
review: pending
---

## Question
A session under a hub token starts on Claude Code's own default model, because its call
for the organization's settings fails ([[EXP-20261124-1pg2cq]]). The fix planned gives each
hub account a default model, which the hub passes as `--model` on spawn and resume, and
which `saarthi run` gives the program it wraps as `ANTHROPIC_MODEL`, an environment
variable, so as not to add arguments to a command line it does not own. Does Claude Code
2.1.330 take its model from `ANTHROPIC_MODEL` under the node's environment for a hub
token, and does `--model` still beat it? And does it beat a `model` in the user's
`settings.json`, which would mean the account default overrides a machine's own choice?

## Hypothesis and predictions
H1: `ANTHROPIC_MODEL=opus` makes the session's main turn ask for an Opus model, in `-p`
mode and at a terminal. If false, the main turn asks for the built-in default (Sonnet),
and `saarthi run` has to pass `--model` instead.

H2: `--model sonnet` beats `ANTHROPIC_MODEL=opus` (the main turn asks for Sonnet). If
false, the account default would override a model the user typed.

H3: `ANTHROPIC_MODEL=opus` beats `"model": "sonnet"` in `settings.json` (the documented
order: `/model`, `--model`, `ANTHROPIC_MODEL`, settings). If true, the account default
overrides a model set in a machine's settings; that goes into the ADR's consequences.

## Plan
1. One run, here, nothing leaving the machine: the stand-ins of [[RUN-20261124-1qtntf]],
   the proxy's socket now logging the `model` of each `POST /v1/messages` body. Claude
   Code 2.1.330's bare binary with the node's variables for a hub token and a fresh config,
   given a prompt, in five cases: no model set; `ANTHROPIC_MODEL=opus`; that plus
   `--model sonnet`; `settings.json` `model: sonnet` alone; and that plus
   `ANTHROPIC_MODEL=opus`. `-p` mode for each, and a terminal for the `ANTHROPIC_MODEL`
   case.

## Success criteria
Each case's log names the model of every `/v1/messages` request; the main turn's model is
the one not a small/fast model (Haiku), and decides H1 to H3.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

- [[RUN-20261124-gftzzx]] five `-p` cases and one terminal, under the node's environment for a hub token: `ANTHROPIC_MODEL=opus` gives Opus in both modes, `--model sonnet` beats it, and it beats `model: haiku` in settings

## Conclusion
Draft by agent:claude, for the owner to approve.

H1, H2 and H3 hold ([[RUN-20261124-gftzzx]]): under a hub token Claude Code 2.1.330 takes
its main model from `ANTHROPIC_MODEL` in `-p` mode and at a terminal; `--model` beats it;
it beats a `model` in `settings.json`. Its small, fast model (Haiku) is untouched.

So `saarthi run` can give the account's default model as `ANTHROPIC_MODEL` without touching
the command line, leaving `--model` and `/model` to the user, and setting it only when the
environment has none keeps a machine's own `ANTHROPIC_MODEL` too. A `model` in a machine's
`settings.json` is overridden by the account default; the ADR names that.
