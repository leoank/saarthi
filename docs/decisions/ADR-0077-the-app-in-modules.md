# ADR-0077: The app in modules

Status: accepted
Date: 2026-11-23

## Context
The Android app is one Gradle module of about 9,300 lines of Kotlin, its state one view
model, `HubModel`, of over a thousand lines: the connection to the hub, the sessions and
their streams, photos, history and search, the shells, the notebooks, health, pairing, the
demo and every workspace's preferences. The app now holds three parts that are separate
concerns, the agents inbox, yajna's notebooks and the machines with their terminals, and
Nitya will be a fourth. The owner asked (2026-11-23) for the app to hold only the shell and
the shared infrastructure, each part a library of its own, and for the idea to be
critiqued first. The critique: the parts share one hub connection, one tailnet identity,
one notification link and one boot, so they are workspaces of one app, not apps; the
coupling is in `HubModel`, not the screens, so the split has to start there; build time is
the Rust core's, so the gain is boundaries, not speed; and the parts link to each other (a
notebook starts a session, a session opens its machine's health), which only the shell
should know.

## Decision
- **First the state, in one module.** `HubModel` becomes a thin owner of a shared
  `Connection` (the hubs, the link and its reconnects, nodes, sessions and requests from the
  watch, the demo, pairing, notices) and one model per workspace: `AgentsModel`,
  `YajnaModel`, `MachinesModel`. Each workspace's screens take their own model and the
  connection, never another workspace's.
- **Then the modules**: `:core` (the UniFFI bindings, the native library, the fonts, the
  `buildCore` task), `:ui` (the theme, the shared pieces, Markdown), `:agents`, `:yajna`,
  `:machines`, and `:app` (the activity, navigation, the bar and switcher, panes, Settings,
  push and the link service, boot). A workspace depends on `:core` and `:ui` only; it
  exposes its screens and takes callbacks for anything outside it, which `:app` routes.
  Nitya becomes `:nitya`.
- Settings stays in `:app`, each workspace's preferences in its model, shown on that
  workspace's tab of the one Settings screen (ADR-0076).

## Alternatives considered
- Packages within the one module: cheaper, and enforces nothing; `internal` means the
  whole app.
- Modules first, with `HubModel` moved whole into a shared module: every workspace would
  still reach into every other's state.
- Separate APKs per part: separate pairing, identity and notifications for one owner and
  one hub.

## Consequences
- Every step builds and installs: the state split lands before the modules.
- A workspace cannot open another's screen by itself; it asks the shell through a callback.
- More build files to keep in step (the Compose compiler plugin, the SDK levels), kept in
  one place in the root build.
