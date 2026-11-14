---
id: PHS-20261114-rbv2fs
type: phase
schema_version: 1
title: "Notebooks, a terminal and voice: yajna through a service relay, a node terminal, push to talk"
project: PRJ-0001
status: planned           # planned | active | done | abandoned
since: null               # the commit it started at: yajna phase start
until: null               # the commit it finished at: yajna phase done
created: 2026-11-14
author: agent:claude
tags: []
---

## Goal
The app is the owner's one client for agents and their yajna notebooks: a machine's
notebooks over a relay through the hub, new projects, reviews, a terminal on a node, push
to talk and spoken replies, then bench work from yajna's run sheets (`docs/notebooks.md`,
ADR-0052 to ADR-0055, yajna's ADR-0036). Done when a bench day and a review are done from
the phone alone.

## Progress
One item per piece of work, ticked when it lands; add items as the plan grows and keep the
done ones. The project page's *Progress* lists this phase on one line.
- [ ] 1. The service relay: does a streamed request reach a socket service through the hub, and feed a WebView ([[EXP-20261114-raqjjy]])
- [ ] 2. The service relay: services in the node's config, started on demand; `ServiceRequest`, `OpenService` and `ServiceStream` (ADR-0052)
- [ ] 3. The notebook section in the app: the notebooks of each node from yajna's API, the files viewer and pages in a web view (ADR-0053; needs yajna's `serve --socket`)
- [ ] 4. New project: a spawn of `/yajna:project <brief>` in a chosen notebook (ADR-0053)
- [ ] 5. The terminal: does libghostty-vt build for Android and keep up ([[EXP-20261114-raqjf0]])
- [ ] 6. The terminal: `--terminal` on the node, the `Terminal` stream, the app's terminal screen (ADR-0054)
- [ ] 7. Voice: which recognizer ([[EXP-20261114-raqk9b]])
- [ ] 8. Voice: push to talk and spoken replies (ADR-0055)
- [ ] 9. Reviews: a native list and threads, and "address this" to the branch's session (ADR-0053)
- [ ] 10. Bench: timers and offline ticks on the phone ([[EXP-20261114-raqkym]]; needs yajna's run sheets, its phase 2b)
- [ ] 11. Bench: a run sheet's steps, ticks and timers, and voice commands (ADR-0053)

## Notes
iOS is out of scope (ADR-0053). Items 1 to 4 can start now; 5 and 7 are independent
spikes; 10 and 11 wait on yajna.
