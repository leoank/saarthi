# The terminal dashboard

`saarthi tui` shows the hub in a terminal: every node with its live sessions, what waits
for you, and your Claude accounts. It is for keeping an eye on things and answering them
from the keyboard; read a session's conversation in Claude Code or the app. It reads the
same `~/.config/saarthi/ctl.toml` as [saarthi ctl](ctl.md), and takes `--hub` and
`--token-file` the same way.

```console
$ saarthi tui
```

```text
 saarthi ghost001:7700
╭ Sessions ──────────────────────────────────╮╭ Waiting (1) ─────────────────────╮
│▾ ● ghost001  online · 1 live · account work││Bash  nix flake check             │
│    ◐ saarthi  ~/workspace                  ││  rogue / neusis · 10 min ago     │
│      working · terminal · 10 min ago · work││                                  │
│▾ ● rogue  online · 1 live · account work   ││                                  │
│    ! neusis  ~/workspace/hub/leoank        ││                                  │
│      waiting for you · terminal · 10 min ago││                                 │
│▸ ○ curiosity  offline                      ││                                  │
╰────────────────────────────────────────────╯╰──────────────────────────────────╯
 work 5h 3% · 7d 37% · 399d left
 ↑↓ move  ⏎/space fold node  p prompt  i interrupt  x kill  n new  tab waiting  ? help  q quit
```

On the left, each node with its default account, and under it its live sessions: `◐`
working, `!` waiting for you, `·` idle, then how it was started (*terminal* for `saarthi
run`, *headless* for one started from the app or here, *bare* for plain `claude`) and the
account it runs under. Enter or space on a node folds its sessions away.

On the right, the permission requests and questions waiting, the oldest first. Tab moves
between the two. The line under them is each Claude account's 5-hour and 7-day usage and
the days until its token expires, red in the last month.

## Keys

| Where | Key | Does |
|---|---|---|
| a session | `p` or Enter | send it a prompt |
| | `i` | interrupt its turn (a terminal session needs `saarthi run`) |
| | `x` | end it, after a `y` |
| a request | `a` or Enter | allow it |
| | `d` | deny it, with a message Claude sees (optional) |
| a question | Enter | answer it: ↑↓ picks, space ticks for a multi-select, Enter moves on and sends after the last |
| anywhere | `n` | start a session: node, directory (made if missing), prompt, model, permissions and account; ←→ changes a choice |
| | `r` | reload everything |
| | `?` | the keys |
| | `q` | quit |

Everything updates as it happens, from the hub's watch stream. If the hub goes away the
header says so and the dashboard reconnects on its own.
