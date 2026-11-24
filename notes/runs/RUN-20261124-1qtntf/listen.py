"""EXP-20261124-1pg2cq: stand-ins for the node's proxy and for the internet.

  listen.py SOCKET PORT LOG

SOCKET: a unix socket answering HTTP like the node's Claude API proxy, but with 401 to
every request, after logging `SOCKET <method> <path>` and whether a token came.
PORT: an HTTP proxy on 127.0.0.1 that logs `DIRECT <method> <target>` for every request
(a CONNECT names the host) and refuses it, so nothing leaves the machine.
Runs until killed; each line goes to LOG at once.
"""
import http.server
import socketserver
import sys
import threading

sock_path, port, log_path = sys.argv[1], int(sys.argv[2]), sys.argv[3]
log = open(log_path, "a", buffering=1)


def note(line):
    log.write(line + "\n")


class Proxied(http.server.BaseHTTPRequestHandler):
    """The node's proxy: what reaches the socket, answered 401."""

    def handle_one(self):
        auth = self.headers.get("authorization", "")
        token = "dummy" if "saarthi-proxied" in auth else ("other" if auth else "none")
        note(f"SOCKET {self.command} {self.path.split('?')[0]} token={token}")
        n = int(self.headers.get("content-length") or 0)
        if n:
            self.rfile.read(n)
        body = b'{"type":"error","error":{"type":"authentication_error","message":"probe"}}'
        self.send_response(401)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = do_HEAD = handle_one

    def log_message(self, *args):
        pass

    # A unix socket has no client address to format.
    def address_string(self):
        return "unix"


class Direct(http.server.BaseHTTPRequestHandler):
    """The internet: every request and tunnel logged and refused."""

    def refuse(self):
        note(f"DIRECT {self.command} {self.path.split('?')[0]}")
        self.send_response(403)
        self.send_header("content-length", "0")
        self.end_headers()

    do_CONNECT = do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = do_HEAD = refuse

    def log_message(self, *args):
        pass


class UnixServer(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True


class TcpServer(socketserver.ThreadingMixIn, socketserver.TCPServer):
    daemon_threads = True
    allow_reuse_address = True


unix = UnixServer(sock_path, Proxied)
tcp = TcpServer(("127.0.0.1", port), Direct)
threading.Thread(target=unix.serve_forever, daemon=True).start()
note("READY")
tcp.serve_forever()
