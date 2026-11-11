#!/usr/bin/env bash
# Integration check of a built firmware: the NEW rootfs (cramfs) plus the NEW image (.pfw) laid out like the device
# (image contents at /mnt/vf, which /pmds links to; FAT volume at /mnt/public), the real /pmds/bin/pmplayer wrapper run under the device's own
# shells with qemu-arm, hdparm and dd stubbed, and the player replaced by a stub that prints what it was started with.
# Default path: no flag files. It must reach the exec of pmplayer.real quickly and with LD_PRELOAD=/pmds/bin/libvocalfx.so,
# and the preload library must load under the rootfs's own glibc 2.4. Nothing is installed or flashed.
#   wsl.exe -d Debian -u root bash scripts/device/test-wrapper-chroot.sh FIRMWARE.pfw   (needs qemu-arm-static, binfmt arm)
set -u
pfw=$(readlink -f "$1")
[ $EUID = 0 ] || { echo "run as root" >&2; exit 2; }
W=$(mktemp -d /root/wrapper-chroot.XXXXXX)
C=$W/root
cleanup() { umount "$C/proc" "$C/dev" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fails=0
ck() { if eval "$2"; then echo "  ok   $1"; else echo "  FAIL $1"; fails=$((fails + 1)); fi; }

size=$(stat -c %s "$pfw"); head -c $((size - 512)) "$pfw" > "$W/image.ext3"
mkdir "$W/img"; debugfs -R "rdump / $W/img" "$W/image.ext3" >/dev/null 2>&1
debugfs -R "dump /boot/cramfs.little $W/cramfs" "$W/image.ext3" >/dev/null 2>&1
fsck.cramfs --extract="$C" "$W/cramfs" >/dev/null || { echo "cannot extract the rootfs"; exit 1; }
cp -a "$W/img/." "$C/mnt/vf/"
cp /usr/bin/qemu-arm-static "$C/usr/bin/"
mount -t proc proc "$C/proc"; mount --bind /dev "$C/dev"
mkdir -p "$C/mnt/public/.Pacemaker"
# stubs first in the wrapper's PATH (/pmds/bin is first), the real player replaced by a reporter
mv "$C/mnt/vf/bin/pmplayer.real" "$C/mnt/vf/bin/pmplayer.real.orig"
printf '#!/bin/sh\necho "hdparm $*" >> /tmp/stub-calls\nexit 0\n' > "$C/mnt/vf/bin/hdparm"
printf '#!/bin/sh\necho "dd $*" >> /tmp/stub-calls\nexit 0\n' > "$C/mnt/vf/bin/dd"
printf '#!/bin/sh\necho "PLAYER LD_PRELOAD=${LD_PRELOAD-} ARGS=$*" > /tmp/player-ran\n' > "$C/mnt/vf/bin/pmplayer.real"
chmod 755 "$C/mnt/vf/bin/hdparm" "$C/mnt/vf/bin/dd" "$C/mnt/vf/bin/pmplayer.real"

echo "image: wrapper $(sha256sum "$C/mnt/vf/bin/pmplayer" | cut -c1-16), library $(sha256sum "$C/mnt/vf/bin/libvocalfx.so" | cut -c1-16), mode $(stat -c %a "$C/mnt/vf/bin/libvocalfx.so")"
ck "wrapper has a shebang the rootfs can run" "head -1 $C/mnt/vf/bin/pmplayer | grep -q '^#!/bin/sh'"
for sh in /bin/sh /bin/ash; do
  echo "-- default path under $sh (no flag files)"
  rm -f "$C/tmp/player-ran" "$C/tmp/stub-calls" "$C/tmp/revival-"* "$C/mnt/public/.Pacemaker/"*
  t0=$(date +%s.%N)
  timeout 120 chroot "$C" $sh /pmds/bin/pmplayer testarg > "$W/out" 2>&1; rc=$?
  t1=$(date +%s.%N)
  el=$(awk -v a=$t1 -v b=$t0 "BEGIN{printf \"%.1f\", a-b}")
  echo "     rc=$rc, ${el}s"
  ck "reached the exec (player stub ran)" "[ -f $C/tmp/player-ran ]"
  ck "player started with LD_PRELOAD=/pmds/bin/libvocalfx.so and the original arguments" "[ \"\$(cat $C/tmp/player-ran)\" = 'PLAYER LD_PRELOAD=/pmds/bin/libvocalfx.so ARGS=testarg' ]"
  ck "quickly (under 20 s with stubbed hdparm)" "[ ${el%.*} -lt 20 ]"
  ck "revival-diag.txt written with partitions, ide files and dmesg" "grep -q '== /proc/partitions' $C/mnt/public/.Pacemaker/revival-diag.txt && grep -q '== /proc/ide/hda/identify' $C/mnt/public/.Pacemaker/revival-diag.txt && grep -q '^== dmesg' $C/mnt/public/.Pacemaker/revival-diag.txt"
  ck "no DMA attempt without the flag (no -d 1 call)" "! grep -q -- '-d 1' $C/tmp/stub-calls"
  ck "no failed or trying marker" "[ ! -e $C/mnt/public/.Pacemaker/revival-dma-failed ] && [ ! -e $C/mnt/public/.Pacemaker/revival-dma-trying ]"
done
echo "-- kill switch"
rm -f "$C/tmp/player-ran"; : > "$C/mnt/public/.Pacemaker/revival-no-preload"
timeout 120 chroot "$C" /bin/sh /pmds/bin/pmplayer > /dev/null 2>&1
ck "revival-no-preload: player started with an empty LD_PRELOAD" "[ \"\$(cat $C/tmp/player-ran)\" = 'PLAYER LD_PRELOAD= ARGS=' ]"
rm -f "$C/mnt/public/.Pacemaker/revival-no-preload"
echo "-- the preload library under the rootfs's own glibc 2.4 (ld.so), in a program that is not pmplayer.real"
# bash is dynamically linked; the variable is set inside the chroot (set outside, it would hit the host's chroot binary)
out=$(chroot "$C" /bin/bash -c 'export LD_PRELOAD=/pmds/bin/libvocalfx.so; /bin/bash -c '"'"'m=0; while read -r l; do case $l in *libvocalfx.so*) m=1;; esac; done < /proc/self/maps; echo mapped=$m'"'"'' 2>&1)
ck "ld.so maps libvocalfx.so into a dynamic ARM program without complaint (its constructor declines: not the player)" "[ \"\$out\" = mapped=1 ]"
echo "$out" | head -3
[ -f "$C/mnt/public/.Pacemaker/revival-vocal.log" ] && { echo "-- log:"; head -3 "$C/mnt/public/.Pacemaker/revival-vocal.log"; }
echo; [ $fails = 0 ] && echo "all checks passed" || echo "$fails check(s) FAILED"
exit $fails
