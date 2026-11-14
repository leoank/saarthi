"""Drives an interactive `claude` in a PTY for EXP-20261114-d82rcz: waits for the prompt,
sends each message (or a raw key, KEY:<escaped>), waits for its marker on the screen, then exits. Writes the screen text,
ANSI stripped, to <out>/tui-screen.txt and a JSON line per step to stdout.

    python3 -I tui.py <out dir> <claude> <cwd> <marker>=<message> ...
Environment: the caller's, passed through to claude.
"""
import json, os, pty, re, select, sys, time

ANSI = re.compile(rb"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07]*\x07|\x1b[()][0-9A-B]|\x1b[=>]")


def main(out, claude, cwd, *steps):
    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(cwd)
        os.execv(claude, [claude])
    os.set_blocking(fd, False)
    import fcntl, struct, termios
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 160, 0, 0))
    screen = bytearray()

    def read_for(secs, until=None):
        end = time.time() + secs
        while time.time() < end:
            r, _, _ = select.select([fd], [], [], 0.2)
            if r:
                try:
                    data = os.read(fd, 65536)
                except OSError:
                    return False
                if not data:
                    return False
                screen.extend(data)
            if until and until(text()):
                return True
        return False

    def text():
        return ANSI.sub(b"", bytes(screen)).decode("utf-8", "replace")

    def send(s):
        os.write(fd, s.encode())

    t0 = time.time()
    ready = read_for(60, lambda t: "forshortcuts" in t.replace(" ", ""))
    print(json.dumps({"step": "start", "ready": ready, "secs": round(time.time() - t0, 1)}), flush=True)
    for step in steps:
        marker, _, message = step.partition("=")
        mark = len(text())
        t1 = time.time()
        if message.startswith("KEY:"):
            # A raw key, such as shift-tab; the marker is matched with spaces removed.
            send(message[4:].encode().decode("unicode_escape"))
            got = read_for(5, lambda t: marker in t[mark:].replace(" ", ""))
        else:
            send(message)
            time.sleep(0.5)
            send("\r")
            got = read_for(120, lambda t: marker in t[mark + len(message):])
        read_for(3)
        print(json.dumps({"step": message, "answered": got, "secs": round(time.time() - t1, 1)}), flush=True)
    send("/exit")
    time.sleep(0.5)
    send("\r")
    read_for(10)
    try:
        os.kill(pid, 9)
    except ProcessLookupError:
        pass
    open(os.path.join(out, "tui-screen.txt"), "w").write(text())


if __name__ == "__main__":
    main(*sys.argv[1:])
