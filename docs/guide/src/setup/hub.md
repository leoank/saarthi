# The hub

The hub should run on a machine that is always on and reachable from your nodes and your
phone over Tailscale: a small cloud VM, a home server, a Raspberry Pi. It needs little:
one process, a SQLite file, a few tens of MB of memory.

## What it needs

- `saarthi-hub` from this repository's flake (`packages.<system>.saarthi-hub`). It builds on
  Linux and macOS; the NixOS module below is for Linux.
- A directory on persistent storage for `hub.db`, created on first start.
- Two token files, kept secret:
  - **node tokens**, one line per machine: `<node name> <token>`;
  - **client tokens**, one line per client (the phone, `saarthi ctl`): `<client name> <token>`.

  The hub knows a peer by its token, not its name; the name is what logs and the app show.
  Use long random tokens (`openssl rand -hex 24`), and give every node and client its
  own, so one can be revoked by deleting its line and restarting the hub.
- Optionally, a file of **Claude tokens**, kept secret: `<name> <token>` lines of tokens
  from `claude setup-token`, one per Claude account sessions should be able to run under
  (see [Claude accounts](#claude-accounts)).
- Optionally, an [ntfy](https://ntfy.sh) topic URL for notifications. The phone's push
  works without it; see [Notifications](notifications.md).

## Running it

```console
$ saarthi-hub --listen 100.101.102.103:7700 --db /var/lib/saarthi-hub/hub.db \
    --node-tokens /run/secrets/saarthi-node-tokens \
    --client-tokens /run/secrets/saarthi-client-tokens \
    --claude-tokens /run/secrets/saarthi-claude-tokens \
    --ntfy https://ntfy.example.org/saarthi
```

| Flag | What it does |
|---|---|
| `--listen` | Where nodes and clients connect. Default `127.0.0.1:7700`, reachable only from the same machine. Use the machine's tailnet address, or `0.0.0.0:7700` with the port closed in every firewall except to the tailnet. |
| `--db` | The SQLite database. Back it up like any other SQLite file; it is the hub's whole record. |
| `--node-tokens`, `--client-tokens` | The token files. Read at start; restart the hub after editing them. |
| `--claude-tokens` | The Claude tokens file. Read at start, like the others. Without it, or when it does not parse (the log says why), sessions run under each node's own login. |
| `--usage-probe-node` | A node that runs a throwaway Haiku turn under each Claude token about every `--usage-probe-minutes` (default 30), so every account's usage stays fresh. It must allow headless sessions. |
| `--ntfy` | An ntfy topic URL to publish notifications to, besides the phones' own push. |
| `--no-notify <kind>` | Turn one kind of notification off: `permission`, `question`, `turn`, `ended`, `usage` or `token` (a Claude token about to expire). Repeat for several. |

The hub speaks plain gRPC with no TLS. Tailscale encrypts the traffic between machines;
do not expose the port to the internet.

## Claude accounts

Sessions started from the app or `saarthi ctl` can run under a Claude account other than
the node's own login: under a named token the hub holds (ADR-0039). Make one per account
with `claude setup-token`, in a terminal logged in to that account; it lasts a year and
never needs refreshing, so any number of sessions on any number of nodes can share it.
Put them in the file, one per line:

```text
# <name> <token>
work      sk-ant-oat01-...
personal  sk-ant-oat01-...
```

and give it to the hub with `--claude-tokens`. The names are what the app and `ctl` show;
the tokens never leave the hub except to a node starting a session, which keeps it in
memory. Adding, replacing or removing one is an edit to the file and a restart. Then:

- each node gets a **default account**, set from its health screen in the app or with
  `saarthi ctl default-token <node> <name>`; sessions started on it run under that token
  unless another account, or the node's own login, is picked on *New session*;
- the hub dates each token from when it first read it and **reminds you a month before**
  it expires, a year later; set another date from *Claude accounts* in the app's menu or
  with `saarthi ctl token-expiry`. A new token under the same name starts a new year;
- each account's **5-hour and 7-day usage** shows under *Claude accounts*, from the last
  session that ran under its token, and the hub sends the
  [rate-limit alerts](notifications.md#rate-limit-alerts) per account. Anthropic's usage
  endpoint does not accept these tokens, so the figures come from the sessions themselves;
  with `--usage-probe-node <node>` (`usageProbeNode` in the NixOS module), that node runs
  one throwaway Haiku turn under each token about every half hour, at a slightly random
  time, skipping an account a session reported on lately. A probe costs a small turn
  (about $0.0015 at list price); it is never listed and leaves nothing behind. Without
  probes, an account no session has used lately shows older figures, with their age.

Interactive sessions you start in a terminal are never touched: they use whatever login
that machine has.

## On NixOS

The flake exports a NixOS module, with the token files from your secrets manager
(agenix, sops-nix):

```nix
{ inputs, ... }:
{
  imports = [ inputs.saarthi.nixosModules.saarthi-hub ];
  services.saarthi-hub = {
    enable = true;
    listen = "0.0.0.0:7700";
    nodeTokensFile = "/run/agenix/saarthiNodeTokens";
    clientTokensFile = "/run/agenix/saarthiClientTokens";
    claudeTokensFile = "/run/agenix/saarthiClaudeTokens";   # optional
    usageProbeNode = "server";                               # optional, a node on the hub's host
    ntfy = "http://127.0.0.1:2586/saarthi";                  # optional
  };
}
```

with `inputs.saarthi.url = "github:leoank/saarthi";` in your flake. It creates the
`saarthi-hub` user, keeps the database in `/var/lib/saarthi-hub` (`dataDir`), and runs a
hardened systemd service. The port stays closed in the firewall unless you set
`openFirewall`. Every option is in
[`nix/hub-module.nix`](https://github.com/leoank/saarthi/blob/main/nix/hub-module.nix).

## Checking it

`journalctl -u saarthi-hub` shows `hub serving` at start and one `node registered` line per
node that connects. From any machine with a client token (or with
[`ctl.toml`](../using/ctl.md) set up, just `saarthi ctl nodes`):

```console
$ saarthi ctl --hub http://hub:7700 --token-file ~/.config/saarthi/client-token nodes
```

lists every node the hub knows and whether it is connected.

## Upgrading

Stop the hub, replace the binary, start it. The database is migrated forward
automatically at start. Nodes reconnect on their own within seconds and re-send anything
the hub had not acknowledged; the app reconnects by itself.
