---
id: PHS-20261118-kbka9b
type: phase
schema_version: 1
title: "Workspaces and the blue flame: the app as a home for agents, notebooks, terminals and nitya"
project: PRJ-0001
status: active
since: 2f32e5af5e5d17965bbd6aa3cafbf4fcb61fcacf
until: null               # the commit it finished at: yajna phase done
created: 2026-11-18
author: agent:claude
tags: []
---

## Goal
The app is a home for the owner's parts, not a sessions app with side doors: agents,
notebooks, terminals and, later, nitya each a workspace one tap away, in a dark theme of
its own, the blue of a flame, with each screen's details looked at (ADR-0068). Chosen on
2026-11-18 from three palettes shown on the app's real screens. Done when every screen
uses the new theme's tokens, the four parts are reached from the bar, a shell survives
switching tabs, and the model chip and its sheet are as finished as Settings.

## Progress
One item per piece of work, ticked when it lands; add items as the plan grows and keep the
done ones. The project page's *Progress* lists this phase on one line.
- [ ] 1. The blue-flame theme, dark only: the palette of ADR-0068 as Material colour roles plus the app's own (the hot amber for what needs the owner, green for ready, a hue per workspace), a monospace style for machines, paths and branches, and the screens' hard-coded colours moved onto them. App.
- [ ] 2. Workspaces behind a bottom bar: Agents, Notebooks, Terminals (Nitya once it exists), each with its own back stack, the needs-you count on Agents, the bar hidden inside a session, notebook, terminal and changes view; the book and terminal icons leave Home. App.
- [ ] 3. Shells that stay open while the app runs: the terminal's handle and screen state in the model, *Terminals* listing the open shells and the machines offering one, closing a shell there. App.
- [ ] 4. The model chip and its sheet: a flame glyph, the model and the effort as five bars; a sheet of model cards with what each is for and the effort as the same bars. App.
- [ ] 5. A detail pass over every screen in the new theme: cards with hairlines, the needs-you glow, state pills, section headers, the New session button, the prompt bar, sheets and dialogs, empty and error states; checked screen by screen on the emulator and the phone. App.

## Notes
