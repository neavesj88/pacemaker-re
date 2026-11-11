#!/bin/sh
# Pacemaker Revival image wrapper (installed as /bin/pmplayer in the image, real player = pmplayer.real).
# Records what the kernel and the drive say about the storage path, times PIO (and DMA when asked),
# optionally turns DMA on, then runs the player. Plain POSIX sh for busybox 1.1.3 (no printf, no tr, no timeout).
#
# Optional flag files in the .Pacemaker folder of the FAT volume (create or delete them over USB):
#   revival-dma      try DMA after the mounts. The file may hold a level (udma0..udma5, mdma0..mdma2);
#                    empty or anything else means udma2. The attempt reads up to 64 MiB of the system image
#                    in PIO and in DMA (md5), writes and reads back a 4 MiB file on each volume under DMA,
#                    times the drive, and requires a clean kernel log. Any problem puts the drive back in PIO
#                    and writes revival-dma-failed, which stops every later attempt until you delete it.
#   revival-unmask   hdparm -u1: allow interrupts during PIO sector transfers (audio IRQ latency).
#   revival-nobench  skip the hdparm timings (the first start of a boot takes about 6 s longer with them).
# Files the wrapper writes there:
#   revival-dma-trying   exists only while an attempt runs. If it is still there at the next start the attempt
#                        hung or lost power: it is turned into revival-dma-failed and DMA stays off.
#   revival-dma-failed   why DMA was given up. Delete it (and the trying file) to allow another attempt.
# A drive that reports DMA itself (IDENTIFY word 49 bit 8) is left as the stock boot made it. Without the flag
# a drive that does not report DMA is put back in PIO if something turned DMA on.
# The whole storage section runs under a time limit (45 s, 180 s with the flag): if it has not finished, it is
# killed, DMA is switched off and the player starts anyway. A task stuck inside the driver cannot be killed
# by anything; that case is what the trying file is for.
# After a verified attempt a small watcher (every 5 s, no disk access) turns DMA off for good on the first new
# IDE error line, and turns it back on when a standby/wake dropped it.
# Test hooks (all default to the device paths): REVIVAL_P, REVIVAL_H, REVIVAL_TMP, REVIVAL_IMG, REVIVAL_REAL,
# REVIVAL_DROP, REVIVAL_LIMIT, REVIVAL_PATH, REVIVAL_WATCH.
PATH=${REVIVAL_PATH:-/pmds/bin:/usr/local/bin:/sbin:/usr/sbin:/bin:/usr/bin:$PATH}; export PATH
P=${REVIVAL_P:-/mnt/public/.Pacemaker}
H=${REVIVAL_H:-/proc/ide/hda}
T=${REVIVAL_TMP:-/tmp}
DROP=${REVIVAL_DROP:-/proc/sys/vm/drop_caches}
REAL=${REVIVAL_REAL:-/pmds/bin/pmplayer.real}
D=$P/revival-diag.txt
FLAG=$P/revival-dma
TRY=$P/revival-dma-trying
FAIL=$P/revival-dma-failed
OK=$T/revival-dma-ok          # exists after a verified attempt in this boot; holds the level in use
BOOT=$T/revival-diag-boot
WPID=$T/revival-watch.pid
REFUSED=$T/revival-dma-refused
IMG=${REVIVAL_IMG:-/mnt/private/active_sw_file}
[ -f $IMG ] || IMG=/dev/hda2
ERRPAT='dma_intr|DMA timeout|BadCRC|lost interrupt|ide0: reset|resetting ATA|unexpected interrupt|fifo_fill|hda: .*(error|timeout)|status=0x[0-9a-f]+ \{.*Error|I/O error'

sval() { awk -v k="$1" '$1==k{print $2; exit}' $H/settings 2>/dev/null; }
# IDE error signature: line count plus the last matching line, so a ring-buffer wrap that evicts an old
# line while a new one arrives still changes it
esig() { dmesg | grep -i -E "$ERRPAT" > $T/revival-esig; wc -l < $T/revival-esig; tail -1 $T/revival-esig; }
warns() { dmesg | grep -c 'Speed warnings'; }
cpu() { head -1 /proc/stat; }
irq() { grep -i -E 'ata|ide' /proc/interrupts; }
dropc() { sync; echo 3 > $DROP 2>/dev/null; }

claimed() {  # IDENTIFY word 49 bit 8 (DMA); bit 0 of the 2nd hex digit of the 4-digit word
  set -- `cat $H/identify 2>/dev/null`
  [ $# -ge 50 ] || return 1
  shift 49
  case ${1#?} in [13579bdfBDF]*) return 0 ;; esac
  return 1
}

level() {  # first word of the flag file; a CRLF file or trailing text is fine
  l=
  [ -f $FLAG ] && read l rest < $FLAG
  for c in udma0 udma1 udma2 udma3 udma4 udma5 mdma0 mdma1 mdma2; do
    case $l in $c|$c[!0-9A-Za-z]*) echo $c; return ;; esac
  done
  for c in UDMA0 UDMA1 UDMA2 UDMA3 UDMA4 UDMA5; do
    case $l in $c|$c[!0-9A-Za-z]*) echo udma${c#UDMA}; return ;; esac
  done
  for c in MDMA0 MDMA1 MDMA2; do
    case $l in $c|$c[!0-9A-Za-z]*) echo mdma${c#MDMA}; return ;; esac
  done
  echo udma2
}

cpurep() {  # cpurep "</proc/stat line before>" "<after>"
  { echo "$1"; echo "$2"; } | awk '
    NR==1{for(i=1;i<=8;i++)x[i]=$(i+1)}
    NR==2{t=0; for(i=1;i<=8;i++){y[i]=$(i+1)-x[i]; t+=y[i]}
      if(t>0) printf "cpu over %d jiffies: user %d%% system %d%% irq+softirq %d%% iowait %d%% idle %d%%\n", t, 100*(y[1]+y[2])/t, 100*y[3]/t, 100*(y[6]+y[7])/t, 100*y[5]/t, 100*y[4]/t}'
}

bench() {  # bench <label> <hdparm flags>
  echo "-- $1: hdparm $2 /dev/hda"
  c0=`cpu`; i0=`irq`
  hdparm $2 /dev/hda 2>&1
  c1=`cpu`; i1=`irq`
  cpurep "$c0" "$c1"
  echo "irq before: $i0"
  echo "irq after:  $i1"
}

sample() {  # md5 of the first 64 MiB (or the whole file) of the image, page cache dropped first so the device is read
  dropc
  dd if=$IMG bs=64k count=1024 2>/dev/null | md5sum
}

head4() {  # md5 of the first 4 MiB
  dropc
  dd if=$IMG bs=64k count=64 2>/dev/null | md5sum
}

wtest() {  # wtest <dir> <md5 of the first 4 MiB>: write 4 MiB there, drop the cache, read it back
  f=$1/.revival-dmatest
  dd if=$IMG of=$f bs=64k count=64 2>/dev/null
  dropc
  g=`dd if=$f bs=64k 2>/dev/null | md5sum`
  rm -f $f
  sync
  [ "$g" = "$2" ]
}

markfail() {  # markfail <reason>: remember across boots, and for this one
  { date; echo "$1"; } > $FAIL
  rm -f $TRY $OK
  sync
}

off_if_on() {
  if [ "`sval using_dma`" = 1 ]; then
    echo "$1 - DMA off"
    hdparm -d 0 /dev/hda 2>&1
    dropc
  fi
}

try_dma() {  # try_dma <1 = first start of this boot>
  lvl=`level`
  echo "== DMA attempt at $lvl (image $IMG)"
  { date; echo "attempt at $lvl"; } > $TRY
  sync
  e0=`esig`; w0=`warns`
  ref=`sample`; ref4=`head4`
  echo "PIO md5 $ref, first 4 MiB $ref4"
  hdparm -d 1 /dev/hda 2>&1
  if [ "`sval using_dma`" != 1 ]; then
    echo "kernel refused DMA (ide-core not patched?) - staying in PIO"
    : > $REFUSED    # not a DMA fault: no failed file, but no retry (and no 64 MiB PIO read) on every restart of this boot
    rm -f $TRY
    return 1
  fi
  # -d 1 programmed host and drive for UDMA3 (hard-coded in mxc_ide_dma_on); nothing touches the disk until -X
  hdparm -X $lvl /dev/hda 2>&1
  sleep 1
  eff=$lvl
  if [ "`warns`" -gt "$w0" ]; then
    eff=udma3
    echo "kernel ignored -X $lvl (80-wire check, 'Speed warnings'): host and drive stay at the UDMA3 that -d 1 set"
  fi
  why=
  got=`sample`
  [ "$got" = "$ref" ] || why="$why read md5 differs;"
  wtest $P "$ref4" || why="$why write/read-back on the FAT volume differs;"
  if [ -f $IMG ]; then
    wd=${IMG%/*}
    if [ "$wd" != "$P" ] && [ -d "$wd" ] && [ -w "$wd" ]; then
      wtest $wd "$ref4" || why="$why write/read-back in $wd differs;"
    fi
  fi
  if [ $1 = 1 ] && [ ! -f $P/revival-nobench ]; then bench "DMA ($eff)" -t; fi
  e1=`esig`
  [ "$e1" = "$e0" ] || why="$why new IDE error line ($e1);"
  [ "`sval using_dma`" = 1 ] || why="$why using_dma dropped to 0;"
  if [ -z "$why" ]; then
    echo "DMA ok ($eff)"
    echo $eff > $OK
    rm -f $TRY
    return 0
  fi
  echo "DMA FAILED:$why back to PIO"
  hdparm -d 0 /dev/hda 2>&1
  dropc    # nothing read or written under DMA may stay cached, the player image is read through this cache
  markfail "DMA at $lvl failed:$why"
  return 1
}

reenable() {  # DMA was verified earlier in this boot and a standby/wake dropped it
  eff=`cat $OK`
  echo "re-enabling DMA ($eff) after it was dropped"
  hdparm -d 1 /dev/hda 2>&1
  hdparm -X $eff /dev/hda 2>&1
  sleep 1
  [ "`sval using_dma`" = 1 ] || { hdparm -d 0 /dev/hda 2>&1; markfail "re-enabling DMA ($eff) failed"; return 1; }
}

watch() {  # background: never touches the disk (PATH without /pmds, which is a loop mount of the drive)
  PATH=${REVIVAL_PATH:-/usr/local/bin:/sbin:/usr/sbin:/bin:/usr/bin}; export PATH
  s0=`esig`
  while sleep ${REVIVAL_WATCH:-5}; do
    [ -f $OK ] || exit 0
    s1=`esig`
    if [ "$s1" != "$s0" ]; then
      { echo; echo "=== watcher: new IDE error line while DMA was in use: $s1"; hdparm -d 0 /dev/hda 2>&1; } >> $D 2>&1
      markfail "IDE error after a verified start: $s1"
      exit 0
    fi
    if [ "`sval using_dma`" != 1 ]; then
      { echo; echo "=== watcher: using_dma dropped to 0 (standby/wake?) at `date`"; reenable; } >> $D 2>&1
      [ -f $OK ] || exit 0
      s0=`esig`
    fi
  done
}

state() {
  echo "-- state: using_dma `sval using_dma`, unmaskirq `sval unmaskirq`, multcount `sval multcount`, io_32bit `sval io_32bit`, current_speed `sval current_speed`, pio_mode `sval pio_mode`"
  echo "-- IDE kernel log"; dmesg | grep -i -E 'ide|hda|mxc_ata|dma'
}

run() {  # run <1 = first start of this boot, 0 = player restart>
  date; uname -a
  if [ $1 = 1 ]; then
    echo "== /proc/partitions"; cat /proc/partitions
    for f in identify model capacity geometry media driver settings; do
      echo "== $H/$f"; cat $H/$f
    done
    echo "== hdparm /dev/hda"; hdparm /dev/hda 2>&1
    echo "== hdparm -i /dev/hda"; hdparm -i /dev/hda 2>&1
    echo "== hdparm -I /dev/hda"; hdparm -I /dev/hda 2>&1 | head -90
    echo "== /proc/interrupts"; cat /proc/interrupts
  fi
  echo "== storage path"
  [ -f $P/revival-unmask ] && { echo "revival-unmask: hdparm -u1"; hdparm -u1 /dev/hda 2>&1; }
  if claimed; then echo "drive reports DMA (word 49 bit 8)"; else echo "drive does NOT report DMA (word 49 bit 8 clear)"; fi
  state
  if [ $1 = 1 ] && [ ! -f $P/revival-nobench ] && [ "`sval using_dma`" != 1 ]; then
    bench "PIO baseline" -tT
  fi
  lvl=udma5
  if claimed; then
    if [ "`sval using_dma`" != 1 ]; then
      echo "stock DMA for a drive that reports it"
      hdparm -d 1 /dev/hda 2>&1; hdparm -X udma5 /dev/hda 2>&1
    fi
    if [ $1 = 1 ] && [ ! -f $P/revival-nobench ] && [ "`sval using_dma`" = 1 ]; then bench "DMA (udma5)" -t; fi
  elif [ -f $FAIL ]; then
    echo "DMA stays off: $FAIL exists (delete it to allow another attempt)"; cat $FAIL
    off_if_on "revival-dma-failed"
  elif [ -f $FLAG ]; then
    if [ -f $OK ]; then
      [ "`sval using_dma`" = 1 ] || reenable
    elif [ -f $REFUSED ]; then
      echo "the kernel refused DMA earlier in this boot - not trying again"
    else
      try_dma $1
    fi
  else
    off_if_on "no revival-dma flag and the drive does not report DMA"
  fi
  state
}

if [ -d $P ]; then
  if [ -f $TRY ]; then
    { date; echo "the previous DMA attempt never finished (hang, reset or power loss):"; cat $TRY; } > $FAIL
    rm -f $TRY
    sync
  fi
  limit=${REVIVAL_LIMIT:-45}
  [ -f $FLAG ] && limit=${REVIVAL_LIMIT:-180}
  if [ -f $BOOT ]; then
    [ "`wc -c < $D`" -gt 400000 ] 2>/dev/null && : > $D
    { echo; echo "=== player restart"; } >> $D
    ( run 0 ) >> $D 2>&1 &
  else
    : > $BOOT
    : > $D
    ( run 1 ) >> $D 2>&1 &
  fi
  w=$!
  i=0
  while kill -0 $w 2>/dev/null && [ $i -lt $limit ]; do
    sleep 1
    i=`expr $i + 1`
  done
  if kill -0 $w 2>/dev/null; then
    { echo; echo "=== TIMEOUT: storage section still running after ${limit}s - killed, DMA off, starting the player"; } >> $D
    kill $w 2>/dev/null
    killall hdparm dd md5sum 2>/dev/null
    ( hdparm -d 0 /dev/hda >> $D 2>&1 ) &
    if [ -f $TRY ]; then markfail "storage section timed out after ${limit}s"; else rm -f $OK; fi
  fi
  # the watcher survives the exec below; one per boot
  if [ -f $OK ]; then
    alive=0
    [ -f $WPID ] && kill -0 "`cat $WPID`" 2>/dev/null && alive=1
    if [ $alive = 0 ]; then
      ( watch ) > /dev/null 2>&1 &
      echo $! > $WPID
    fi
  fi
  sync &
fi
exec $REAL "$@"
