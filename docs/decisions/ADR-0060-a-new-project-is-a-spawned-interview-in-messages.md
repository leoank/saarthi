# ADR-0060: A new project from the app is a spawned interview, asked in messages

Status: accepted
Date: 2026-11-17
Amends: ADR-0053

## Context
ADR-0053 makes *New project* a spawn of `/yajna:project <brief>` in the chosen notebook,
"whose interview arrives as questions the app already answers": the question cards the
app shows for `AskUserQuestion`. A session the node spawns runs `claude -p`, where
`AskUserQuestion` does not exist ([[EXP-20261016-rq1404]], ADR-0034).
[[EXP-20261117-de4f82]] spawned the command as the app would: it runs, reads the notebook,
looks for `AskUserQuestion`, does not find it, and asks in plain text, one topic at a time
with numbered options, then waits idle for a prompt.

## Decision
- **New project** on the notebooks screen spawns `/yajna:<kind> <brief>` in a lab notebook
  under one of the node's spawn roots, the kind picking yajna's command (`project`,
  `code-project`, `bio-project`, `ml-project`, `hardware-project`), and opens the session.
- **The interview is a conversation**: its questions arrive as messages and the person
  answers in the prompt bar. The sheet says so before the session starts.
- **A prompting command reads as what was typed**: the core shows a plugin's or the
  person's own command (`<command-message>` first) as the person's prompt,
  `/yajna:project <brief>`, where a built-in command stays a caption.

## Alternatives considered
- **A question tool the node serves to its spawned sessions**: an MCP server with an
  `ask_user_question` tool whose call the node holds as a pending question until the app
  answers, as github.com/amosblomqvist/learnharness does for its own sessions (the tool
  pre-approved, MCP tool timeouts raised, the session told to use it). It would give the
  question cards ADR-0053 meant, for every spawned session, without the hub or the app
  changing; it is the next step if the owner wants it, not built here.
- **Spawning the interview interactively under a PTY**, where `AskUserQuestion` exists: a
  second way of running spawned sessions for one feature.

## Consequences
- A new project works from the phone today, at the cost of typing answers rather than
  tapping options.
- Permission requests of the interview's reads come to the app like any spawned
  session's; under a hub token a spawn runs in auto mode and asks none.
