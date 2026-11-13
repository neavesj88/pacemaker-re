#!/usr/bin/env bash
# Build a modified .pfw from a stock one: replace files inside its ext3 image, optionally wrap the
# player with a drive diagnostic, re-check the filesystem and write a fresh trailer with the same
# versions (so the device installs it without flashing kernel, rootfs or bootloader).
# Runs on Linux (WSL: wsl -d Debian -u root -- scripts/build-pfw.sh ...); needs e2fsprogs, python3.
#
#   scripts/build-pfw.sh STOCK.pfw OUT.pfw [--diag] [--fwver=N] [/image/path=local/file ...] [+/new/path=local/file ...]
#
# "/path=file" replaces a file that exists in the image (owner kept). "+/path=file" adds one that does not
# (mode 0755, owner of the parent directory, which must exist). "mkdir:/path" creates a directory (mode 0755,
# owner of the parent, which must exist); "=/path=file" adds a data file like "+" but with mode 0644.
# Arguments run in order, so put mkdir before the files that go inside.
#
# --fwver=N writes N as the trailer's fwver. A fwver different from FWVERSION in the device's
# running /etc/system.xml makes the device FLASH the image's boot/zImage and boot/cramfs.little
# (SWUpgrade::startFwUpgrade), so only use it together with a rootfs whose /etc/system.xml says N
# (scripts/build-rootfs.sh), or the install ends in "FW Upgrade is NOT OK".
#
# --diag renames /bin/pmplayer to pmplayer.real and installs scripts/device/pmplayer-wrapper.sh in its place
# (the one committed wrapper). It writes what the kernel sees of the drive to .Pacemaker/revival-diag.txt on
# each start, handles the SD adapter DMA flags (notes/sd-adapter-dma.md), sets LD_PRELOAD for the optional
# libraries present in the image (device/vocalfx), then execs the player (same PID, which is how
# pwrApplication tracks it).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
stock="$1"; out="$2"; shift 2
diag=0
fwver=""
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

add() {  # add <new image path> <local file> [mode, default 755]
  local dst="$1" src="$2" mode="${3:-755}" dir base u g
  dir="$(dirname "$dst")"; base="$(basename "$dst")"
  [[ -z "$(debugfs -R "stat \"$dst\"" "$img" 2>/dev/null)" ]] || { echo "already in image: $dst" >&2; exit 1; }
  read -r _ u g < <(meta "$dir")
  [[ -n "$u" ]] || { echo "no such directory in image: $dir" >&2; exit 1; }
  debugfs -w -f - "$img" > "$work/debugfs.log" 2>&1 <<EOF
cd "$dir"
write "$src" "$base"
sif "$base" mode 0100$mode
sif "$base" uid $u
sif "$base" gid $g
EOF
  [[ -n "$(debugfs -R "stat \"$dst\"" "$img" 2>/dev/null)" ]] || { cat "$work/debugfs.log" >&2; echo "could not add $dst" >&2; exit 1; }
  echo "added $dst ($(stat -c %s "$src") bytes, mode $mode, owner $u:$g)"
}

mkdirs() {  # mkdirs <new image dir>  (parent must exist; mode 0755, owner of the parent)
  local dst="$1" dir base u g
  dir="$(dirname "$dst")"; base="$(basename "$dst")"
  [[ -z "$(debugfs -R "stat \"$dst\"" "$img" 2>/dev/null)" ]] || { echo "already in image: $dst" >&2; exit 1; }
  read -r _ u g < <(meta "$dir")
  [[ -n "$u" ]] || { echo "no such directory in image: $dir" >&2; exit 1; }
  debugfs -w -f - "$img" > "$work/debugfs.log" 2>&1 <<EOF
cd "$dir"
mkdir "$base"
sif "$base" mode 040755
sif "$base" uid $u
sif "$base" gid $g
EOF
  [[ -n "$(debugfs -R "stat \"$dst\"" "$img" 2>/dev/null)" ]] || { cat "$work/debugfs.log" >&2; echo "could not create $dst" >&2; exit 1; }
  echo "created directory $dst (mode 755, owner $u:$g)"
}

for arg in "$@"; do
  case "$arg" in
    mkdir:/*) mkdirs "${arg#mkdir:}" ;;
    +/*=*) a="${arg#+}"; add "${a%%=*}" "${a#*=}" ;;
    =/*=*) a="${arg#=}"; add "${a%%=*}" "${a#*=}" 644 ;;
    --diag) diag=1 ;;
    --fwver=*) fwver="${arg#--fwver=}"; [[ $fwver =~ ^[0-9]+$ ]] || { echo "bad --fwver" >&2; exit 2; } ;;
    /*=*) replace "${arg%%=*}" "${arg#*=}" ;;
    *) echo "bad argument: $arg" >&2; exit 2 ;;
  esac
done

if [[ $diag == 1 ]]; then
  install -m 0755 "$here/scripts/device/pmplayer-wrapper.sh" "$work/pmplayer"
  sh -n "$work/pmplayer"
  grep -q '^exec \$REAL "\$@"$' "$work/pmplayer" || { echo "wrapper does not end in exec of the real player" >&2; exit 1; }
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
  echo "wrapped /bin/pmplayer with scripts/device/pmplayer-wrapper.sh $(sha256sum "$work/pmplayer" | cut -c1-12) (real player kept as pmplayer.real, mode $m)"
fi

e2fsck -fn "$img" > "$work/fsck.log" 2>&1 || { cat "$work/fsck.log"; echo "e2fsck found problems" >&2; exit 1; }
dumpe2fs -h "$img" 2>/dev/null | grep -E '^Filesystem features' > "$work/features.after"
cmp -s "$work/features.before" "$work/features.after" || { echo "filesystem features changed" >&2; exit 1; }
echo "e2fsck clean: $(tail -1 "$work/fsck.log")"

python3 - "$stock" "$img" "$out" "$here/scripts" "$fwver" <<'EOF'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("pfw", sys.argv[4] + "/pfw-trailer.py")
pfw = importlib.util.module_from_spec(spec); spec.loader.exec_module(pfw)
tail = open(sys.argv[1], "rb").read()[-512:]
vals, o = {}, 0
for name, n in pfw.FIELDS:
    vals[name] = pfw.field(tail[o:o + n]); o += n
image = open(sys.argv[2], "rb").read()
fw = sys.argv[5] or vals["fwver"]
open(sys.argv[3], "wb").write(pfw.build(image, vals["swver"], fw, vals["bootloaderver"]))
EOF
python3 "$here/scripts/pfw-trailer.py" "$out"
