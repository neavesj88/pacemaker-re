#!/bin/sh
# Installed as /sbin/hdpboot. patch-ide-dma.py points pwrApplication's boot-time system() call here
# instead of "hdparm -d 1 /dev/hda". A drive that reports DMA itself (IDENTIFY word 49 bit 8) gets
# exactly the stock command; any other drive (the SD adapter's corrupt IDENTIFY block) is left in PIO,
# because the patched ide-core would otherwise let "-d 1" succeed for it and put it in UDMA3 before the
# first mount. If IDENTIFY cannot be read the stock command runs (the wrapper turns DMA off again for a
# drive that does not report it and has no revival-dma flag).
ID=${REVIVAL_IDENTIFY:-/proc/ide/hda/identify}
set -- `cat $ID 2>/dev/null`
if [ $# -ge 50 ]; then
	shift 49
	case ${1#?} in
		[13579bdfBDF]*) ;;
		*) echo "hdpboot: drive does not report DMA (word 49 = $1), staying in PIO"; exit 0 ;;
	esac
fi
exec hdparm -d 1 /dev/hda
