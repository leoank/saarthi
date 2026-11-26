#!/usr/bin/env bash
# RUN-20261126-hsed5n: what in crates/node belongs to the process or the machine rather than
# to one node: statics, environment reads and writes, fixed or home-relative paths, sockets
# and ports, signals, the log subscriber. Run from the repo root; writes survey.txt.
set -euo pipefail
out=notes/runs/RUN-20261126-hsed5n
mkdir -p "$out"
cd crates/node/src
{
  section() { echo; echo "## $1"; grep -rnE "$2" --include='*.rs' . | grep -vE '^\./[^:]+:[0-9]+:\s*//' | grep -v '#\[cfg(test)\]' || true; }
  echo "# crates/node at $(git rev-parse --short HEAD)"
  section "statics (process-wide)" '^\s*(pub(\([a-z]+\))? )?static [A-Z_]+'
  section "environment written" 'set_var|remove_var|set_current_dir'
  section "environment read" 'env::var(_os)?\("(HOME|XDG_[A-Z_]+|TMPDIR|SHELL|CLAUDE_CONFIG_DIR)"\)'
  section "fixed paths" '"/(private/)?tmp/|\.local/state/saarthi'
  section "sockets and ports bound" '(UnixListener|TcpListener)::bind'
  section "signals and the log subscriber" 'signal::|ctrl_c|tracing_subscriber::fmt'
  section "the registry scan and the user's settings" 'scan_registry|apply_registry|settings\.json'
} > "../../../$out/survey.txt"
wc -l "../../../$out/survey.txt"
