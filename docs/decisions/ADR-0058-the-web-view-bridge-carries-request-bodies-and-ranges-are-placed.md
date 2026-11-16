# ADR-0058: The web view bridge carries request bodies, and range answers are placed at their offset

Status: accepted
Date: 2026-11-16
Amends: ADR-0056

## Context
Building the app's notebook section ([[AGS-20261116-fmnt2e]]) on ADR-0056 found two
things the spike had not:

- `shouldInterceptRequest` never sees a request's body. yajna's review writes are `fetch`
  POSTs with a JSON body, so through interception alone they would reach yajna empty.
- The web view applies a request's `Range` itself to what interception returns, taking
  that for the whole resource: it skips to the range's start and checks the range against
  `available()` as the resource's length. yajna's answer to a range is already the part
  (206), so the part was cut a second time, and the text viewer, which follows a growing
  file with ranges, showed each new line without its start. It also writes
  `available()` as a `Content-Length` next to any the answer carries.

## Decision
- **The bridge also carries a same-origin `fetch` with a string body.** The script ADR-0056
  puts at the top of each page wraps `fetch`: such a request goes over the message port,
  the app sends it through the relay with its method, headers and body, reads the answer
  whole and hands it back as a `Response`; a failure rejects with a `TypeError`, as a
  network failure does. Any other `fetch` (no body, a body that is not a string, another
  origin) is left to interception.
- **A range answer is placed at its offset.** The stream given to the web view for a 206
  starts with as many virtual bytes as the range's start, which it skips without anything
  being sent, and `available()` reports the length up to the range's end. Only the part
  crosses the relay. A plain answer reports its `Content-Length` through `available()`
  and the header is dropped, so the page sees one length.
- **The bridge's `EventSource` reconnects** three seconds after a stream that ends or
  breaks, as a browser's does; an answer that is not an event stream fails it for good.
- Each node's pages get an origin of their own, `https://<node id>.yajna.saarthi`, so one
  machine's pages never share storage with another's. A redirect for a page becomes a page
  that goes there; a redirect for anything else is followed by the app. Links off the
  origin open in the browser.

## Alternatives considered
- Sending a range request without its `Range` and letting the web view cut the whole
  file: correct with no tricks, but the text viewer follows logs by range so that only the
  new bytes travel; a long log would cross the relay in full at every append.
- A review API that takes its input in the query string: a change in yajna for one client,
  and URLs have limits that comments do not.
- Answering POSTs through a JavaScript interface (`addJavascriptInterface`) rather than the
  port: a second channel into the app, reachable from any frame the page loads.

## Consequences
- yajna's pages work in the app as they are, review writes included, as long as they send
  bodies as strings; a `FormData` or binary body would need the bridge to grow again.
- The placement depends on how Android's web view treats intercepted ranges; a web view
  that stopped applying the range itself would show the parts at the wrong place, so the
  notebook section's checks follow a growing file.
