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

The phone has to reach the hub, so install the [Tailscale](https://tailscale.com/download)
app and join the hub's tailnet. Then open saarthi.

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

*Save and connect* connects and opens the sessions. Everything stays on the phone.

## Settings

*Settings*, in the home screen's menu, is where the app's settings live:

- **Hubs**: every hub you have added, the one the app connects to selected, with how its
  connection stands. Tap another to switch to it; the pencil edits one, and *Remove this
  hub* forgets its address and token. *Add a hub* adds another, for example a second hub
  or the same hub at another address.
- **Display**: whether code, diffs and tool output wrap instead of scrolling sideways.
- **About**: the app's and the core's versions and the phone.

The app reaches every hub through the phone's own network for now. A tailnet identity of
the app's own, per hub with its own auth key, comes with the phone's embedded tsnet.

## Notifications

After connecting, the app asks for permission to show notifications and registers for
push. For push to work you also need the **ntfy** app on the phone as the UnifiedPush
distributor; see [Notifications](notifications.md). Without it everything else works,
you just have to open the app to see what is waiting.
