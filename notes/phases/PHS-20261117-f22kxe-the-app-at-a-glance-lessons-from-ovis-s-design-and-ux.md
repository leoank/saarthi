---
id: PHS-20261117-f22kxe
type: phase
schema_version: 1
title: "The app at a glance: lessons from ovis's design and UX"
project: PRJ-0001
status: active
since: 6d5fd16be624b3dc10069156e731d2b58c479cd2
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
- [x] 1. A turn folds into one header: "Worked for 2m 36s · 2 edits · 2 file reads · 1 command", *Working…* with a breathing dot while it runs and *Needs you* while it waits; a tap opens it to one verb-and-object row per tool ("Read src/metrics.ts", "Ran cargo test"), failures in red with the word, MCP tools shortened to server and tool; and each row still opens to everything shown today (the command or script, its output, the diff, full screen), so exactly what the agent did is always two taps away. Folded or open is remembered per turn, with a setting to open every turn by default. Core and app. Built in the app alone (the core already marks a turn's end): finished turns fold, the running one stays open, Settings › Display › *Show every step* opens all; checked on the emulator against rogue's own sessions.
- [x] 2. Question and permission cards docked above the prompt bar, not in the scroll: an *Input needed* pill, numbered options with their descriptions and a *Recommended* tag, the option's number typed in the prompt bar as an answer, multi-select as checkbox rows with *Submit (n)*, *Or type your own answer*; its height capped, only its text scrolling. App. Built: the core now carries the options' descriptions; checked on the emulator with a spawned session's single and multi-select questions through its question tool (ADR-0061), answered by a typed number and by Submit (2).
- [ ] 3. A session reads as what it is about: its title from Claude Code's own transcript lines (the node passes them on; today it drops lines without a uuid), ranked by where it came from as atuin does, a name the owner gave (`custom-title`, `/rename`) over `ai-title` over the summary over the first typed prompt, the source kept so a title re-sent or cleared falls back to the next; "repo · folder" under it, and a state pill, RUN, INPUT, READY or DONE, in place of directory names and "working · started …". Node, hub, app.
- [ ] 4. An inbox to start from: every machine's sessions in one list, those that need the owner first with their count, then working, then done, with filters; scope chips, all, this machine, this repo, this directory, and a preview of a session's first prompt and last reply, as atuin's threads have; it grows out of *Waiting for you*. Hub and app.
- [ ] 5. Push that interrupts only when it should: a question's alert held about 10 s (the owner may be answering anyway), a finished turn alerted only after a turn of a minute or more, nothing while the app is in front; a rising chime for a question, a falling one for a finish. Hub and app.
- [ ] 6. Quiet reconnects and safe sends: *Reconnecting* only after about 4 s of a dropped link, unless something is waiting on it; a prompt kept in the bar until the hub takes it or 45 s pass, never resent by itself. App.
- [ ] 7. Queue while Claude works: *Send* becomes *Queue*, queued prompts sit as bubbles above the bar with *Send now* and discard, the stop button round and red. App and node.
- [ ] 8. The prompt bar and the transcript, polished: a progress chip from the session's todo list (`TodoWrite`, "1/3"), copy as Markdown or text and the time under a reply, copy and *edit and resend* under a prompt; replies unboxed, prompts as neutral right-aligned cards. App.
- [ ] 9. Find your way: search across sessions with SQLite's FTS5 at the hub, as atuin does (an index without stored text over titles and transcript text, a title hit weighted five times, older sessions ranked lower, the last word typed as a prefix, a snippet of the match), and within a session; a minimap of the owner's prompts to jump between turns; older items paged in as the person scrolls up. Hub, core and app.
- [ ] 10. Review what a session changed: its working tree, staged changes and commits, unified diffs on the phone, files opened from there; reached through the node like the notebooks (ADR-0052). Node and app; a design record first.
- [ ] 11. The terminal's key bar: Esc, Tab, a sticky Ctrl, arrows and ^C, editable, under the terminal screen of [[PHS-20261114-rbv2fs]] item 6. App.
- [ ] 12. Voice at the cursor: dictation put into the prompt bar, never sent by itself; part of [[PHS-20261114-rbv2fs]] items 7 and 8. App.
- [ ] 13. Pairing and a first look: a hub's address, client token and tailnet key handed to the phone as a QR code (`saarthi ctl pair`), and a demo mode with a recorded session for a first start with no hub. Node, hub, app.
- [ ] 14. History that orients: a second line on each row ("2h ago · rogue · saarthi/main · 42 messages") and day headers (Today, Yesterday, the date), as atuin lists threads. App.
- [ ] 15. Related sessions together: a resumed copy, a fork and a subagent's transcript linked to the session they came from and listed under it, not as rows of their own; a search hit in a child credited to its parent. Node and hub.
- [ ] 16. Deleting and keeping: a session deleted from the app stays deleted (the hub remembers it, so a node re-sending its lines cannot bring it back), and a retention setting for old transcripts; atuin's synced threads outgrew its shell history 25 times over (its issue 4434), and the hub keeps every line today. Hub and app; a design record first.
- [ ] 17. A resume whose copies diverged: when the node's transcript and the hub's differ (the session went on elsewhere), catch up or fork, never merge, and never write to a session that is open, as atuin's resume does. Node and hub; a design record first.
- [ ] 18. The model in the prompt bar, and a switch: a chip in the bar showing the session's current model and reasoning effort ("Opus 5.5 · High"), as ovis's prompt dialog does, read from the transcript (the last reply's `message.model` and its `effort`, and a `/model` or `/effort` answer at once, before the next reply); tapping it lists the models and efforts and switches the session: typed as `/model <name>` and `/effort <level>` into a `saarthi run` session, the SDK's `set_model` control request (and its effort setting, if it has one) for a spawned session, read-only for a plain `claude` the node cannot type into. Whether `set_model` and an effort change work in `-p` is an experiment first. Node, core, app.
- [ ] 19. *About this session* in place of *Usage* in a session's menu: everything known about it on one sheet, the token usage among it.
  - *Session*: its title, its id (copied with a tap), the agent (Claude Code, and its version, from the transcript's `version`; another agent's name once there are others), how it was started (at a terminal, under `saarthi run`, spawned from a client), the launch profile, the Claude account, the permission mode (`permissionMode`), the entry point (`entrypoint`).
  - *Where*: the node, the folder, the git branch (`gitBranch`), the transcript's path on the node, and whether the node still has it or a resume would restore it from the hub (ADR-0059).
  - *When*: started, last active, ended, how long it ran, how long Claude worked (the turns' durations summed).
  - *Model*: the current model and effort (item 18), every model used with its share of the turns.
  - *Usage*: tokens in, out and cached and the estimated cost per model, as *Usage* shows today; the context in use for a spawned session (ADR-0061); the account's rate-limit windows at the last report.
  - *Activity*: turns, prompts, tool calls by kind, files edited, failed calls, questions and permission requests asked and how they were answered.
  - *On the hub*: the lines kept for it and their size, so a large session shows before it matters (item 16).
  Hub (a call that sums a session's events and their size), core and app.

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
of [[PHS-20261114-rbv2fs]]. Items 3, 4, 9 and 14 to 17 come from atuin's threads
(github.com/atuinsh/atuin at fd95859, its `docs/docs/ai/threads.md` and
`crates/atuin-client/src/ai_session/`), studied the same day: a captured agent session
searchable and resumable on any machine, close to saarthi's history. Not taken from it:
continuing a session in another agent (atuin's largest source of complexity), plaintext
copies on a shared server without opting in, capture with no per-machine opt-out. From
learnharness (github.com/amosblomqvist/learnharness),
studied the same day, the question tool and the context report are already built
(ADR-0061, 0.12.0).
