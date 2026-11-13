#!/usr/bin/env bash
# Build firmware 180096074 from the stock 180096069 .pfw: everything in 180096073 (R boot logo and kernel logo,
# lighter VU meters, full-capacity ide-disk, magenta power-on ring, SD adapter DMA, Vocal FX) plus the About /
# Legal rebrand (notes/about-screen.md): the Model row says "Pacemaker Revival", the Support pane shows the
# project URL, the Legal notice is device/about/legal.txt, string 49 says "(c) Revival contributors", and the
# original Legal text plus GPL-2 and LGPL-2.1 ship in /bin/resources/licenses/. The Tonium sweep (notes/about-screen.md)
# takes the rest of the visible or tag "Tonium" text out as well: the kernel's machine name, the author tag of
# evrout.ko and pmic_input.ko, string 49 of the cramfs english.lang, and the 22 description strings of the 14 audio
# plugin libraries, plus the logo symbol name in the kernel's embedded build config (/proc/config.gz). Left as they
# are on purpose: the functional identifiers (toniumId, Tonium;Device;, com.tonium.*), the verbatim notices file,
# and boot/redboot.bin (byte-identical, bootloaderver 12838: this image never flashes the bootloader).
# scripts/sweep-tonium.py lists what is left in the built image and fails on anything unexpected.
# Runs as root in WSL Debian from the repo checkout (a git worktree is fine):
#   wsl.exe -d Debian -u root bash scripts/build-180096074.sh
# Needs what build-180096073.sh needs (e2fsprogs, util-linux-extra, python3, gcc-arm-linux-gnueabi), plus
# /usr/share/common-licenses/{GPL-2,LGPL-2.1} (Debian base-files). The gitignored inputs (work/, vendor/) are looked
# up from this checkout upwards, so a worktree under <repo>/.claude/worktrees/ reads and writes the main
# checkout's work/ and vendor/ (override with DATA=<dir containing work/ and vendor/>). The kernel is the one 073
# built (work/build/073/zImage-revival, the Revival logo; see build-180096073.sh) with the machine name changed by
# scripts/patch-kernel-name.py. That step recompresses with 7-Zip, so run it on Windows beforehand (python needs no
# Pillow for it); this script only verifies the result (WSL has no 7-Zip):
#   tools/venv/Scripts/python scripts/patch-kernel-name.py work/fw-180096069/boot/zImage \
#       work/build/073/zImage-revival work/build/074/zImage-revival-name
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
ZIMAGE=${ZIMAGE:-$B/zImage-revival-name}
LOGO_ZIMAGE=${LOGO_ZIMAGE:-$W/build/073/zImage-revival}
[[ -f $STOCK && -f $ZIMAGE && -f $W/branding/blips.png ]] || { echo "missing $STOCK, $ZIMAGE or $W/branding/blips.png (see the header)" >&2; exit 1; }
STOCK_REDBOOT_SHA=636cea0be6cdd6b358ad75019f246a8ee92383cdfadf3fa43906b0737f5d40db
for l in GPL-2 LGPL-2.1; do [[ -f /usr/share/common-licenses/$l ]] || { echo "missing /usr/share/common-licenses/$l" >&2; exit 1; }; done
mkdir -p "$B/licenses"
python3 scripts/patch-kernel-name.py --verify "$W/fw-180096069/boot/zImage" "$LOGO_ZIMAGE" "$ZIMAGE"

# Tonium sweep: author tags of the two input modules, the 14 plugin libraries
python3 scripts/patch-modinfo.py "$RT/lib/modules/2.6.19.2/kernel/drivers/input/evrout.ko" "$B/evrout.ko"
python3 scripts/patch-modinfo.py "$RT/lib/modules/2.6.19.2/kernel/drivers/input/misc/pmic_input.ko" "$B/pmic_input.ko"
python3 scripts/patch-plugin-strings.py "$W/fw-180096069/bin" "$B/plugins"
python3 scripts/patch-about.py lang "$RT/usr/local/share/fonts/english.lang" "$B/english.lang-cramfs"
cmp "$W/fw-180096069/bin/resources/fonts/english.lang" "$RT/usr/local/share/fonts/english.lang"   # same stock file as the image's

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
# hddmount.sh, new /sbin/hdpboot) + the Tonium sweep (the two author tags, string 49 of the cramfs english.lang that
# pwrApplication loads).
bash scripts/build-rootfs.sh "$STOCK" "$W/build/cramfs-$FW.little" "$FW" \
  /lib/modules/2.6.19.2/kernel/drivers/ide/ide-disk.ko="$B/ide-disk-lba.ko" \
  /lib/modules/2.6.19.2/kernel/drivers/ide/ide-core.ko="$B/ide-core-dma.ko" \
  /usr/local/bin/pwrApplication="$B/pwrApplication-ring-nodma" \
  /usr/local/bin/hddmount.sh=scripts/device/hddmount.sh \
  /lib/modules/2.6.19.2/kernel/drivers/input/evrout.ko="$B/evrout.ko" \
  /lib/modules/2.6.19.2/kernel/drivers/input/misc/pmic_input.ko="$B/pmic_input.ko" \
  /usr/local/share/fonts/english.lang="$B/english.lang-cramfs" \
  +/sbin/hdpboot=scripts/device/hdparm-boot.sh

# image: 073's files + the patched player (replaced before the --diag step renames it to pmplayer.real, so the
# wrapper runs the patched one) + the three lang files + the licences directory + the 14 plugin libraries
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
  /bin/libcontainer_aiffc.so="$B/plugins/libcontainer_aiffc.so" \
  /bin/libcontainer_flac.so="$B/plugins/libcontainer_flac.so" \
  /bin/libcontainer_id3.so="$B/plugins/libcontainer_id3.so" \
  /bin/libcontainer_mp3.so="$B/plugins/libcontainer_mp3.so" \
  /bin/libcontainer_mpeg4.so="$B/plugins/libcontainer_mpeg4.so" \
  /bin/libcontainer_ogg.so="$B/plugins/libcontainer_ogg.so" \
  /bin/libcontainer_snd.so="$B/plugins/libcontainer_snd.so" \
  /bin/libcontainer_wav.so="$B/plugins/libcontainer_wav.so" \
  /bin/libdecoder_aac.so="$B/plugins/libdecoder_aac.so" \
  /bin/libdecoder_au.so="$B/plugins/libdecoder_au.so" \
  /bin/libdecoder_flac.so="$B/plugins/libdecoder_flac.so" \
  /bin/libdecoder_mp3.so="$B/plugins/libdecoder_mp3.so" \
  /bin/libdecoder_pcm.so="$B/plugins/libdecoder_pcm.so" \
  /bin/libdecoder_vorbis.so="$B/plugins/libdecoder_vorbis.so" \
  +/bin/libvocalfx.so="$B/vocalfx/libvocalfx.so" \
  mkdir:/bin/resources/licenses \
  =/bin/resources/licenses/THIRD-PARTY-NOTICES.txt="$B/licenses/THIRD-PARTY-NOTICES.txt" \
  =/bin/resources/licenses/GPL-2="$B/licenses/GPL-2" \
  =/bin/resources/licenses/LGPL-2.1="$B/licenses/LGPL-2.1"

# --- checks on the built image: the bootloader stays stock and is never flashed, and the sweep finds nothing new ---
O="$W/build/pacemaker-revival-$FW.pfw"
chk="$(mktemp -d)"; trap 'rm -rf "$chk"' EXIT   # a Linux filesystem: fsck.cramfs --extract needs mknod
head -c $(( $(stat -c %s "$O") - 512 )) "$O" > "$chk/image.ext3"
mkdir -p "$chk/img"; debugfs -R "rdump / $chk/img" "$chk/image.ext3" >/dev/null
[[ $(sha256sum < "$chk/img/boot/redboot.bin" | cut -d' ' -f1) == "$STOCK_REDBOOT_SHA" ]] || { echo "boot/redboot.bin is not stock" >&2; exit 1; }
cmp "$chk/img/boot/zImage" "$ZIMAGE"
python3 scripts/pfw-trailer.py "$O" | tee "$chk/trailer.txt"
grep -q 'bootloaderver=12838 ' "$chk/trailer.txt" && grep -q "fwver=$FW " "$chk/trailer.txt" && grep -q 'md5=OK' "$chk/trailer.txt"
fsck.cramfs --extract="$chk/rootfs" "$W/build/cramfs-$FW.little" >/dev/null
python3 scripts/sweep-tonium.py "$chk/img" "$chk/rootfs"
sha256sum "$O" "$W/build/cramfs-$FW.little"
