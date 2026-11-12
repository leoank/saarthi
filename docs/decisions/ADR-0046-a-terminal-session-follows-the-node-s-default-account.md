# ADR-0046: A terminal session follows the node's default account

Status: accepted
Date: 2026-11-12

## Context
ADR-0039 put named `claude setup-token` tokens on the hub and handed one to a node with a
spawn or resume; a node's default account, set from the app, chose it when none was
picked. A session started at the machine's own terminal through `saarthi run` still ran
under the machine's login, so with two accounts the owner could not put terminal work on
the second. They asked for terminal sessions to follow the node's default, which the app
already changes through the hub.

## Decision
- `saarthi run` asks the node daemon, over its wrapper socket and before it starts its
  program, for a token: the node's default without a flag, or `--claude-token <name>`.
  `--own-login` skips the question. The daemon asks the hub with a new unary node-service
  call, `ClaudeToken`, authenticated by the node's token, and the hub answers with the
  named token or the node's default, or none for the own login.
- The wrapper starts the program with the token as `CLAUDE_CODE_OAUTH_TOKEN`, which beats
  the keychain login (EXP-20261111-q6sgt8), says which account on stderr, and names it in
  its hello, so the node reports the session under that account and the hub records it.
  The hub keeps a session's account when a node reports it without one.
- A named account the hub does not hold, or no hub to ask, stops the wrapper before it
  starts the program. Without a flag, any failure falls back to the machine's own login and
  says why; no default set is silent.

## Alternatives considered
- Only when asked (`--claude-token`): every terminal session stays on the machine's login
  unless the alias changes; the owner chose the default to follow the app's setting.
- The hub pushing every token to every node at registration: a terminal session would not
  need the hub to be up, but every node would hold every token all the time.
- A command on the Link stream instead of a unary call: the stream carries the hub's
  commands to the node; a request from the node would need its own reply routing.

## Consequences
- Changing a node's default in the app moves the next terminal session on that machine,
  not ones already running.
- Starting `claude` through the wrapper waits for the hub when a default is set; with the
  hub down it starts under the machine's login after the dial times out (six seconds).
- A terminal session's account usage still comes from usage probes (ADR-0043): an
  interactive session reports no rate-limit windows.
