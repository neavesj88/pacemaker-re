#!/usr/bin/env bash
# Runs scripts/device/pmplayer-wrapper.sh and hdparm-boot.sh under dash against stubbed hdparm / dmesg / dd
# and a fake /proc/ide + FAT tree, for every DMA path (no device involved). Linux or WSL:
#   wsl -d Debian -- bash scripts/device/test-dma-wrapper.sh
# Needs dash, procps (pkill), coreutils, gawk. Exit status is the number of failed checks.
#
# With CHROOT=/path/to/extracted/rootfs (fsck.cramfs --extract of the firmware rootfs, plus qemu-arm-static in
# its usr/bin and a registered arm binfmt) the same checks run under the device's own /bin/sh and busybox 1.1.3
# applets (DEVSHELL=/bin/ash for busybox ash instead of the default /bin/sh = bash): the fake tree lives in $CHROOT/tmp/fts and is bind-mounted at the same path on both sides.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
W="$HERE/pmplayer-wrapper.sh"
FB=/tmp/fts; mkdir -p "$FB"
CHROOT=${CHROOT:-}
RUN=dash; WATCH=0.4; WAIT=1.5; SPATH="/usr/bin:/bin"
if [ -n "$CHROOT" ]; then
  mkdir -p "$CHROOT$FB"; mountpoint -q "$CHROOT$FB" || mount --bind "$FB" "$CHROOT$FB"
  mountpoint -q "$CHROOT/proc" || mount -t proc proc "$CHROOT/proc"
  mountpoint -q "$CHROOT/dev" || mount --bind /dev "$CHROOT/dev"
  cp "$HERE/pmplayer-wrapper.sh" "$HERE/hdparm-boot.sh" "$FB/"; W="$FB/pmplayer-wrapper.sh"
  RUN="chroot $CHROOT ${DEVSHELL:-/bin/sh}"; WATCH=1; WAIT=3; SPATH="/usr/bin:/bin:/sbin:/usr/sbin"
fi
B="$HERE/hdparm-boot.sh"; [ -n "$CHROOT" ] && B="$FB/hdparm-boot.sh"
fails=0
ck() { if eval "$2"; then echo "  ok   $1"; else echo "  FAIL $1"; fails=$((fails + 1)); fi; }

setup() {  # setup <name> [claimed]: fresh fake device in $F
  F="$(mktemp -d -p "$FB")"; S="$F/stubs"; mkdir -p "$S" "$F/ide" "$F/pub/.Pacemaker" "$F/priv" "$F/tmp"
  head -c 5242880 /dev/urandom > "$F/priv/active_sw_file"
  local w49=6c61; [ "${2:-}" = claimed ] && w49=0f00
  local ws=""; for i in $(seq 0 255); do if [ $i = 49 ]; then ws="$ws $w49"; else ws="$ws 0000"; fi; done
  echo "${ws# }" > "$F/ide/identify"
  printf 'using_dma 0\nunmaskirq 0\nmultcount 0\nio_32bit 0\ncurrent_speed 0\npio_mode 0\n' > "$F/ide/settings"
  : > "$F/dmesg"; : > "$F/calls"; : > "$F/real-ran"
  cat > "$S/hdparm" <<'EOS'
#!/bin/sh
# stub: state in $F/ide/settings, calls in $F/calls; knobs are files in $F (hang, refuse, errd1, warn, nopatch)
echo "hdparm $*" >> $F/calls
set_dma() { sed -i "s/^using_dma .*/using_dma $1/" $F/ide/settings; }
cur() { awk '$1=="using_dma"{print $2}' $F/ide/settings; }
case "$1" in
  -d) if [ "$2" = 1 ]; then
        if [ -f $F/refuse ]; then echo "HDIO_SET_DMA failed: Operation not permitted"; exit 1; fi
        set_dma 1; [ -f $F/errd1 ] && echo "hda: dma_intr: status=0x51 { DriveReady SeekComplete Error }" >> $F/dmesg
      else set_dma 0; fi ;;
  -X) [ -f $F/hang ] && exec sleep 1000
      case "$2" in udma[3-5]) [ -f $F/warn ] && echo "hda: Speed warnings UDMA 3/4/5 is not functional." >> $F/dmesg ;; esac ;;
  -t|-tT|-I|-i|-u1) echo "stub hdparm $1" ;;
  *) echo "stub hdparm" ;;
esac
exit 0
EOS
  cat > "$S/dd" <<'EOS'
#!/bin/sh
# stub: real dd, but while using_dma is 1 reads of the image (corrupt_read) or writes (corrupt_write) are damaged
dma=`awk '$1=="using_dma"{print $2}' $F/ide/settings`
out=; for a in "$@"; do case $a in of=*) out=${a#of=} ;; esac; done
if [ "$dma" = 1 ] && [ -z "$out" ] && [ -f $F/corrupt_read ]; then /bin/dd "$@" | { cat; echo x; }; exit 0; fi
/bin/dd "$@"; r=$?
if [ "$dma" = 1 ] && [ -n "$out" ] && [ -f $F/corrupt_write ]; then
  case "$out" in $F/pub/*) echo x >> "$out" ;; esac
fi
exit $r
EOS
  printf '#!/bin/sh\ncat $F/dmesg\n' > "$S/dmesg"
  [ -z "$CHROOT" ] && printf '#!/bin/sh\nexec pkill -x "$@"\n' > "$S/killall"
  printf '#!/bin/sh\necho "REAL PLAYER $*" > $F/real-ran\n' > "$F/real"
  chmod +x "$S"/* "$F/real"
  export F REVIVAL_P="$F/pub/.Pacemaker" REVIVAL_H="$F/ide" REVIVAL_TMP="$F/tmp" REVIVAL_IMG="$F/priv/active_sw_file" \
    REVIVAL_REAL="$F/real" REVIVAL_DROP="$F/drop" REVIVAL_PATH="$S:$SPATH" REVIVAL_WATCH=$WATCH REVIVAL_LIMIT=60
}
run() { $RUN "$W" "$@" > "$F/out" 2>&1; }
diag() { cat "$REVIVAL_P/revival-diag.txt"; }
calls() { grep -c -- "$1" "$F/calls"; }
done_() { pkill -f "$F" 2>/dev/null; sleep 0.3; rm -rf "$F"; }
newboot() { rm -f "$F/tmp/revival-diag-boot" "$F/tmp/revival-dma-ok" "$F/tmp/revival-dma-refused"; pkill -f "revival-watch" 2>/dev/null; rm -f "$F/tmp/revival-watch.pid"; }

echo "1. no flag, drive without DMA bit: stays PIO, player starts"
setup a; run
ck "player exec'd" "grep -q 'REAL PLAYER' $F/real-ran"
ck "no -d 1 call" "[ \$(calls '-d 1') = 0 ]"
ck "diag says does NOT report" "diag | grep -q 'does NOT report DMA'"
ck "PIO baseline benchmark ran" "diag | grep -q 'PIO baseline'"
done_

echo "2. no flag, DMA found on (old pwrApplication + patched ide-core): turned off"
setup a; sed -i 's/^using_dma .*/using_dma 1/' "$F/ide/settings"; run
ck "-d 0 issued" "[ \$(calls '-d 0') = 1 ]"
ck "using_dma 0 afterwards" "grep -q '^using_dma 0' $F/ide/settings"
done_

echo "3. drive that reports DMA: stock -d 1 and -X udma5 when boot did not"
setup a claimed; run
ck "-d 1 and -X udma5" "[ \$(calls '-d 1') = 1 ] && [ \$(calls '-X udma5') = 1 ]"
ck "DMA benchmark label" "diag | grep -q 'DMA (udma5)'"
done_

echo "4. flag, clean DMA (CRLF flag file with a level)"
setup a; printf 'udma1\r\n' > "$REVIVAL_P/revival-dma"; run
ck "level udma1 used" "[ \$(calls '-X udma1') = 1 ]"
ck "DMA ok verdict" "diag | grep -q 'DMA ok (udma1)'"
ck "trying marker removed, no failed file" "[ ! -e $REVIVAL_P/revival-dma-trying ] && [ ! -e $REVIVAL_P/revival-dma-failed ]"
ck "ok file holds the level" "[ \$(cat $F/tmp/revival-dma-ok) = udma1 ]"
ck "benchmark label is the level" "diag | grep -q 'DMA (udma1)'"
ck "no leftover test files" "[ -z \"\$(ls -A $F/priv | grep revival-dmatest)\" ] && [ -z \"\$(ls -A $REVIVAL_P | grep revival-dmatest)\" ]"
ck "watcher started" "kill -0 \$(cat $F/tmp/revival-watch.pid)"
ck "player exec'd" "grep -q 'REAL PLAYER' $F/real-ran"
echo "4b. watcher re-enables DMA after a standby/wake dropped it"
sed -i 's/^using_dma .*/using_dma 0/' "$F/ide/settings"; sleep $WAIT
ck "using_dma 1 again" "grep -q '^using_dma 1' $F/ide/settings"
ck "log says so" "diag | grep -q 'using_dma dropped to 0'"
echo "4c. watcher gives DMA up on the first new IDE error line"
echo "hda: dma_intr: status=0x51 { DriveReady SeekComplete Error }" >> "$F/dmesg"; sleep $WAIT
ck "using_dma 0" "grep -q '^using_dma 0' $F/ide/settings"
ck "failed file written" "[ -f $REVIVAL_P/revival-dma-failed ] && grep -q 'IDE error' $REVIVAL_P/revival-dma-failed"
ck "ok file gone" "[ ! -e $F/tmp/revival-dma-ok ]"
echo "4d. a later boot with the failed file never tries DMA again"
newboot; : > "$F/calls"; run
ck "no -d 1" "[ \$(calls '-d 1') = 0 ]"
ck "diag says why" "diag | grep -q 'DMA stays off'"
done_

echo "5. flag, restart after a verified start with DMA lost (no watcher interval wait)"
setup a; : > "$REVIVAL_P/revival-dma"; REVIVAL_WATCH=1000 run
sed -i 's/^using_dma .*/using_dma 0/' "$F/ide/settings"; : > "$F/calls"; REVIVAL_WATCH=1000 run
ck "re-enabled without a new test (no md5 passes)" "diag | sed -n '/player restart/,\$p' | grep -q 're-enabling DMA' && ! diag | sed -n '/player restart/,\$p' | grep -q 'PIO md5'"
pkill -f "$F" 2>/dev/null; done_

echo "6. flag, read data differs under DMA"
setup a; : > "$REVIVAL_P/revival-dma"; touch "$F/corrupt_read"; run
ck "FAILED verdict names the read" "diag | grep -q 'read md5 differs'"
ck "back to PIO" "grep -q '^using_dma 0' $F/ide/settings"
ck "failed file, no trying file, no ok file" "[ -f $REVIVAL_P/revival-dma-failed ] && [ ! -e $REVIVAL_P/revival-dma-trying ] && [ ! -e $F/tmp/revival-dma-ok ]"
ck "page cache dropped after the fallback (drop file written last)" "[ -f $F/drop ]"
: > "$F/calls"; run
ck "player restart does not retry" "[ \$(calls '-d 1') = 0 ]"
done_

echo "7. flag, FAT write corrupted under DMA (reads fine)"
setup a; : > "$REVIVAL_P/revival-dma"; touch "$F/corrupt_write"; run
ck "FAILED names the FAT write" "diag | grep -q 'write/read-back on the FAT volume'"
ck "PIO" "grep -q '^using_dma 0' $F/ide/settings"
done_

echo "8. flag, IDE error line appears while DMA is on"
setup a; : > "$REVIVAL_P/revival-dma"; touch "$F/errd1"; run
ck "FAILED names the error line" "diag | grep -q 'new IDE error line'"
ck "PIO" "grep -q '^using_dma 0' $F/ide/settings"
done_

echo "9. flag, kernel refuses -d 1 (ide-core not patched)"
setup a; : > "$REVIVAL_P/revival-dma"; touch "$F/refuse"; run
ck "refusal reported, no failed file, refused marker" "diag | grep -q 'kernel refused DMA' && [ ! -e $REVIVAL_P/revival-dma-failed ] && [ -f $F/tmp/revival-dma-refused ]"
: > "$F/calls"; run
ck "restart does not try again" "[ \$(calls '-d 1') = 0 ] && diag | grep -q 'refused DMA earlier'"
done_

echo "10. trying marker left by an attempt that hung or lost power"
setup a; : > "$REVIVAL_P/revival-dma"; echo "attempt at udma2" > "$REVIVAL_P/revival-dma-trying"; run
ck "turned into the failed file" "[ -f $REVIVAL_P/revival-dma-failed ] && [ ! -e $REVIVAL_P/revival-dma-trying ] && grep -q 'never finished' $REVIVAL_P/revival-dma-failed"
ck "no attempt" "[ \$(calls '-d 1') = 0 ]"
ck "player exec'd" "grep -q 'REAL PLAYER' $F/real-ran"
done_

echo "11. hdparm -X hangs: time limit, DMA off, player starts"
setup a; : > "$REVIVAL_P/revival-dma"; touch "$F/hang"; REVIVAL_LIMIT=3 run
ck "player exec'd" "grep -q 'REAL PLAYER' $F/real-ran"
ck "TIMEOUT in the diag" "diag | grep -q TIMEOUT"
ck "-d 0 issued" "[ \$(calls '-d 0') -ge 1 ]"
ck "failed file written, trying file gone" "[ -f $REVIVAL_P/revival-dma-failed ] && [ ! -e $REVIVAL_P/revival-dma-trying ]"
done_

echo "12. level above udma2 dropped by the kernel (Speed warnings)"
setup a; echo udma5 > "$REVIVAL_P/revival-dma"; touch "$F/warn"; run
ck "reported as udma3" "diag | grep -q 'kernel ignored -X udma5' && diag | grep -q 'DMA ok (udma3)' && diag | grep -q 'DMA (udma3)'"
pkill -f "$F" 2>/dev/null; done_

echo "13. hdparm-boot.sh"
setup a
export REVIVAL_IDENTIFY="$F/ide/identify"
PATH="$S:$SPATH" $RUN "$B" > "$F/out" 2>&1
ck "adapter (word 49 = 6c61): no hdparm call" "[ \$(calls hdparm) = 0 ] && grep -q 'staying in PIO' $F/out"
setup a claimed; export REVIVAL_IDENTIFY="$F/ide/identify"
PATH="$S:$SPATH" $RUN "$B" > "$F/out" 2>&1
ck "drive reporting DMA: stock hdparm -d 1 /dev/hda" "grep -q -- 'hdparm -d 1 /dev/hda' $F/calls"
setup a; export REVIVAL_IDENTIFY="$F/none"
PATH="$S:$SPATH" $RUN "$B" > "$F/out" 2>&1
ck "unreadable IDENTIFY: stock command" "grep -q -- 'hdparm -d 1 /dev/hda' $F/calls"
unset REVIVAL_IDENTIFY; done_

echo; [ $fails = 0 ] && echo "all checks passed" || echo "$fails check(s) FAILED"
exit $fails
