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
- [x] 1. The service relay: does a streamed request reach a socket service through the hub, and feed a WebView ([[EXP-20261114-raqjjy]]): yes, through ghost001 too; the WebView's event streams over the app's bridge (ADR-0056)
- [x] 2. The service relay: services in the node's config, started on demand; `ServiceRequest`, `OpenService` and `ServiceStream` (ADR-0052): built in 0.8.0 with `--services`, the module's `services` option and `saarthi ctl service`; `OpenService` carries the request's head (ADR-0057); tested end to end on loopback, and on 2026-11-16 from rogue to its yajna through the hub on ghost001
- [x] 3. The notebook section in the app: the notebooks of each node from yajna's API, the files viewer and pages in a web view fed through `shouldInterceptRequest`, its event streams over the app's `EventSource` bridge (ADR-0053, ADR-0056; needs yajna's `serve --socket`): built in 0.9.0; the bridge also carries request bodies and range answers are placed at their offset (ADR-0058); checked on the emulator against a hub and node on this machine ([[AGS-20261116-fmnt2e]]), not yet on the phone
- [x] 4. New project: a spawn of `/yajna:project <brief>` in a chosen notebook (ADR-0053): built in 0.12.0, a sheet on the notebooks screen spawning yajna's command for the kind of project; a spawned session has no AskUserQuestion, so the node gives it a question tool of its own and the interview arrives as question cards ([[EXP-20261117-de4f82]], ADR-0060, ADR-0061); checked on the emulator against a local hub and node, the question tool against real Claude Code
- [x] 5. The terminal: does libghostty-vt build for Android and keep up ([[EXP-20261114-raqjf0]]): yes; on the Fold it takes a shell's output in at about 91 MB/s and draws a 10-second flood at 60 fps, 40 to 80 times Termux's library ([[RUN-20261118-f1904n]]), so ADR-0054 keeps libghostty-vt
- [x] 6. The terminal: `--terminal` on the node, the `Terminal` stream, the app's terminal screen (ADR-0054): built, with the module's `terminal` option; the hub joins the client's `Terminal` stream to the `TerminalStream` the node dials back, tested end to end on loopback; the app draws libghostty-vt's cells and takes keys through a view whose input connection keeps no text (a hidden text field doubled keystrokes); checked on the emulator against a local hub and node (`ls` colours, `top`, ^C, sticky Ctrl, arrows), not yet on the phone
- [ ] 7. Voice: which recognizer ([[EXP-20261114-raqk9b]])
- [ ] 8. Voice: push to talk and spoken replies (ADR-0055)
- [ ] 9. Reviews: a native list and threads, and "address this" to the branch's session (ADR-0053)
- [ ] 10. Bench: timers and offline ticks on the phone ([[EXP-20261114-raqkym]]; needs yajna's run sheets, its phase 2b)
- [ ] 11. Bench: a run sheet's steps, ticks and timers, and voice commands (ADR-0053)
- [x] 12. Shells that persist: does libghostty-vt's VT formatter restore a shell's screen, scrollback and alternate screen in a fresh terminal, on the machine's own platform ([[EXP-20261123-j9wyqh]])? Yes, with the primary screen kept by the holder when a program enters the alternate one ([[ADR-0078]]).
- [ ] 13. The shell holder: `saarthi shells`, a per-user process apart from the node (its own launchd agent or systemd user unit in the node's module, started by the node when run by hand), owning each shell's PTY and screen with 10,000 lines of scrollback; attach with the screen restored, several clients at once, list and kill; the node its client over a unix socket ([[ADR-0078]]). Node and module.
- [ ] 14. Shells across the hub: the `Terminal` stream attaches by shell id or starts one, and the hub lists and kills a node's shells ([[ADR-0078]]). Proto, hub, core.
- [ ] 15. The app's shells: Machines lists the shells each machine holds; a shell reattaches by itself after a drop, with backoff and at once on a new network, holding what is typed meanwhile ([[ADR-0078]]). App.

## Notes
iOS is out of scope (ADR-0053). Items 1 to 4 can start now; 5 and 7 are independent
spikes; 10 and 11 wait on yajna.
