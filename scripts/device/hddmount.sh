#!/bin/sh
# Stock hddmount.sh (only the production-test tool pmptest runs it; the normal boot is pwrApplication) with its
# "hdparm -d 1 /dev/hda" replaced by /sbin/hdpboot (scripts/device/hdparm-boot.sh): the same command for a drive that
# reports DMA, nothing for the SD adapter, because the patched ide-core would let it through and put that drive in
# UDMA3. DMA for the adapter is opt-in and handled only by the image wrapper (revival-dma flag).
KERNEL_PATCHLEVEL=`uname -a | cut -d - -f 1 | cut -d . -f 3`

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
/sbin/hdpboot
hdparm -X udma5 /dev/hda

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
