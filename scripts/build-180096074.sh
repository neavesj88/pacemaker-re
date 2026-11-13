#!/usr/bin/env bash
# Build firmware 180096074 from the stock 180096069 .pfw: everything in 180096073 (R boot logo and kernel logo,
# lighter VU meters, full-capacity ide-disk, magenta power-on ring, SD adapter DMA, Vocal FX) plus the About /
# Legal rebrand (notes/about-screen.md): the Model row says "Pacemaker Revival", the Support pane shows the
# project URL, the Legal notice is device/about/legal.txt, string 49 says "(c) Revival contributors", and the
# original Legal text plus GPL-2 and LGPL-2.1 ship in /bin/resources/licenses/.
# Runs as root in WSL Debian from the repo checkout (a git worktree is fine):
#   wsl.exe -d Debian -u root bash scripts/build-180096074.sh
# Needs what build-180096073.sh needs (e2fsprogs, util-linux-extra, python3, gcc-arm-linux-gnueabi), plus
# /usr/share/common-licenses/{GPL-2,LGPL-2.1} (Debian base-files). The gitignored inputs (work/, vendor/) are looked
# up from this checkout upwards, so a worktree under <repo>/.claude/worktrees/ reads and writes the main
# checkout's work/ and vendor/ (override with DATA=<dir containing work/ and vendor/>). The kernel logo (zImage)
# is the one 073 built: work/build/073/zImage-revival (see build-180096073.sh for how to make it).
# Outputs: $DATA/work/build/pacemaker-revival-180096074.pfw, $DATA/work/build/cramfs-180096074.little
# (intermediates in $DATA/work/build/074).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
FW=180096074
if [[ -z "${DATA:-}" ]]; then
  DATA="$here"
  while [[ "$DATA" != / && ! -d "$DATA/work/rootfs-180096069/lib" ]]; do DATA="$(dirname "$DATA")"; done
fi
[[ -d "$DATA/work/rootfs-180096069/lib" ]] || { echo "cannot find work/rootfs-180096069 above $here (set DATA)" >&2; exit 1; }
W="$DATA/work"
B="$W/build/074"
STOCK=${STOCK:-$DATA/vendor/firmware/dfw_180096069.pfw}
RT="$W/rootfs-180096069"
ZIMAGE=${ZIMAGE:-$W/build/073/zImage-revival}
[[ -f $STOCK && -f $ZIMAGE && -f $W/branding/blips.png ]] || { echo "missing $STOCK, $ZIMAGE or $W/branding/blips.png (see the header)" >&2; exit 1; }
for l in GPL-2 LGPL-2.1; do [[ -f /usr/share/common-licenses/$l ]] || { echo "missing /usr/share/common-licenses/$l" >&2; exit 1; }; done
mkdir -p "$B/licenses"

python3 scripts/patch-ide-disk.py "$RT/lib/modules/2.6.19.2/kernel/drivers/ide/ide-disk.ko" "$B/ide-disk-lba.ko"
python3 scripts/patch-pwr-ring.py "$RT/usr/local/bin/pwrApplication" "$B/pwrApplication-ring"
python3 scripts/patch-ide-dma.py "$RT/lib/modules/2.6.19.2/kernel/drivers/ide/ide-core.ko" "$B/ide-core-dma.ko" \
  "$B/pwrApplication-ring" "$B/pwrApplication-ring-nodma"
OUT="$B/vocalfx" bash device/vocalfx/build.sh

# About / Legal: patched player + the stock Legal text, extracted from the stock player before it is overwritten
python3 scripts/patch-about.py --notices-out "$B/licenses/THIRD-PARTY-NOTICES.txt" "$W/fw-180096069/bin/pmplayer" "$B/pmplayer-about"
cp /usr/share/common-licenses/GPL-2 /usr/share/common-licenses/LGPL-2.1 "$B/licenses/"
[[ $(stat -c %s "$B/licenses/THIRD-PARTY-NOTICES.txt") == 13152 ]]

# the stock image's /system.xml and /boot/system.xml are identical; only FWVERSION changes
size=$(stat -c %s "$STOCK"); head -c $((size - 512)) "$STOCK" > "$B/stock.ext3"
debugfs -R "dump /system.xml $B/system.xml-stock" "$B/stock.ext3" 2>/dev/null
debugfs -R "dump /boot/system.xml $B/boot_system.xml-stock" "$B/stock.ext3" 2>/dev/null
cmp "$B/system.xml-stock" "$B/boot_system.xml-stock"
sed -E "s/FWVERSION=\"[0-9]+\"/FWVERSION=\"$FW\"/" "$B/system.xml-stock" > "$B/system.xml-$FW"
cp "$B/system.xml-$FW" "$B/boot_system.xml-$FW"
grep -q "FWVERSION=\"$FW\"" "$B/system.xml-$FW"

# the lang files are the stock ones with string 49 "(c) Tonium" -> "(c) Revival contributors" and, for english and
# japanese, string 236 "Wah" -> "Vocal" (Vocal FX), and nothing else
for l in english japanese korean; do
  debugfs -R "dump /bin/resources/fonts/$l.lang $B/$l.lang-stock" "$B/stock.ext3" 2>/dev/null
  python3 - "$B/$l.lang-stock" "device/about/lang/$l.lang" "$l" <<'PY'
import sys
a, b = (open(p, "rb").read() for p in sys.argv[1:3])
assert a.count(b'49 "\xc2\xa9 Tonium"') == 1, "stock string 49 not found exactly once"
want = a.replace(b'49 "\xc2\xa9 Tonium"', b'49 "\xc2\xa9 Revival contributors"')
if sys.argv[3] != "korean":
    assert a.count(b'236 "Wah"') == 1, "stock string 236 not found exactly once"
    want = want.replace(b'236 "Wah"', b'236 "Vocal"')
assert want == b, "lang file differs from stock in more than strings 49 (and 236)"
print("lang ok:", sys.argv[2])
PY
done
rm -f "$B/stock.ext3"

# rootfs: 072's three changes (system.xml version, ide-disk, ring) + DMA (ide-core, pwrApplication boot command,
# hddmount.sh, new /sbin/hdpboot). The cramfs english.lang is read by pwrApplication, not the About screen: left alone.
bash scripts/build-rootfs.sh "$STOCK" "$W/build/cramfs-$FW.little" "$FW" \
  /lib/modules/2.6.19.2/kernel/drivers/ide/ide-disk.ko="$B/ide-disk-lba.ko" \
  /lib/modules/2.6.19.2/kernel/drivers/ide/ide-core.ko="$B/ide-core-dma.ko" \
  /usr/local/bin/pwrApplication="$B/pwrApplication-ring-nodma" \
  /usr/local/bin/hddmount.sh=scripts/device/hddmount.sh \
  +/sbin/hdpboot=scripts/device/hdparm-boot.sh

# image: 073's files + the patched player (replaced before the --diag step renames it to pmplayer.real, so the
# wrapper runs the patched one) + the three lang files + the licences directory
bash scripts/build-pfw.sh "$STOCK" "$W/build/pacemaker-revival-$FW.pfw" --diag --fwver=$FW \
  /bin/pmplayer="$B/pmplayer-about" \
  /bin/resources/symbols/blips.png="$W/branding/blips.png" \
  /bin/resources/symbols/boot.png=branding/out/r-logo-240x296.png \
  /boot/zImage="$ZIMAGE" \
  /boot/cramfs.little="$W/build/cramfs-$FW.little" \
  /system.xml="$B/system.xml-$FW" \
  /boot/system.xml="$B/boot_system.xml-$FW" \
  /bin/resources/fonts/english.lang=device/about/lang/english.lang \
  /bin/resources/fonts/japanese.lang=device/about/lang/japanese.lang \
  /bin/resources/fonts/korean.lang=device/about/lang/korean.lang \
  +/bin/libvocalfx.so="$B/vocalfx/libvocalfx.so" \
  mkdir:/bin/resources/licenses \
  =/bin/resources/licenses/THIRD-PARTY-NOTICES.txt="$B/licenses/THIRD-PARTY-NOTICES.txt" \
  =/bin/resources/licenses/GPL-2="$B/licenses/GPL-2" \
  =/bin/resources/licenses/LGPL-2.1="$B/licenses/LGPL-2.1"
sha256sum "$W/build/pacemaker-revival-$FW.pfw" "$W/build/cramfs-$FW.little"
