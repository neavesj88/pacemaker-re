#!/usr/bin/env bash
# Headless Ghidra decompilation for targets rea's Windows lane won't admit (ARM ELF, Mach-O).
# First run per binary imports + auto-analyzes into work/ghidra/<name>.gpr (slow); later runs reuse it.
# Usage: scripts/ghidra-decompile.sh <binary> <outdir> <name-regex> [callers|list]
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
bin="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"; out="$2"; regex="$3"; mode="${4:-}"
ghidra="${GHIDRA_INSTALL_DIR:-/c/tools/ghidra_12.1.4_PUBLIC}"
export JAVA_HOME="${REA_JAVA_HOME:-C:\tools\jdk-21.0.12.1+1}"
proj_dir="$here/work/ghidra"; mkdir -p "$proj_dir"
name="$(basename "$(dirname "$(dirname "$bin")")")-$(basename "$bin")"
w() { cygpath -w "$1"; }
mkdir -p "$out"; printf '%s\n' "$regex" > "$out/REGEX.txt"
if [[ -d "$proj_dir/$name.rep" ]]; then
  src=(-process "$(basename "$bin")" -noanalysis)
else
  src=(-import "$(w "$bin")")
fi
"$ghidra/support/analyzeHeadless.bat" "$(w "$proj_dir")" "$name" "${src[@]}" \
  -scriptPath "$(w "$here/scripts/ghidra")" -postScript DecompileMatching.java "$(w "$out")" "@$(w "$out/REGEX.txt")" $mode \
  -log "$(w "$proj_dir/$name.log")" > "$out/headless.log" 2>&1 && status=0 || status=$?
grep -E 'ERROR|decompiled|REPORT|Exception' "$out/headless.log" || true
exit $status
