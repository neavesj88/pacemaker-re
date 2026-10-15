#!/usr/bin/env bash
# Each .pfw is a raw ext2 image of the device's system partition; 7-Zip reads ext2 natively on Windows.
# Idempotent: skips images already unpacked into work/fw-<id>.
set -euo pipefail
cd "$(dirname "$0")/.."
SZ="${SEVENZIP:-/c/Program Files/7-Zip/7z.exe}"
for img in vendor/firmware/dfw_*.pfw; do
  id=$(basename "$img" .pfw); id=${id#dfw_}
  out=work/fw-$id
  [[ -d $out/bin ]] && continue
  "$SZ" x -y -bso0 -bsp0 -o"$out" "$img" || echo "warn: 7z reported issues for $img" >&2
done
for d in work/fw-*/; do
  v=$(grep -o 'SWVERSION="[^"]*"' "$d/system.xml" 2>/dev/null || echo '?')
  e=$(grep -o 'EDITOR_COMPATIBLE_FLAG="[^"]*"' "$d/system.xml" 2>/dev/null || echo '?')
  s=$(stat -c %s "$d/bin/pmplayer" 2>/dev/null || echo '-')
  printf '%-22s %-20s %-30s pmplayer=%s\n' "$d" "$v" "$e" "$s"
done
