# ADR-0079: A default model per Claude account

Status: accepted
Date: 2026-11-24
Amends: ADR-0039, ADR-0046

## Context
The owner's organizations set their default model to Opus, yet a session saarthi starts
under a hub token (ADR-0039, ADR-0051) comes up on Sonnet, Claude Code's own default.
EXP-20261124-1pg2cq found why and what cannot fix it. Claude Code fetches its organization's
settings straight from Anthropic, not through the node's proxy, with the dummy token, so
the call fails. And the hub cannot make the call for it: a `claude setup-token` token gets
404 on `/api/claude_code/settings` and 403 on `/api/oauth/profile`. It is answered only on
`/api/claude_code/policy_limits`, which carries on/off restrictions and no model.

So saarthi has to name the model itself. EXP-20261124-gfp268 checked how, for a session
`saarthi run` starts: under a hub token Claude Code takes its main model from
`ANTHROPIC_MODEL`, in `-p` mode and at a terminal. `--model` beats it, and it beats a
`model` in `settings.json`.

## Decision
- Each Claude token the hub holds gets a **default model**, empty by default. It is stored
  with the token's other records (expiry, usage) and set from the app's Claude accounts
  screen. A new control call, `SetClaudeTokenModel`, sets it, and `ClaudeToken` in
  `ListClaudeTokens` carries it. The value is what `--model` and `/model` take (an alias
  such as `opus`, or a model id), checked as the node checks a `/model`; the app offers
  Claude's default, Fable, Opus, Sonnet and Haiku.
- **Spawn and resume:** when the request names no model and the session runs under a hub
  token, the hub sends that token's default model to the node, which passes it as
  `--model`. A model picked in the app wins. A resume without a model takes the default
  of the token it resumes under, as a fresh `claude --resume` takes Claude Code's own default.
- **`saarthi run`:** the hub's `ClaudeToken` reply to the node carries the account's
  default model with the token's name. The wrapper gives it to the program as
  `ANTHROPIC_MODEL`, unless the environment it was started in already has one. It adds
  nothing to the command line it wraps, so `--model` there, and `/model` in the session,
  still win.
- Sessions under the machine's own login are left alone: their Claude Code reads the
  organization's settings itself. Usage probes stay on Haiku (ADR-0043).

## Alternatives considered
- **Let `/api/` paths through the proxy, or have the hub fetch managed settings:** ruled
  out by EXP-20261124-1pg2cq. Claude Code makes those calls directly, and a setup-token is
  not answered on them.
- **A default model per node, beside the node's default account:** one node runs sessions
  under several accounts, and the model follows the organization, which is the account's.
- **The model in the hub's token file, or a hub flag:** declarative, in neusis with the
  tokens. But the token file is a secret to edit for a setting that is not one, and every
  change would need a deploy. The app already sets each account's expiry.
- **`--model` added to `saarthi run`'s command line:** the wrapper does not own that line
  (`claude mcp list`, other programs), and it would override a `/model` habit no more than
  the variable does. The variable needs no parsing.

## Consequences
- A session under a hub token starts on the account's model with nothing picked. The
  account's model has to be kept in step with the organization's by hand: saarthi cannot
  read it.
- A `model` in a machine's `settings.json` is overridden by the account's default for
  `saarthi run` sessions under a hub token (EXP-20261124-gfp268, H3). An `ANTHROPIC_MODEL`
  the machine exports is not.
- A `saarthi run` started inside a session under a hub token inherits that session's
  `ANTHROPIC_MODEL`, and so keeps its model even under another account. The variable cannot
  be told apart from one the user exported.
- An older node ignores the model in `ClaudeTokenReply`: its terminal sessions keep Claude
  Code's default until it is updated. Spawns and resumes need only the hub.
