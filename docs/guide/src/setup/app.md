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

The first time there is no hub yet, so the app goes on to *Settings*, with three ways in.

**Pair with a QR code.** On a machine with saarthi and `ctl` set up for the hub, run

```console
$ saarthi ctl pair --phone-token-file ~/.config/saarthi/phone.token \
    --address http://hub:7700 --name home --tailnet-key-file ~/tailnet-phone.key
```

with the token file holding one of the hub's client tokens for the phone (not `ctl`'s
own, so each keeps its name), the address as the phone reaches the hub (`ctl`'s own hub
by default), and the tailnet key only if the app is to join the hub's tailnet itself.
It checks the token against the hub first, then prints a QR code. Point the phone's
camera at it and open the link in saarthi: the hub appears in *Add a hub*, filled in, for
you to look at and save. The code holds the token: show it to your phone and no one
else. Without a camera at hand, `--link` prints the link as text too; copy it to the
phone and tap **Paste a pairing link** in Settings.

**Try the demo.** A recorded session, to look around the app before there is a hub: a
small fix to a failing test, with its tool calls, diff and reply. Nothing in the demo
reaches a hub; adding a hub leaves it.

**Add a hub** by hand:

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
  or the same hub at another address; *Paste a pairing link* adds one from
  `saarthi ctl pair --link`.
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

If joining fails, the boot log says why within a few seconds, or after 45 s at the most:
the key was not accepted (revoked, expired or used up), the device was removed from the
tailnet or expired, or the tailnet's admin has to approve it. Once the phone has joined, it
uses its saved identity and never looks at the key again, so a new key alone would not help:
saving the hub with a different key gives the phone a new identity, which joins with it;
*Forget this phone's tailnet identity* in the hub's settings does the same with the key it
has. The old device stays in the tailnet until you remove it there, and the new one may be
named with `-1` until then.

## Notifications

After connecting, the app asks for permission to show notifications and registers for
push. For push to work you also need the **ntfy** app on the phone as the UnifiedPush
distributor; see [Notifications](notifications.md). Without it everything else works,
you just have to open the app to see what is waiting.
