# ADR-0045: Keep-awake on Linux holds a logind inhibitor

Status: accepted
Date: 2026-11-12

## Context
ADR-0031 kept a Mac out of idle sleep with `caffeinate -i` while a session is busy and
held nothing on other platforms. A Linux node still reported the switch, so the app
offered it for ghost001, where it held nothing and read as "letting it sleep". The owner
asked for the hold on Linux too, for curiosity, a Linux laptop. Checked on ghost001
(NixOS, no polkit running): logind refuses every inhibitor, blocking or delaying, idle or
sleep, to a user process ("Access denied"). On a desktop with polkit, logind's default
policy lets a process in an active local session take a blocking sleep lock and refuses one
outside a session, which is where a systemd user service runs.

## Decision
- On Linux the hold is `systemd-inhibit --what=idle:sleep --who=saarthi --mode=block`
  around `cat` reading a pipe from the node: dropping the child kills it, and `cat` ends
  when the node's end closes, so no lock outlives the node.
- At start the node takes the lock once around `true`. If logind refuses, or there is no
  `systemd-inhibit` on PATH, the node holds nothing and reports no keep-awake metrics, so a
  client offers no switch; the log says why. macOS is unchanged.
- A machine that should hold grants it with a polkit rule for the user
  (`org.freedesktop.login1.inhibit-block-sleep` and `inhibit-block-idle`); the setup guide
  shows one. saarthi's node module is a home-manager module and does not set system policy.

## Alternatives considered
- The hold without the check: a node on a machine that refuses would show a switch that
  does nothing, the confusion this replaces.
- `--mode=delay`: delays sleep for a few seconds; it does not keep a turn running.
- `systemd-run --scope` under the user's session or a D-Bus call of our own: the same
  logind policy decides, with more code.

## Consequences
- ghost001 (no polkit) shows no switch, which suits a server that never sleeps.
- curiosity shows the switch once its NixOS config carries the polkit rule; the lock is
  untested on a real desktop until then (it was booted into Windows when this was built).
- Lid-close still suspends: logind's lid handling ignores a sleep lock unless
  `LidSwitchIgnoreInhibited=no`.
