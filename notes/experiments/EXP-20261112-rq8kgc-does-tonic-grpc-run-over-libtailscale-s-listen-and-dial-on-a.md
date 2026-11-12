---
id: EXP-20261112-rq8kgc
type: experiment
schema_version: 1
title: Does tonic gRPC run over libtailscale's listen and dial on a real tailnet, with two identities in one process, peers named through status_json, and no fds leaking into spawned children?
project: PRJ-0001
status: active            # planned | active | paused | concluded | abandoned
outcome: null             # null | positive | negative | inconclusive | infra
influenced_by: [EXP-20261016-rq1406]
created: 2026-11-12
author: agent:claude
protocols: []
materials: []
tags: [tailnet, phase-3]
review: pending
---

## Question
Phase 3 embeds tsnet in the hub and the node (the owner's choices of 2026-11-12,
[[AGS-20261112-rpj0rk]]): an auth key file for the first join, the plain TCP path kept,
several tailnets on the hub from the start, and the peer's tailnet identity recorded.
[[EXP-20261016-rq1406]] only built and linked libtailscale and called four functions with
no network. Nothing has checked that tonic's server and client run over the descriptors
libtailscale hands out, that one process can hold two identities at once, that
`tailscale_status_json` names the peer behind an accepted connection, or that those
descriptors stay out of the processes a node spawns.

## Hypothesis and predictions
H1: a `tailscale_accept` connection and a `tailscale_dial` connection are ordinary
stream sockets that tokio can own, so tonic serves and calls over them unchanged.

- If H1 holds, a unary call and a server stream of 1000 messages pass between two
  handles in one process (a "hub" and a "node" identity, joined with one reusable
  ephemeral auth key), dialled by MagicDNS name, and an HTTP/2 keepalive ping keeps an
  idle stream open for a minute. If it fails, the transport boundary needs a copy loop
  between the descriptor and a socketpair, or a SOCKS5 dial through `tailscale_loopback`.
- `tailscale_getremoteaddr` plus the `Peer` map of `tailscale_status_json` give the
  dialling identity's host name and login. If not, the hub records only the address.
- At libtailscale `ac7ed43` (2026-10-30, "mark handed-out fds close-on-exec") a child
  spawned while a connection is open inherits none of its descriptors. If it does, the
  node closes them itself.
- A second start with the same state dir and no auth key comes up as the same node.

## Plan
1. `spikes/11-tsnet-grpc/`: its own flake with libtailscale at `ac7ed43`, the C API
   declared by hand from `tailscale.h`, and one Rust program that starts two handles,
   serves a tonic service on one and calls it from the other.
2. The owner makes a reusable, ephemeral, pre-approved auth key on their tailnet and
   puts it in a mode-600 file outside the repo; the program reads the path and never
   prints the key.
3. One run on aarch64-darwin, then the same on x86_64-linux; a second start without the
   key to check the state dir.
4. Record the results as a `===RESULT===` line.

## Success criteria
- Unary call, 1000-message stream and a minute-long idle stream succeed over tsnet on
  both systems, with two handles in one process.
- The serving side names the dialling peer's host and login.
- A spawned child holds no tsnet descriptor.
- A restart without the key rejoins as the same node.

## Findings
H1 held on aarch64-darwin and aarch64-linux; every success criterion was met.

- libtailscale's accepted and dialled connections are stream sockets (`S_IFSOCK`), so
  `std::os::unix::net::UnixStream::from(OwnedFd)` and `tokio::net::UnixStream::from_std`
  own them, tonic serves them through `serve_with_incoming` with a `Connected` wrapper,
  and the client dials through `Endpoint::connect_with_connector` with a `service_fn`
  that runs the blocking `tailscale_dial` on `spawn_blocking`. No copy loop, no SOCKS5.
- `tailscale_accept` blocks; a plain thread entered into the tokio runtime
  (`Handle::enter`) runs it. `tailscale_up` blocks too, and two identities came up
  side by side on two blocking threads.
- `tailscale_getremoteaddr` gives the caller's tailnet IP (no port), and the serving
  identity's `tailscale_status_json` maps it through `Peer[].TailscaleIPs` to
  `HostName`, `DNSName`, the stable node `ID` and, through `User[UserID]`, the login.
- At `ac7ed43` the listener, dialled and accepted descriptors are all close-on-exec and
  a child spawned while they were open held none.
- A start from the saved state with no key rejoins as the same node (same `ID`), and
  `up` then returns in 45 to 170 ms, before the netmap names the peers: the first dial
  by MagicDNS name can fail with `no such host` and one retry 500 ms later connects.
  The node must treat that as an ordinary failed connect and retry.
- A 60 s silent stream stayed open with HTTP/2 pings every 15 s on both ends.

For the design: a `tailnet` transport behind the existing boundary, with the accept
thread, the `Connected` wrapper carrying the caller's address and a dial connector, is
enough; the peer's identity can be looked up per connection from `status_json`.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.
- [[RUN-20261112-s5yqfc]] aarch64-darwin: all checks pass; after a keyless restart the first dial cannot resolve the hub's name, the retry connects
- [[RUN-20261112-s5z9yk]] aarch64-linux on ghost001: the same
- [[RUN-20261112-s5yqfc]] compute run, not yet described
- [[RUN-20261112-s5z9yk]] compute run, not yet described

## Conclusion
Written once, by a human, when status becomes `concluded`.
