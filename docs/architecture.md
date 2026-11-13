# saarthi architecture

Design for [[PRJ-0002]]. Each decision is recorded with its alternatives in `decisions/`
(ADR-0001 to ADR-0012 are the owner's answers in `architecture-questions.md`); this
document says how the pieces fit. The facts about Claude Code hooks, Tailscale libraries
and mxc were checked against the official docs on 2026-10-16 and then tested in the phase 0
spikes, [[EXP-20261016-rq1401]] to [[EXP-20261016-rq1409]], whose findings are folded in below. What is built so
far is the *Progress* checklist on the project page.

## What saarthi is

A command and control center for agentic coding sessions. A small daemon on every
machine that runs agents reports to one hub. The hub knows every running session, can
pause a session at a decision point and hand the decision to a human on another device,
records every run, and keeps a time series of node health. Clients talk only to the hub.

What it adds over existing remote-driving tools:

- **Many harnesses, many accounts.** Existing tools tie one agent harness to one account.
  saarthi treats the harness as an adapter and the account as a credential the hub owns
  and hands to a node for as many sessions as the node wants to run under it.
- **Many networks.** The hub, the nodes and the clients may each sit on different
  tailnets. Every part embeds its own Tailscale identity, so nothing depends on the
  host's network setup.
- **Sessions with declared capabilities.** A session launched by saarthi runs in a
  sandbox with an explicit filesystem, network and secret policy, chosen per launch,
  instead of inheriting the whole user account. Interactive sessions you start by hand
  are observed and gated, but not confined.
- **Starts on its own.** Later, inbound messages and scheduled jobs start background
  sessions without a human in the loop.

Scope for now: Claude Code is the only agent, Android is the only client, one person is
the only user.

## Components

```
 phone (android)                      hub machine                        node machines
 ┌─────────────────┐   gRPC/tailnet   ┌──────────────────┐  gRPC/tailnet  ┌─────────────────┐
 │ compose UI      │◄───────────────►│ saarthi-hub      │◄──────────────►│ saarthi-node    │
 │ saarthi-core    │                 │  control service │                │  hook endpoint  │
 │  (rust, uniffi) │   push (ntfy)   │  node service    │                │  pty wrapper    │
 │ tsnet           │◄────────────────│  credential store│                │  transcript     │
 │ ntfy/unifiedpush│                 │  sqlite, ntfy    │                │  health         │
 └─────────────────┘                 │  tsnet x N       │                │  sandbox runtime│
                                     └──────────────────┘                │  tsnet          │
                                                                         └────────┬────────┘
                                                                                  │ hooks, pty
                                                                         ┌────────┴────────┐
                                                                         │ claude          │
                                                                         │ (tty or sandbox)│
                                                                         └─────────────────┘
```

### saarthi-node

One Rust daemon per machine. It never parses terminal output for meaning; structured
facts come from hooks and the transcript, and the PTY is only a channel for input.

- **Hook endpoint.** Claude Code hooks can be HTTP endpoints, so the daemon listens on
  localhost and a per-user `~/.claude/settings.json` points every event at it. That
  covers every session on the machine, including ones started by hand. Hooks passed with
  `--settings` miss SessionStart, which fires before they load, so the daemon's hooks must
  live in the settings file. Events:
  SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, Notification, Stop,
  SubagentStop, SessionEnd, and PermissionRequest. PreToolUse can allow, deny, ask, or
  rewrite the input; PermissionRequest can allow or deny the dialog. Those two are the
  interception points.
- **Rate limits.** With `--claude-usage` (ADR-0030) the daemon reads the OAuth access token
  from Claude Code's credential store, read only (the keychain item through `security` on
  macOS, `.credentials.json` elsewhere), calls the usage endpoint every five minutes (longer
  after a 429, which once-a-minute polls drew within half an hour) and reports
  the 5-hour and 7-day utilisation as `usage.five_hour_pct` and `usage.seven_day_pct`, and
  the Fable week (a `weekly_scoped` entry in the response's `limits`) as
  `usage.seven_day_fable_pct`, each with its reset time as `usage.<window>_resets_s`, and
  `usage.age_s` 0; without the flag the sampler reports Claude Code's own cached value
  under the same names with its age, which only `/usage` refreshes. [[EXP-20261025-25f6vm]]
  checked the read on the real credential, across Claude Code's own refresh.
- **Keep awake.** While switched on (ADR-0031; `--keep-awake` at start, `SetKeepAwake`
  from a client, ADR-0034) the daemon holds `caffeinate -i` (macOS) or a blocking `idle:sleep` lock from
  `systemd-inhibit` (Linux, ADR-0045) while any session is busy and reports
  `node.keep_awake` and `node.keep_awake_enabled` as health. A Linux node checks the lock
  once at start; where logind refuses it (no polkit rule for the user) it reports neither
  metric, and clients offer no switch.
- **Ignored directories.** `--ignore-cwd <glob>`, repeatable: a session whose working
  directory matches is not tracked from the registry or from hooks, so the live checks'
  throwaway sessions under `target/live-work-*` never reach the live hub (ADR-0028).
- **Transcript tailer.** Hooks carry tool calls and results but not Claude's own text.
  The hook input includes the transcript path, a JSONL file; the daemon tails it to get
  the full conversation for display, and to backfill anything a hook missed. Shapes are in
  [[EXP-20261016-rq1402]]: keep the `uuid`-bearing `user`, `assistant` and `system` lines, ignore
  attachments and state lines, watch the `subagents/` directory next to the file, and
  de-duplicate on `message.id` before summing usage, since one API message is written as one
  line per content block.
- **Session registry.** `~/.claude/sessions/<pid>.json` lists every interactive session with
  pid, session id, cwd, idle or busy status and a messaging socket path ([[EXP-20261016-rq1405]]). The
  daemon reads it to discover sessions that predate it or whose hooks it missed, checking the
  pid since entries outlive their process.
- **PTY wrapper.** There is no supported way to send a prompt or an interrupt to a running
  interactive session from outside; Remote Control is the only mechanism and it is not an
  API. So the daemon ships `saarthi run`, which starts `claude` under a PTY it owns and
  mirrors it to your terminal. Through that PTY the hub can send Escape to interrupt,
  followed by a prompt if one is given, or a signal to kill. The wrapper reaches the daemon
  over a per-user unix socket and reports when a person types, which releases a held
  permission request; replies the terminal sends to the TUI's queries do not count
  (ADR-0023). A session started without the wrapper is observed, gated and promptable
  (prompts use the messaging socket, ADR-0014) and killed by SIGTERM, but cannot be
  interrupted.
  Before it starts its program the wrapper asks the daemon for a Claude token, the node's
  default or one named with `--claude-token`; the daemon asks the hub (`ClaudeToken` on the
  node service), and the wrapper runs the program with the token in its environment and
  names the account in its hello, so the session is recorded under it (ADR-0046).
- **Health collector.** CPU, memory, disk, network, GPU when present, running claude
  processes, and account usage state. One sample every 30 seconds, sent to the hub. GPU on
  macOS comes from IOKit `IOAccelerator` `PerformanceStatistics` (device utilisation, memory
  in use, recovery count) in under 100 µs per sample through `io-kit-sys`; Linux uses NVML or
  the amdgpu sysfs files ([[EXP-20261016-rq1409]]). Rate-limit state ([[EXP-20261016-rq1402]]): spawned sessions
  report it in every `rate_limit_event`; for a named token the hub polls
  `GET /api/oauth/usage` for tokens no session is using, if the endpoint accepts a
  setup-token ([[EXP-20261110-g5mht5]]; [[RUN-20261017-ebyrjv]] showed it does for a login); for a person's own login on a node the only on-disk source is
  `cachedUsageUtilization` in `~/.claude.json`, which nothing but `/usage` writes
  ([[RUN-20261017-dqeg0a]]), so the daemon shows it with its age and runs
  `claude -p "/usage"` there when it is stale, a 4 s call with no model turn.
- **Hub link.** One outbound gRPC bidirectional stream, reconnecting with backoff. The
  daemon buffers events until the hub acks them, so a hub restart loses nothing, and
  numbers them from the clock so a node restart never reuses a seq (ADR-0019). The
  registration carries the live sessions and the requests still held.
- **Spawner (phase 2, ADR-0034).** Starts headless sessions (`claude -p` with stream-json
  input and output, through the user's login shell for a terminal's PATH) in a directory
  under one of the node's `--spawn-root`s (created first when the spawn asks, ADR-0035),
  under the session id the hub chose, with or without a first prompt, in the permission
  mode the spawn picked. It passes the node's hooks block as `--settings`, so the
  session's permission requests reach the node even when the user's settings file lacks
  it ([[EXP-20261106-qt94y4]], ADR-0037). It owns
  their stdin and stdout: a prompt is a `user` line, an interrupt the SDK's
  `control_request`, kill an interrupt and the input closed; busy and idle come from the
  stream's `result` lines, since the registry keeps such a session `busy`
  ([[EXP-20261106-37f79k]]). The session is otherwise tracked like any other, as
  `LAUNCH_SPAWNED`, and its permission dialogs are held as long as a wrapped one's.
- **Sandbox runtime (phase 3).** Runs a spawned session inside a containment backend
  under a launch profile. See "Sandboxed sessions".
- **Tailnet identity.** Optionally (`--tailnet`), an embedded tsnet identity of its own,
  so the daemon reaches the hub on the hub's tailnet whatever network the host is on;
  every connection to the hub is dialled through it (ADR-0048).

### saarthi-hub

One Rust binary, tonic for gRPC, sqlx on SQLite in WAL mode.

- **Node service.** The daemon-facing API: register, stream events, receive commands,
  push health samples, receive a session's Claude token with its spawn.
- **Control service.** The client-facing API. It is a superset of the node service: list
  nodes and sessions across every node, subscribe to a session's event stream, submit
  decisions and prompts, and later spawn sessions. The two services are separate
  protobuf packages from the start since the client sees the hub's view, not a node's.
  Clients present their own pre-shared tokens (ADR-0021); `saarthi ctl` is a command-line
  client, which reads the hub and its token file from `~/.config/saarthi/ctl.toml` unless
  given as flags or in the environment. `saarthi tui` is a dashboard on the same client and config:
  nodes with their live sessions, the held requests and the accounts' usage, from `Watch`,
  with keys to decide, prompt, interrupt, end and start sessions (ADR-0047).
- **Live state.** Which nodes are linked now, and the commands waiting for a node's
  answer. Sessions and pending requests are tables, so a hub restart loses neither; a node
  that reconnects re-sends the requests it still holds.
- **Event log.** Append-only table of every event from every node. This is the record of
  all runs. Tool inputs and outputs, diffs, usage and cost, and git state at start and
  end are all events. An event is stored once by its node and seq, and a transcript line
  once per session by its uuid, so a restarted node re-reading its transcripts adds
  nothing (ADR-0020).
- **Secrets.** Named Claude tokens and the tokens nodes and clients present, read from
  agenix files at start; the database holds only the Claude tokens' fingerprints, dates and
  the nodes' defaults. Tailnet auth keys later. See "Credentials".
- **Notifier.** Publishes to a self-hosted ntfy server when a session is blocked on a
  human, finishes a turn, or ends, and when a rate-limit window is used too fast, nearly
  used up or used up, once per window and reset (ADR-0032): to the UnifiedPush endpoint
  each phone registered, which the app shows as its own notification with Allow and Deny
  (ADR-0026), and to an optional ntfy topic (ADR-0025). Each kind can be switched off
  with `--no-notify <kind>` (ADR-0029).
- **Tailnet identities.** One embedded tsnet identity per tailnet the hub serves
  (`--tailnet`, repeatable), each serving both services beside the TCP listener. On each
  node registration over a tailnet the hub records which tailnet and who the tailnet says
  the caller is (ADR-0048).

### saarthi-proto

Protobuf definitions for both services and the event types. prost and tonic generate
Rust; the Android build generates Kotlin from the same files. This is the one place the
wire format is defined.

### saarthi-core

The client logic in Rust, exposed to Kotlin through UniFFI: connect to the hub, keep the
node and session list current, stream a session's events, submit commands, and persist
the hub address and token. The Android app is a Compose UI over this core; iOS and the
custom integrations reuse the core later. The embedded tsnet for the phone lives here
too: an identity of the app's own per hub that has a tailnet auth key (ADR-0049).

The core turns a session's events into a conversation a person reads: the user's turns,
Claude's text and thinking, each tool call as its short name, the one input that matters
and a detail worth a look (an edit's diff, a file's content, an agent's prompt), tool
results clipped and marked when they failed, and the daemon's own facts (interrupted,
killed). Claude Code writes more than prompts into the transcript's user lines, and the
core makes each readable or drops it: a prompt sent over the messaging socket comes back
inside the "another session sent a message" banner and is unwrapped; slash commands and
their output, `!` shell commands and their output, background-task notices and the
interrupt marker become short notices or outputs; system reminders, skill bodies and other
`isMeta` text injected for the model are dropped; images and tool definitions in results
are named, not shown.

The session screen shows this the way the terminal does, loosely: the person's prompt as
a boxed line, Claude's text as rendered markdown, a tool call as one line with its output
folded to a count of lines (or the error's first line) until tapped, thinking folded,
notices as captions.

The Android app (`android/`) starts on a boot log: on black, in the manner of a Linux
boot, one line per real step of the start (the app, phone and core versions, the settings,
the network, the push distributor, the hub and each node), then the home screen, or on
failure Retry and Settings. Settings holds the hubs (name, address and client token each,
one of them active, kept on the phone; the active one's address and token also under the
keys the notification actions read), the display toggle and the versions; a first run
with no hub goes there. The other screens: home (requests waiting on the person first, then each
node with its live sessions), a session (its waiting requests, the conversation live, a
prompt box, interrupt when the session runs under the wrapper or was spawned, and kill)
and a node's health as charts over a chosen span, network traffic as a rate between
samples, with the keep-awake switch. *New session* on the home screen spawns a session on
a node that allows it. History lists every session newest first, titled by its first
prompt, with its tokens and cost (the hub's `History` sends the opening user lines and the
latest usage payload, the core reads them); an ended session's screen resumes it. The app
reconnects by itself with backoff when the watch stream breaks and resumes an open
session's stream after the last item it saw; a command's outcome, or the node's note,
shows briefly as a snackbar. A notification tap opens the session it is about, or for a
usage alert the node's health. A diff or a long script shows its first lines under its row
and opens whole, full screen, on a tap; one app-wide toggle, remembered, wraps monospace
text instead of scrolling it sideways.

### Agent adapter

Claude Code is the only implementation, but the daemon talks to it through a trait with
two capability levels:

- **structured**: the harness exposes hooks or an SDK, so events are typed and decisions
  can be injected (Claude Code).
- **terminal**: the harness has no hooks, so the PTY wrapper is all there is. The
  adapter can observe process lifetime, prompt, interrupt, and kill, but cannot gate
  tool calls.

The wrapper from the first level is the whole second level, which is why it exists even
for Claude. The adapter also says how to hand the harness its credentials: for Claude
Code that is a config directory and an environment variable.

## Sandboxed sessions
A session saarthi starts runs under a **launch profile**: a named policy the node holds and
the hub catalogs, chosen at spawn time. A profile says:

- **backend**: `host` (no containment, phase 2), a process sandbox (bubblewrap on Linux,
  seatbelt on macOS), a container, or a micro VM.
- **filesystem**: read-only paths, read-write paths, denied paths. The working directory
  and the session state directory are read-write; the rest of the home directory is denied
  unless the profile says otherwise.
- **network**: none, loopback only, allowlisted hosts, or everything, optionally through
  an egress proxy the daemon runs. Per-host allowlists exist only on backends that support
  them; seatbelt offers all or nothing plus a cooperative proxy.
- **secrets**: which Claude token the session runs under, and whether the agent sees
  the real token or the egress proxy injects it so the sandbox never holds it (ADR-0039);
  the token is chosen per session, not per profile.
- **resources**: CPU, memory, disk, and a wall-clock limit. Only the wall clock is
  enforceable on a process sandbox; the rest needs a container or VM.
- **persistence**: throwaway, or a provisioned container that survives across sessions
  so a workspace keeps its build cache. Not applicable to process sandboxes.

The daemon implements this through a `Containment` trait with the containment backend
behind it. The backend is mxc, Microsoft's sandboxing SDK: a pure Rust crate (`mxc-sdk`)
with one containment model over bubblewrap, LXC and micro VMs on Linux and seatbelt on
macOS, with JSON policies for filesystem, network and UI, and a lifecycle of provision,
start, exec, stop and deprovision for persistent containers. [[EXP-20261016-rq1407]] confirmed it
builds offline under Nix on macOS with no runtime download, and that `claude -p` completes
a turn inside a seatbelt profile with its hooks reaching the daemon and its transcript
readable outside. Its request types have no serde and the generated seatbelt profile cannot
be dumped, so the daemon owns its own policy format and maps it.

What a Claude session needs inside the sandbox, from the minimum policy in [[EXP-20261016-rq1407]]:
read-only `/nix/store` (or the harness closure) and the working directory; read-write the
state directory and a scratch directory, with `CLAUDE_CONFIG_DIR` and `CLAUDE_CODE_TMPDIR`
pointing at them, since claude ignores `TMPDIR` for its own temp files; a `PATH` of the
claude wrapper plus `/usr/bin:/bin`; outbound network; no home directory and no keychain.

The hook endpoint crosses the boundary as a unix socket, not as loopback TCP: on seatbelt,
allowing loopback means allowing all inbound traffic, and it is incompatible with a
proxy-only egress policy. Claude Code's HTTP hooks need a TCP URL, so the settings inside
the sandbox use a command hook, `/usr/bin/curl --unix-socket <state dir>/hooks/hook.sock
--data-binary @- http://hook/hook`, and the daemon listens on that socket. This runs
([[RUN-20261017-dssgwx]]): with `hostLoopback: deny` the TCP loopback is refused while the
unix socket carries Stop and a held PermissionRequest, with the allow applied, in about six
seconds per turn. One seatbelt subtlety matters for the policy mapping: under
`egress: allow` the profile's unfiltered outbound rule also covers AF_UNIX, so the sandbox
can connect to any host unix socket that is not under a read-only or denied path, which
includes other sessions' `/tmp/cc-socks/*.sock`; the profile therefore denies `/tmp/cc-socks`
and the daemon's own socket directory explicitly, or uses `egress: deny` plus the proxy, under
which only sockets under read-write paths connect.

As built (ADR-0038, phase 3): profiles are a TOML file the node is given
(`--profiles`), each a `host` or `seatbelt` backend with a state directory, a token file,
`PATH`, read-only, read-write and denied paths, Claude files linked from `~/.claude`, and
extra environment; a spawn names one, and the empty name is the host. Until the hub's
hub's named tokens exist (ADR-0039) a sandbox's credential is a long-lived token from
`claude setup-token` in the profile's token file, given as `CLAUDE_CODE_OAUTH_TOKEN`
([[EXP-20261107-413q6m]]). Egress is open; no inbound, no
loopback TCP. Hooks come back as command hooks over `--hook-socket`
(`~/.local/state/saarthi/hook.sock`); `/private/tmp/cc-socks` and the wrappers' socket are
denied. Each profile has one config directory, `<state>/claude`, whose registry the node
scans beside its own; a resume runs under the profile whose directory holds the transcript.
A hard kill also kills the process groups claude starts its tools in, which a kill of
claude alone leaves running ([[EXP-20261107-413q82]]). The node registers its profiles
(name, description, sandboxed) beside its spawn roots, the hub hands them to clients with
the node, and the app offers them on *New session*; a session carries the profile it runs
under (`Session.profile`, stored with it), shown in its list row, header and History.

An interactive session started by hand runs on the host with your own privileges.
saarthi observes and gates it but does not confine it; confinement is only for sessions
saarthi launches, where it controls the whole environment.

## Credentials
The hub is the only long-term holder of secrets. Three kinds:

- **Claude tokens**: named long-lived tokens from `claude setup-token`, such as `work`
  or `personal` (ADR-0039).
- **Tailnets**: auth keys for the tailnets the hub joins and for enrolling nodes and
  clients onto them.
- **Peers**: the pre-shared token each node and client presents to the hub.

Storage: Claude tokens and peer tokens are files of `<name> <token>` lines that agenix
provides on the hub machine through neusis (`--claude-tokens`, `--node-tokens`,
`--client-tokens`), read at start; tailnet keys will need a store of their own. The threat
model is a single trusted operator; secrets never land in the database, the event log, a
sandbox that should not see them, or a client.

Claude tokens (ADR-0039, superseding the leased slots of ADR-0018): a token from
`claude setup-token` is its own grant, lasts a year and never refreshes; given alone as
`CLAUDE_CODE_OAUTH_TOKEN` it runs and resumes sessions in an empty config directory and
writes nothing back ([[EXP-20261107-413q6m]]). Nothing rotates, so any number of sessions
and nodes can hold one token. Adding, renaming or replacing one is a change in neusis and
a hub restart. The database keeps only metadata per name: a fingerprint (a hash of the
token), when it was first seen, and an expiry, a year after first seen unless set by hand
from the app or `saarthi ctl`; a new fingerprint under a name is a renewal and restarts the
dates. The notifier reminds once, a month before a token expires. Each node has a default
token, set from the app or `ctl`. A spawn names a token, or takes the node's default, or,
with neither, runs under the node's own login, which the picker also offers by name for
host sessions; a sandboxed profile refuses to run with neither. The hub sends the token
with the spawn or resume command; the node keeps it in memory and passes it to the
session, or, for a profile whose egress goes through the node's proxy, to the proxy, which
adds it to requests to the Anthropic API while the sandbox holds a dummy. Nothing is
written to disk, read back or returned. Any node may use any token for now; a list of
nodes per token comes later. Clients see names and dates, never a value. A launch
profile's `token_file` (ADR-0038) remains a fallback for a sandbox the hub sends no token
to. On the host, the token in the environment wins over the keychain login
([[EXP-20261111-q6sgt8]]).

Usage per token (ADR-0042): the usage endpoint refuses a setup-token
([[RUN-20261111-q60gf1]]), so the hub takes each account's 5-hour and 7-day windows from
the `rate_limit_event`s of sessions running under its token, which the node forwards as
daemon `rate_limit` events, and alerts on them per account as on a node's health
(ADR-0032). With `--usage-probe-node`, one node also runs a throwaway Haiku turn under
each token about every half hour, in an empty config directory nothing lists, skipping an
account a session reported on lately, so idle accounts stay fresh too (ADR-0043).

Still to check against the real thing: that one token stays good used from several
machines at once ([[EXP-20261110-g5mh7g]]), that one token can be revoked on its own
([[EXP-20261110-g5mh2z]]), and how a sandboxed session reaches the proxy without TLS
interception ([[EXP-20261110-g5mh89]]).

Why not the person's own login ([[RUN-20261017-dssg3d]]): an ordinary OAuth login is a
*grant* whose refresh rotates both tokens and revokes the previous pair at once, so two
*stores* holding it kill each other at the first refresh, including a person's own login
if it was copied from their keychain. Many *sessions* on one store are fine. saarthi
therefore never copies a refreshing login anywhere; ADR-0018 built leased slots around
this, which setup-tokens make unnecessary.

An interactive session you start by hand uses whatever credentials are already on the
node; no token is ever applied to it.

Enrollment: a new node or phone needs a tailnet auth key and a hub token before it can
reach the hub, so it cannot fetch them from the hub. The hub mints an **enrollment
bundle**, a one-time blob with the hub's tailnet address, an ephemeral auth key for that
tailnet, and the peer token. On a node it is pasted into `saarthi node enroll`; on the
phone it is a QR code. Both keys are revocable from the hub.

## Key flows

### Permission request
1. Claude is about to show a permission dialog; the PermissionRequest hook posts to the
   daemon and blocks. The dialog is drawn in the terminal at the same time.
2. Daemon forwards to the hub; hub records a pending request, streams it to connected
   clients, and publishes an ntfy notification.
3. You tap allow or deny on the phone. Hub forwards to the daemon, daemon answers the
   hook, Claude proceeds. The terminal shows "Allowed by PermissionRequest hook".
4. A keystroke typed at the terminal while the hook is pending is queued and applied only
   after the hook returns. So the daemon releases the hook with an empty response the moment
   the PTY wrapper sees any human keystroke; the queued keystroke then answers the dialog
   within a fraction of a second. No idle timer, no hold-time heuristic. Sessions without the
   wrapper get a fixed hold, long enough for a phone round trip, then release (120 s,
   `--hold-secs`); wrapped sessions hold until a keystroke, at most 540 s
   (`--wrapped-hold-secs`), below the hook timeout. Deny carries a message the model sees
   ([[EXP-20261019-cqpw0d]]).
5. The per-hook `timeout` is enforced to the second (default 600 s); on cancel the terminal
   dialog simply stays. The daemon sets a large timeout and relies on the release rule.

Measured in [[EXP-20261016-rq1401]]. The `Notification` hook for `permission_prompt` arrives only after
the PermissionRequest hook returns, so the PermissionRequest itself is the push trigger.
PreToolUse is wired the same way for tools that would not show a dialog, so a future policy
engine can gate them too; in phase 1 it only records.

### Question from Claude
AskUserQuestion is a tool, so PreToolUse and then PermissionRequest fire with the questions in
the input. Either hook can answer it ([[EXP-20261016-rq1404]]): return allow with `updatedInput` equal to
the original input plus an `answers` map from question text to the answer: one label, an
array of labels for a multi-select question, or any free text when the person types their
own answer ([[RUN-20261017-e1d84z]], [[RUN-20261017-e35g6f]]). The tool returns at once and no
dialog is shown. saarthi uses the PermissionRequest path so the
request is held and released exactly like a permission. The tool does not exist in `-p` mode,
so spawned sessions never block on it.

### Prompt and interrupt
A prompt goes hub to daemon to the session's messaging socket ([[RUN-20261017-dp90a6]]):
`~/.claude/sessions/<pid>.json` names `/tmp/cc-socks/<pid>.sock`, and one newline-delimited
JSON line `{"type":"user","message":{"role":"user","content":...}}` from a process of the
same user becomes a user turn, queued behind the current one, with no keystrokes and no
wrapper. This works for sessions started by hand as well, so prompting does not depend on
the PTY wrapper at all. A session in bypass-permissions mode holds such messages for
approval at its terminal, and the daemon reports that state instead of pretending the
prompt landed, judging it from the permission mode of the session's latest hook
(ADR-0022); `crossSessionInbound: accept` in the session's settings lifts the hold for
sessions saarthi starts.

Since ADR-0027 that is the path for sessions saarthi did not start. A session under the
wrapper gets its prompt typed into the PTY instead, text then Enter, so Claude Code sees a
typed prompt and the model answers its user, not a peer; the client says which path a
session has before sending. [[EXP-20261025-25f4bf]] checks typed prompts while idle, while a
turn runs (Claude Code queues typed input) and in bypass mode.

An interrupt has no socket message type, so it goes hub to daemon to PTY and needs the
wrapper. The sequence that works ([[EXP-20261016-rq1403]]): Escape to interrupt, pause, Ctrl-U to
empty the input box (Claude Code refills it with the interrupted prompt), then, if a new
prompt follows, text and Enter. No Stop hook fires for an interrupted turn, so the daemon
marks the turn ended itself. Kill is a signal to the wrapped process. Sessions without the
wrapper reject interrupt with a clear reason that the client shows, unless they were
spawned: the spawner interrupts those over their input.

Every `CLAUDE*` environment variable must be stripped before the wrapper starts a session, and
the working directory must already be trusted: the trust dialog cannot be driven through the
PTY. The wrapper never parses terminal output for state; the TUI positions text with cursor
moves and the text is unreliable. Hooks and the transcript are the only sources of truth.

### Spawn (phase 2, sandboxed in phase 3)
Built so far (ADR-0034, ADR-0038, ADR-0039): steps 1 to 3 with the token choice, without
the proxy; on exit the node only reports the end.

1. Client picks a node, a directory from that node's allowed roots, a token (the node's
   default unless changed), and a launch profile. The allowed roots are configured on the node and reported to the hub.
2. Hub looks up the token by name (ADR-0039), refusing one it does not hold, sends the
   token with the spawn command, and records the token's name on the session once the
   node has started it.
3. Daemon prepares the state directory, provisions the sandbox if the profile asks for
   one, and starts `claude -p --input-format stream-json --output-format stream-json`
   with the profile's `CLAUDE_CONFIG_DIR` and the token as `CLAUDE_CODE_OAUTH_TOKEN` (or a
   dummy, when the profile's proxy adds the token). One process serves many turns, keeps
   context, idles between them and exits when stdin closes ([[EXP-20261016-rq1405]]). Permission requests
   arrive through the same PermissionRequest hook as everywhere else and an allow from the
   hook runs the tool ([[EXP-20261016-rq1401]]); no MCP permission tool is needed. Every turn re-emits
   `init` and a `rate_limit_event`.
4. On exit the daemon wipes the state directory, deprovisions a throwaway sandbox or stops
   a persistent one, drops the token from memory, and reports the end. Nothing goes back
   to the hub: the token does not change.

Resume works by session id in both directions, so a terminal session can be continued
headless and a spawned one can be attached at a terminal later. Headless, it continues
the same session in place ([[EXP-20261106-62st4b]]); the hub's `Resume` sends the node
the directory from its record, and the spawner runs it with `--resume` (ADR-0036). `claude --bg` is the
alternative for sessions the person may want to `attach` to: Claude Code keeps the process
and offers `logs`, `stop` and `rm`, but its output is a terminal stream, so the daemon still
relies on hooks and the transcript.

## Data model

Hub tables, all keyed by hub-assigned ids with the node's own ids stored alongside. Built
so far, in `crates/hub/migrations/`: `nodes`, `sessions`, `events`, `health`, `pending`
and `claude_tokens`; the rest come with tailnets and profiles at the hub.

- `nodes`: id, name, tailnets, last seen, capabilities, allowed spawn roots, profiles.
- `claude_tokens`: name, fingerprint, first seen, expiry set by hand, reminded, and the
  latest usage windows seen under it (ADR-0042); never the token (ADR-0039). Each node's
  default token is a column of `nodes`.
- `profiles`: name, node, backend, policy JSON.
- `sessions`: id, node, token name, profile, harness, cwd, started, ended, how it was
  started (wrapper, bare, spawned), transcript path.
- `events`: id, session, sequence, time, kind, payload JSON, and the line id of a
  transcript line or usage update, unique per session. Append-only. Payloads are
  redacted of any Claude token before they are written.
- `pending`: open permission requests and questions, with how each ended and the
  resolution a client sent.
- `health`: node, time, metric, value.

Migrating to Postgres later is a change to the sqlx driver and the migrations; nothing
else should know which database it is.

## Network and auth

The hub, the nodes and the clients may each live on a different tailnet. The end state
is that every part embeds tsnet: the hub holds one identity per tailnet it serves, and
each node and client holds an identity on the hub's tailnet, obtained through its
enrollment bundle, next to whatever tailnet the host already uses for other things.
Nothing relies on the host's tailscaled.

Phase 1 is simpler: everything on one tailnet through the host's tailscaled and the
Tailscale app on the phone, with pre-shared tokens read from one file for nodes and one
for clients (`--node-tokens`, `--client-tokens`), and the hub also recording each peer's
tailnet identity. The hub and daemon get a transport boundary from day one, a
listener and a dialer, so the embedded tsnet lands later without touching the services.

Phase 3 lands it for the hub and the node (ADR-0048, [[EXP-20261112-rq8kgc]]). The crate
`saarthi-tailnet` wraps one libtailscale identity: libtailscale's connections are stream
sockets, so tonic serves those accepted on the tailnet through `serve_with_incoming` and
dials through `connect_with_connector`, with the blocking C calls on threads of their own.
The hub serves on every tailnet it is given and keeps its TCP listener, which the phone,
`ctl` and the tests use; a node with `--tailnet` dials the hub only through its own
identity. A first join takes an auth key file; the state directory carries the identity
after. A start from saved state returns before the hub's name resolves, so the first dial
can fail and the link's backoff retries it.

The phone follows (ADR-0049, [[EXP-20261113-1h4ztf]]): libtailscale cross-built for arm64
Android as a shared library beside the core, with a Go file of ours that gives netmon the
interfaces from `getifaddrs` (the app sandbox denies netlink) and tsnet a directory for
its log state. A hub in the app's settings with an auth key is reached only through an
identity of the app's own on that hub's tailnet, one per hub, started on the first
connect and kept for the life of the process; the key is kept encrypted under the
Android Keystore. `saarthi ctl` and `saarthi tui` still use TCP over the host's tailnet.

The realistic libraries, checked on 2026-10-16:

- **libtailscale**: full tsnet, multiple handles per process, NAT traversal, certs via
  the local API. Costs a Go toolchain and cgo in the build. [[EXP-20261016-rq1406]] settled the shape:
  a `buildGo127Module` derivation (libtailscale needs Go 1.27) builds the static archive
  with `-buildmode=c-archive`; a hand-written sys crate reads `LIBTAILSCALE_LIB_DIR` and
  links `static=tailscale` plus the CoreFoundation and Security frameworks on Apple
  targets. The published `libtailscale-sys` crate is not used, since its build script runs
  `go build` and downloads modules. Built and run on aarch64-darwin and, through the remote
  builder, x86_64-linux; archive about 27 MB. For Android it builds only as a shared
  library, cross-built with the NDK (ADR-0049).
- **tailscale-rs**, Tailscale's pure Rust tsnet: clean build, but an experimental
  preview with no TLS certs, weaker NAT traversal, and no Android support yet.

Hub and node take libtailscale in phase 3. The phone must embed tsnet as well, decided
2026-10-17: the Tailscale app on the phone may be joined to a different tailnet than the
hub's, and the app cannot hold two. Until phase 3 it uses the Tailscale app on the hub's
tailnet. The Android path is libtailscale cross-built with the NDK inside the client core's
aar; tailscale-rs is the fallback if it gains Android support first. That cross-build is
a phase 3 spike of its own.

## Repository layout

```
saarthi/
  flake.nix              flake-parts; rust toolchain, protobuf, go, android sdk
  Cargo.toml             workspace
  crates/
    proto/               .proto files and generated rust
    tailnet/             embedded tsnet: libtailscale for the hub and node
    hub/
    node/
    core/                uniffi client core
    uniffi-bindgen/      generates the Kotlin bindings for core
  android/               compose app, consumes core as an aar
  .github/workflows/     the user guide to GitHub Pages
  nix/
    hub-module.nix       nixos module, `nixosModules.saarthi-hub`
    node-module.nix      home-manager module (launchd and systemd), `homeModules.saarthi-node`
    module-tests.nix     their evaluation tests, `checks.modules`
  docs/
    guide/               the user guide (mdBook, ADR-0033), `nix build .#docs`
```

The project flake is a path input of the root flake with nixpkgs following the root, and
it already uses flake-parts. Its default shell has the Rust toolchain from nixpkgs; the
Android SDK, NDK, emulator and a Rust toolchain with the Android target are in
`nix develop .#android` (ADR-0024); it may add inputs of its own, such as crane or rust-overlay,
followed by `nix flake lock` at the root. Rust, protoc, Go, and the Android SDK all exist
in nixpkgs. The Android SDK needs an unfree licence accepted in the nixpkgs config. The
nix modules live here and are their source of truth (ADR-0041): neusis imports them and
keeps only what is its own, the secrets, the data volume and ntfy.

## Phases

The project page's *Progress* checklist tracks how far each phase has got.

### Phase 0: spikes
Done on 2026-10-16, one experiment page each under the project's `experiments/`; findings
are folded into the sections above. [[EXP-20261016-rq1401]] hook round trip, [[EXP-20261016-rq1402]] transcript
shapes and rate limits, [[EXP-20261016-rq1403]] PTY injection, [[EXP-20261016-rq1404]] AskUserQuestion,
[[EXP-20261016-rq1405]] headless multi-turn, resume and background sessions, [[EXP-20261016-rq1406]] libtailscale
under Nix, [[EXP-20261016-rq1407]] mxc seatbelt, [[EXP-20261016-rq1408]] credential leasing, [[EXP-20261016-rq1409]] GPU
metrics on macOS. Throwaway code is under `spikes/`. The open questions it left were closed
on 2026-10-17 with follow-up runs on [[EXP-20261016-rq1402]], [[EXP-20261016-rq1404]], [[EXP-20261016-rq1405]], [[EXP-20261016-rq1407]]
and [[EXP-20261016-rq1408]], the last of which changed the credential model to grants and slots.

### Phase 1: watch and intervene

The daemon, the hub, the proto crate, the client core, the Android app, and the nix
modules, all minimal. Done when, from the phone:

- every node and every live session is listed, with the conversation streaming live;
- a permission dialog can be allowed or denied, and falls through to the terminal when
  ignored;
- a question from Claude can be answered;
- a session under the wrapper can be prompted and interrupted;
- a notification arrives when a session blocks or finishes;
- health shows as a chart per node.

Host tailscaled, one tailnet, pre-shared tokens, SQLite. The adapter trait exists with
one implementation. The credential store exists only for peer tokens.

### Phase 1.5: fit for daily use

What using the phase 1 app for a day asked for (the owner's notes of 2026-10-25,
ADR-0027 to ADR-0030):

- the app's reconnect bug ("Hub object has already been destroyed");
- prompts typed into the PTY for wrapped sessions, the socket only for the rest
  ([[EXP-20261025-25f4bf]]);
- `saarthi node serve --ignore-cwd <glob>`, so the live checks' throwaway sessions under
  `target/live-work-*` never reach the live hub;
- an `ended` notification and `--no-notify <kind>` on the hub;
- an alert when a rate-limit window is used too fast, nearly used up or used up
  (ADR-0032);
- time and tokens per turn, totals per session, cost from a price table in the hub
  ([[EXP-20261025-25f8mm]]), and the account's rate-limit utilisation as health metrics
  from the usage endpoint, with the access token read, never written
  ([[EXP-20261025-25f6vm]]);
- the prompt bar and the person's turns designed properly.

Done when the owner has used the app for a week against ghost001 without reaching for
`saarthi ctl` or the terminal for anything it covers.

### Phase 2: launch and own sessions

Built in the order of ADR-0034: spawn and kill under the node's own login first, then
resume and history, then named setup-tokens chosen at spawn (ADR-0039).

- Spawn a headless session on a node in a chosen directory, from the phone, under the
  `host` profile.
- Kill any session.
- Named setup-tokens from the hub's secrets, a default per node, a token chosen at
  spawn time, expiry reminders (ADR-0039).
- Resume a finished session by id.
- Session history browsing in the client: past runs, their events, cost.

### Phase 3: confinement and many networks

- Sandbox runtime through mxc: bubblewrap and seatbelt profiles with filesystem and
  secret policy; persistent workspaces.
- Embedded tsnet in hub and node through libtailscale; tailnet auth keys in the
  credential store; enrollment bundles; hub on its own tailnet.
- libtailscale cross-built for Android with the NDK and embedded in the client core, so
  the phone joins the hub's tailnet regardless of what the Tailscale app is joined to.
- Policy rules on PreToolUse: auto-allow by tool, node, and path.
- A second adapter, terminal-level, for a harness without hooks.
- Health exported to the separate monitoring stack; the hub stops being the long-term
  store for it.
- Postgres.

### Phase 4: autonomous starts and more clients

- Micro VM and container backends; network policy with an egress proxy that injects
  secrets so the sandbox never holds them.
- Triggers: inbound messages and scheduled scrape jobs that start background sessions,
  always under a sandbox profile.
- Web and iOS clients over the same core.
- Integrations into other apps through the control service.

## Open questions
None left from phase 0. New ones go here as they come up.

