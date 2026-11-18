# ADR-0069: A terminal look

Status: accepted
Date: 2026-11-18
Amends: ADR-0068

## Context
ADR-0068 chose workspaces and the blue-flame palette, shown with rounded cards and a sans
face throughout. Before it was built the owner asked for the whole app to feel more like
a terminal, as its boot log does, with minimal terminal-inspired elements for sections and
the like (2026-11-18). A mockup of four screens in that style (the Agents inbox, a session
with a question docked, the model sheet, Terminals) was shown with three settings, more,
as shown and less; the owner chose it as shown.

## Decision
- **Two faces.** JetBrains Mono, bundled with the app, for structure and facts: machine
  names, `repo:branch`, paths, counts, states, section rules, the workspace bar, the prompt
  bar and code. The phone's own sans face for what people and Claude wrote: titles,
  prompts, replies and questions.
- **A prompt as the top line**: where you are, `ank@ghost001:~/agents` on a workspace's
  root, `rogue:saarthi@main` in a session; under a workspace's root, the hub's state on one
  dim line.
- **Sections as rules**, `── needs you ── 2`, in the section's colour when it needs the
  owner.
- **Rows, not cards**, for sessions, machines and shells: a gutter mark (`▌` amber for
  what needs the owner, a braille spinner while working, `·` otherwise), the title, meta in
  dim mono, separated by dashed hairlines; what needs the owner also gets a faint amber
  wash.
- **States as coloured lowercase words**: `input` amber, `run` blue, `ready` green,
  `done` dim.
- **The workspace bar as a tmux status line**: `◆ agents 2 │ ▤ notes │ ❯_ term │ ✦ nitya`,
  the active one on the page's background with a glowing underline in its hue.
- **The prompt bar** starts with `❯` and shows a block cursor; the model chip reads
  `◢ opus-5.5 ▁▃▅▆█`; sheets and docked cards have `── title ──` headings and echo what
  they will do (`❯ /model opus · /effort high`).
- **Corners of 4 to 6 px**; the only glows are what needs the owner, the active tab and
  the New session button.

## Alternatives considered
- More terminal: mono for prose too, square corners and ASCII borders. Closest to the boot
  log, but long replies read worse.
- Less terminal: ADR-0068's rounded cards, borrowing only the rules, the top line and the
  bar.

## Consequences
- The APK carries the monospace font, a few hundred KB.
- Screens with their own looks (the boot log, the terminal, notebook pages) stay as they
  are; the boot log is now in keeping with the rest.
