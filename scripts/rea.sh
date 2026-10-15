#!/usr/bin/env bash
# Run rea with Ghidra/JDK 21 scoped to this process only: the machine-wide JDK stays 17
# because the Android/Capacitor builds are pinned to it.
set -euo pipefail
export GHIDRA_INSTALL_DIR="${GHIDRA_INSTALL_DIR:-C:\tools\ghidra_12.1.4_PUBLIC}"
export JAVA_HOME="${REA_JAVA_HOME:-C:\tools\jdk-21.0.12.1+1}"
exec rea "$@"
