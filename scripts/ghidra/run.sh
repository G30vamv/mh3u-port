#!/usr/bin/env bash
# run.sh Script.java args...: runs one of these scripts read-only on an analysed Ghidra project and prints only its output.
#   GHIDRA_HOME  Ghidra's folder (with support/analyzeHeadless)
#   GHIDRA_PROJECTS  the folder holding the project "mh3u" (default ~/.local/share/mh3u/ghidra), outside any git working tree
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
: "${GHIDRA_HOME:?set GHIDRA_HOME to your Ghidra folder}"
projects="${GHIDRA_PROJECTS:-$HOME/.local/share/mh3u/ghidra}"
s="$1"; shift
"$GHIDRA_HOME/support/analyzeHeadless" "$projects" mh3u -process mh3u.rpx -noanalysis -readOnly \
  -scriptPath "$here" -postScript "$s" "$@" 2>/dev/null | awk -v s="$s> " '
  index($0, "INFO  " s) == 1 { on = 1; $0 = substr($0, length("INFO  " s) + 1) }
  /^(INFO|WARN|ERROR) / { on = 0; next }
  on { sub(/ \(GhidraScript\)  $/, ""); print }'
