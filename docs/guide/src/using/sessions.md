# Sessions

## The home screen

The home screen lists, from the top:

- **Waiting for you**: every permission dialog and question held on any node, each with
  its buttons, so what needs you is always the first thing you see.
- **Each node**, with whether it is online and its live sessions, newest first. A session
  shows its working directory, whether Claude is working or idle, a terminal icon when
  it runs under `saarthi run`, and a rocket when it was started from the app. The chart
  icon next to a node opens its [health](health.md).

Pull down or tap refresh to reload. The app keeps one stream open to the hub and
reconnects on its own with backoff when it breaks. A phone's link often drops for a
second or two, so the subtitle says *Reconnecting to …* only after about 4 seconds, or at
once when a prompt you sent is waiting on it.

## Starting a session

*New session* at the bottom of the home screen starts Claude Code on a node, headless.
It shows only when a connected node allows it: the node must run
with at least one `--spawn-root` (see [A node](../setup/node.md#starting-sessions-from-the-app)).
Pick:

- the **node**, when more than one allows it;
- the **directory**: one of the node's spawn roots, the directory of a live session under
  them, or any path you type below a root. With *Create it if it does not exist* on (the
  default), a path that does not exist yet is made first;
- the **first prompt**, optional: left empty, the session starts idle and you type the
  first prompt in its screen;
- the **model**: Claude Code's default, or Fable, Opus, Sonnet or Haiku;
- **runs as**, when the node has launch profiles: *This node's user*, or a profile such
  as a sandbox (see [Sandboxed sessions](../setup/node.md#sandboxed-sessions)). A session
  under a profile shows its name beside it;
- the **Claude account**, when the hub holds [named tokens](../setup/hub.md#claude-accounts):
  *Default* (the node's default account, named in brackets), one of the accounts, or
  *Node's login*, the Claude login of the machine itself;
- the **permissions**: *Ask* (your default mode: what needs approval comes to the app),
  *Accept edits* (file edits go ahead, the rest asks), *Plan* (Claude plans and changes
  nothing) or *Bypass*, which runs every tool without asking anyone, as
  `--dangerously-skip-permissions` does.

*Start* opens the new session as soon as its node has started it. It runs under the
account chosen (with no accounts at the hub, the node's own login, or a sandboxed
profile's token), as `claude -p` with its input and output owned by the node, so:

- prompts, interrupts and kill go straight to the process;
- its permission requests come to the app like any other, and wait as long as a wrapped
  session's (`--wrapped-hold-secs`); one left unanswered is denied, and Claude carries on
  without the tool. The node gives the session its hooks itself, so this works even if
  your `~/.claude/settings.json` has lost them;
- it asks you its questions as any session does, as question cards: AskUserQuestion does
  not exist in that mode, so the node gives the session a question tool of its own that
  waits for your answer (in a sandbox profile it has none, and asks in its messages);
- after each turn the header shows how full its context is (*context 12%*);
- it ends if the node daemon stops, and you cannot attach a terminal to it while it
  runs. Afterwards `claude --resume <session id>` in its directory continues it.

## A session

Tap a session to open it. The conversation reads loosely like the terminal:

- your prompts as cards on the right, Claude's replies as formatted text, its thinking
  folded;
- each tool call as one line, such as `Bash  cargo test`, with its output folded to a count
  of lines (or the error's first line) until you tap it;
- an edit shows its diff, a long command its script: the first lines under the call, and
  the whole thing full screen when you tap *N more lines*;
- notices from Claude Code and from saarthi (interrupted, killed, a slash command) as small
  captions.

While Claude is on a turn, a turning star and *Working…* with the seconds so far sit
under the last line. The view follows new output while you are at the bottom; scroll up
to read and a *Jump to latest* button brings you back. When your last prompt is off
screen, *Your last prompt* at the top scrolls to it. Long-press any text to select and
copy it.

Your prompts sit on the right as cards; Claude's replies read as plain text across the
screen. Under a reply, *Copy* copies it as Markdown or as plain text (no `**`, ticks or
heading marks; a link keeps its address in brackets), with its time beside. Under a
prompt, *Copy* copies it and *Edit and resend* puts it back in the prompt bar, in place
of what you had typed, to change and send again.

Your prompts and Claude's replies show the time Claude Code wrote them. A prompt you send
stays in the bar, *Sending…* above it, until the hub has it; it then shows with one tick,
and two once it is in the transcript. If the link is down it waits in the bar for the hub
to come back, up to 45 seconds. It is sent once and never again by itself: if the hub
refuses it, or does not answer in time, it stays in the bar with the reason, for you to
send again (when the hub took it but did not answer, look at the conversation first: it
may have arrived). Allowing, denying or answering a request shows no
confirmation: the card leaving is the confirmation, and a message appears only when the
node has something to say.

The header shows the session's tokens in and out and its estimated cost; *Usage* in the
menu shows the totals per model. Time and tokens also appear under each turn.
Costs are estimates from list prices, counted once per message, subagents included.

### Finding your way

A session opens with its last 300 transcript lines; older ones come in as you scroll
towards the top, without moving what you are reading. The magnifier in the top bar
searches the session: the count shows which match of how many, the arrows step to older
and newer ones (a folded turn opens to show its match), and the newest comes first. A
search brings in the session's older lines first, so it covers the whole session.

Down the right edge, a tick for each of your prompts, those on screen brighter: touch or
drag along it to see the nearest prompt, and let go to jump there.

### Wrapping long lines

Code, diffs and command output scroll sideways by default. The wrap button in the top
bar (also in the full-screen viewer) wraps them to the screen instead. It is one setting
for the whole app and is remembered.

## Prompting

Type in the prompt bar at the bottom and tap send. What you type and do not send stays as
that session's draft: leave the session, open others, and it is there when you come back,
also after the app restarts. What happens when you send depends on the session:

- **Under `saarthi run`**: the prompt is typed into the session's terminal, exactly as if
  you had typed it.
- **Started from the app**: the prompt goes to the process's input as your next turn.
- **Started with plain `claude`**: the prompt goes through Claude Code's own messaging
  socket and arrives as a message from another session; Claude sees it that way. The
  prompt bar says so before you send.

While Claude is working, send becomes *Queue* (a clock) and the stop button turns red.
A queued prompt waits on the node, not in Claude Code, and shows as a bubble above the
prompt bar until the turn ends; then the node sends the oldest, one per turn. Each bubble
has *Send now*, which hands it to Claude straight away (Claude takes it at its next step,
without stopping the turn), and a cross that drops it. The queue is the same on every
device, and survives the app closing; a restart of the node loses it (ADR-0062).

A session in bypass-permissions mode holds socket messages for approval at its
terminal. The app tells you when that has happened rather than pretending the prompt
landed.

## Interrupting and killing

**Interrupt** (the stop button in the prompt bar, or the menu) ends the current turn, for
sessions under `saarthi run` (it presses Escape, as you would at the keyboard) and sessions
started from the app. With text in the prompt bar, the stop button interrupts and then
sends that text as the next prompt. A session started with plain `claude` cannot be
interrupted from outside, so it has no stop button.

**Kill session** (the menu) ends any session's Claude Code process after a confirmation:
a wrapped one through its wrapper, one started from the app by interrupting it and
closing its input, and one started with plain `claude` by sending it SIGTERM, which
ends that terminal's session too. The transcript stays; nothing else is saved.

## When a session ends

It leaves the home screen's live list, and its screen says *This session has ended*.
With the `ended` notification on, you get a push.

A session whose machine went away while it ran (a reboot, a live disk restarted, a crash)
ends when that machine's node connects again and no longer has it, without a push; it
can then be resumed, from the hub's copy of its transcript if the machine lost it.

## History

The clock icon in the home screen's top bar opens **History**: every session the hub
knows, live and ended, newest first and grouped by day. A chip per node narrows it to one
machine. Each row shows the first thing typed in the session, its directory and node,
when it started, how long it ran, and its tokens and estimated cost; a terminal icon or a
rocket says it ran under `saarthi run` or was started from the app. *Show older* at the
bottom loads the next 50. Tap a row to read the whole conversation.

The field at the top searches every session: what you typed, what Claude replied, and
session titles and folders (not tool output). Every word must match; the last may be the
start of one, so results come as you type. Sessions come best match first, a match in a
title counting most and older sessions lower, each with the words around its match. Tap
one to open it already searching for the same words. A line is searchable a second or
two after it arrives (ADR-0063).

## Resuming a session

A finished session can be continued from the phone when its node is online and its
directory is under one of the node's spawn roots (see
[Starting a session](#starting-a-session)), whether it was started from the app or in a
terminal. Its screen then keeps the prompt bar: type and send, and the node runs
`claude --resume` on it in its directory, headless, with your text as the next turn.
*Resume* in the menu resumes it without a prompt, waiting for one.

It continues in place: the same session, the same conversation, with everything Claude
knew before, under the Claude account it last ran under. From then on it is like a session started from the app: prompts, interrupt
and kill all work, and it ends if the node daemon stops. It runs in your default
permission mode, whatever mode it was started in. In History it moves to the top,
with its start time and duration from the latest run.

A session that is still running somewhere cannot be resumed; end it first.

A session started from the app can be continued at a terminal on its machine too: end it
here (*Kill session*), take its id with *Copy session id* in the menu, then run
`claude --resume <id>` in its directory. Killing a session whose process is already gone
(its machine restarted) just marks it ended.

A node that no longer has the session's transcript (a reinstalled machine, a live disk
after a reboot, a deleted `~/.claude`) gets it back from the hub when you resume: the hub
keeps every line of the conversation, and the node writes them where Claude Code looks,
in the launch profile the session ran under, before running `claude --resume`. The
session's directory must exist on the node again, cloned or created. Sessions recorded
before 0.11.0 lack the attachments Claude Code adds between turns (a file you mentioned,
a hook's output); they resume with their prompts, replies and tool calls, without those.
Subagents' own transcripts are not restored.
