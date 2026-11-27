# saarthi

*Guiding agents to victory.*

Watch and steer your Claude Code sessions from your phone. A small daemon on every
machine reports each session to one hub; an Android app talks to the hub. When Claude
stops for a permission or a question, your phone buzzes and you answer from the
notification.

**Documentation: <https://leoank.github.io/saarthi/>**

- Every Claude Code session on every machine, listed live with its conversation, including
  sessions started by hand in a terminal.
- Allow or deny permission dialogs and answer Claude's questions from the phone; ignore
  them and the terminal's dialog works as usual.
- Start a session on any machine from the phone; prompt and kill any session, and
  interrupt those started from the phone or with `saarthi run`.
- Notifications through UnifiedPush and ntfy, including alerts when your Claude rate
  limits are running out early.
- Node health and rate-limit usage as charts per machine.
- Your own apps start sessions through the hub, label them and find them again, or run
  them on a node they embed with the Rust library.

saarthi is in active development, built for one person's own machines on their tailnets.
Claude Code is the only agent and Android the only phone client so far; authorization by
policy, other people and a web admin page for the hub are being built.

## Try it

With [Nix](https://nixos.org/download) (flakes enabled):

```console
$ nix profile install github:leoank/saarthi#saarthi github:leoank/saarthi#saarthi-hub
```

then follow the [quickstart](https://leoank.github.io/saarthi/quickstart.html) to run a hub
and a node on one machine, and [Setting up](https://leoank.github.io/saarthi/setup/hub.html)
for a real deployment and the Android app.

## Repository

| Path | What |
|---|---|
| `crates/node` | `saarthi`: the node daemon, the `saarthi run` wrapper and `saarthi ctl` |
| `crates/hub` | `saarthi-hub`: gRPC services, SQLite, notifications |
| `crates/proto` | The protobuf definitions of both services |
| `crates/core` | The client core in Rust, exposed to Kotlin through UniFFI |
| `android/` | The Compose app |
| `docs/guide/` | The user guide (mdBook) |
| `docs/architecture.md`, `docs/decisions/` | The design and one record per decision |
| `notes/` | The project's [yajna](https://github.com/leoank/yajna) lab notebook: experiments, runs, progress |

## Development

```console
$ nix develop                         # Rust toolchain and protoc
$ cargo test --workspace
$ nix flake check                     # the workspace built and tested, the guide built
$ nix build .#docs                    # the guide, into result/
$ nix develop .#android               # Android SDK, NDK, Gradle (several GB)
$ cd android && gradle assembleDebug
```

Agents picking this up: start with `docs/architecture.md` and the decision records in
`docs/decisions/`, then the *Progress* checklist on the notebook page, `notes/README.md`,
for what has been built. `docs/HANDOFF.md` is the phase 0 handoff, kept for its history.
