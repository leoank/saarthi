"""EXP-20261114-d82rcz: what real sessions would send through the hub, estimated from the
token counts in this machine's Claude Code transcripts (usage fields only, no content).
Each API call sends its whole context; bytes per context token come from a measured call.

    python3 -I estimate.py <bytes per token> [days]
Prints one ===RESULT=== line.
"""
import glob, json, os, sys, time

per_token = float(sys.argv[1])
days = int(sys.argv[2]) if len(sys.argv) > 2 else 30
cut = time.time() - days * 86400
reqs, sessions = [], []
for f in glob.glob(os.path.expanduser("~/.claude/projects/*/*.jsonl")):
    if os.path.getmtime(f) < cut:
        continue
    seen, ctx = set(), []
    for line in open(f, errors="replace"):
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if d.get("type") != "assistant":
            continue
        m = d.get("message") or {}
        u, mid = m.get("usage") or {}, m.get("id")
        if not u or mid in seen:
            continue
        seen.add(mid)
        c = sum(u.get(k) or 0 for k in ("input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"))
        if c:
            ctx.append(c)
    if ctx:
        reqs += ctx
        sessions.append((len(ctx), sum(ctx)))
reqs.sort()
q = lambda a, p: a[min(len(a) - 1, int(p * len(a)))]
mb = lambda t: round(t * per_token / 1e6, 2)
tot = sorted(s[1] for s in sessions)
n = sorted(s[0] for s in sessions)
print("===RESULT=== " + json.dumps({
    "days": days, "bytes_per_token": per_token, "sessions": len(sessions), "requests": len(reqs),
    "request_mb": {"p50": mb(q(reqs, .5)), "p90": mb(q(reqs, .9)), "p99": mb(q(reqs, .99)), "max": mb(reqs[-1])},
    "session_mb": {"p50": mb(q(tot, .5)), "p90": mb(q(tot, .9)), "max": mb(tot[-1])},
    "requests_per_session": {"p50": q(n, .5), "p90": q(n, .9), "max": n[-1]},
    "total_mb": mb(sum(reqs)),
}))
