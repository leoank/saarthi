# ADR-0076: Workspaces that stay live

Status: accepted
Date: 2026-11-23
Amends: ADR-0068

## Context
ADR-0068 put the app's parts behind a bottom bar that hides inside a session, a notebook,
a terminal and the changes view. In use that bar is the limit: from a notebook page, seeing
a session takes back, back, the Agents tab, then the session, and the page is gone from
view. The owner pointed at DaVinci Resolve (2026-11-23): its pages are always one tap away
and keep their exact state, the objects of one page (its media pool) are reachable from
the others, and the project settings sit in the same corner on every page. The owner also
asked for the machine list to move from Home to the Terminals workspace, renamed
*Machines*; Notebooks renamed *yajna*; the tailnet each machine and session is reached
over shown; a settings entry reachable the same way from every workspace; and the palette
a little less saturated. The phone folds open to a near-square screen.

Mockups of three answers were shown, folded and unfolded (2026-11-23): A, a bar that
never hides; B, A plus a switcher of everything open; C, two
panes on the unfolded screen. The owner chose all three.

## Decision
- **The bar never hides.** Agents, yajna and Machines (and Nitya later) are on it on every
  screen, a session, a notebook page and a shell included; it steps aside only while the
  keyboard is up. Each workspace keeps its own back stack, so a tab returns to the exact
  screen it showed; a second tap on the current tab goes to its root.
- **A gear at the bar's right end** opens Settings over whatever is showing, on the tab of
  the workspace in front (Agents, yajna, Machines), with *General* (hubs, the link to the
  hub, appearance) one tab away. One Settings, reached the same way everywhere.
- **The switcher.** Everything open across the workspaces (sessions, notebook pages,
  shells, a machine's health), most recent first. A drag up on the bar opens it as a
  sheet; a sideways swipe along the bar steps to the previous or next entry without
  opening it, as alt-tab does. The owner keeps the swipe until it is seen to misfire
  against Android's gesture handle below it; a button would replace it.
- **Needs you, answered in place.** The amber count on Agents opens a sheet of what is
  asking, with its Allow, Deny or answer, from any workspace; *Open* goes to the session
  and back returns.
- **Panes when unfolded.** A screen wide enough (600 dp and up) shows two panes, each
  holding a workspace with its own screen. The focused pane carries its workspace's hue
  along its top; the bar marks which workspace each pane holds; a tab fills the focused
  pane, a long press on a tab or a switcher entry opens it in the other. Swap and
  full-width buttons in each pane's header. Folding keeps the focused pane. Two sessions
  can be open at once, one per pane, each with its own stream.
- **Machines** replaces Terminals: the open shells, then every machine with whether it is
  up, its tailnet, version and load, and its shell, health and keep-awake. Home keeps one
  line, "3/4 machines up", and drops its machine list. A session's health button and a
  usage alert open the machine's health in Machines.
- **yajna** replaces Notebooks as the workspace's name.
- **The tailnet shown**: the hub's own next to its name, each machine's on its row, each
  session's (its machine's) in its line and header, as `⌁ name`, or `⇄ tcp` for a link
  that is not on a tailnet. The hub already records the tailnet a node's link came over
  (ADR-0048); the core passes it to the app.
- **A muted palette**: the accents and workspace hues keep their hue and lose about 40% of
  their saturation, the surfaces some of their blue; the amber loses less, being the one
  colour that must catch the eye. Blue `#5388D7`, core `#91C8E5`, amber `#E6AC58`, green
  `#68BC9B`, red `#DF7676`; Agents `#6999DC`, yajna `#D09C68`, Machines `#61C099`, Nitya
  `#AF9BE8`; background `#090B10`, surfaces `#0D1016`, `#131922`, `#1A222E`, `#202935`.

## Alternatives considered
- A alone: the bar stays, but moving between a notebook page and a session still means
  two workspaces' worth of navigation; no answer for the unfolded screen.
- A strip of open items across the top, as browser tabs: takes height on every screen and
  grows without bound; the switcher shows the same list only when asked.
- Panes on the folded phone: too narrow for two of these screens.
- A button for the switcher in place of the swipe: kept as the fallback.

## Consequences
- 48 dp of every screen go to the bar outside the keyboard.
- The app's state holds several open sessions, not one; a session's stream lives while it
  is in a pane or was open within the last few minutes.
- Routing across workspaces (a notebook starting a session, a session opening its
  machine's health) belongs to the app's shell, which the module split (ADR-0077) relies on.
- ADR-0068's palette values and its hidden bar are replaced; its workspaces, per-workspace
  back stacks and shells kept while the app runs stay.
