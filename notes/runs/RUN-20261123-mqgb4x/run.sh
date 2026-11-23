#!/usr/bin/env bash
# Re-creates RUN-20261123-mqgb4x for EXP-20261123-j9wyqh. Run from the repo root, inside the
# project's dev shell, at the commit recorded on the run page:
#   nix develop -c notes/runs/RUN-20261123-mqgb4x/run.sh
# Builds libghostty-vt for this machine from the flake, compiles probe.c against it and runs
# it; the probe prints one line per case and a ===RESULT=== line. Then checks that the same
# library builds for x86_64-linux (on a remote builder where the machine has one).
set -euo pipefail
here=notes/runs/RUN-20261123-mqgb4x
out=$(mktemp -d)
lib=$(nix build .#libghostty-vt --no-link --print-out-paths)
cc -O2 -std=c11 -I"$lib/include" "$here/probe.c" "$lib/lib/libghostty-vt.a" -o "$out/probe"
"$out/probe" | tee "$here/probe.log"
nix build .#packages.x86_64-linux.libghostty-vt --no-link --print-out-paths | sed 's/^/x86_64-linux build: /' | tee -a "$here/probe.log"
