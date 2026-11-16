# ADR-0056: The app carries a web view's event streams over a bridge

Status: accepted
Date: 2026-11-15
Amends: ADR-0053

## Context
ADR-0053 shows yajna's pages in a WebView over the service relay, without saying how the
WebView is fed. [[RUN-20261114-rwcse4]] found that `shouldInterceptRequest`, the way to
answer a WebView's requests from the app, hands an intercepted body to the page only in
large blocks: a server-sent events stream, which yajna's files viewer uses to follow open
files, never arrives. Loading from a port the core would serve on the phone's loopback
worked, but any app on the phone can reach such a port. The owner chose on 2026-11-15 a
JavaScript bridge instead. [[RUN-20261115-218yt4]] built one that needs no change in yajna.

## Decision
- **Every request goes through `shouldInterceptRequest`**, answered by the core over
  `ServiceRequest` (ADR-0052); no port is opened on the phone.
- **The app supplies the bridge.** Into every HTML page it serves, at the top of `<head>`,
  the app puts a script that replaces `window.EventSource` with one carried over a
  `WebMessagePort`: the page asks to open a stream, the app opens it through the relay and
  posts each event (its name and data) to the page as it arrives, and closes it when the
  page does. A stream that fails becomes an `error` event, as with `EventSource`.
- **What a page may rely on**: plain requests (range requests included) and `EventSource`
  with same-origin URLs. A page that opens a WebSocket or streams a `fetch` body would need
  the bridge to grow; yajna's pages do neither today.
- The WebView keeps DOM storage on: yajna's files viewer renders blank without it.

## Alternatives considered
- A port on the phone's loopback, guarded by a per-start secret: worked unchanged, but a
  listener every app on the phone can reach.
- A bridge inside yajna's viewer: the viewer would have to know it is embedded; the app's
  own `EventSource` replacement gives the same with nothing changed in yajna.
- Polling instead of events: slower and more traffic, and a change in yajna.

## Consequences
- yajna's pages work in the app as they are, as long as their live parts use
  `EventSource`; yajna changing that would break them in the app.
- The app rewrites the HTML it serves (one script inserted), so a page's integrity checks
  over its own HTML, if yajna ever adds any, must allow for it.
