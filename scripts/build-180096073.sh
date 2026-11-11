#!/usr/bin/env bash
# Build firmware 180096073 from the stock 180096069 .pfw: everything in 180096072 (R boot logo and kernel logo,
# lighter VU meters, full-capacity ide-disk, magenta power-on ring) plus the SD adapter DMA work
# (notes/sd-adapter-dma.md) and the Vocal FX (device/vocalfx/README.md).
# Runs as root in WSL Debian from the repo checkout:  wsl.exe -d Debian -u root bash scripts/build-180096073.sh
# Needs: e2fsprogs, util-linux-extra, python3, gcc-arm-linux-gnueabi (device/vocalfx/build.sh), the gitignored
# firmware trees (work/rootfs-180096069, vendor/firmware/dfw_180096069.pfw) and work/branding/blips.png
# (branding/vu_blips.py). The kernel logo patcher needs Pillow, which the project venv has and WSL's python3
# does not, so run this first on Windows (once; the result is reproducible, sha256 in the commit message):
#   tools/venv/Scripts/python scripts/patch-kernel-logo.py work/fw-180096069/boot/zImage \
#       branding/out/r-logo-240x320.png work/build/073/zImage-revival
# Outputs: work/build/pacemaker-revival-180096073.pfw, work/build/cramfs-180096073.little (intermediates in work/build/073).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
FW=180096073
B=work/build/073
STOCK=${STOCK:-vendor/firmware/dfw_180096069.pfw}
RT=work/rootfs-180096069
ZIMAGE=${ZIMAGE:-$B/zImage-revival}
[[ -f $STOCK && -f $ZIMAGE && -f work/branding/blips.png ]] || { echo "missing $STOCK, $ZIMAGE or work/branding/blips.png (see the header)" >&2; exit 1; }
mkdir -p "$B"

python3 scripts/patch-ide-disk.py "$RT/lib/modules/2.6.19.2/kernel/drivers/ide/ide-disk.ko" "$B/ide-disk-lba.ko"
python3 scripts/patch-pwr-ring.py "$RT/usr/local/bin/pwrApplication" "$B/pwrApplication-ring"
python3 scripts/patch-ide-dma.py "$RT/lib/modules/2.6.19.2/kernel/drivers/ide/ide-core.ko" "$B/ide-core-dma.ko" \
  "$B/pwrApplication-ring" "$B/pwrApplication-ring-nodma"
OUT="$here/$B/vocalfx" bash device/vocalfx/build.sh

# the stock image's /system.xml and /boot/system.xml are identical; only FWVERSION changes
size=$(stat -c %s "$STOCK"); head -c $((size - 512)) "$STOCK" > "$B/stock.ext3"
debugfs -R "dump /system.xml $B/system.xml-stock" "$B/stock.ext3" 2>/dev/null
debugfs -R "dump /boot/system.xml $B/boot_system.xml-stock" "$B/stock.ext3" 2>/dev/null
cmp "$B/system.xml-stock" "$B/boot_system.xml-stock"
sed -E "s/FWVERSION=\"[0-9]+\"/FWVERSION=\"$FW\"/" "$B/system.xml-stock" > "$B/system.xml-$FW"
cp "$B/system.xml-$FW" "$B/boot_system.xml-$FW"
grep -q "FWVERSION=\"$FW\"" "$B/system.xml-$FW"
rm -f "$B/stock.ext3"

# the lang files are the stock ones with string 236 "Wah" -> "Vocal" and nothing else
head -c $((size - 512)) "$STOCK" > "$B/stock.ext3"
for l in english japanese; do
  debugfs -R "dump /bin/resources/fonts/$l.lang $B/$l.lang-stock" "$B/stock.ext3" 2>/dev/null
  python3 - "$B/$l.lang-stock" "device/vocalfx/lang/$l.lang" <<'PY'
import sys
a, b = (open(p, "rb").read() for p in sys.argv[1:3])
assert a.count(b'236 "Wah"') == 1, "stock string 236 not found exactly once"
assert a.replace(b'236 "Wah"', b'236 "Vocal"') == b, "lang file differs from stock in more than string 236"
print("lang ok:", sys.argv[2])
PY
done
rm -f "$B/stock.ext3"

# rootfs: 072's three changes (system.xml version, ide-disk, ring) + DMA (ide-core, pwrApplication boot command,
# hddmount.sh, new /sbin/hdpboot)
bash scripts/build-rootfs.sh "$STOCK" "work/build/cramfs-$FW.little" "$FW" \
  /lib/modules/2.6.19.2/kernel/drivers/ide/ide-disk.ko="$B/ide-disk-lba.ko" \
  /lib/modules/2.6.19.2/kernel/drivers/ide/ide-core.ko="$B/ide-core-dma.ko" \
  /usr/local/bin/pwrApplication="$B/pwrApplication-ring-nodma" \
  /usr/local/bin/hddmount.sh=scripts/device/hddmount.sh \
  +/sbin/hdpboot=scripts/device/hdparm-boot.sh

# image: 072's files + the committed wrapper (--diag) + libvocalfx.so + lang files
bash scripts/build-pfw.sh "$STOCK" "work/build/pacemaker-revival-$FW.pfw" --diag --fwver=$FW \
  /bin/resources/symbols/blips.png=work/branding/blips.png \
  /bin/resources/symbols/boot.png=branding/out/r-logo-240x296.png \
  /boot/zImage="$ZIMAGE" \
  /boot/cramfs.little="work/build/cramfs-$FW.little" \
  /system.xml="$B/system.xml-$FW" \
  /boot/system.xml="$B/boot_system.xml-$FW" \
  /bin/resources/fonts/english.lang=device/vocalfx/lang/english.lang \
  /bin/resources/fonts/japanese.lang=device/vocalfx/lang/japanese.lang \
  +/bin/libvocalfx.so="$B/vocalfx/libvocalfx.so"
sha256sum "work/build/pacemaker-revival-$FW.pfw" "work/build/cramfs-$FW.little"
