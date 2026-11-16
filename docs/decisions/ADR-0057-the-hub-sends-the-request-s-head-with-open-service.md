# ADR-0057: The hub sends the request's head with `OpenService`

Status: accepted
Date: 2026-11-16
Amends: ADR-0052

## Context
ADR-0052 has the hub ask a node to open a service with `OpenService{id, service}` on its
`Link`; the node dials `ServiceStream` back, receives the request down it, sends it to the
service and streams the answer up. Through a hub 86 ms away, a small request took three
round trips, 260 ms ([[RUN-20261115-2t8qqy]]): the request reached the node only after its
stream back was open.

## Decision
- `OpenService` carries the request's head (service, method, path, headers) and whether a
  body follows. A node starts on a request without a body at once, while it opens
  `ServiceStream`; the answer goes up that stream. A request with a body waits for the body
  pieces on the stream, as before.
- Two round trips for a request without a body, the common case (pages, listings, ranges,
  event streams).

## Alternatives considered
- Sending the request's body with `OpenService` too: the `Link` would carry bulk data and
  hold up session events behind it, which a stream per request avoids.
- Requests multiplexed on the `Link` itself: one round trip less again, the same objection.

## Consequences
- A request without a body reaches the service before the hub has seen the node's stream:
  if the stream never arrives, the service has done the work for nothing.
