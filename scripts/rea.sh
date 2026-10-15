#!/usr/bin/env bash
# Run rea with Ghidra/JDK 21 scoped to this process only, so a different machine-wide JDK
# (needed by other projects) is left alone. Override with GHIDRA_INSTALL_DIR / REA_JAVA_HOME.
set -euo pipefail
export GHIDRA_INSTALL_DIR="${GHIDRA_INSTALL_DIR:-C:\tools\ghidra_12.1.4_PUBLIC}"
export JAVA_HOME="${REA_JAVA_HOME:-C:\tools\jdk-21.0.12.1+1}"
exec rea "$@"
