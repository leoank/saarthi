#!/usr/bin/env bash
# EXP-20261118-298ryc: in a `claude -p` stream-json session, do `list_models`, `set_model`
# and an effort change through `apply_flag_settings` work, and where does the transcript
# record the model and the effort? Runs Haiku, switches to Sonnet at low effort, asks once
# more. Uses the machine's own Claude login; runs in a temporary directory, removed after.
# Usage: notes/runs/RUN-20261118-29b3r2/run.sh [path to claude]
set -euo pipefail
CLAUDE=${1:-claude}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"
python3 - "$CLAUDE" <<'PY'
import json, subprocess, sys, uuid, glob, os

claude = sys.argv[1]
sid = str(uuid.uuid4())
p = subprocess.Popen(
    [claude, "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose",
     "--model", "haiku", "--session-id", sid],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)

def send(obj):
    p.stdin.write(json.dumps(obj) + "\n"); p.stdin.flush()

def until(pred):
    for line in p.stdout:
        try: m = json.loads(line)
        except ValueError: continue
        if pred(m): return m

def control(subtype, **kw):
    rid = f"x-{subtype}"
    send({"type": "control_request", "request_id": rid, "request": {"subtype": subtype, **kw}})
    return until(lambda m: m.get("type") == "control_response" and m["response"].get("request_id") == rid)["response"]

def turn(text):
    send({"type": "user", "message": {"role": "user", "content": text}})
    models = []
    def done(m):
        if m.get("type") == "assistant": models.append(m["message"].get("model"))
        return m.get("type") == "result"
    until(done)
    return models

out = {}
out["first_models"] = turn("Reply with just the word one.")
catalog = control("list_models")
out["list_models"] = {"subtype": catalog.get("subtype"), "keys": sorted((catalog.get("response") or {}).keys()),
                      "sample": json.dumps(catalog.get("response"))[:800]}
out["set_model"] = control("set_model", model="sonnet")
out["effort"] = control("apply_flag_settings", settings={"effortLevel": "low"})
out["second_models"] = turn("Reply with just the word two.")
p.stdin.close(); p.wait(timeout=60)

# Where the transcript says which model answered and at what effort.
path = glob.glob(os.path.expanduser(f"~/.claude/projects/*/{sid}.jsonl"))[0]
lines = [json.loads(l) for l in open(path)]
out["transcript_models"] = [l["message"].get("model") for l in lines if l.get("type") == "assistant"]
out["transcript_effort"] = [[l.get("effort"), l.get("perTurnEffort")] for l in lines if l.get("type") == "assistant"]
out["effort_fields"] = sorted({k for l in lines for k in l if "effort" in k.lower()} |
                              {k for l in lines if l.get("type") == "assistant" for k in l["message"] if "effort" in k.lower()})
# The lines that mention effort, by kind, and the effort-named fields they carry.
def effort_keys(x, path=""):
    if isinstance(x, dict):
        return [k2 for k, v in x.items() for k2 in ([path + k] if "effort" in k.lower() else []) + effort_keys(v, path + k + ".")]
    if isinstance(x, list):
        return [k for v in x for k in effort_keys(v, path)]
    return []
out["effort_lines"] = [{"type": l.get("type"), "subtype": l.get("subtype"), "fields": effort_keys(l)[:6]}
                       for l in lines if effort_keys(l)][:8]
os.remove(path)
print(json.dumps(out, indent=1))
print("===RESULT===", json.dumps({
    "set_model_ok": out["set_model"].get("subtype") == "success",
    "effort_ok": out["effort"].get("subtype") == "success",
    "switched": bool(out["second_models"]) and out["second_models"][-1] != out["first_models"][-1],
    "list_models_ok": out["list_models"]["subtype"] == "success",
    "effort_recorded": [e[0] for e in out["transcript_effort"]],
}))
PY
