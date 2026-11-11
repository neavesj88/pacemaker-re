#!/usr/bin/env bash
# Verification for libvocalfx: native DSP test + benchmark, then (if build.sh ran) the ARM .so under qemu-user
# with the DEVICE's ld.so and libc. Runs in WSL Debian:  wsl.exe -d Debian -u root bash device/vocalfx/test.sh
# Needs: gcc, gcc-arm-linux-gnueabi, qemu-user-static, python3.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
OUT="${OUT:-$repo/work/build/vocalfx}"
T="$OUT/test"; mkdir -p "$T"

echo "######## native DSP test (x86, gcc -O2)"
gcc -std=gnu11 -O2 -Wall -Wextra -o "$T/dsp_test" "$here/test/dsp_test.c" "$here/test/ref_double.c" "$here/vocal_dsp.c" -lm
gcc -std=gnu11 -O2 -DVFX_CHECK_DENORM -o "$T/dsp_test_dn" "$here/test/dsp_test.c" "$here/test/ref_double.c" "$here/vocal_dsp.c" -lm
"$T/dsp_test_dn"

echo; echo "######## static cost of the DSP loop on ARM (objdump of the cross-built object)"
CROSS="${CROSS:-arm-linux-gnueabi}"
if [[ -f "$OUT/obj/vocal_dsp.o" ]]; then
  "$CROSS-objdump" -d --no-show-raw-insn "$OUT/obj/vocal_dsp.o" | awk '/<vfx_process>:/{f=1;next} /^$/{f=0} f && /^ +[0-9a-f]+:/ {n++; if ($2 ~ /^v/ || $2 ~ /^f/) v++} END{printf "vfx_process: %d instructions, %d VFP (whole function incl. setup; the per-sample loop is most of it)\n", n, v}'
fi


echo; echo "######## wrapper snippet (POSIX sh, paths redirected into a temp dir)"
W="$(mktemp -d)"; trap 'rm -rf "$W"' EXIT
mkdir -p "$W/pmds/bin" "$W/mnt/public/.Pacemaker"
sed "s|/pmds|$W/pmds|g; s|/mnt/public|$W/mnt/public|g" "$here/wrapper-snippet.sh" > "$W/snippet.sh"
wcheck() { local want="$1" label="$2" got; got="$(sh -c '. "$1"; printf "%s" "${LD_PRELOAD-}"' sh "$W/snippet.sh")"; got="${got//$W/}"
  if [[ "$got" == "$want" ]]; then echo "  ok   $label: LD_PRELOAD='${got}'"; else echo "  FAIL $label: got '${got}' want '${want}'"; wfail=1; fi; }
wfail=0
wcheck "" "library not in the image"
touch "$W/pmds/bin/libvocalfx.so"; wcheck "/pmds/bin/libvocalfx.so" "library present"
touch "$W/mnt/public/.Pacemaker/revival-no-preload"; wcheck "" "kill switch present"
rm "$W/mnt/public/.Pacemaker/revival-no-preload"; wcheck "/pmds/bin/libvocalfx.so" "kill switch removed"
sh -n "$here/wrapper-snippet.sh" && echo "  ok   sh -n"; (command -v busybox >/dev/null && busybox sh -n "$here/wrapper-snippet.sh" && echo "  ok   busybox sh -n") || true
[[ $wfail == 0 ]] || { echo "WRAPPER SNIPPET FAILED"; exit 1; }

########## ARM .so under qemu-user with the DEVICE's ld.so and glibc 2.4
echo; echo "######## ARM build under qemu-user (device ld-2.4.so + libc-2.4.so)"
sys="$OUT/sysroot"
[[ -f "$OUT/libvocalfx.so" && -d "$sys/lib" ]] || { echo "run build.sh first"; exit 1; }
command -v qemu-arm-static >/dev/null || { echo "qemu-arm-static missing (apt-get install qemu-user-static)"; exit 1; }
mkdir -p "$sys/pmds/bin"
cp "$OUT/libvocalfx.so" "$sys/pmds/bin/libvocalfx.so"
CFLAGS=(-std=gnu11 -O2 -Wall -Wextra -march=armv6 -mfpu=vfp -mfloat-abi=softfp -marm -fno-pie
        -fno-stack-protector -U_FORTIFY_SOURCE -U_TIME_BITS -U_FILE_OFFSET_BITS -fno-unwind-tables -fno-asynchronous-unwind-tables)
LINK=(-no-pie -nostartfiles -nostdlib -Wl,--hash-style=sysv -Wl,--dynamic-linker=/lib/ld-linux.so.3 -Wl,-rpath-link,"$sys/lib")
# the stub's fake firmware symbols go to the firmware's real addresses
"$CROSS-gcc" "${CFLAGS[@]}" "${LINK[@]}" -Wl,--export-dynamic \
  -Wl,--section-start=.peq_proc=0x4f524c -Wl,--section-start=.peq_d1=0x4f5730 -Wl,--section-start=.peq_d0=0x4f58d8 \
  -Wl,--section-start=.peq_vt=0x877e48 -o "$sys/pmds/bin/pmplayer.real" "$here/test/stub_pmplayer.c" \
  "$sys/lib/libm.so.6" "$sys/lib/libc.so.6"
"$CROSS-gcc" "${CFLAGS[@]}" "${LINK[@]}" -o "$sys/pmds/bin/dlopen_test" "$here/test/dlopen_test.c" "$sys/lib/libdl.so.2" "$sys/lib/libc.so.6"
"$CROSS-readelf" -l -W "$sys/pmds/bin/pmplayer.real" | grep -E 'LOAD|INTERP'
"$CROSS-nm" -D "$sys/pmds/bin/pmplayer.real" | grep -E 'PeqAudio|m_sampleRate'

Q() { qemu-arm-static -L "$sys" "$@"; }   # guest absolute paths (/lib, /mnt/public, /pmds) resolve inside the sysroot
FLAG="$sys/mnt/public/.Pacemaker/revival-vocal"; LOG="$sys/mnt/public/.Pacemaker/revival-vocal.log"
STUB="$sys/pmds/bin/pmplayer.real"
# qemu-user maps a guest path into the sysroot only if the file exists there, so create the (empty) log first
run() { : > "$LOG"; Q -E LD_PRELOAD=/pmds/bin/libvocalfx.so "$STUB" "$@" | grep RESULT; }
expect() {   # expect <label> <awk-condition on key=value fields> -- reads the RESULT line on stdin
  local label="$1" cond="$2" line; line="$(cat)"
  if awk -v RS=' ' -F= '{v[$1]=$2} END{exit !('"$cond"')}' <<<"$line"; then echo "  ok   $label: ${line#RESULT }"; else echo "  FAIL $label: $line"; fails=1; fi
}
fails=0
rm -f "$FLAG"

echo "-- dlopen test (not pmplayer.real: must load, must not patch)"
Q "$sys/pmds/bin/dlopen_test" /pmds/bin/libvocalfx.so | tee "$T/dlopen.out"; grep -q "DLOPEN OK version=0.1.0 status=-1" "$T/dlopen.out" || { echo "  FAIL dlopen"; fails=1; }

echo "-- control: no preload, stub alone shows the stock Wah path (gain 0.5)"
Q "$STUB" tone 100 120 | grep RESULT | expect "no preload" 'v["ratio"]>0.499 && v["ratio"]<0.501 && v["stock_hits"]==120'

echo "-- flag file ABSENT: hook installed, observer only, stock Wah untouched"
run tone 100 120 | expect "observer" 'v["ratio"]>0.499 && v["ratio"]<0.501 && v["stock_hits"]==120 && v["slot"]!="" '
[[ -s "$LOG" ]] && grep -q "status=1" "$LOG" && grep -q "dsp=off" "$LOG" && echo "  ok   log written, status=1 dsp=off" || { echo "  FAIL log"; cat "$LOG"; fails=1; }
echo "     (the RESULT lines below show the slot word read by the program: 004f524c = stock, anything else = our function)"

echo "-- flag file PRESENT"
echo > "$FLAG"
run tone 0 200   | expect "neutral (0)"        'v["ratio"]>0.9999 && v["ratio"]<1.0001 && v["stock_hits"]==0'
run tone 100 200 | expect "focus +100 @1kHz"   'v["ratio"]>1.38 && v["ratio"]<1.42 && v["stock_hits"]==0 && v["orig_calls"]==200'
run tone -100 200| expect "cut -100 @1kHz"     'v["ratio"]<0.01 && v["stock_hits"]==0'
run tone 50 200  | expect "focus +50 @1kHz"    'v["ratio"]>1.17 && v["ratio"]<1.23'
run side 100 200 | expect "focus +100, side"   'v["ratio"]<0.001'
run side -100 200| expect "cut -100, side"     'v["ratio"]>0.97 && v["ratio"]<1.03'
run bass 100 200 | expect "focus +100, 60 Hz" 'v["ratio"]<0.06'
run tone 0.5 200 | expect "dead zone (0.5)"    'v["ratio"]>0.9999 && v["ratio"]<1.0001 && v["stock_hits"]==0'
run silent 100 50| expect "silent buffer"     'v["ratio"]>0.9999 && v["ratio"]<1.0001 && v["stock_hits"]==0'
run nonalias 100 50 | expect "in != out -> stock" 'v["ratio"]>0.499 && v["ratio"]<0.501 && v["stock_hits"]==50'
echo "-- invert flag ('i' in the flag file)"
printf 'invert\n' > "$FLAG"
run tone 100 200 | expect "invert: +100 is a cut" 'v["ratio"]<0.01'
rm -f "$FLAG"
echo "-- log thread: 300 blocks, then the player lives on for 7 s so the periodic writer fires"
echo > "$FLAG"
run tone 100 300 7 | expect "periodic log run" 'v["ratio"]>1.38 && v["ratio"]<1.42'
grep -q '\[run\]' "$LOG" && grep -q "calls=300 dsp_blocks=300" "$LOG" && echo "  ok   periodic log written" || { echo "  FAIL periodic log"; cat "$LOG"; fails=1; }
echo "-- sample log:"; cat "$LOG"

echo "-- fail-safe: a stub whose vtable sits at another address (as in a different firmware) must NOT be patched"
mkdir -p "$sys/tmp/bad"
"$CROSS-gcc" "${CFLAGS[@]}" "${LINK[@]}" -Wl,--export-dynamic   -Wl,--section-start=.peq_proc=0x4f524c -Wl,--section-start=.peq_d1=0x4f5730 -Wl,--section-start=.peq_d0=0x4f58d8   -Wl,--section-start=.peq_vt=0x877e58 -o "$sys/tmp/bad/pmplayer.real" "$here/test/stub_pmplayer.c" "$sys/lib/libm.so.6" "$sys/lib/libc.so.6"
echo > "$FLAG"; : > "$LOG"
Q -E LD_PRELOAD=/pmds/bin/libvocalfx.so "$sys/tmp/bad/pmplayer.real" tone 100 60 | grep RESULT | expect "mismatch -> untouched stock" 'v["stock_hits"]==60 && v["ratio"]>0.499 && v["ratio"]<0.501'
grep -q "status=-3" "$LOG" && echo "  ok   log says status=-3 (stock bytes differ, nothing patched)" || { echo "  FAIL mismatch log"; cat "$LOG"; fails=1; }
rm -f "$FLAG"

echo; [[ $fails == 0 ]] && echo "QEMU TESTS OK" || { echo "QEMU TESTS FAILED"; exit 1; }
