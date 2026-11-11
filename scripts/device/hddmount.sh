#!/bin/sh
# Stock hddmount.sh (only the production-test tool pmptest runs it; the normal boot mounts from
# pwrApplication) with the DMA step moved behind the mounts and made conditional:
#  - a drive that reports DMA itself (IDENTIFY word 49 bit 8) gets the stock hdparm -d 1 / -X udma5,
#    still before the mounts, exactly as before;
#  - any other drive (the SD adapter's corrupt IDENTIFY block) stays in PIO unless
#    /mnt/public/.Pacemaker/revival-dma exists. That file is only visible once hda1 is mounted, so the
#    attempt comes after the mounts. Its first word may name the level (udma0..5, mdma0..2, default
#    udma2). DMA is kept only if a 2 MiB read of hda2 gives the same md5 as before and the kernel log
#    has no new IDE error; otherwise it is switched off again.
# Needs the ide-core.ko of scripts/patch-ide-dma.py, else the kernel refuses "hdparm -d 1" for this drive.
KERNEL_PATCHLEVEL=`uname -a | cut -d - -f 1 | cut -d . -f 3`
FLAG=/mnt/public/.Pacemaker/revival-dma

dma_claimed() {
	set -- `cat /proc/ide/hda/identify 2>/dev/null`
	[ $# -ge 50 ] || return 1
	shift 49
	case ${1#?} in [13579bdfBDF]*) return 0 ;; esac
	return 1
}

using_dma() { awk '$1=="using_dma"{print $2; exit}' /proc/ide/hda/settings; }
sample() { sync; echo 3 > /proc/sys/vm/drop_caches 2>/dev/null; dd if=/dev/hda2 bs=64k count=32 skip=64 2>/dev/null | md5sum; }
errors() { dmesg | grep -c -i -E 'dma_intr|DMA timeout|BadCRC|lost interrupt|ide0: reset|resetting ATA|unexpected interrupt|fifo_fill|hda: .*(error|timeout)'; }

pmptest -8 -1
sleep 1
rmmod g_file_storage
if [ "$KERNEL_PATCHLEVEL" = "28" ]; then
	modprobe ide_gd_mod
else
	modprobe ide-disk
fi
modprobe mxc_ide
sleep 1
dd if=/dev/hda of=/tmp/kalle bs=1 count=1
if dma_claimed; then
	hdparm -d 1 /dev/hda
	hdparm -X udma5 /dev/hda
fi

pkill pwr
pkill player
umount /mnt/vf
if [ "$KERNEL_PATCHLEVEL" = "28" ]; then
	losetup -d /dev/loop0
else
	losetup -d /dev/loop/0
fi
umount /dev/hda1
umount /dev/hda2

mount -t vfat -o noatime,nodiratime /dev/hda1 /mnt/public
mount -t ext3 -o noatime,nodiratime /dev/hda2 /mnt/vf

# opt-in DMA for a drive that does not report it: only after the mounts, with a read-back check
if [ -f $FLAG ] && [ "`using_dma`" != 1 ]; then
	read lvl rest < $FLAG
	case $lvl in
		udma[0-5]|mdma[0-2]) ;;
		UDMA[0-5]) lvl=udma${lvl#UDMA} ;;
		MDMA[0-2]) lvl=mdma${lvl#MDMA} ;;
		*) lvl=udma2 ;;
	esac
	e0=`errors`
	ref=`sample`
	hdparm -d 1 /dev/hda
	if [ "`using_dma`" = 1 ]; then
		hdparm -X $lvl /dev/hda
		sleep 1
		if [ "`sample`" != "$ref" ] || [ "`errors`" -gt "$e0" ] || [ "`using_dma`" != 1 ]; then
			echo "hddmount: DMA ($lvl) failed the read-back check, back to PIO"
			hdparm -d 0 /dev/hda
		fi
	else
		echo "hddmount: kernel refused DMA, staying in PIO"
	fi
fi
