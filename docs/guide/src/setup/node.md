# A node

A node is any machine where you run Claude Code: a laptop, a desktop, a build server.
Each runs `saarthi node serve` as the user who runs Claude Code, since it reads that
user's `~/.claude`. macOS and Linux are supported.

## 1. A token

Add a line for the machine to the hub's node-token file and restart the hub:

```text
laptop 3f9a…
```

Put the token alone in a file on the machine, readable only by you, for example
`~/.config/saarthi/node-token`.

## 2. The daemon

```console
$ saarthi node serve --hub http://hub:7700 --token-file ~/.config/saarthi/node-token --name laptop
```

`--name` is what the app shows; it defaults to the hostname. The daemon connects out to
the hub and reconnects by itself, with backoff, when the link drops. While the hub is
unreachable it keeps up to 200,000 events and sends them when the link is back.

Useful flags (all of them are in the [command line reference](../reference/cli.md)):

| Flag | What it does |
|---|---|
| `--claude-usage` | Report your Claude rate-limit usage as health, polled every five minutes. See [Rate limits](../using/health.md#rate-limits). Off by default. |
| `--keep-awake` | Keep the machine from idle-sleeping while any session is busy (`caffeinate -i` on macOS, a `systemd-inhibit` lock on Linux where polkit allows it; see [Keep-awake on Linux](#keep-awake-on-linux)), so a turn finishes when the screen turns off. Lid-close sleep still happens. Off at start by default; the switch on the node's health screen turns it on and off while the daemon runs. |
| `--spawn-root <dir>` | Allow the app to start sessions in this directory or below it. Repeatable; without it, nobody can start sessions on this node. A missing directory is created at start; one that cannot be is left out, with an error in the log. See [Starting sessions from the app](#starting-sessions-from-the-app). |
| `--claude <path>` | The `claude` that started sessions run. Default `claude`, found on your login shell's PATH. |
| `--profiles <file>` | Launch profiles a started session may run under, such as a sandbox. A file that does not parse is left out, with an error in the log, and sessions run on the host. See [Sandboxed sessions](#sandboxed-sessions). |
| `--hook-socket <path>` | Where sandboxed sessions send their hooks. Default `~/.local/state/saarthi/hook.sock`. |
| `--ignore-cwd <glob>` | Do not track sessions whose working directory matches, for example throwaway test sessions. Repeatable. |
| `--hold-secs` | How long a permission dialog of a plain `claude` session waits for the phone before falling through to the terminal. Default 120. |
| `--wrapped-hold-secs` | The same for sessions under `saarthi run`, where a key press releases it at once. Default 540. |
| `--hook-addr` | Where the hook endpoint listens. Default `127.0.0.1:7777`; must match the hooks block. |
| `--claude-dir` | Claude Code's config directory, if not `~/.claude` (honours `CLAUDE_CONFIG_DIR`). |

Without `--hub`, the daemon prints everything it observes to stdout as JSON lines, which
is a handy way to see what it sees.

### Reaching the hub on another tailnet

If the hub serves a tailnet this machine is not on (see
[On several tailnets](hub.md#on-several-tailnets)), the daemon can join it with a machine
of its own, whatever tailnet the host itself uses (ADR-0048). Make a pre-approved auth key
on the hub's tailnet and:

```console
$ saarthi node serve --hub http://saarthi-hub:7700 --token-file ~/.config/saarthi/node-token \
    --tailnet --tailnet-authkey-file ~/.config/saarthi/tailnet-key
```

Every connection to the hub then goes through that machine, named `saarthi-<name>`
(`--tailnet-hostname` to change it), with its state in `~/.local/state/saarthi/tailnet`
(`--tailnet-state`). The key is needed for the first join; you can delete it after, but
if the tailnet ever logs the machine out, the node waits for a login until you give it a
key again.
The log says `on the tailnet` once it has joined; until then, and for a moment after a
restart, the link reports `not on the tailnet yet` or a name it cannot resolve, and
retries.

## 3. The hooks

Claude Code has to be told to send its events to the daemon. `saarthi node hooks` prints
the block to merge into `~/.claude/settings.json`; it registers one HTTP hook per event
(`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PermissionRequest`,
`Notification`, `Stop`, `SubagentStop`, `SessionEnd`), all pointing at
`http://127.0.0.1:7777/hook`.

```console
$ saarthi node hooks                     # print the block
$ saarthi node hooks --hook-addr 127.0.0.1:7788 --timeout 600
```

The block must be in your user settings file. Claude Code reads hooks passed with
`--settings` only after the session has started, so the session-start event would be
missed. Merge it by hand, or with `jq` as in the [Quickstart](../quickstart.md#5-point-claude-code-at-the-daemon)
when you have no hooks of your own. Restart sessions that were already running so they
load the hooks; until then the daemon still finds them through the session registry and
their transcripts, but cannot hold their dialogs.

If the daemon is not running, the hook calls fail and Claude Code carries on without
them: a failed HTTP hook does not block a session.

## 4. Start at login

### macOS: a launchd agent

`~/Library/LaunchAgents/me.leoank.saarthi-node.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>me.leoank.saarthi-node</string>
  <key>ProgramArguments</key>
  <array>
    <string>/Users/you/.nix-profile/bin/saarthi</string>
    <string>node</string><string>serve</string>
    <string>--hub</string><string>http://hub:7700</string>
    <string>--token-file</string><string>/Users/you/.config/saarthi/node-token</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardErrorPath</key><string>/tmp/saarthi-node.err.log</string>
</dict>
</plist>
```

```console
$ launchctl load ~/Library/LaunchAgents/me.leoank.saarthi-node.plist
```

### Linux: a systemd user service

`~/.config/systemd/user/saarthi-node.service`:

```ini
[Unit]
Description=saarthi node daemon

[Service]
ExecStart=%h/.nix-profile/bin/saarthi node serve --hub http://hub:7700 --token-file %h/.config/saarthi/node-token
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

```console
$ systemctl --user enable --now saarthi-node
$ loginctl enable-linger $USER     # keep it running when you are logged out
```

#### Keep-awake on Linux

Keep-awake takes a blocking `idle:sleep` lock from logind with `systemd-inhibit`. logind
asks polkit, and polkit's default refuses the lock to a user service, which runs outside
your login session. The node checks at start: when the lock is refused it logs
`keep-awake unavailable` and the app shows no switch. To allow it, add a polkit rule; on
NixOS:

```nix
security.polkit.enable = true;
security.polkit.extraConfig = ''
  polkit.addRule(function(action, subject) {
    if ((action.id == "org.freedesktop.login1.inhibit-block-sleep" ||
         action.id == "org.freedesktop.login1.inhibit-block-idle") &&
        subject.user == "me") {
      return polkit.Result.YES;
    }
  });
'';
```

Replace `me` with your user, and restart the node after the switch. Closing the lid
still suspends the laptop.

### With home-manager

The flake exports a home-manager module that writes the launchd agent or the systemd
user service, with an option per flag, and puts `saarthi` on your PATH:

```nix
{ inputs, config, ... }:
{
  imports = [ inputs.saarthi.homeModules.saarthi-node ];
  services.saarthi-node = {
    enable = true;
    hub = "http://hub:7700";
    tokenFile = "${config.home.homeDirectory}/.config/saarthi/node-token";
    spawnRoots = [ "${config.home.homeDirectory}/workspace" ];
    # optional: reach the hub through a tailnet machine of the node's own
    # tailnet = { enable = true; authKeyFile = "/run/agenix/saarthiNodeTailnetKey"; };
  };
}
```

with `inputs.saarthi.url = "github:leoank/saarthi";` in your flake. It adds the hooks
block to the Claude settings home-manager declares (`claudeHooks`, on by default), and its
`profiles` option is written to the file `--profiles` reads, so a sandbox's token can be a
secret at the path its `token_file` names. On macOS give `tokenFile` a fixed path: launchd
does not expand the per-user temporary directory agenix uses by default. Every option is
in [`nix/node-module.nix`](https://github.com/leoank/saarthi/blob/main/nix/node-module.nix).

## 5. The wrapper

For sessions you want to interrupt or kill from the phone, start Claude Code with
`saarthi run` instead of `claude`. Everything after `--` goes to `claude`:

```console
$ saarthi run
$ saarthi run -- --resume
$ saarthi run -- --model opus
```

A shell alias makes it the default: `alias claude='saarthi run --'`. The wrapper talks to
the daemon over a unix socket under `$XDG_RUNTIME_DIR` (or the temporary directory); if
the daemon is not running, the session still works and the wrapper says it cannot be
reached remotely. It tries again every two seconds, so a session started before the
daemon, or running while the daemon restarts, becomes reachable as soon as it is back.

When the hub holds [Claude accounts](hub.md#claude-accounts), a wrapped session runs under
the node's **default account**, the one set on the node's health screen in the app, and
the wrapper prints which before Claude Code starts. A change in the app applies to the next
session you start. The session holds a dummy token and sends its API requests through the
daemon to the hub, which adds the token (ADR-0051), so Claude Code shows a warning at start
that remote managed settings could not be loaded. It starts in auto permission mode, the
default the dummy cannot read from the account, unless you pass a mode of your own. To
pick another account, or the machine's own login:

```console
$ saarthi run --claude-token personal
$ saarthi run --own-login
```

A name the hub does not hold stops the wrapper before Claude Code starts. If the default
cannot be had (the hub is down), the session starts under the machine's own login and the
wrapper says why.

Two things the wrapper needs: the working directory must already be trusted in Claude
Code (the trust dialog cannot be answered remotely), and Claude Code's own `CLAUDE*`
environment variables are cleared before it starts, so a session started from inside
another one is a fresh session.

## Starting sessions from the app

To let the app start sessions on this node, give the daemon the directories they may run
in:

```console
$ saarthi node serve ... --spawn-root ~/workspace --spawn-root ~/scratch
```

A session can then start in any directory at or below a root, never outside one (the
path is checked after symlinks are resolved). The daemon starts it through your login
shell (`$SHELL -l`), so it gets the PATH and environment a terminal would, even when the
daemon itself runs under launchd or systemd with almost none; `claude` must be on that
PATH, or named with `--claude`. It runs with this machine's own Claude login.

## Sandboxed sessions

On macOS a started session can run confined, under a *launch profile* (ADR-0038). The
session then sees only its working directory, the Nix store and the paths the profile
lists; the rest of your home directory, your SSH keys and other sessions are out of reach.
It has the network, but no inbound connections. It never sees your own Claude login: it
runs under a Claude token, the account picked when it starts or the node's default, from
the [hub's Claude accounts](hub.md#claude-accounts). It never sees that token either: Claude
inside holds a dummy and sends its API requests over a socket in the profile's state
directory to the node, which relays them to the hub, which adds the token (ADR-0050,
ADR-0051). Its other traffic goes out directly. A profile can also name a token file
of its own, used when the session runs under no hub account (none picked and no default);
then the node adds that token itself:

```console
$ claude setup-token        # in your own terminal; paste the token into a file
$ chmod 600 ~/.config/saarthi/claude-oauth-token
```

The profiles live in a TOML file given with `--profiles`:

```toml
[profiles.sandbox]
backend = "seatbelt"
description = "Confined: the working directory, the nix store, git identity and the SSH agent"
state_dir = "~/.local/state/saarthi/sandbox"     # its Claude config, scratch and home
token_file = "~/.config/saarthi/claude-oauth-token"   # optional, see above
path = ["~/.nix-profile/bin", "/etc/profiles/per-user/me/bin", "/run/current-system/sw/bin"]
readonly = ["/private/etc/profiles", "/private/var/run/current-system", "~/.nix-profile",
            "~/.local/state/nix/profiles", "~/.config/git", "~/.ssh/config", "~/.ssh/known_hosts"]
claude_files = ["CLAUDE.md", "skills", "commands", "agents"]
env = { GIT_CONFIG_GLOBAL = "~/.config/git/config", SSH_AUTH_SOCK = "~/.bitwarden-ssh-agent.sock" }
```

- `state_dir` holds the profile's own Claude config directory, where its sessions'
  transcripts live; History and resume find them there, and a resumed session runs under
  the same profile. Each session's API socket is made in its `tmp/`, so keep the path short:
  a unix socket path may be at most about 100 characters, and a spawn under a longer one
  is refused.
- `path` and `readonly` bring in your tools; the profile directories are links into the
  Nix store, which is always readable.
- `claude_files` links those entries of `~/.claude` into the profile, so a sandboxed
  session follows your `CLAUDE.md` and has your skills.
- `env` adds variables. With `SSH_AUTH_SOCK` naming your agent's socket, the session can
  fetch and push with your keys without seeing them; leave it out to keep it from pushing.
  `nix develop` works through the Nix daemon.

The app's *New session* offers the node's profiles under *Runs as*, with each one's
`description`; `saarthi ctl spawn ... --profile sandbox` does the same from a terminal. A
session started under a profile shows its name in the session list, its header and
History.

## Checking it

The node appears in the app and in `saarthi ctl nodes` as connected. Start a session and it
appears under the node within a couple of seconds. On macOS the daemon logs to the file
named in the plist; on Linux, `journalctl --user -u saarthi-node`.
