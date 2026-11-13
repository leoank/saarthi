# The Android app

The app runs on Android 8 (API 26) and newer, on arm64 phones. There is no store listing
or prebuilt APK yet; you build it from this repository and install it with `adb`.

## Build it

Everything the build needs (the Android SDK and NDK, Gradle, a JDK and a Rust toolchain
for Android) comes from the flake's `android` shell. It is several GB the first time.

```console
$ git clone https://github.com/leoank/saarthi && cd saarthi
$ nix develop .#android
$ cd android && gradle assembleDebug
```

The build compiles the Rust client core for arm64 Android first, then the app. The APK
lands at `android/app/build/outputs/apk/debug/app-debug.apk`.

## Install it

Enable USB debugging on the phone (Settings, About phone, tap *Build number* seven times,
then Developer options, *USB debugging*), connect it, and from the same shell:

```console
$ adb install -r app/build/outputs/apk/debug/app-debug.apk
```

`-r` keeps the app's data, so upgrading the same way keeps your hubs.

## Connect

The phone has to reach the hub. Either install the
[Tailscale](https://tailscale.com/download) app and join the hub's tailnet, or give the
hub a tailnet auth key in saarthi and let the app join that tailnet itself (see
[On the hub's tailnet](#on-the-hubs-tailnet)). Then open saarthi.

Every start shows a boot log on black while the app connects: the app, the phone and the
core, the settings, the network, the push distributor, the hub and each of its nodes, one
line per step as it happens. When the hub answers, the app goes on to its sessions. When
it does not, the log ends in a `[FAILED]` line with the reason, and *Retry* or *Settings*.

The first time there is no hub yet, so the app goes on to *Settings* with **Add a hub**
open:

- **Address**: `http://<hub's tailnet name or address>:7700`, for example
  `http://hub:7700`; `http://` is added if you leave it out.
- **Client token**: the token from one of the hub's client-token lines (just the token,
  not the name).
- **Name**, optional: what the app calls the hub; its host name without one.
- **Auth key**, optional: a tailnet auth key, for the app to reach the hub through a
  tailnet device of its own.

*Save and connect* connects and opens the sessions. Everything stays on the phone.

## Settings

*Settings*, in the home screen's menu, is where the app's settings live:

- **Hubs**: every hub you have added, the one the app connects to selected, with how its
  connection stands. Tap another to switch to it; the pencil edits one, and *Remove this
  hub* forgets its address and token. *Add a hub* adds another, for example a second hub
  or the same hub at another address.
- **Display**: whether code, diffs and tool output wrap instead of scrolling sideways.
- **About**: the app's and the core's versions and the phone.

## On the hub's tailnet

The Tailscale app holds one tailnet at a time. To reach a hub on another one, or without
the Tailscale app at all, give the hub an auth key: the app then joins the hub's tailnet
itself, as a device of its own for that hub (ADR-0049).

1. In the hub's tailnet admin console, *Settings → Keys*, make a **pre-approved** auth
   key, not ephemeral. Reusable or not: the app keeps it to log in again if the tailnet
   ever logs the device out, so a one-off key then needs replacing.
2. In saarthi, edit the hub (or add it) and paste the key under **Auth key**. The address
   is the hub as its tailnet names it, for example `http://saarthi-hub:7700` for a hub
   serving with `--tailnet` ([On several tailnets](hub.md#on-several-tailnets)).
3. **Device name**, optional: the phone's machine name there, `saarthi-<phone model>`
   by default.

The boot log then shows `Joined tailnet as <device name>` before connecting. The key is
stored on the phone encrypted with a key held by the Android Keystore. Removing the hub
forgets the key and the identity; the device stays in the tailnet until you remove it in
the admin console. A hub without a key is reached through the phone's own network as
before.

## Notifications

After connecting, the app asks for permission to show notifications and registers for
push. For push to work you also need the **ntfy** app on the phone as the UnifiedPush
distributor; see [Notifications](notifications.md). Without it everything else works,
you just have to open the app to see what is waiting.
