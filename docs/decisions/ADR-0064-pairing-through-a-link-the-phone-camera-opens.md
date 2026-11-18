# ADR-0064: Pairing through a link the phone's camera opens, and a recorded demo

Status: accepted
Date: 2026-11-18

## Context
Adding a hub on the phone meant typing an address, a client token and perhaps a tailnet
auth key into a form: long secrets on a phone keyboard. A first start with no hub showed
only that form. The phase *The app at a glance* ([[PHS-20261117-f22kxe]], item 13), after
ovis, asks for the hub handed over as a QR code, and a demo for a first look.

## Decision
- **`saarthi ctl pair` prints a QR code of a link**, `saarthi://pair?v=1&address=…&token=…`
  with `name` and `tailnet_key` when given, each value percent-encoded. The token is the
  phone's own, read from a file (`--phone-token-file`), never `ctl`'s, so each client keeps
  its name at the hub. `pair` first lists the hub's nodes with that token and stops if the
  hub refuses it, and warns when the address is this machine's own.
- **The app opens the link** (an intent filter on `saarthi://pair`): the phone's camera
  reads the code and offers the app, which shows the hub in its *Add a hub* form, filled in,
  to be saved by the person. The same link can be pasted in Settings.
- **The demo is a recording bundled with the app** (`assets/demo.json`): a real spawned
  session's transcript lines and usage reports, stripped of everything else and of the
  machine it ran on, read by the core's own conversation code (`demo()`), moved in time to
  have ended a few minutes ago. The app shows it as a hub with one node and one session;
  commands and prompts say it is the demo. Adding a hub leaves it.

## Alternatives considered
- A QR scanner in the app (ML Kit or ZXing): a camera permission and a library for what the
  phone's camera app already does with a link.
- `ctl`'s own token in the code: one fewer file, but the phone and `ctl` would share a name
  at the hub, and with it push registrations and anything else keyed by client.
- A demo built by hand, or a fake hub in the core: a recording through the real code shows
  what the app really shows, and stays right as the app changes.

## Consequences
- The pairing code is a secret on screen for as long as it is shown.
- The demo shows one session; it has no held request or live turn to try.
