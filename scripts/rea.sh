#!/usr/bin/env bash
# rea needs Node >= 22.19 on Windows (system Node 22.14 fails with ESTALE on every target).
# Use the project-local portable Node until the system install is upgraded.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
node_exe="$here/tools/node/node.exe"
[[ -x $node_exe ]] || node_exe=node
exec "$node_exe" "$(npm root -g)/rea-agents/scripts/rea.mjs" "$@"
