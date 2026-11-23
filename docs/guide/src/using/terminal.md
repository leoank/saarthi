# The terminal

The app opens a shell on any of your machines whose node offers one: your login shell, in
your home directory, through the hub. Nothing is opened on the machine or the phone, and
no SSH key goes onto the phone. The shell stays on the machine until it exits or you end
it: the phone sleeping, the connection dropping, or the node restarting does not end it,
and you can come back to it from any phone with the app.

## What a machine needs

- The node started with `--terminal`, or `terminal = true;` in the home-manager module.
  It is off by default: anyone holding one of your client tokens gets a shell as you.
- A hub and node at 0.22.0 or later for shells that persist (0.17.0 for a terminal at
  all). With the home-manager module the shell holder runs as a service of its own,
  `saarthi-shells`; a node run by hand starts one itself.

## In the app

*machines* on the bar at the bottom lists your machines. Each one that offers a shell has
*new shell*, and under it the shells it keeps: what each last set as its title, when it
started, and how many phones are on it. Tapping a shell opens it with its screen as it is;
*new shell* starts one at the size of the screen. Turning
the phone or opening the keyboard resizes it, and full-screen programs such as `top` or an
editor redraw to fit.

Type with the phone's keyboard. The key bar above it has the keys a phone keyboard lacks:
at first Esc, Tab, Ctrl, the arrows and ^C. Ctrl and Alt are held for the next key: tap
Ctrl, then a letter, for that control key (Ctrl, then `d`, sends end-of-file), or Alt, then
a key, for that key with Alt (Alt, then `b`, goes back a word in most shells). Tap the
screen to bring the keyboard back.

The pencil at the end of the bar edits it: move a key along the bar or remove it, add keys
it does not have yet (Shift-Tab, Alt, ^D, ^Z, ^L, ^R, Enter, Home, End, Page up and down,
Delete, and `|`, `~`, `/`, `-`), or go back to the default keys. The bar is kept on the
phone, the same for every machine.

When the connection drops, a banner says *Reconnecting* and the app attaches again by
itself, at once when the hub is back and otherwise every few seconds, up to half a
minute apart; what you type meanwhile is sent once it is back. The screen comes back as
the shell left it, full-screen programs too, with 10,000 lines of scrollback kept on the
machine. Two phones can be on one shell at once and see the same.

The ✕ in the shell's top bar leaves it running. To end a shell, use *End this shell* in its
menu, or the ✕ beside it on *machines*; it is killed with whatever runs in it, for every
phone on it. When a shell exits, a banner says why, with *Open again* to start a new one.

## What it does not do yet

- No text selection on the phone.
- A machine's reboot ends its shells, as does stopping the shell holder.
- With two phones on one shell, both answer a program's terminal queries.
- The hub sees every key and every byte of output, as it sees your sessions.
