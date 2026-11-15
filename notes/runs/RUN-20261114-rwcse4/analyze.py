"""RUN-20261114-rwcse4: when each write's change event reached the WebView's page, per mode.
    python3 -I analyze.py <run dir>    prints one ===RESULT=== line"""
import json, os, re, statistics, sys
d, out = sys.argv[1], {}
for mode in ("loopback", "intercept"):
    writes = [int(x) for x in open(os.path.join(d, f"writes-{mode}.txt")).read().split()]
    log = open(os.path.join(d, f"logcat-{mode}.txt"), errors="replace").read()
    seen = [int(m) for m in re.findall(r"SSE (\d{13}) size=", log)]
    lat = []
    for w in writes:
        after = [s for s in seen if s >= w]
        if after:
            lat.append(min(after) - w)
    out[mode] = {"writes": len(writes), "events": len(seen), "matched": len(lat),
                 "ms_p50": statistics.median(lat) if lat else None,
                 "ms_max": max(lat) if lat else None,
                 "errors": len(re.findall(r"SSE error", log)),
                 "intercepted": len(re.findall(r"intercepted ", log))}
    files_log = open(os.path.join(d, f"logcat-files-{mode}.txt"), errors="replace").read()
    out[mode]["files_viewer_requests_intercepted"] = len(re.findall(r"intercepted ", files_log))
burst_w = [int(x) for x in open(os.path.join(d, "writes-burst.txt")).read().split()]
blog = open(os.path.join(d, "logcat-burst.txt"), errors="replace").read()
bseen = [int(m) for m in re.findall(r"SSE (\d{13}) size=", blog)]
groups = sorted(set(bseen))
out["intercept_burst"] = {"writes": len(burst_w), "events": len(bseen),
    "first_event_after_first_write_ms": (min(bseen) - burst_w[0]) if bseen else None,
    "last_event_after_last_write_ms": (max(bseen) - burst_w[-1]) if bseen else None,
    "distinct_arrival_ms": len(groups)}
skew = [int(x) for x in open(os.path.join(d, "skew.txt")).read().split()]
out["device_clock_minus_host_ms"] = statistics.median(skew)
print("===RESULT=== " + json.dumps(out))
