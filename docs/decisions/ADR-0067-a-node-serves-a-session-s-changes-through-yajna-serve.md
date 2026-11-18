# ADR-0067: A node serves a session's changes through yajna-serve

Status: accepted
Date: 2026-11-18

## Context
The phase *The app at a glance* ([[PHS-20261117-f22kxe]], item 10) asks to review what a
session changed from the phone: its working tree, staged changes and commits, unified
diffs, and files opened from there, reached through the node like the notebooks
(ADR-0052, ADR-0053). yajna needs the same view for its notebooks. The owner chose to build
it once, in yajna's `yajna-serve` crate, for both; to have the node embed that crate rather
than run the `yajna` binary; and to start read-only (2026-11-18). yajna's ADR-0038 records
its side: a read-only git API and a phone-first changes view in `yajna-serve`.

## Decision
- **The node links `yajna-serve`** as a Cargo dependency on github:leoank/yajna, pinned by
  `Cargo.lock` to a commit, and serves it in-process as a built-in local service,
  `changes`, on a socket of its own, listed with the configured services (ADR-0052). No
  yajna install is needed on a node.
- **Rooted at a session's directory.** A request names the session
  (`/s/<session id>/…`); the node serves `yajna-serve` with that session's directory as
  its one root, and refuses a session it does not know or that has no directory. The
  files and git routes then see only that worktree.
- **The app opens it from a session**: *Changes* in a session's menu opens the view in the
  web view the notebooks use (ADR-0053, ADR-0056), at `/s/<id>/files/changes`.
- **Read-only**, as `yajna-serve`'s git API is; whatever writes come later come through
  yajna's write rules.

## Alternatives considered
- Running `yajna serve` per directory as a configured service: every node would need
  yajna, and the CLI would have to serve directories that are not notebooks.
- A native changes screen in the app: yajna would need its own, and the two would drift.
- The hub computing diffs from the transcript's edits: misses what changed outside
  Claude's tools (a build, a `git` command), and has no working tree.

## Consequences
- saarthi builds against a pinned yajna commit; a change to `yajna-serve`'s routes needs
  both bumped together.
- Any session's repository, notebook or not, can be read from the phone, as far as the
  node's user can read it.
