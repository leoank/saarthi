---
id: PHS-20261126-f3e51h
type: phase
schema_version: 1
title: "Apps on the hub: Cedar RBAC, who launched a session, the node as a library and an admin UI"
project: PRJ-0001
status: planned           # planned | active | done | abandoned
since: null               # the commit it started at: yajna phase start
until: null               # the commit it finished at: yajna phase done
created: 2026-11-26
author: agent:claude
tags: []
---

## Goal
Apps the owner writes use the hub as the phone does: they start agent sessions for their own
work on the owner's nodes, or on a node they embed as a library, and the owner sees which
sessions are whose. Other people are onboarded too: the schema has people as principals
from the start, each with their own clients, apps, nodes and grants, the owner an admin. Every control call is authorized by Cedar policies
(<https://github.com/cedar-policy>) instead of "any client token may do anything", so an app
is held to what it was granted: its own sessions, the nodes, profiles and Claude accounts it
may use, no terminal or shells unless granted. Nothing assumes where an app runs: it is a principal with a credential, reaching
the hub over any listener the hub has. Most apps are daemons on machines using the Rust
library (the client and the node), which comes first. A web page the hub serves lets the owner watch clients and
nodes, add a client's or node's token, revoke it and edit policies, without a deploy. The
agenix files stay: they bootstrap the hub (the first admin client, nodes, Claude tokens),
and what the UI adds is kept by the hub beside them.

Done when an app of the owner's, with its own token, spawns sessions on ghost001 and on a
node it embeds, sees and steers only those, is refused a terminal; the phone lists the
sessions as the app's; and the owner adds and later revokes that app's token from the admin
UI, with the agenix-bootstrapped clients working throughout.

## Progress
One item per piece of work, ticked when it lands; add items as the plan grows and keep the
done ones. The project page's *Progress* lists this phase on one line.

### Design and experiments
- [ ] 1. Cedar in the hub: [[EXP-20261126-f3e5a9]], a spike of the schema (principals, actions per control call, resources and their attributes), the policies for the owner, another person held to their own nodes and sessions, and an app held to its own sessions, checked case by case, and the cost of an authorization at the hub's sizes. Then an ADR: authorization with Cedar, superseding the all-or-nothing client token of ADR-0021.
- [ ] 2. Two nodes on one machine and in one process: [[EXP-20261126-f3e5v3]], what the daemon holds that is the machine's or the process's (the hooks in `~/.claude/settings.json`, the session registry it scans, its sockets and state directory, the Claude API proxy, tsnet's state, process environment and signals). Then an ADR: the node as a library.
- [ ] 3. An ADR on identity and secrets: who a principal is (a person; a client or a node, each belonging to a person, with the hub's name for it, never one it claims), `launched_by` stamped by the hub, an app's own labels on a session, tokens the hub keeps beside the agenix files (client and node tokens stored as hashes, shown once; Claude tokens encrypted under a key from agenix), revocation of either kind, and the admin principal the agenix files bootstrap.

### Who launched a session
- [ ] 4. `launched_by` on every session: the client whose token made the `Spawn` or `Resume`, or the node for a session started there (by hand, under `saarthi run`, or by an embedded node's own app). Stored by the hub, carried in `Session`, kept through a resume.
- [ ] 5. Labels: a small map an app sets on `Spawn` (`job=1234`) to match a session to its own work, returned with the session and searchable; separate from `launched_by`, which the app cannot set.
- [ ] 6. Lists, watch, search and history filter by launcher and label; the app gets a filter (mine, apps, one app) and the launcher on each session row.

### Authorization with Cedar
- [ ] 7. The policy engine in the hub: every control call builds its request (principal, action, resource, context) and is allowed or refused by Cedar; lists, watch, streams and search return only what the principal may read. The default policy set keeps today's behaviour for the agenix clients, so nothing changes until a policy says so.
- [ ] 8. Policies from two places: a file (from Nix or agenix, the bootstrap, read-only to the UI) and policies the hub stores, validated against the schema before they are taken; a change applies without a restart.
- [ ] 9. Node principals under the same engine: what a node may ask of the hub (a Claude token by name, `ClaudeApi` under which accounts, transcripts back), so an embedded node in an app is held to its grant too.
- [ ] 10. Requests from an app's sessions: who may resolve a held permission or answer a question (the app, the owner, either), and whether they notify the phone, set per app by policy rather than every request reaching the owner.
- [ ] 11. A refused call says which action was refused; refusals are logged for the admin UI.

### Apps as clients
- [ ] 12. What an app needs from a spawn beyond today's (an appended system prompt, MCP servers, allowed tools, structured output): through the node's launch profiles, per spawn, or both, each gated by policy. Decided in the ADR of item 3 or its own.
- [ ] 13. A turn's result without following the stream: wait for a session's turn to end and get its last reply, for apps that start work and collect it.
- [ ] 14. A client library for apps: the Rust client the core and `ctl` use, published as a crate with the protos; apps in other languages generate from the protos, nothing more for now.

### The node as a library
- [ ] 15. A `Node` an app builds and runs (hub address and credential, state directory, spawn roots, profiles, which Claude login), started and stopped by the app, holding nothing process-wide that a second node or the app itself would trip over (what item 2 found).
- [ ] 16. An embedded node is registered as the app's: the hub records its owner, and the app's Agents list keeps it apart from the owner's machines.
- [ ] 17. An example app and an end-to-end test: an embedded node and a client in one process, spawning and collecting a session against a test hub.

### The hub admin UI
- [ ] 18. A web page the hub serves (the owner's choice of 2026-11-26), on a listener of its own, signed in to as a person and allowed by the same policies; the phone app gets no admin screen for now.
- [ ] 19. Monitoring: people, clients and nodes with their last call, address or tailnet peer, open streams, sessions launched, recent refusals; a client's or node's calls over time.
- [ ] 20. People and secrets: onboard a person (and offboard one, revoking all they hold); add a client or node token (shown once, stored as a hash), revoke or rotate one, add or revoke a Claude token (stored encrypted); the agenix ones listed as bootstrap, revocable here, replaced only by editing the agenix file.
- [ ] 21. Policies: view the bootstrap and stored policies, edit the stored ones with the schema's validation, and try a request against them before saving.
- [ ] 22. The user guide and the nix modules: the hub's new options (policy file, the encryption key's agenix file, the admin listener), and how to add an app.

## Notes
Planned on 2026-11-26 with the owner ([[AGS-20261126-f3e4hp]]): the apps are the owner's
own, but this is the time for proper RBAC, with Cedar; no assumption about where an app
runs; the node usable as a library; an admin UI for the hub; agenix stays and bootstraps
the hub.

Open questions:
- How does a person sign in to the admin page: a token the agenix file or another admin
  gives them, or something more (a passkey)? For the ADR of item 3.

Settled with the owner on 2026-11-26: the hub serves the admin page; the Rust library
comes first, other languages later; no gRPC-web, since most apps are daemons on machines;
the schema allows more people, whom the owner will onboard.
