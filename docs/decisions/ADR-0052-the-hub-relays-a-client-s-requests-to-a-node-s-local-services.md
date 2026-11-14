# ADR-0052: The hub relays a client's requests to a node's local services

Status: accepted
Date: 2026-11-14

## Context
The owner wants yajna, their lab notebook, on the phone beside the sessions: a machine's
notebooks, their files and reviews, new projects, run sheets at the bench
([[AGS-20261114-rapbh2]], `docs/notebooks.md`). yajna already has an HTTP server,
`yajna serve` (yajna's ADR-0027), which binds loopback and has no authentication of its
own; its roadmap planned a central server reached over Tailscale with tokens of its own.

saarthi already has what that would need: a daemon on every machine with one
authenticated outbound link to the hub, the hub reachable over its own tailnet, the phone
with an identity on it (ADR-0048, ADR-0049), and since ADR-0051 a relay of HTTP requests
over the node's connection. The owner chose on 2026-11-14 to keep yajna independent of
saarthi, with its API on each machine, and have saarthi carry it.

## Decision
- **Services in the node's config.** A node lists local HTTP services by name, each a unix
  socket path, and optionally a command to start it: `[services.yajna] socket = …,
  start = ["yajna", "serve", "--socket", …], idle = "10m"`. With a command, the node starts
  the service on the first request when nothing answers on the socket and stops it after
  `idle` without requests; a service already running (a user service) is just used. The
  node reports the names in its hello; nothing else about a service is saarthi's.
- **A control call**, `ServiceRequest`, a bidirectional stream from a client: the node, the
  service, the method, the path and the headers, then the body in pieces of at most 1 MB;
  back come the answer's status and headers, then its body as it arrives, as long as the
  service keeps it open (server-sent events).
- **The node dials back.** The hub sends `OpenService{id, service}` on the node's `Link`;
  the node connects to the socket and opens a new node-service stream, `ServiceStream`,
  whose first message is the id; the hub joins the two streams and pipes both ways. Each
  request gets its own HTTP/2 stream on the existing connection, so a large file does not
  hold up the link's events, and the node needs no listener.
- **What passes.** The client's token authorises any service on any node, as it does a
  spawn. The node refuses a service it does not list. Hop-by-hop headers are dropped and
  `Host` is set to the service's name; anything else (paths, methods, the service's own
  authorisation) is the service's business. The node's sandbox profiles deny the
  services' sockets as they deny the Claude API sockets (ADR-0051).
- **Done when** [[EXP-20261114-raqjjy]] shows a page, a range of a large file and an events
  stream arriving whole from the phone through the hub, and how a WebView is fed.

## Alternatives considered
- yajna's own server reached over the tailnet (its architecture, section 8): a second
  daemon with its own listener, tailnet identity and tokens on every machine, for what the
  node's link already gives.
- yajna linked into the node as a library: one process, but the node would ship and
  version yajna, and yajna would need saarthi to be useful anywhere but the laptop.
- Requests multiplexed on the `Link` stream itself: no new call, but a large answer would
  hold up the session events behind it.
- A loopback TCP port as well as unix sockets: other users of a shared machine could reach
  a loopback port; a socket's permissions are the authorisation here.

## Consequences
- Any local HTTP service can be put in front of the phone by one config entry, yajna the
  first; each must not need more protection than "the owner's clients may reach it".
- Every request crosses the hub, which sees it in plain text, as the Claude API's do.
- A node and a hub must be upgraded together to use it; an older node lists no services.
