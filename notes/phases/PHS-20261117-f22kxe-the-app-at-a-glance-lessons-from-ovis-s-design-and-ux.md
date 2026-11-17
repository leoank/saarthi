---
id: PHS-20261117-f22kxe
type: phase
schema_version: 1
title: "The app at a glance: lessons from ovis's design and UX"
project: PRJ-0001
status: planned           # planned | active | done | abandoned
since: null               # the commit it started at: yajna phase start
until: null               # the commit it finished at: yajna phase done
created: 2026-11-17
author: agent:claude
tags: []
---

## Goal
The phone shows what needs the owner and what each agent is doing at a glance, and acting
on it takes one tap: lessons from ovis (github.com/devswha/ovis at ac48916, a chat, phone
and browser front end for coding agents over the herdr multiplexer), whose design and UX
were studied on 2026-11-17 from its code, its `DESIGN.md` and its screenshots
([[AGS-20261117-drdxrg]]). Done when a day of steering sessions from the phone needs no
scrolling to find what is waiting, and no reading of raw tool lines to know what a turn did.

## Progress
One item per piece of work, ticked when it lands; add items as the plan grows and keep the
done ones. The project page's *Progress* lists this phase on one line.
- [ ] 1. A turn folds into one header: "Worked for 2m 36s · 2 edits · 2 file reads · 1 command", *Working…* with a breathing dot while it runs and *Needs you* while it waits; a tap opens it to one verb-and-object row per tool ("Read src/metrics.ts", "Ran cargo test"), failures in red with the word, MCP tools shortened to server and tool; and each row still opens to everything shown today (the command or script, its output, the diff, full screen), so exactly what the agent did is always two taps away. Folded or open is remembered per turn, with a setting to open every turn by default. Core and app.
- [ ] 2. Question and permission cards docked above the prompt bar, not in the scroll: an *Input needed* pill, numbered options with their descriptions and a *Recommended* tag, the option's number typed in the prompt bar as an answer, multi-select as checkbox rows with *Submit (n)*, *Or type your own answer*; its height capped, only its text scrolling. App.
- [ ] 3. A session reads as what it is about: its title from Claude Code's own `ai-title` transcript lines (the node passes them on; today it drops lines without a uuid), "repo · folder" under it, and a state pill, RUN, INPUT, READY or DONE, in place of directory names and "working · started …". Node, core, app.
- [ ] 4. An inbox to start from: every machine's sessions in one list, those that need the owner first with their count, then working, then done, with filters; it grows out of *Waiting for you*. App.
- [ ] 5. Push that interrupts only when it should: a question's alert held about 10 s (the owner may be answering anyway), a finished turn alerted only after a turn of a minute or more, nothing while the app is in front; a rising chime for a question, a falling one for a finish. Hub and app.
- [ ] 6. Quiet reconnects and safe sends: *Reconnecting* only after about 4 s of a dropped link, unless something is waiting on it; a prompt kept in the bar until the hub takes it or 45 s pass, never resent by itself. App.
- [ ] 7. Queue while Claude works: *Send* becomes *Queue*, queued prompts sit as bubbles above the bar with *Send now* and discard, the stop button round and red. App and node.
- [ ] 8. The prompt bar and the transcript, polished: the model chip inside the bar, a progress chip from the session's todo list (`TodoWrite`, "1/3"), copy as Markdown or text and the time under a reply, copy and *edit and resend* under a prompt; replies unboxed, prompts as neutral right-aligned cards. App.
- [ ] 9. Find your way in a long session: search in the conversation, and a minimap of the owner's prompts to jump between turns; older items paged in as the person scrolls up. Core and app.
- [ ] 10. Review what a session changed: its working tree, staged changes and commits, unified diffs on the phone, files opened from there; reached through the node like the notebooks (ADR-0052). Node and app; a design record first.
- [ ] 11. The terminal's key bar: Esc, Tab, a sticky Ctrl, arrows and ^C, editable, under the terminal screen of [[PHS-20261114-rbv2fs]] item 6. App.
- [ ] 12. Voice at the cursor: dictation put into the prompt bar, never sent by itself; part of [[PHS-20261114-rbv2fs]] items 7 and 8. App.
- [ ] 13. Pairing and a first look: a hub's address, client token and tailnet key handed to the phone as a QR code (`saarthi ctl pair`), and a demo mode with a recorded session for a first start with no hub. Node, hub, app.

## Notes
What ovis looks like, from its screenshots: near-black warm neutrals, one amber accent, and
the other colours kept for state (RUN blue, INPUT red, READY and DONE muted); sans-serif
prose and monospace only for tool rows, paths and code; the agent's mark on each session;
a breadcrumb "machine › repo › path" under the title; a *Chat | Terminal* switch and a live
dot in the header; generous space and quiet metadata. saarthi's Material theme can take
the same discipline: one accent, state colours only for state.

Not to copy: ovis answers prompts by reading the terminal screen and sending keystrokes,
with a parser per agent that breaks with Claude Code releases (its notes on 2.1.311);
saarthi's hooks and its question tool (ADR-0061) stay. Its phone apps are WebView shells
around a UI the PC serves, it counts usage by default, and its design and change notes run
to hundreds of kilobytes.

Order: 1, 2, 5 and 6 first (each small, felt every day, no new protocol), then 3, 4 and 8,
then 7 and 9; 10 and 13 need design records; 11 and 12 ride on the terminal and voice items
of [[PHS-20261114-rbv2fs]]. From learnharness (github.com/amosblomqvist/learnharness),
studied the same day, the question tool and the context report are already built
(ADR-0061, 0.12.0).
