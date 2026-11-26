---
id: EXP-20261107-413q82
type: experiment
schema_version: 1
title: Does a headless stream-json claude session spawned through mxc seatbelt keep stdin control (prompt, interrupt), deliver hooks over a unix socket, reach the nix daemon, ssh agent and git identity, and leave its registry entry and transcript in the sandbox config dir for resume
project: PRJ-0001
status: concluded          # planned | active | paused | concluded | abandoned
outcome: positive         # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-07
author: agent:claude
protocols: []
materials: []
tags: [confinement, phase-3, sandbox, macos]
review: pending
---

## Question
[[EXP-20261016-rq1407]] ran `claude -p` once under mxc's seatbelt backend. The node's
spawner drives a stream-json session over stdin and stdout for its whole life (ADR-0034).
Through `mxc_sdk::v1::spawn`, do prompts and an interrupt control request on stdin still
work, do hooks (PermissionRequest held and allowed) arrive over a unix socket command hook,
can the session use the nix daemon, the owner's SSH agent, git identity and neusis tools,
are other sessions' sockets and the home directory refused, and do the registry entry and
the transcript land in the sandbox config dir so a later `--resume` there works?

## Hypothesis and predictions
mxc spawn hands back piped stdin and stdout and a kill, so the stream-json protocol is
unchanged; the minimum policy of [[EXP-20261016-rq1407]] plus read-only profile
directories, `~/.gitconfig` and `~/.claude` user files, and the nix daemon and SSH agent
sockets (reachable under `egress: allow`) is enough for `nix develop -c` and
`git ls-remote` over SSH. If stdin or the kill do not reach the child, the spawner needs
its own fork-and-`sandbox_init` instead of mxc; if a socket is refused, the profile maps
it read-write.

## Plan
1. A spike binary on `mxc-sdk` 1.0.0 (beside `spikes/07-mxc`) that spawns
   `claude -p --input-format stream-json --output-format stream-json` under a seatbelt
   policy built from a launch profile, with the token of [[EXP-20261107-413q6m]].
2. Turn 1 asks for `nix develop -c rustc --version` in a flake checkout, `git config
   user.name`, `ssh -T git@github.com` through the agent, a read of `~/.ssh/id_*` and of
   `/tmp/cc-socks` (both must fail); a PermissionRequest is held on the unix hook socket
   and allowed.
3. Turn 2 is interrupted mid-tool with a control request; then stdin is closed.
4. Check the sandbox config dir for `sessions/<pid>.json` and the transcript; resume the
   session id under the same profile with a code word.

## Success criteria
- Prompt, interrupt, end on closed stdin and kill all work through mxc.
- The held PermissionRequest is answered over the unix socket and applied.
- nix develop, git identity, SSH agent and neusis tools work; `~/.ssh` keys, the home
  directory and `/tmp/cc-socks` are refused.
- Registry entry and transcript are in the sandbox config dir; the resume keeps the code word.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.
- [[RUN-20261107-438rq0]] placeholder made before `yajna result`; holds the script, driver and outputs of the result run
- [[RUN-20261107-4dxw07]] the full session through mxc: probes as predicted, 13 PermissionRequests held and allowed over the unix socket, interrupt stopped a running tool in 0.2 s, resume kept the code word, registry and transcript in the sandbox config dir; a kill through mxc left the Bash tool's process group running

## Findings
From [[RUN-20261107-4dxw07]] (outputs in `notes/runs/RUN-20261107-438rq0/out/`), Claude Code
2.1.318 on rogue, Haiku, the token of [[EXP-20261107-413q6m]]:

- **stdio through mxc works unchanged.** `mxc_sdk::v1::spawn` hands back stdin and stdout;
  stream-json prompts, the `interrupt` control request (acknowledged, the running tool gone
  0.2 s later), and end on closed stdin all behave as on the host. The command runs as
  `exec <cmd>` so the pid mxc reports is claude's; that pid is the registry file's name.
- **Hooks over a unix socket.** Command hooks given with `--settings` running
  `/usr/bin/curl --unix-socket <hooks>/hook.sock` deliver SessionStart, UserPromptSubmit,
  PreToolUse, PermissionRequest (13 held a second and allowed), PostToolUse, Stop and
  SessionEnd. SessionStart arrives, unlike the HTTP hooks of the node today.
- **Inside.** `nix develop -c rustc --version` works through the nix daemon socket once
  `TMPDIR` points inside (nix makes its temp files under `/tmp` otherwise); git identity
  through `GIT_CONFIG_GLOBAL` pointing at the read-only `~/.config/git/config`; SSH to
  GitHub through the Bitwarden agent socket (`~/.bitwarden-ssh-agent.sock`, reachable
  under `egress: allow` because nothing denies it); the owner's tools from the profile
  directories on `PATH` (read-only `/etc/profiles`, `/run/current-system`,
  `~/.nix-profile`, `/nix/var/nix/profiles`); `getpwuid` with the opendirectoryd lookup.
- **Refused.** `~/.ssh/id_ed25519`, `~/Documents`, `~/.claude/.credentials.json`,
  `/tmp/cc-socks` (denied explicitly), loopback TCP to the node's hook port, writes to `~`.
- **Placement.** The registry entry (`sessions/<pid>.json`) and the transcript
  (`projects/<cwd slug>/<id>.jsonl`) land in the sandbox's `CLAUDE_CONFIG_DIR`, and
  `--resume` with the same profile finds the session; the owner's CLAUDE.md, skills and
  commands symlinked into it are read.
- **The kill gap.** `MxcProcess::kill` ends claude in 0.1 s, but claude runs each Bash
  command in its own process group (`zsh -c ...` as group leader); that group was
  reparented to launchd and kept running, still confined. The interrupt does stop it, so
  the node's interrupt-then-close-then-kill order covers a normal kill; the hard kill must
  also signal the tool process groups, found as descendants of claude before it is killed.
- A killed session leaves its `sessions/<pid>.json` behind, as on the host.
- Claude Code's Bash tool refuses `sleep N; ...` chains as a wait pattern; a test that
  needs a long tool uses something else (`python3 -c 'time.sleep(N)'`).

## Conclusion
Draft by agent:claude, for the owner to approve.

Mostly as predicted ([[RUN-20261107-4dxw07]], Claude Code 2.1.318 on rogue, the token of
[[EXP-20261107-413q6m]]): through `mxc_sdk::v1::spawn` the stream-json protocol is
unchanged (prompts, an interrupt that stopped the running tool in 0.2 s, end on closed
stdin); command hooks over a unix socket deliver every event, SessionStart included, and
13 held PermissionRequests were allowed; `nix develop`, git identity, SSH through the
agent and the owner's tools work inside once `TMPDIR` points inside; `~/.ssh` keys, the
home directory, the owner's credentials file and `/tmp/cc-socks` are refused; the registry
entry and transcript land in the sandbox config dir and a resume there keeps the code word.
So mxc's seatbelt backend serves the spawner; no fork-and-`sandbox_init` of its own is
needed.

One prediction failed: a kill through mxc ends claude but not the Bash tool's process
group, which is reparented to launchd and keeps running (still confined). ADR-0038 builds
sandboxed spawns on mxc with launch profiles and adds to the hard kill the process groups
of the session's descendants. The session that was interrupted last exited 1 on closed
stdin, the other 0; not looked into.
