# ADR-0049: The phone holds a tailnet identity per hub

Status: accepted
Date: 2026-11-13

## Context
ADR-0016 has the phone embed tsnet through libtailscale cross-built with the NDK, so the
app reaches a hub on a tailnet the Tailscale app is not on. ADR-0048 embedded it in the
hub and the node. The owner's choices of 2026-11-13: one identity per hub, the hub's auth
key kept on the phone encrypted so the app can log in again by itself, a hub without a
key still reached over the phone's own network, and persistent devices.
[[EXP-20261113-1h4ztf]] found that Go builds no c-archive for Android, that the app
sandbox's ban on binding netlink route sockets stops `tsnet.Up` (`netlinkrib: permission
denied`), and that tsnet then panics for want of a directory for its log state. With both
fixed, an emulator with no Tailscale app joined the test tailnet and reached a hub by its
MagicDNS name, and a cold start joined from saved state in 161 ms.

## Decision
- The flake builds `libtailscale-android` from the same source at `ac7ed43`:
  `GOOS=android GOARCH=arm64`, cgo through the NDK's `aarch64-linux-android26-clang`,
  `-buildmode=c-shared` with soname `libtailscale.so`, stripped. `build-core.sh` copies it
  into the app's `jniLibs` beside `libsaarthi_core.so`, which links it dynamically; the
  tailnet crate's `build.rs` reads `LIBTAILSCALE_ANDROID_LIB_DIR` for Android targets.
- One Go file of ours, `nix/libtailscale-android.go`, is added to that build only. It
  registers a netmon interface getter built on bionic's `getifaddrs`, reports the
  interface of the kernel's outbound route as the default route, and exports
  `tailscale_android_set_logs_dir`, which the tailnet crate calls with the directory
  above the identity's state before each start (`TS_LOGS_DIR`; Go copies the environment
  at load, so the host cannot set it).
- The core adds `Hub.connectViaTailnet(address, token, TailnetLogin{dir, hostname,
  authkey})` and `joinTailnet(login)`. It keeps one running identity per state directory
  for the life of the process, so reconnects do not join again, and retries a dial a few
  times while the netmap of a fresh start arrives.
- The app keeps, per hub, an optional tailnet auth key and device name
  (`saarthi-<phone model>` by default). The key is stored AES-GCM-encrypted under a key in
  the Android Keystore and handed to tsnet on every start; tsnet uses it only when the
  identity has to log in. A hub with a key is reached only through its identity, with
  state under the app's files in `tailnet/<hub id>`; removing the hub deletes that state.
  The boot log shows the join as its own step.
- `saarthi-tailnet`'s auth key is a file or a value. A file is now read on every start
  while it exists, not only the first: a hub or node whose identity the tailnet logged
  out could otherwise never log in again without its state being deleted.

## Alternatives considered
- One identity per tailnet, shared by the hubs on it: fewer devices, but Settings would
  have to model tailnets apart from hubs; the owner chose per hub.
- Using the key once and dropping it: no reusable key on the phone, but a logged-out
  identity needs the key pasted again; the owner chose to keep it, encrypted.
- `GOOS=linux` with a c-archive: Go's linux runtime on bionic is unsupported.
- Handing the interfaces in from Kotlin through `ConnectivityManager`, as Tailscale's own
  app does: more plumbing through the core for the same list getifaddrs gives.
- tailscale.com's `androidbin` fallback: registered only in builds that are not
  `android && cgo`, and it reports one synthetic interface.
- tailscale-rs: still no Android support.

## Consequences
- The APK carries libtailscale, about 22 MB uncompressed.
- On Android netmon polls for changes only every ten minutes and expects the app to
  report them. Nothing reports them yet: after a switch between Wi-Fi and mobile data the
  link recovers through the core's keepalive and tsnet's own reconnects, possibly slower
  than the Tailscale app would. A hook from a `ConnectivityManager` callback is the
  follow-up if the phone shows it is needed.
- The reusable key on the phone admits any machine holding it to the tailnet; revoking it
  in the admin console is the remedy, and the identity keeps working until logged out.
- Each hub with a key is a device on its tailnet; removing the hub in the app does not
  remove the device there.
