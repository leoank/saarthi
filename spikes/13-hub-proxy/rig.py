"""Two-hop rig for EXP-20261114-d82rcz (Python, no dependencies): the hub holds the token.

node: listens on a unix socket for Claude Code's plain-HTTP API requests
  (ANTHROPIC_UNIX_SOCKET with ANTHROPIC_BASE_URL=http://api.anthropic.com) and relays the
  bytes to the hub hop over TCP; answers 502 (logged) when the hub cannot be reached, and closes the
  client's connection when the hub's goes, as a node would.
hub: listens on 127.0.0.1:<port>, replaces the Authorization header with the real token
  read from a file, and relays the request over TLS to api.anthropic.com. One request per
  connection (Connection: close). Logs per request the path, the status, bytes each way,
  the time, and the rate-limit headers of the answer; never the token or a body.
  With --die-after <n>, the process exits once a streamed answer has relayed n bytes: a
  hub restart in the middle of a turn.

    python3 -I rig.py node <socket> <hub port> <log.jsonl>
    python3 -I rig.py hub <port> <token file> <log.jsonl> [--die-after <n>]
"""
import json, os, socket, ssl, sys, threading, time

UPSTREAM = "api.anthropic.com"


def log(path, rec):
    rec["t"] = round(time.time(), 3)
    with open(path, "a") as f:
        f.write(json.dumps(rec) + "\n")


def shut(*socks):
    for s in socks:
        try:
            s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        s.close()


def pipe(a, b, count=None, on_chunk=None):
    n = 0
    try:
        while data := a.recv(65536):
            b.sendall(data)
            n += len(data)
            if on_chunk:
                on_chunk(n)
    except OSError:
        pass
    if count is not None:
        count.append(n)
    return n


def read_head(conn):
    buf = b""
    while b"\r\n\r\n" not in buf:
        chunk = conn.recv(65536)
        if not chunk:
            return None, b""
        buf += chunk
    head, rest = buf.split(b"\r\n\r\n", 1)
    return head.decode("latin1").split("\r\n"), rest


def node(sock_path, hub_port, log_path):
    if os.path.exists(sock_path):
        os.unlink(sock_path)
    srv = socket.socket(socket.AF_UNIX)
    srv.bind(sock_path)
    srv.listen(64)

    def handle(conn):
        try:
            up = socket.create_connection(("127.0.0.1", int(hub_port)), timeout=5)
            up.settimeout(None)
        except OSError:
            lines, _ = read_head(conn)
            log(log_path, {"event": "502, the hub cannot be reached", "path": lines[0] if lines else None})
            body = b"saarthi: the hub cannot be reached"
            conn.sendall(b"HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\nConnection: close\r\n"
                         + f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
            shut(conn)
            return
        threading.Thread(target=pipe, args=(conn, up), daemon=True).start()
        pipe(up, conn)
        shut(conn, up)

    while True:
        c, _ = srv.accept()
        threading.Thread(target=handle, args=(c,), daemon=True).start()


def hub(port, token_file, log_path, *opts):
    die_after = int(opts[1]) if opts[:1] == ("--die-after",) else None
    token = open(token_file).read().strip()
    ctx = ssl.create_default_context()
    srv = socket.create_server(("127.0.0.1", int(port)))

    def handle(conn):
        started = time.time()
        lines, rest = read_head(conn)
        if not lines:
            conn.close()
            return
        method, target, _ = lines[0].split(" ", 2)
        out, auth, length = [f"{method} {target} HTTP/1.1"], "none", 0
        for line in lines[1:]:
            name, _, value = line.partition(":")
            n = name.strip().lower()
            if n == "authorization":
                auth = "dummy" if value.strip() == "Bearer saarthi-proxied" else "other"
                continue
            if n == "content-length":
                length = int(value.strip())
            if n in ("host", "connection", "proxy-connection", "keep-alive"):
                continue
            out.append(line)
        out += [f"Host: {UPSTREAM}", f"Authorization: Bearer {token}", "Connection: close"]
        up = ctx.wrap_socket(socket.create_connection((UPSTREAM, 443)), server_hostname=UPSTREAM)
        up.sendall(("\r\n".join(out) + "\r\n\r\n").encode("latin1") + rest)
        threading.Thread(target=pipe, args=(conn, up), daemon=True).start()
        head = b""
        while b"\r\n\r\n" not in head:
            chunk = up.recv(65536)
            if not chunk:
                break
            head += chunk
        conn.sendall(head)
        rlines = head.split(b"\r\n\r\n", 1)[0].decode("latin1").split("\r\n")
        status = rlines[0]
        hdrs = {}
        for line in rlines[1:]:
            k, _, v = line.partition(":")
            hdrs[k.strip().lower()] = v.strip()
        streamed = hdrs.get("content-type", "").startswith("text/event-stream")
        rec = {"method": method, "path": target, "auth_in": auth, "status": status,
               "req_bytes": length, "streamed": streamed,
               "ratelimit": {k: v for k, v in hdrs.items() if "ratelimit" in k}}

        def maybe_die(n):
            if die_after is not None and streamed and n >= die_after:
                log(log_path, dict(rec, resp_bytes=len(head) + n, secs=round(time.time() - started, 2),
                                   event="hub exits mid-stream"))
                os._exit(0)

        n = pipe(up, conn, on_chunk=maybe_die)
        log(log_path, dict(rec, resp_bytes=len(head) + n, secs=round(time.time() - started, 2)))
        shut(conn, up)

    while True:
        c, _ = srv.accept()
        threading.Thread(target=handle, args=(c,), daemon=True).start()


if __name__ == "__main__":
    {"node": node, "hub": hub}[sys.argv[1]](*sys.argv[2:])
