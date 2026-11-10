#!/usr/bin/env bash
# Build a modified .pfw from a stock one: replace files inside its ext3 image, optionally wrap the
# player with a drive diagnostic, re-check the filesystem and write a fresh trailer with the same
# versions (so the device installs it without flashing kernel, rootfs or bootloader).
# Runs on Linux (WSL: wsl -d Debian -u root -- scripts/build-pfw.sh ...); needs e2fsprogs, python3.
#
#   scripts/build-pfw.sh STOCK.pfw OUT.pfw [--diag] [/image/path=local/file ...]
#
# --diag renames /bin/pmplayer to pmplayer.real and installs a shell wrapper that writes what the
# kernel sees of the drive to .Pacemaker/revival-diag.txt on each start, then execs the player
# (same PID, which is how pwrApplication tracks it).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
stock="$1"; out="$2"; shift 2
diag=0
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
img="$work/image.ext3"

size=$(stat -c %s "$stock")
head -c $((size - 512)) "$stock" > "$img"
dumpe2fs -h "$img" 2>/dev/null | grep -E '^Filesystem features' > "$work/features.before"

# copy owner and mode from the file being replaced (or from a sibling for new files)
meta() {  # meta <image path> -> "mode uid gid" in octal/decimal
  debugfs -R "stat \"$1\"" "$img" 2>/dev/null |
    awk 'match($0, /Mode: +[0-7]+/) {m=substr($0, RSTART + 6, RLENGTH - 6); gsub(/ /, "", m)}
         /User:/ {u=$2; g=$4} END {print m, u, g}'
}
replace() {  # replace <image path> <local file> [mode]
  local dst="$1" src="$2" dir base m u g
  dir="$(dirname "$dst")"; base="$(basename "$dst")"
  read -r m u g < <(meta "$dst")
  [[ -n "${3:-}" ]] && m="$3"
  [[ -n "$m" ]] || { echo "no such file in image: $dst" >&2; exit 1; }
  debugfs -w -f - "$img" > "$work/debugfs.log" 2>&1 <<EOF
cd "$dir"
rm "$base"
write "$src" "$base"
sif "$base" mode 0100$m
sif "$base" uid $u
sif "$base" gid $g
EOF
  echo "replaced $dst ($(stat -c %s "$src") bytes, mode $m, owner $u:$g)"
}

for arg in "$@"; do
  case "$arg" in
    --diag) diag=1 ;;
    /*=*) replace "${arg%%=*}" "${arg#*=}" ;;
    *) echo "bad argument: $arg" >&2; exit 2 ;;
  esac
done

if [[ $diag == 1 ]]; then
  cat > "$work/pmplayer" <<'EOF'
#!/bin/sh
# Pacemaker Revival drive diagnostic: record what the kernel sees of the drive, then run the player.
d=/mnt/public/.Pacemaker/revival-diag.txt
{
  date; uname -a; echo "== /proc/partitions"; cat /proc/partitions
  for f in identify model capacity geometry media driver settings; do
    echo "== /proc/ide/hda/$f"; cat /proc/ide/hda/$f
  done
  echo "== dmesg"; dmesg
} > "$d" 2>&1
sync
exec /pmds/bin/pmplayer.real "$@"
EOF
  read -r m u g < <(meta /bin/pmplayer)
  debugfs -w -f - "$img" > "$work/debugfs.log" 2>&1 <<EOF
cd /bin
ln pmplayer pmplayer.real
unlink pmplayer
write "$work/pmplayer" pmplayer
sif pmplayer mode 0100755
sif pmplayer uid $u
sif pmplayer gid $g
EOF
  echo "wrapped /bin/pmplayer with the drive diagnostic (real player kept as pmplayer.real, mode $m)"
fi

e2fsck -fn "$img" > "$work/fsck.log" 2>&1 || { cat "$work/fsck.log"; echo "e2fsck found problems" >&2; exit 1; }
dumpe2fs -h "$img" 2>/dev/null | grep -E '^Filesystem features' > "$work/features.after"
cmp -s "$work/features.before" "$work/features.after" || { echo "filesystem features changed" >&2; exit 1; }
echo "e2fsck clean: $(tail -1 "$work/fsck.log")"

python3 - "$stock" "$img" "$out" "$here/scripts" <<'EOF'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("pfw", sys.argv[4] + "/pfw-trailer.py")
pfw = importlib.util.module_from_spec(spec); spec.loader.exec_module(pfw)
tail = open(sys.argv[1], "rb").read()[-512:]
vals, o = {}, 0
for name, n in pfw.FIELDS:
    vals[name] = pfw.field(tail[o:o + n]); o += n
image = open(sys.argv[2], "rb").read()
open(sys.argv[3], "wb").write(pfw.build(image, vals["swver"], vals["fwver"], vals["bootloaderver"]))
EOF
python3 "$here/scripts/pfw-trailer.py" "$out"
