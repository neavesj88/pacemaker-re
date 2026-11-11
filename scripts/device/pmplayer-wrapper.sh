#!/bin/sh
# Pacemaker Revival image wrapper (installed as /bin/pmplayer in the image, real player = pmplayer.real).
# Records what the kernel and the drive say about the storage path, times PIO (and DMA when asked),
# optionally turns DMA on, then runs the player. Plain POSIX sh for busybox 1.1.3 (no printf, no tr).
#
# Optional flag files in the .Pacemaker folder of the FAT volume (create or delete them over USB):
#   revival-dma      try DMA after the mounts. The file may hold a level (udma0..udma5, mdma0..mdma2);
#                    empty or anything else means udma2. A verified read (md5 of 8 MiB of the system
#                    image, PIO vs DMA) and the kernel log decide; any error puts the drive back in PIO.
#   revival-unmask   hdparm -u1: allow interrupts during PIO sector transfers (audio IRQ latency).
#   revival-nobench  skip the hdparm timings (the first start of a boot takes about 6 s longer with them).
# Without flags the only change to a stock boot is the diagnostic and the timing. A drive that reports DMA
# itself (IDENTIFY word 49 bit 8) gets the stock "hdparm -d 1" + "-X udma5" here, because the patched
# pwrApplication no longer does it before the mounts.
PATH=/pmds/bin:/usr/local/bin:/sbin:/usr/sbin:/bin:/usr/bin:$PATH; export PATH
P=/mnt/public/.Pacemaker
D=$P/revival-diag.txt
H=/proc/ide/hda
IMG=/mnt/private/active_sw_file
[ -f $IMG ] || IMG=/dev/hda2
ERRPAT='dma_intr|DMA timeout|BadCRC|lost interrupt|ide0: reset|resetting ATA|unexpected interrupt|fifo_fill|hda: .*(error|timeout)|status=0x[0-9a-f]+ \{.*Error|I/O error'

sval() { awk -v k="$1" '$1==k{print $2; exit}' $H/settings 2>/dev/null; }
ecount() { dmesg | grep -c -i -E "$ERRPAT"; }
cpu() { head -1 /proc/stat; }
irq() { grep -i -E 'ata|ide' /proc/interrupts; }

claimed() {  # IDENTIFY word 49 bit 8 (DMA); bit 0 of the 2nd hex digit of the 4-digit word
  set -- `cat $H/identify 2>/dev/null`
  [ $# -ge 50 ] || return 1
  shift 49
  case ${1#?} in [13579bdfBDF]*) return 0 ;; esac
  return 1
}

level() {
  l=
  [ -f $P/revival-dma ] && read l rest < $P/revival-dma
  case $l in
    udma[0-5]|mdma[0-2]) echo $l ;;
    UDMA[0-5]) echo udma${l#UDMA} ;;
    MDMA[0-2]) echo mdma${l#MDMA} ;;
    *) echo udma2 ;;
  esac
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

sample() {  # md5 of two 4 MiB windows of the image, page cache dropped first so the device is read
  sync
  echo 3 > /proc/sys/vm/drop_caches 2>/dev/null
  { dd if=$IMG bs=64k count=64 skip=0; dd if=$IMG bs=64k count=64 skip=384; } 2>/dev/null | md5sum
}

try_dma() {
  lvl=`level`
  echo "== DMA attempt at $lvl (image $IMG)"
  n0=`ecount`
  ref=`sample`
  echo "PIO md5 $ref"
  hdparm -d 1 /dev/hda 2>&1
  if [ "`sval using_dma`" != 1 ]; then
    echo "kernel refused DMA (ide-core not patched?) - staying in PIO"
    return 1
  fi
  hdparm -X $lvl /dev/hda 2>&1
  sleep 1
  got=`sample`
  echo "DMA md5 $got"
  n1=`ecount`
  if [ "$got" = "$ref" ] && [ "$n1" -le "$n0" ] && [ "`sval using_dma`" = 1 ]; then
    echo "DMA ok ($lvl)"
    return 0
  fi
  echo "DMA FAILED (data equal: `[ "$got" = "$ref" ] && echo yes || echo no`, error lines $n0 -> $n1, using_dma `sval using_dma`) - back to PIO"
  hdparm -d 0 /dev/hda 2>&1
  return 1
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
  if [ -f $P/revival-dma ]; then
    [ "`sval using_dma`" = 1 ] || try_dma
  elif claimed && [ "`sval using_dma`" != 1 ]; then
    echo "stock DMA for a drive that reports it"
    lvl=udma5
    hdparm -d 1 /dev/hda 2>&1; hdparm -X udma5 /dev/hda 2>&1
  fi
  if [ $1 = 1 ] && [ ! -f $P/revival-nobench ] && [ "`sval using_dma`" = 1 ]; then
    bench "DMA ($lvl)" -t
  fi
  state
}

if [ -d $P ]; then
  if [ -f /tmp/revival-diag-boot ]; then
    [ "`wc -c < $D`" -gt 400000 ] 2>/dev/null && : > $D
    { echo; echo "=== player restart"; run 0; } >> $D 2>&1
  else
    : > /tmp/revival-diag-boot
    run 1 > $D 2>&1
  fi
  sync
fi
exec /pmds/bin/pmplayer.real "$@"
