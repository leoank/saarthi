# Decisions

One architecture decision record per file, `ADR-0001-short-title.md`, numbered in order
and never renumbered. A decision that changes an earlier one supersedes it; both stay.

ADR-0001 to ADR-0012 record the owner's answers in `../architecture-questions.md`
(2026-10-16); ADR-0013 to ADR-0018 the choices the phase 0 experiments settled; the rest
were made while building phase 1 (ADR-0024 to ADR-0026 with the owner's choices for
step 5); ADR-0027 to ADR-0032 came from using the app, with the owner's answers of
2026-10-25 (phase 1.5); ADR-0033 is the user guide (2026-10-26); ADR-0034 plans phase 2
with the owner's answers of 2026-11-06, ADR-0035 follows their first use of it,
ADR-0036 is step 2, history and resume, and ADR-0037 fixes spawned sessions' permission
requests and adds a permission mode to a spawn. ADR-0038 starts phase 3 with sandboxed
spawns, with the owner's answers of 2026-11-07.

| ADR | Decision | Status |
|-----|----------|--------|
| [ADR-0001](ADR-0001-observe-through-hooks-and-transcript.md) | Observe sessions through hooks and the transcript, drive them through a PTY wrapper, and spawn headless sessions in phase 2 | accepted |
| [ADR-0002](ADR-0002-phase-1-intervention.md) | Phase 1 intervention is allow or deny, answering questions, interrupting and prompting | accepted |
| [ADR-0003](ADR-0003-node-daemons-and-one-hub.md) | One daemon per node keeps an outbound link to one hub on the tailnet; clients talk only to the hub | accepted |
| [ADR-0004](ADR-0004-tailnet-and-pre-shared-tokens.md) | Tailnet only with pre-shared tokens, moving to embedded tsnet on many tailnets | accepted |
| [ADR-0005](ADR-0005-grpc-with-separate-node-and-control-services.md) | gRPC and protobuf on the wire, with separate node and control services | accepted |
| [ADR-0006](ADR-0006-sqlite-in-the-hub.md) | SQLite in WAL mode inside the hub keeps the full event stream, with Postgres later | accepted |
| [ADR-0007](ADR-0007-health-as-a-time-series.md) | Node health is a time series sampled every 30 seconds | accepted |
| [ADR-0008](ADR-0008-push-through-ntfy.md) | Push notifications go through a self-hosted ntfy | accepted |
| [ADR-0009](ADR-0009-rust-core-under-compose.md) | A Rust client core through UniFFI under a Compose app | accepted |
| [ADR-0010](ADR-0010-agent-adapter-trait.md) | A thin agent adapter trait with Claude Code as its one implementation | accepted |
| [ADR-0011](ADR-0011-humans-decide-in-phase-1.md) | Only humans decide in phase 1; policy rules come later | accepted |
| [ADR-0012](ADR-0012-nix-modules-in-the-project.md) | The hub and node Nix modules live in the project until they move to neusis | superseded by ADR-0041 |
| [ADR-0013](ADR-0013-permission-request-hook-with-keystroke-release.md) | The PermissionRequest hook is the permission callback, held and released on a keystroke | accepted |
| [ADR-0014](ADR-0014-prompts-over-the-messaging-socket.md) | Prompts go over Claude Code's messaging socket; interrupts over the PTY | accepted |
| [ADR-0015](ADR-0015-libtailscale-through-a-hand-written-sys-crate.md) | libtailscale is linked through a hand-written sys crate built by Nix | accepted |
| [ADR-0016](ADR-0016-the-phone-embeds-tsnet.md) | The phone embeds tsnet as well | accepted |
| [ADR-0017](ADR-0017-sandbox-through-mxc.md) | Sandboxed sessions run through mxc behind a Containment trait | accepted |
| [ADR-0018](ADR-0018-credentials-as-leased-slots.md) | Agent credentials are grants the hub leases to nodes as slots | superseded by ADR-0039 |
| [ADR-0019](ADR-0019-event-seq-from-the-clock.md) | A node numbers its events from the time in microseconds | accepted |
| [ADR-0020](ADR-0020-dedup-transcript-lines-by-id.md) | The hub stores a transcript line once per session by its uuid | accepted |
| [ADR-0021](ADR-0021-client-tokens-and-rc2s2-ctl.md) | Clients authenticate with their own token file, and `rc2s2 ctl` stands in for the phone | accepted |
| [ADR-0022](ADR-0022-held-prompts-reported-from-permission-mode.md) | A prompt to a session that bypasses permissions is reported as held, from the session's permission mode | accepted |
| [ADR-0023](ADR-0023-wrapper-socket-and-terminal-replies.md) | Wrappers reach the daemon over a per-user unix socket, and terminal replies are not keystrokes | accepted |
| [ADR-0024](ADR-0024-android-toolchain-through-the-project-flake.md) | The Android toolchain comes from the project flake, in a shell of its own | accepted |
| [ADR-0025](ADR-0025-push-to-a-configurable-ntfy-topic.md) | The hub pushes to a configurable ntfy topic, a local ntfy in development | accepted |
| [ADR-0026](ADR-0026-unifiedpush-through-the-ntfy-app.md) | The phone receives pushes through UnifiedPush, with the ntfy app as distributor | accepted |
| [ADR-0027](ADR-0027-prompts-through-the-pty-when-wrapped.md) | A prompt is typed into the PTY when the session runs under the wrapper; the socket stays for the rest (amends ADR-0014) | accepted |
| [ADR-0028](ADR-0028-the-node-ignores-sessions-by-directory.md) | The node ignores sessions whose working directory matches a pattern | accepted |
| [ADR-0029](ADR-0029-notify-on-held-requests-finished-turns-and-ended-sessions.md) | The hub notifies on held requests, finished turns and ended sessions, each kind switchable | accepted |
| [ADR-0030](ADR-0030-usage-turn-time-rate-limits-and-cost-in-the-app.md) | Usage, turn time, rate-limit utilisation and cost come from the daemon's events and the usage endpoint | accepted |
| [ADR-0031](ADR-0031-the-node-keeps-the-machine-awake-while-a-session-is-busy.md) | The node keeps the machine from idle-sleeping while a session is busy, when asked | accepted |
| [ADR-0032](ADR-0032-the-hub-alerts-when-a-rate-limit-runs-out-early.md) | The hub pushes an alert when a rate-limit window is used too fast, nearly used up or used up | accepted |
| [ADR-0033](ADR-0033-a-user-guide-on-github-pages.md) | A user guide on GitHub Pages, built by Nix with mdBook in yajna's theme | accepted |
| [ADR-0034](ADR-0034-phase-2-spawns-headless-sessions-under-the-node-s-own-login-first.md) | Phase 2 spawns headless sessions under the node's own login first, leases last | accepted |
| [ADR-0035](ADR-0035-a-spawned-session-may-start-idle-and-in-a-new-directory.md) | A spawned session may start idle, and in a directory the spawn creates | accepted |
| [ADR-0036](ADR-0036-history-and-resume-in-place.md) | Session history on one screen, and finished sessions resumed in place | accepted |
| [ADR-0037](ADR-0037-spawned-sessions-carry-the-node-hooks-and-a-permission-mode.md) | Spawned sessions carry the node's hooks, and a spawn picks a permission mode | accepted |
| [ADR-0038](ADR-0038-sandboxed-spawns-on-seatbelt-with-a-long-lived-token.md) | Sandboxed spawns on seatbelt, under launch profiles from the node's config, with a long-lived token | accepted |
| [ADR-0039](ADR-0039-sessions-run-under-named-setup-tokens-the-hub-holds.md) | Sessions run under named `claude setup-token` tokens the hub holds, not leased slots | accepted |
| [ADR-0040](ADR-0040-the-project-is-renamed-saarthi.md) | The project is renamed saarthi, "Guiding agents to victory", all the way down | accepted |
| [ADR-0041](ADR-0041-the-nix-modules-live-here-as-their-source-of-truth.md) | The hub and node Nix modules live in this repo as their source of truth | accepted |
| [ADR-0042](ADR-0042-a-token-s-usage-comes-from-the-sessions-under-it.md) | A Claude token's usage comes from the sessions that run under it | accepted |
| [ADR-0043](ADR-0043-usage-probes-keep-every-account-fresh.md) | Usage probes keep every account's usage fresh, half-hourly | accepted |
| [ADR-0044](ADR-0044-a-bad-optional-setting-costs-that-setting-not-the-process.md) | A bad optional setting costs that setting, not the process | accepted |
| [ADR-0045](ADR-0045-keep-awake-on-linux-holds-a-logind-inhibitor.md) | Keep-awake on Linux holds a logind inhibitor | accepted |

Each record:

```markdown
# ADR-0001: <the decision, as a sentence>

Status: proposed | accepted | superseded by ADR-xxxx
Date: YYYY-MM-DD

## Context
What forced the decision, and the alternatives considered.

## Decision
What was chosen.

## Consequences
What follows, good and bad.
```
