# The terminal

The app opens a shell on any of your machines whose node offers one: your login shell, in
your home directory, through the hub. Nothing is opened on the machine or the phone, and
no SSH key goes onto the phone.

## What a machine needs

- The node started with `--terminal`, or `terminal = true;` in the home-manager module.
  It is off by default: anyone holding one of your client tokens gets a shell as you.
- A hub and node at 0.17.0 or later.

## In the app

A terminal icon appears beside each connected machine that offers one, in the machines
list on the sessions screen. Tapping it opens the shell at the size of the screen; turning
the phone or opening the keyboard resizes it, and full-screen programs such as `top` or an
editor redraw to fit.

Type with the phone's keyboard. The row above it has the keys a phone keyboard lacks: Esc,
Tab, Ctrl, the arrows and ^C. Ctrl is sticky: tap it, then a letter, for that control key
(Ctrl, then `d`, sends end-of-file). Tap the screen to bring the keyboard back.

Leaving the screen ends the shell. When the shell exits, or the node or hub goes away, a
banner says why, with *Open again* to start a new one.

## What it does not do yet

- A shell does not survive leaving the screen or losing the connection; run `tmux` or
  `screen` in it for that.
- There is no scrollback on the phone and no text selection.
- The hub sees every key and every byte of output, as it sees your sessions.
