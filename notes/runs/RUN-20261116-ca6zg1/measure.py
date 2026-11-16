"""RUN-20261116-ca6zg1: the same requests straight to `yajna serve` and through the relay.

    python3 -I measure.py <service port> <relay proxy port> <log file to append to>
Prints one ===RESULT=== line.
"""
import hashlib, http.client, json, socket, statistics, sys, threading, time

direct, relay, logf = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
MB = 1 << 20


def get(port, path, headers=None):
    c = http.client.HTTPConnection("127.0.0.1", port, timeout=60)
    t = time.perf_counter()
    c.request("GET", path, headers=dict(headers or {}, Host="localhost"))
    r = c.getresponse()
    first = time.perf_counter()
    body = r.read()
    end = time.perf_counter()
    c.close()
    return r.status, body, (first - t) * 1000, (end - t) * 1000


def times(port, path, headers=None, n=10):
    out, digest, status = [], None, None
    for _ in range(n):
        status, body, _, total = get(port, path, headers)
        out.append(total)
        digest = hashlib.sha256(body).hexdigest()[:12]
    return {"status": status, "bytes": len(body), "sha": digest,
            "ms_p50": round(statistics.median(out), 2), "ms_max": round(max(out), 2)}


def pair(path, headers=None, n=10):
    d, r = times(direct, path, headers, n), times(relay, path, headers, n)
    return {"direct": d, "relay": r, "same": (d["status"], d["sha"]) == (r["status"], r["sha"])}


class Events(threading.Thread):
    """Reads change events from /api/events and stamps when each arrives."""
    def __init__(self, port):
        super().__init__(daemon=True)
        self.port, self.got = port, []
        self.ready = threading.Event()

    def run(self):
        s = socket.create_connection(("127.0.0.1", self.port))
        s.sendall(b"GET /api/events?watch=data/log.txt HTTP/1.1\r\nHost: localhost\r\n"
                  b"Accept: text/event-stream\r\n\r\n")
        buf = b""
        while True:
            chunk = s.recv(65536)
            if not chunk:
                return
            now = time.time()
            buf += chunk
            if b"\r\n\r\n" in buf:
                self.ready.set()
            while b"event: change" in buf:
                i = buf.index(b"event: change")
                buf = buf[i + len(b"event: change"):]
                self.got.append(now)


def events(writes=10, gap=1.0, during=None):
    a, b = Events(direct), Events(relay)
    a.start(); b.start()
    a.ready.wait(10); b.ready.wait(10)
    time.sleep(1.5)                              # the first event, what the file is now
    a.got.clear(); b.got.clear()
    if during:
        threading.Thread(target=during, daemon=True).start()
        time.sleep(0.3)
    written = []
    for i in range(writes):
        with open(logf, "a") as f:
            f.write(f"line {i} {time.time()}\n" * 100)
        written.append(time.time())
        time.sleep(gap)
    time.sleep(1.5)

    def lat(got):
        out = []
        for w in written:
            after = [g for g in got if g >= w]
            if after:
                out.append((min(after) - w) * 1000)
        return out
    da, db = lat(a.got), lat(b.got)
    # relay minus direct for the same write: what the relay adds
    diff = [y - x for x, y in zip(da, db)]
    return {"writes": writes, "direct_seen": len(da), "relay_seen": len(db),
            "direct_ms_p50": round(statistics.median(da), 1) if da else None,
            "relay_ms_p50": round(statistics.median(db), 1) if db else None,
            "added_ms_p50": round(statistics.median(diff), 1) if diff else None,
            "added_ms_max": round(max(diff), 1) if diff else None}


rng = lambda off, n: {"Range": f"bytes={off}-{off + n - 1}"}
res = {
    "viewer": pair("/files/"),
    "site_index": pair("/"),
    "listing": pair("/api/files/list?path=data"),
    "range_1mb_at_512mb": pair("/api/files/raw?path=data/big.bin", rng(512 * MB, MB), n=5),
}
big = {}
for name, port in (("direct", direct), ("relay", relay)):
    st, body, first, total = get(port, "/api/files/raw?path=data/big.bin", rng(0, 32 * MB))
    big[name] = {"status": st, "mb": len(body) // MB, "first_byte_ms": round(first, 1),
                 "mb_per_s": round(len(body) / MB / (total / 1000), 1)}
res["range_32mb"] = big
res["events_idle"] = events()


def download():
    get(relay, "/api/files/raw?path=data/big.bin", rng(0, 128 * MB))
res["events_during_128mb_download"] = events(writes=5, gap=0.5, during=download)
print("===RESULT=== " + json.dumps(res))
