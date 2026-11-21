# ADR-0075: Notifications held from the hub

Status: accepted
Date: 2026-11-21
Amends: ADR-0026

## Context
Notifications reach the phone through a push server: the hub posts each to `ntfy.sh` or the
owner's ntfy, and the ntfy app on the phone holds the connection to it (ADR-0008,
ADR-0025, ADR-0026). That needs a second app, and the notifications cross the public
internet outside the hub's tailnet. Phase [[PHS-20261117-f22kxe]] item 20 asked for push
without either. On 2026-11-21 the owner chose between the app holding a stream to the hub
over its tailnet and the app embedding an ntfy client for the same topic, and accepted a
foreground service's permanent, silent notification.

## Decision
- **The hub serves `Notifications`**, a stream of the same JSON a UnifiedPush endpoint gets,
  with an empty message every 45 s to keep it alive. While a client holds one, the hub
  posts nothing to that client's UnifiedPush endpoints; it lets go as soon as the client
  goes away, so the next notification goes to the endpoints again.
- **The app holds it in a foreground service** (`remoteMessaging`), through the same
  connection as the app (its own tailnet identity where the hub has an auth key), and shows
  each notification as before (Allow and Deny on a permission request, a tap opening the
  session), nothing while the app is in front. It opens the stream again after a drop,
  waiting 2 s up to a minute, at once when a network comes up, and when two beats are
  missed; it starts again at boot.
- **Its own notification** is silent, of the lowest importance: *Holding the link to
  <hub>*, or *Reconnecting*.
- **Settings**: *Hold a link to the hub*, on by default; off, UnifiedPush alone, as before.
  Where Android restricts the app's battery use, a row offers the exemption, as the ntfy
  app asks for it.
- UnifiedPush stays registered as the fallback for when the stream is down.

## Alternatives considered
- An embedded ntfy client subscribed to the same topic: replaces only the ntfy app; the
  notifications still go through a push server.
- Holding the link only for a while after the app was last used: fewer hours connected, but
  nothing arrives later in the day.

## Consequences
- A permanent notification while the link is held, which the owner can collapse in the
  phone's settings.
- Battery and delays are measured by [[EXP-20261121-3k3cxv]]; on the emulator a foreground
  service kept its network in forced deep idle without the exemption.
- A client with two phones on one token: one holding the stream keeps the hub from posting
  to the other's endpoint.
