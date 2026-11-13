---
id: EXP-20261113-1h4ztf
type: experiment
schema_version: 1
title: Can libtailscale be cross-built for arm64 Android under Nix, linked into the uniffi core, and carry tonic gRPC to the hub from inside the app without the Tailscale app?
project: PRJ-0001
status: active            # planned | active | paused | concluded | abandoned
outcome: null             # null | positive | negative | inconclusive | infra
influenced_by: [EXP-20261112-rq8kgc]
created: 2026-11-13
author: agent:claude
protocols: []
materials: []
tags: [tailnet, phase-3, android]
review: pending
---

## Question
ADR-0016 has the phone embed tsnet so the app reaches a hub on a tailnet the Tailscale
app is not on. The owner chose on 2026-11-13 ([[AGS-20261113-1h4tz8]]): one tailnet
identity per hub, its auth key kept on the phone under the Android Keystore, a hub
without one still reached over the phone's own network, and persistent (not ephemeral)
devices. [[EXP-20261112-rq8kgc]] showed tonic over libtailscale on darwin and linux; no
one has built libtailscale for Android, linked it into the uniffi core, or run tsnet
inside an Android app's sandbox.

## Hypothesis and predictions
H1: libtailscale at `ac7ed43` cross-builds for `android/arm64` as a cgo c-archive with
the NDK's clang under Nix, links into `libsaarthi_core.so`, and inside the app joins the
tailnet and carries the core's tonic calls to the hub.

- If the build holds, `nix build .#libtailscale-android` gives an archive and
  `cargo ndk` links the core against it. If not (cgo, the NDK's sysroot, Go's Android
  runtime), the fallback is tailscale-rs (ADR-0016), a different project.
- Since Android 11 an app may not read the interface list over netlink. If tsnet's
  network monitor needs it and has no fallback, `up` fails or never finds a route, and
  libtailscale needs a hook for the app to hand it the interfaces (as Tailscale's own
  app does through `netmon.RegisterInterfaceGetter`). If it works without one, no patch.
- A start from the saved state with no key rejoins as the same device, as on darwin.

## Plan
1. A `libtailscale-android` package in the flake: the same source, `GOOS=android
   GOARCH=arm64 CGO_ENABLED=1` with the NDK's `aarch64-linux-android26-clang`.
2. The core takes an optional tailnet per connect through `saarthi-tailnet`; the
   android shell points the crate at the Android archive.
3. In the emulator and on the owner's phone: a hub on the Mac serving on the test
   tailnet, the app given its tailnet name and the key, with no Tailscale app in the
   way. Then a cold start without the key.
4. Record what the tsnet log says about the interfaces.

## Success criteria
- The archive builds under Nix and the core links it for arm64 Android.
- In the app, with no Tailscale app running, the boot log reaches the hub over tsnet
  and lists its nodes.
- A restart joins from the saved state with no key.

## Findings
H1 held once two things were added to the build; every success criterion was met on the
emulator ([[RUN-20261113-1v2rh6]]).

- Go builds no c-archive for Android; a c-shared `libtailscale.so` beside the core works.
  Under Nix the build phase has to set `CC` to the NDK's clang itself and drop the macOS
  SDK variables the Darwin stdenv sets.
- The sandbox denies netlink, and tsnet stops at `Up` without a netmon interface getter;
  bionic's `getifaddrs` gives one with no Kotlin involved.
- tsnet then panics for want of a log-state directory unless `TS_LOGS_DIR` is set from
  inside Go, since Go copies the environment when the library loads.
- A cold start joins from saved state in about 160 ms and connects within a second.
- On Android netmon only polls every ten minutes and expects the app to report network
  changes; nothing does yet. How the link behaves across a switch between Wi-Fi and
  mobile data was open; on the owner's phone at 0.5.0 the app connected through its own
  identity and held across such a switch, so no hook is needed for now.
- Unplanned: a hub whose identity the tailnet had logged out never came back, because its
  key file was read only on the first join (ADR-0048); now it is read on every start.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

- [[RUN-20261113-1v2rh6]] the Android build, the two fixes it needed, and the app reaching a hub through its own identity on the emulator; positive
- [[RUN-20261113-1v2rh6]] compute run, not yet described

## Conclusion
Written once, by a human, when status becomes `concluded`.
