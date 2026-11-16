---
id: EXP-20261114-raqjjy
type: experiment
schema_version: 1
title: Does a streamed HTTP request from the app reach a unix-socket service on a node through the hub and back, server-sent events and range reads included, inside an Android WebView
project: PRJ-0001
status: active            # planned | active | paused | concluded | abandoned
outcome: null             # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-14
author: agent:claude
protocols: []
materials: []
tags: [relay, yajna, app]
review: pending
---

## Question
ADR-0052 has the hub relay a client's HTTP request to a service on a node's unix socket,
over the node's existing link, and ADR-0053 shows `yajna serve` in an Android WebView
through that relay. Does a request make that trip whole, with the answer streamed, for the
kinds `yajna serve` answers: a page, a range of a large file, a long-lived server-sent
events stream? And can a WebView be fed from the relay at all, with streaming kept?

## Hypothesis and predictions
H1: a relay built like `ClaudeApi` (a request head, the body in pieces, the answer head,
then the body as it arrives) carries all three kinds; an event sent by the service reaches
the phone within 0.5 s over the hub, and a 1 MB range arrives within 1 s. If false, the
likely failure is buffering at one hop (an event arrives only with the next), which
changes the relay to flush per piece.

H2: `WebViewClient.shouldInterceptRequest` returning a `WebResourceResponse` over a piped
stream lets an `EventSource` in the page receive events as they come. If false (the
WebView reads the whole stream first), the core serves the relay on a loopback port of
the phone instead and the WebView loads from there.

## Plan
1. A throwaway relay in `spikes/14-service-relay/`: a client stub, a hub and a node stub
   with the proposed messages, the node connecting to a unix socket; the service is
   `yajna serve` on a socket (or a stub with the same three routes if `--socket` is not
   built yet).
2. From the laptop, through the hub on ghost001: time a page, a 1 MB range of a 1 GB file,
   and the gap between an event written and received, ten times each.
3. A minimal Android activity with a WebView fed by `shouldInterceptRequest` from the stub
   client, loading the files viewer; watch a file being appended to from the node.
4. Repeat 3 with the loopback-port alternative if H2 fails.

## Success criteria
- Each of the three kinds arrives whole and in order; the times above are met or the
  hop that misses them is named.
- A clear answer on the WebView: intercept with streaming, or the loopback port.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.
- [[RUN-20261114-rn027z]] the spike relay on one machine in front of a real `yajna serve`: pages, a 1 MB range and 256 MB pass byte for byte (1.3 GB/s), events arrive with 0.2 ms added, also during a 1 GB download; H1 holds locally
- [[RUN-20261114-rwcse4]] the WebView on the emulator: loaded from a loopback port, events arrive one by one in about 230 ms; through `shouldInterceptRequest` none of ten arrive, the body is handed over in large blocks; H2 false, the core serves a loopback port; the files viewer needs DOM storage
- [[RUN-20261115-218yt4]] the bridge the owner chose: a script the app puts at the top of each page replaces EventSource and the app carries the stream over a WebMessagePort; events arrive in about 240 ms, and yajna's files viewer, unchanged, follows an open file live, its range reads through the intercept
- [[RUN-20261115-2t8qqy]] the hub on ghost001 (86 ms away): all bytes whole, a small request costs three round trips (260 ms; two if `OpenService` carried the request's head), an event one (86 ms); throughput was this Mac's hostel link (0.9 MB/s)
- [[RUN-20261116-ca6zg1]] an 86 ms link simulated on one machine: with tonic's default windows (1 MB and 2 MB per stream, not HTTP/2's 64 KB) the relay carries 9.8 MB/s; the adaptive window is slower (7.4 MB/s, a 1 MB range twice as long) as it starts every stream at 64 KB; keep the defaults

## Conclusion
Written once, by a human, when status becomes `concluded`.
