#!/usr/bin/env bash
# Regenerates golden.f32, the output of the device's C Vocal DSP for scenario.txt, which
# tests/vocal_equiv.rs compares the Rust port against. Run it only when device/vocalfx/vocal_dsp.c
# (or the scenario) changes. Needs gcc; on Windows run it in WSL:
#   wsl.exe -d Debian -u root bash /mnt/c/.../crates/pm-mixer/tests/vocal_ref/gen.sh /mnt/c/.../device/vocalfx
# $1 (or $VFX_SRC): the folder holding vocal_dsp.c and vocal_dsp.h (device/vocalfx on the firmware branch).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
src="${1:-${VFX_SRC:-}}"
[[ -n "$src" && -f "$src/vocal_dsp.c" && -f "$src/vocal_dsp.h" ]] || { echo "usage: gen.sh <folder with vocal_dsp.c and vocal_dsp.h>" >&2; exit 2; }
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
# -ffp-contract=off: no fused multiply-add, so every product and sum is rounded separately like in Rust
gcc -std=gnu11 -O2 -ffp-contract=off -Wall -Wextra -I"$src" -o "$tmp/vocal_ref" "$here/vocal_ref.c" "$src/vocal_dsp.c" -lm
"$tmp/vocal_ref" "$here/scenario.txt" "$here/golden.f32"
{
  echo "golden.f32 = output of the device's float DSP for scenario.txt (vocal_ref.c, gen.sh)"
  (cd "$src" && sha256sum vocal_dsp.c vocal_dsp.h)
  sha256sum "$here/vocal_ref.c" | sed 's#  .*/#  #'
  gcc --version | head -1
} > "$here/golden.sources.txt"
ls -l "$here/golden.f32"
