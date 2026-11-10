#!/usr/bin/env bash
# Rebuild the device root filesystem (boot/cramfs.little inside a stock .pfw) with some files
# replaced and FWVERSION in /etc/system.xml set, then prove the result differs from the stock one
# only where intended. Must run as root on Linux (WSL: wsl -d Debian -u root -- ...), so that
# owners, device nodes and symlinks survive extraction; needs e2fsprogs and util-linux-extra.
#
#   scripts/build-rootfs.sh STOCK.pfw OUT.cramfs FWVERSION [/rootfs/path=local/file ...]
#
# The rootfs flash partition is 12.5 MiB (notes/FINDINGS.md section 5); the image must fit.
set -euo pipefail
stock="$1"; out="$2"; fwver="$3"; shift 3
[[ $EUID == 0 ]] || { echo "run as root (owners and device nodes must survive)" >&2; exit 1; }
[[ $fwver =~ ^[0-9]+$ ]] || { echo "FWVERSION must be a number" >&2; exit 2; }
PARTITION=$((12 * 1024 * 1024 + 512 * 1024))
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT

size=$(stat -c %s "$stock")
head -c $((size - 512)) "$stock" > "$work/image.ext3"
debugfs -R "dump /boot/cramfs.little $work/stock.cramfs" "$work/image.ext3" 2>/dev/null
fsck.cramfs "$work/stock.cramfs"
fsck.cramfs --extract="$work/root" "$work/stock.cramfs"

# every entry: path, type, mode, owner, size (files), link target, device numbers
listing() {
  (cd "$1" && find . -printf '%p\t%y\t%m\t%U:%G\t' -exec sh -c '
      for p; do
        case "$(stat -c %F "$p")" in
          "regular file"|"regular empty file") stat -c %s "$p" ;;
          "symbolic link") readlink "$p" ;;
          *special*) stat -c %t:%T "$p" ;;
          *) echo - ;;
        esac
      done' sh {} \; | sort)
}
listing "$work/root" > "$work/stock.list"

changed=()
grep -q "FWVERSION=\"$fwver\"" "$work/root/etc/system.xml" || changed+=(/etc/system.xml)
sed -i -E "s/FWVERSION=\"[0-9]+\"/FWVERSION=\"$fwver\"/" "$work/root/etc/system.xml"
grep -q "FWVERSION=\"$fwver\"" "$work/root/etc/system.xml"
for arg in "$@"; do
  dst="${arg%%=*}"; src="${arg#*=}"
  [[ -f "$work/root$dst" ]] || { echo "no such file in rootfs: $dst" >&2; exit 1; }
  cat "$src" > "$work/root$dst"  # keeps the stock file's mode and owner
  changed+=("$dst")
done

mkfs.cramfs -E -N little -n Compressed "$work/root" "$out"

# --- independent checks on the built image ---
fsck.cramfs "$out"
python3 - "$work/stock.cramfs" "$out" <<'EOF'
import struct, sys
def sb(p):
    d = open(p, "rb").read(64)
    magic, size, flags, _ = struct.unpack("<IIII", d[:16])
    return magic, flags, d[16:32], d[48:64], size
a, b = sb(sys.argv[1]), sb(sys.argv[2])
assert a[:4] == b[:4], f"superblock differs: stock {a} new {b}"
print(f"superblock: magic {a[0]:#x} flags {a[1]:#x} name {a[3].rstrip(bytes(1)).decode()} (same as stock); size {a[4]} -> {b[4]}")
EOF
(( $(stat -c %s "$out") < PARTITION )) || { echo "image does not fit the 12.5 MiB rootfs partition" >&2; exit 1; }
fsck.cramfs --extract="$work/check" "$out"
listing "$work/check" > "$work/new.list"
# metadata may differ only in the sizes of the changed files
filter() { grep -v -F -f <(printf './%s\t\n' "${changed[@]#/}") "$1"; }
diff <(filter "$work/stock.list") <(filter "$work/new.list") || { echo "metadata differs outside the changed files" >&2; exit 1; }
# contents: the built image must hold exactly the tree we packed, and differ from a pristine
# stock extraction only in the changed files
fsck.cramfs --extract="$work/pristine" "$work/stock.cramfs"
(cd "$work/check" && find . -type f | sed 's|^\./||' | sort) > "$work/files"
differs=()
while IFS= read -r p; do
  cmp -s "$work/root/$p" "$work/check/$p" || { echo "content differs after round trip: /$p" >&2; exit 1; }
  cmp -s "$work/pristine/$p" "$work/check/$p" || differs+=("/$p")
done < "$work/files"
[[ "$(printf '%s\n' "${differs[@]}" | sort)" == "$(printf '%s\n' "${changed[@]}" | sort -u)" ]] ||
  { echo "changed files: expected ${changed[*]}, got ${differs[*]}" >&2; exit 1; }
echo "verified: $(wc -l < "$work/new.list") entries, metadata identical to stock; contents differ only in: ${differs[*]}"
echo "image $(stat -c %s "$out") bytes, partition $PARTITION"
