# ADR-0048: The hub and the node embed tsnet beside plain TCP

Status: accepted
Date: 2026-11-12

## Context
ADR-0004 kept phase 1 on the host's tailscaled and one tailnet, with a listener and dialer
boundary so embedded tsnet could replace it; ADR-0015 settled how libtailscale is built
and linked. The owner wants the hub on several tailnets and each node on the hub's
tailnet whatever its host is joined to. [[EXP-20261112-rq8kgc]] ran tonic over
libtailscale on aarch64-darwin and aarch64-linux: accepted and dialled connections are
stream sockets tokio can own, one process holds several identities, `status_json` names
the peer behind an address, the descriptors are close-on-exec at libtailscale `ac7ed43`,
and a start from saved state returns before the peers' names resolve, so the first dial
may fail and a retry succeeds. The owner's choices of 2026-11-12: a first join by an
auth key file, the plain TCP path kept, several tailnets on the hub from the start, and
the peer's tailnet identity recorded while tokens stay the authorisation.

## Decision
- A workspace crate `saarthi-tailnet` declares libtailscale's C API by hand and wraps a
  tsnet identity: start (blocking `tailscale_up` on a blocking thread), an incoming
  stream of connections for tonic's `serve_with_incoming`, a connector for tonic's
  `connect_with_connector`, and `whois` from `tailscale_status_json`. Its `build.rs`
  links `libtailscale.a` from `LIBTAILSCALE_LIB_DIR`; the flake builds libtailscale at
  `ac7ed43` and sets the variable for the packages, the checks and the dev shell.
- The hub keeps `--listen` (plain TCP, for the phone on the Tailscale app, `ctl` and
  machines on the host's tailnet) and adds `--tailnet <name>[=<auth key file>]`,
  repeatable: one identity per tailnet, all serving the same services on
  `--tailnet-port` (7700), named `--tailnet-hostname` (`saarthi-hub`) on each, with state
  under `--tailnet-state/<name>` (beside the database by default). The key file is read
  only when that state is new.
- The node adds `--tailnet`, with `--tailnet-authkey-file`, `--tailnet-hostname`
  (`saarthi-<node name>`) and `--tailnet-state` (`~/.local/state/saarthi/tailnet`). With it
  every connection to the hub, the link and the token request a wrapped session makes,
  is dialled through that identity; `--hub` names the hub as on its tailnet
  (`http://saarthi-hub:7700`). A dial that cannot resolve the name yet fails like any
  connect and the link's backoff retries it.
- On each node registration over a tailnet, the hub looks up the caller and records the
  tailnet's name and the peer's host and login on the node; `ListNodes` returns them
  and `saarthi ctl nodes` shows them. They are not checked against anything.

## Alternatives considered
- tsnet only, no TCP listener: the phone cannot reach the hub until it embeds tsnet
  itself (ADR-0016), and every test would need a tailnet.
- A separate bridge process forwarding a local port onto the tailnet: one more process to
  run and supervise per machine, and the hub would see every caller as localhost.
- Logging in through a URL instead of a key: a fresh machine would need a person.
- Enforcing the recorded identity: a token moved to a new machine would stop working
  until someone edits a record; the owner chose recording only.

## Consequences
- The hub and node binaries grow by about 25 MB and their builds need Go through Nix;
  `cargo build` outside the dev shell fails with a message naming the variable.
- Clients (`saarthi ctl`, `saarthi tui`, the phone) still use TCP over the host's tailnet;
  their own identities come with the phone's tsnet (ADR-0016).
- A tailnet's state directory holds that identity's node key; deleting it costs a new key.
- Tests cannot reach a control server, so the tailnet path has a live test that runs only
  when `SAARTHI_TAILNET_AUTHKEY_FILE` is set.
