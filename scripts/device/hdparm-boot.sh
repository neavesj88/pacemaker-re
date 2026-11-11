#!/bin/sh
# Installed as /sbin/hdpboot. patch-ide-dma.py points pwrApplication's boot-time system() call here
# instead of "hdparm -d 1 /dev/hda". A drive that reports DMA itself (IDENTIFY word 49 bit 8) gets
# exactly the stock command; any other drive (the SD adapter's corrupt IDENTIFY block) is left in PIO,
# because the patched ide-core would otherwise let "-d 1" succeed for it and put it in UDMA3 before the
# first mount. If IDENTIFY cannot be read (fewer than 50 words) the drive stays in PIO too: the stock
# command would succeed for the SD adapter, and a drive that really reports DMA gets it from the
# wrapper once the player starts (claimed() there, needs the .Pacemaker folder), so failing closed costs
# at most speed.
ID=${REVIVAL_IDENTIFY:-/proc/ide/hda/identify}
set -- `cat $ID 2>/dev/null`
if [ $# -lt 50 ]; then
	echo "hdpboot: IDENTIFY unreadable ($# words), staying in PIO"
	exit 0
fi
shift 49
case ${1#?} in
	[13579bdfBDF]*) ;;
	*) echo "hdpboot: drive does not report DMA (word 49 = $1), staying in PIO"; exit 0 ;;
esac
exec hdparm -d 1 /dev/hda
