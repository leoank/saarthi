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
reconnects on its own with backoff when it breaks; the subtitle says
*Reconnecting to …* while it does.

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
- it cannot ask you questions: AskUserQuestion does not exist in that mode;
- it ends if the node daemon stops, and you cannot attach a terminal to it while it
  runs. Afterwards `claude --resume <session id>` in its directory continues it.

## A session

Tap a session to open it. The conversation reads loosely like the terminal:

- your prompts as boxed lines, Claude's replies as formatted text, its thinking folded;
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

A prompt you sent shows a clock while it goes out, one tick once the hub has it and two
once it is in the transcript. Allowing, denying or answering a request shows no
confirmation: the card leaving is the confirmation, and a message appears only when the
node has something to say.

The header shows the session's tokens in and out and its estimated cost; *Usage* in the
menu shows the totals per model. Time and tokens also appear under each turn.
Costs are estimates from list prices, counted once per message, subagents included.

### Wrapping long lines

Code, diffs and command output scroll sideways by default. The wrap button in the top
bar (also in the full-screen viewer) wraps them to the screen instead. It is one setting
for the whole app and is remembered.

## Prompting

Type in the prompt bar at the bottom and tap send. What happens depends on the session:

- **Under `saarthi run`**: the prompt is typed into the session's terminal, exactly as if
  you had typed it. If Claude is working, Claude Code queues it until the turn ends
  (*Send queues after this turn*).
- **Started from the app**: the prompt goes to the process's input as your next turn.
- **Started with plain `claude`**: the prompt goes through Claude Code's own messaging
  socket and arrives as a message from another session; Claude sees it that way. The
  prompt bar says so before you send.

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

## History

The clock icon in the home screen's top bar opens **History**: every session the hub
knows, live and ended, newest first and grouped by day. A chip per node narrows it to one
machine. Each row shows the first thing typed in the session, its directory and node,
when it started, how long it ran, and its tokens and estimated cost; a terminal icon or a
rocket says it ran under `saarthi run` or was started from the app. *Show older* at the
bottom loads the next 50. Tap a row to read the whole conversation.

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
