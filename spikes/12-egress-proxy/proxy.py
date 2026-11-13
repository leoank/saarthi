"""Spike proxy for EXP-20261110-g5mh89 (Python, no dependencies).

anthropic: listens on a unix socket for Claude Code's plain-HTTP API requests
  (ANTHROPIC_UNIX_SOCKET with ANTHROPIC_BASE_URL=http://api.anthropic.com), replaces the
  Authorization header with the real token read from a file, and relays the request over
  TLS to api.anthropic.com. One request per connection (Connection: close), so every
  request's head is seen. The token is never printed; the log records only whether the
  incoming header was the dummy.
connect: an HTTP CONNECT proxy on 127.0.0.1 that logs each tunnel's target, to name what
  Claude Code still reaches directly (HTTPS_PROXY).

    python3 -I proxy.py anthropic <socket> <token file> <dummy> <log.jsonl>
    python3 -I proxy.py connect <port> <log.jsonl>
"""
import json, os, socket, ssl, sys, threading, time

UPSTREAM = "api.anthropic.com"


def log(path, rec):
    rec["t"] = round(time.time(), 3)
    with open(path, "a") as f:
        f.write(json.dumps(rec) + "\n")


def pipe(a, b):
    try:
        while data := a.recv(65536):
            b.sendall(data)
    except OSError:
        pass
    finally:
        for s in (a, b):
            try:
                s.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass


def read_head(conn):
    buf = b""
    while b"\r\n\r\n" not in buf:
        chunk = conn.recv(65536)
        if not chunk:
            return None, b""
        buf += chunk
    head, rest = buf.split(b"\r\n\r\n", 1)
    return head.decode("latin1").split("\r\n"), rest


def anthropic(sock_path, token_file, dummy, log_path):
    token = open(token_file).read().strip()
    ctx = ssl.create_default_context()
    if os.path.exists(sock_path):
        os.unlink(sock_path)
    srv = socket.socket(socket.AF_UNIX)
    srv.bind(sock_path)
    srv.listen(64)

    def handle(conn):
        lines, rest = read_head(conn)
        if not lines:
            conn.close()
            return
        method, target, _ = lines[0].split(" ", 2)
        out, auth = [f"{method} {target} HTTP/1.1"], "none"
        for line in lines[1:]:
            name, _, value = line.partition(":")
            n = name.strip().lower()
            if n == "authorization":
                auth = "dummy" if value.strip() == f"Bearer {dummy}" else "other"
                continue  # replaced below
            if n in ("host", "connection", "proxy-connection", "keep-alive"):
                continue
            out.append(line)
        out += [f"Host: {UPSTREAM}", f"Authorization: Bearer {token}", "Connection: close"]
        up = ctx.wrap_socket(socket.create_connection((UPSTREAM, 443)), server_hostname=UPSTREAM)
        up.sendall(("\r\n".join(out) + "\r\n\r\n").encode("latin1") + rest)
        # The rest of the body follows while the response is awaited.
        threading.Thread(target=pipe, args=(conn, up), daemon=True).start()
        first = up.recv(65536)
        status = first.split(b"\r\n", 1)[0].decode("latin1", "replace")
        log(log_path, {"via": "socket", "method": method, "path": target, "auth_in": auth, "status": status})
        conn.sendall(first)
        pipe(up, conn)

    while True:
        c, _ = srv.accept()
        threading.Thread(target=handle, args=(c,), daemon=True).start()


def connect(port, log_path):
    srv = socket.create_server(("127.0.0.1", int(port)))

    def handle(conn):
        lines, rest = read_head(conn)
        if not lines:
            conn.close()
            return
        method, target, _ = lines[0].split(" ", 2)
        log(log_path, {"via": "direct", "method": method, "target": target})
        if method != "CONNECT":
            conn.close()
            return
        host, _, p = target.rpartition(":")
        up = socket.create_connection((host, int(p)))
        conn.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
        if rest:
            up.sendall(rest)
        threading.Thread(target=pipe, args=(conn, up), daemon=True).start()
        pipe(up, conn)

    while True:
        c, _ = srv.accept()
        threading.Thread(target=handle, args=(c,), daemon=True).start()


if __name__ == "__main__":
    {"anthropic": anthropic, "connect": connect}[sys.argv[1]](*sys.argv[2:])
