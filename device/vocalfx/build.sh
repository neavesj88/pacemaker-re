#!/usr/bin/env bash
# Cross-build libvocalfx.so for the Pacemaker (i.MX31 ARM11, ARMv6, softfp, glibc 2.4) and audit it.
# Runs in WSL Debian:  wsl.exe -d Debian -u root bash device/vocalfx/build.sh
# Needs: gcc-arm-linux-gnueabi (apt), python3, binutils. Firmware trees (gitignored) are found by walking up
# from this checkout to the directory that contains work/rootfs-180096069 (override with ROOTFS=...).
# Output: $OUT (default <repo>/work/build/vocalfx): libvocalfx.so, sysroot/ (device libs, for qemu tests), build-info.txt
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
if [[ -z "${ROOTFS:-}" ]]; then
  d="$repo"
  while [[ "$d" != / && ! -d "$d/work/rootfs-180096069/lib" ]]; do d="$(dirname "$d")"; done
  ROOTFS="$d/work/rootfs-180096069"
fi
[[ -f "$ROOTFS/lib/libc-2.4.so" ]] || { echo "device rootfs not found: $ROOTFS" >&2; exit 1; }
OUT="${OUT:-$repo/work/build/vocalfx}"
CROSS="${CROSS:-arm-linux-gnueabi}"
command -v "$CROSS-gcc" >/dev/null || { echo "missing $CROSS-gcc: apt-get install gcc-arm-linux-gnueabi" >&2; exit 1; }
sys="$OUT/sysroot"
rm -rf "$OUT"; mkdir -p "$sys/lib" "$sys/mnt/public/.Pacemaker" "$sys/tmp" "$OUT/obj"

# Device libs into a sysroot. The rootfs tree comes from a Windows extraction: its symlinks are small text
# files holding the target name, so resolve them by hand.
for n in ld-linux.so.3 libc.so.6 libdl.so.2 libm.so.6 libpthread.so.0 librt.so.1 libgcc_s.so.1; do
  f="$ROOTFS/lib/$n"; [[ -f "$f" ]] || { echo "missing $f" >&2; exit 1; }
  if [[ $(stat -c %s "$f") -lt 64 ]]; then f="$ROOTFS/lib/$(tr -d '\r\n' < "$f")"; fi
  cp "$f" "$sys/lib/$n"
done

CFLAGS=(-std=gnu11 -O2 -Wall -Wextra -D_GNU_SOURCE
        -march=armv6 -mtune=arm1136jf-s -mfpu=vfp -mfloat-abi=softfp -marm   # ARM mode: the player is ARM, not Thumb
        -fPIC -fvisibility=hidden -fno-stack-protector -U_FORTIFY_SOURCE
        -U_TIME_BITS -U_FILE_OFFSET_BITS                                     # Debian defines both = 64; the device libc 2.4 has no *64time symbols
        -funwind-tables                                                      # .ARM.exidx: a C++ exception thrown inside the stock process() can unwind through vocal_process
        "-ffile-prefix-map=$repo=." ${EXTRA_CFLAGS:-})
for s in vocal_dsp vocalfx; do "$CROSS-gcc" "${CFLAGS[@]}" -I"$here" -c "$here/$s.c" -o "$OUT/obj/$s.o"; done

# -nostdlib + explicit device libs: the symbol versions come from the DEVICE libc/libdl/libm/libpthread, so
# the linker cannot record a version the device does not define (checked again below).
"$CROSS-gcc" -shared -nostdlib -o "$OUT/libvocalfx.so" "$OUT/obj/vocal_dsp.o" "$OUT/obj/vocalfx.o" \
  -Wl,--as-needed -Wl,--hash-style=sysv -Wl,-z,now -Wl,-z,noexecstack -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--build-id=none -Wl,--no-undefined \
  -Wl,-soname,libvocalfx.so -Wl,-rpath-link,"$sys/lib" \
  "$sys/lib/libpthread.so.0" "$sys/lib/libdl.so.2" "$sys/lib/libm.so.6" "$sys/lib/libgcc_s.so.1" "$sys/lib/libc.so.6"
"$CROSS-strip" --strip-unneeded "$OUT/libvocalfx.so"

# e_flags: the device libs carry 0x04000002 (EABI v4). ld.so 2.4 ignores e_flags; match them anyway.
python3 - "$OUT/libvocalfx.so" "$sys/lib/libc.so.6" <<'PY'
import struct, sys
ours, dev = sys.argv[1:3]
want = struct.unpack_from('<I', open(dev, 'rb').read(64), 36)[0]
b = bytearray(open(ours, 'rb').read())
assert b[:4] == b'\x7fELF' and b[4] == 1 and b[5] == 1 and struct.unpack_from('<H', b, 18)[0] == 40
struct.pack_into('<I', b, 36, want)
open(ours, 'wb').write(b)
print('e_flags set to 0x%08x (device libc)' % want)
PY

# ---- audit ----
{
echo "== file"; ls -l "$OUT/libvocalfx.so"; sha256sum "$OUT/libvocalfx.so"
echo "== header"; "$CROSS-readelf" -h "$OUT/libvocalfx.so" | grep -E 'Class|Data|Type|Machine|Flags'
echo "== dynamic (NEEDED, SONAME, flags)"; "$CROSS-readelf" -d "$OUT/libvocalfx.so" | grep -E 'NEEDED|SONAME|FLAGS|HASH|INIT'
echo "== version needs"; "$CROSS-readelf" -V "$OUT/libvocalfx.so" | sed -n '/Version needs/,$p'
echo "== undefined dynamic symbols"; "$CROSS-readelf" --dyn-syms -W "$OUT/libvocalfx.so" | awk '$7=="UND" && $8!=""'
echo "== program headers"; "$CROSS-readelf" -l -W "$OUT/libvocalfx.so" | grep -E 'LOAD|DYNAMIC|GNU_STACK|ARM_EXIDX|TLS'
} > "$OUT/build-info.txt"

python3 - "$OUT/libvocalfx.so" "$sys/lib" "$CROSS" <<'PY'
import glob, re, subprocess, sys
so, libdir, cross = sys.argv[1:4]
rd = lambda p: subprocess.run([cross + '-readelf', '--dyn-syms', '-W', p], capture_output=True, text=True, check=True).stdout.splitlines()
need = set(); bad = []
for l in rd(so):
    f = l.split()
    if len(f) >= 8 and f[6] == 'UND' and f[7] != '':
        if '@' not in f[7]: bad.append('unversioned undefined symbol ' + f[7])
        else: need.add(f[7].split(' ')[0])
have = set()
for p in glob.glob(libdir + '/*'):
    for l in rd(p):
        f = l.split()
        if len(f) >= 8 and f[6] != 'UND' and '@' in f[7]: have.add(f[7].replace('@@', '@').split(' ')[0])
for n in sorted(need):
    if n not in have: bad.append('device libs do not define ' + n)
vers = sorted({n.split('@')[1] for n in need})
print('symbol versions used:', ', '.join(vers))
print('undefined symbols checked against device libs:', len(need))
if any(v not in ("GLIBC_2.4", "GCC_3.5") for v in vers): bad.append("version other than GLIBC_2.4 / GCC_3.5: %s" % vers)
if any(v == "GCC_3.5" and not n.startswith("__aeabi_unwind_cpp_pr") for n in need for v in [n.split("@")[1]]): bad.append("GCC_3.5 symbol other than the unwind personality routines")
if bad: print('AUDIT FAILED:\n  ' + '\n  '.join(bad)); sys.exit(1)
print('AUDIT OK: every needed symbol@version is defined by the device libs')
PY
cat "$OUT/build-info.txt" | sed -n '1,4p'
echo "built $OUT/libvocalfx.so"
