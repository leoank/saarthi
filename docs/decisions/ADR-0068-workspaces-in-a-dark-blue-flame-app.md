# ADR-0068: Workspaces in a dark, blue-flame app

Status: accepted
Date: 2026-11-18

## Context
The app has grown past managing agent sessions. It shows the owner's yajna notebooks
(ADR-0053), a shell on any machine (ADR-0054) and what a session changed (ADR-0067), and
more will come: a part the owner calls *nitya*. Today the sessions are the app and every
other part is a side door: notebooks behind a book icon on Home, a terminal behind an
icon in the machine list, changes in a session's menu. Leaving a terminal ends its shell.
The palette is Claude's own, warm charcoal and terracotta, light and dark; its surfaces
sit a few shades apart in one grey-brown and the app reads as washed out. The owner asked
for workspaces, one per part, and a palette of their own, after the blue of a flame on a
dark background as ovis has, with attention to detail (2026-11-18). Three palettes were
shown on the app's real screens (Home, a session, the model picker) before choosing.

## Decision
- **Workspaces behind a bottom bar**: *Agents* (the sessions, today's Home), *Notebooks*,
  *Terminals*, and *Nitya* once it has anything to show. Each keeps its own back stack, so
  a tab returns to where it was. The bar hides inside a session, a notebook, a terminal
  and the changes view, which keep the whole screen; system back leaves those first, then
  goes to Agents, then out. What needs the owner is a count on the Agents tab.
- **Shells stay open while the app runs.** A terminal's handle lives in the app's model,
  not its screen: switching tabs keeps the shell, *Terminals* lists the open shells and the
  machines that offer one, and a shell ends when it is closed there, when it exits, or
  when the app's process dies.
- **One dark theme, "blue flame"**, and no light theme: near-black backgrounds tinted
  blue (`#080B11`), three surface steps a few percent apart (`#0B1018`, `#111824`,
  `#172131`) separated by hairlines (`#1F2A3B`) rather than fills; a gas-flame blue as the
  accent (`#3D8BFF`) and its pale cyan core (`#8FD8FF`) for highlights and gradients;
  the flame's hot amber (`#FFB547`) only for what needs the owner, the one glow; green
  (`#4ED6A0`) for ready and healthy. Each workspace has a hue of its own for its tab,
  icons and highlights: Agents blue (`#5AA0FF`), Notebooks ember (`#F2A65A`, yajna's
  fire), Terminals phosphor green (`#54E0A6`), Nitya violet (`#B69CFF`). Machine names,
  paths and branches are set in the monospace face.
- **The model chip** shows a small flame glyph, the model, and the effort as five bars
  that light from low to max; it opens a sheet of model cards, each with a line on what it
  is for, and the effort as the same five bars.

## Alternatives considered
- A switcher in the title, every screen one workspace's: an extra tap for every switch.
- A launcher of workspace tiles as home: two taps between workspaces and one more screen.
- One accent everywhere: the workspaces then differ only by icons, and the owner moves
  between them often.
- Midnight indigo (a calmer blue-violet) and cyan on teal-black: shown, not chosen.
- Keeping a light theme: every screen tuned twice; the owner uses dark, the terminal is
  dark anyway.

## Consequences
- The phone's light setting is ignored.
- Open shells hold a stream to the hub each while the app runs, in the background too,
  until closed.
- Screens that set colours of their own (the boot log, the terminal, notebook pages in
  the web view) are left as they are; yajna's pages keep yajna's styles.
