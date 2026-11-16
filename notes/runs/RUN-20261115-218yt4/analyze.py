"""RUN-20261115-218yt4: when each write's change event reached the page through the bridge.
    python3 -I analyze.py <run dir>    prints one ===RESULT=== line"""
import json, os, re, statistics, sys
d = sys.argv[1]
writes = [int(x) for x in open(os.path.join(d, "writes-bridge.txt")).read().split()]
log = open(os.path.join(d, "logcat-bridge.txt"), errors="replace").read()
seen = [int(m) for m in re.findall(r"SSE (\d{13}) size=", log)]
skew = statistics.median(int(x) for x in open(os.path.join(d, "skew.txt")).read().split())
lat = []
for w in writes:
    after = [s for s in seen if s >= w + skew]
    if after:
        lat.append(min(after) - skew - w)
arr = sorted(set(seen))
files = open(os.path.join(d, "logcat-files-bridge.txt"), errors="replace").read()
print("===RESULT=== " + json.dumps({
    "writes": len(writes), "events": len(seen), "matched": len(lat),
    "ms_p50_skew_corrected": statistics.median(lat) if lat else None,
    "ms_max_skew_corrected": max(lat) if lat else None,
    "gaps_ms": [b - a for a, b in zip(arr, arr[1:])],
    "device_clock_minus_host_ms": skew,
    "viewer_requests_intercepted": len(re.findall(r"intercepted ", files)),
    "viewer_statuses": sorted(set(re.findall(r"intercepted \S+ (\d{3}) ", files))),
    "viewer_streams_over_bridge": len(re.findall(r"bridge stream \d+ open", files)),
    "viewer_errors": re.findall(r"I SPIKE   : (.*(?:Error|error).*)", files)[:5],
}))
