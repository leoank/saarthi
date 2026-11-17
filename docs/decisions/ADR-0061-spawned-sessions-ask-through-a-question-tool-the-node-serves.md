# ADR-0061: Spawned sessions ask through a question tool the node serves

Status: accepted
Date: 2026-11-17
Amends: ADR-0034, ADR-0060

## Context
A session the node spawns runs `claude -p`, which has no `AskUserQuestion` (ADR-0034,
[[EXP-20261016-rq1404]]). ADR-0060 therefore left the *New project* interview as plain-text
messages answered from the prompt bar ([[EXP-20261117-de4f82]]). The owner asked to learn
from learnharness (github.com/amosblomqvist/learnharness), which drives `claude -p` too and
gives its sessions an `ask_user_question` tool from an MCP server of its own, holding each
call until the person answers; then to adopt what applies and build it.

## Decision
- **The node serves a question tool.** Its hook port answers MCP (streamable HTTP) at
  `/mcp` with one tool, `ask_user_question`, whose input is `AskUserQuestion`'s. A call is
  held as a pending question through the same machinery as a held `AskUserQuestion`, so
  the hub and the clients show and answer it unchanged; the answers return as the tool's
  result and the turn goes on in the same process. The reply is an event stream with a
  keepalive every 20 s, so no HTTP client gives up on a slow answer; a question waits up
  to a day, and the session going away releases it.
- **Every host session the node spawns or resumes gets it**: `--mcp-config` naming the
  endpoint with a bearer token of its own (only it can ask in its name), the tool in
  `--allowedTools` (no permission request for it), `--append-system-prompt` telling it to
  use the tool wherever it would use `AskUserQuestion`, and `MCP_TOOL_TIMEOUT` and
  `CLAUDE_CODE_MCP_TOOL_IDLE_TIMEOUT` raised to the day. Only with a hub link; not in a
  sandbox profile, where the hook port is out of reach.
- **Context left, after each turn**: the spawner sends the session the SDK's
  `get_context_usage` control request (its summary) after each `result`, and records the
  answer (tokens in use, the window before compaction) as a daemon `context` event; the
  app shows it in the session's header.
- **Turn ends stay on `result`**: learnharness found that a retried stream first ends with
  a `message_stop` carrying `abandoned_blocks`; that comes only with partial messages,
  which the spawner does not ask for, so nothing changes.

## Alternatives considered
- A separate MCP process per session over stdio, talking to the node: a second program for
  what an axum route on a port the node already serves does.
- Spawning interactive sessions under a PTY, where `AskUserQuestion` exists: a second way
  of running spawned sessions.
- Answering with a plain JSON reply after the hold: simpler, but an HTTP client may time
  out waiting for headers on a slow answer.

## Consequences
- The *New project* interview, and any spawned session's question, comes as question cards
  ([[EXP-20261117-de4f82]], its second run).
- The session must choose the tool; the appended prompt asks it to, and the tool says it
  replaces `AskUserQuestion`. A model may still ask in prose now and then.
- A sandboxed spawn still cannot ask; giving it the tool needs the hook socket reached over
  a transport Claude Code's MCP client supports.
