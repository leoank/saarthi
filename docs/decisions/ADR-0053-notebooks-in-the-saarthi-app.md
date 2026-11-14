# ADR-0053: Notebooks in the saarthi app

Status: accepted
Date: 2026-11-14

## Context
The owner asked whether yajna's phone app should be written in the yajna repo, reusing
this one's work, or built on top of the saarthi app ([[AGS-20261114-rapbh2]]). What they
want from it: start new projects, look at a project's files, do reviews, open a terminal on
a machine, run bench work from a protocol (a checklist, timers), and talk to it and hear
it. Almost all of it either is a session (a new project is an agent's interview) or sits
beside one. yajna's roadmap had a native capture app in its phase 4. The owner chose on
2026-11-14 one app, saarthi's.

## Decision
- **One app.** The saarthi app gains a notebook section; there is no separate yajna app.
  yajna's logic and API stay in yajna; the app reaches each machine's notebooks through
  the service relay (ADR-0052), and lists them from yajna's own API.
- **The web view first.** What `yajna serve` already renders (the files viewer, the
  site's pages, a review's diff) is shown in a WebView over the relay; native screens
  replace it where the phone needs it more than the browser does.
- **Native where it pays, in this order:** a new project, which is a spawn of
  `/yajna:project <brief>` in the chosen notebook whose interview arrives as questions the
  app already answers; a review list with its threads, and a prompt to the session that
  made a review's branch to address them; then bench work on yajna's run sheets (its
  phase 2b): the steps, ticks, timers kept on the phone ([[EXP-20261114-raqkym]]) and voice
  (ADR-0055).
- **iOS** is not in scope; the core keeps its notebook calls in Rust so an iOS app could
  have them later.

## Alternatives considered
- A yajna app in the yajna repo: it would rebuild the hub connection, the tailnet, push
  and the app's shell, and still need saarthi for every agent-facing feature.
- yajna's screens as a Gradle module in the yajna repo built against saarthi's core: two
  repos to change for one screen, before there is a second app to share with.
- Native screens only: weeks before the first useful screen, where the web view gives
  files and reviews at once.

## Consequences
- The app depends on yajna's HTTP API; yajna versions it and saarthi follows.
- A WebView over the relay is the riskiest part ([[EXP-20261114-raqjjy]]).
- If a standalone yajna app is ever wanted, the hub, tailnet and relay client have to be
  pulled out of the app into a module first.
