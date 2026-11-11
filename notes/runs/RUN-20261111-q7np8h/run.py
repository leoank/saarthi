#!/usr/bin/env python3
"""EXP-20261110-g5mh7g, one machine's half: one `claude setup-token` token shared by
several headless sessions at once for hours.

Starts N stream-json `claude -p` sessions on Haiku, each with an empty config directory
and the token from TOKEN_FILE as CLAUDE_CODE_OAUTH_TOKEN, in a directory the node daemon
ignores (`*/target/live-work-*`). Every INTERVAL seconds for HOURS it sends each session a
short prompt and logs, per turn, the time, the session, the result subtype, whether it was
an error, and any API error text. At the end it closes the sessions, resumes the first one
by id with one more turn, and lists what each config directory holds (a credential file
there would be a failure). The token is never printed.

Usage: TOKEN_FILE=... MACHINE=rogue notes/runs/RUN-20261111-q7np8h/run.py [N] [HOURS] [INTERVAL]
Writes JSON lines to stdout; the last line is `===RESULT=== {...}`.
"""
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
import uuid

N = int(sys.argv[1]) if len(sys.argv) > 1 else 3
HOURS = float(sys.argv[2]) if len(sys.argv) > 2 else 4
INTERVAL = int(sys.argv[3]) if len(sys.argv) > 3 else 600
MACHINE = os.environ.get("MACHINE", os.uname().nodename)
TOKEN = open(os.environ["TOKEN_FILE"]).read().strip()

base = tempfile.mkdtemp(prefix="g5mh7g-")
work = os.path.join(base, "target", "live-work-g5mh7g")
os.makedirs(work)


def log(**kw):
    kw = {"t": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "machine": MACHINE, **kw}
    print(json.dumps(kw), flush=True)


class Session:
    def __init__(self, i, sid=None, resume=False):
        self.i = i
        self.id = sid or str(uuid.uuid4())
        self.cfg = os.path.join(base, f"cfg{i}")
        os.makedirs(self.cfg, exist_ok=True)
        env = {**os.environ, "CLAUDE_CONFIG_DIR": self.cfg, "CLAUDE_CODE_OAUTH_TOKEN": TOKEN}
        how = ["--resume", self.id] if resume else ["--session-id", self.id]
        self.p = subprocess.Popen(
            ["claude", "-p", "--input-format", "stream-json", "--output-format", "stream-json",
             "--verbose", "--model", "haiku", *how],
            cwd=work, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True, bufsize=1)
        self.results = []
        self.cv = threading.Condition()
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        for line in self.p.stdout:
            try:
                m = json.loads(line)
            except ValueError:
                continue
            if m.get("type") == "result":
                with self.cv:
                    self.results.append(m)
                    self.cv.notify_all()

    def turn(self, text, timeout=300):
        with self.cv:
            before = len(self.results)
        msg = {"type": "user", "message": {"role": "user", "content": text}}
        try:
            self.p.stdin.write(json.dumps(msg) + "\n")
            self.p.stdin.flush()
        except (BrokenPipeError, ValueError):
            return {"ok": False, "subtype": "process gone", "error": None}
        with self.cv:
            self.cv.wait_for(lambda: len(self.results) > before, timeout)
            if len(self.results) == before:
                return {"ok": False, "subtype": "timeout", "error": None}
            r = self.results[-1]
        err = None
        if r.get("is_error"):
            err = (r.get("result") or "")[:300]
        return {"ok": not r.get("is_error"), "subtype": r.get("subtype"), "error": err}

    def close(self):
        try:
            self.p.stdin.close()
        except Exception:
            pass
        try:
            self.p.wait(30)
        except subprocess.TimeoutExpired:
            self.p.kill()


sessions = [Session(i) for i in range(N)]
log(event="start", sessions=[s.id for s in sessions], hours=HOURS, interval=INTERVAL)
turns = fails = 0
end = time.time() + HOURS * 3600
k = 0
while True:
    k += 1
    for s in sessions:
        r = s.turn(f"Turn {k}. Reply with the single word ok.")
        turns += 1
        fails += not r["ok"]
        log(event="turn", n=k, session=s.i, **r)
    if time.time() + INTERVAL > end:
        break
    time.sleep(INTERVAL)
for s in sessions:
    s.close()

first = sessions[0]
resumed = Session(0, first.id, resume=True)
r = resumed.turn("After the resume: reply with the single word ok.")
resumed.close()
log(event="resume", session=0, **r)

files = {s.i: sorted(os.listdir(s.cfg)) for s in sessions}
creds = [i for i, f in files.items() if any("credential" in x.lower() for x in f)]
log(event="config_dirs", files=files)
print("===RESULT===", json.dumps({
    "machine": MACHINE, "sessions": N, "turns": turns, "failed_turns": fails,
    "resume_ok": r["ok"], "credential_files": creds,
}), flush=True)
