---
id: storage-alternatives
title: Other ways to upgrade the storage
topic: Firmware and hardware
order: 3
keywords: storage alternatives, ssd, zif ssd, kingspec, compactflash, cf adapter, msata, iflash solo, iflash dual, iflash quad, replacement hard drive, toshiba mk1634gal, pata, 1.8 inch, comparison, lba28, 128 gib
summary: Compares the ways to replace the 1.8-inch hard drive: microSD adapters, iFlash boards, CompactFlash, mSATA, ZIF SSD and another hard drive.
---

# Other ways to upgrade the storage

The Pacemaker takes a 1.8-inch PATA drive with a 40-pin ZIF ribbon. SATA is not supported. Main guide: [Upgrading the storage to a microSD card](storage-upgrade.md).

## Options

| Option | Notes | Status |
|---|---|---|
| microSD on a ZIF adapter (iPod Classic style) | Cheap, silent, large. Needs Revival firmware 180096073 or later | Tested |
| iFlash Solo (one SD card) | Sold for iPods | - |
| iFlash Dual (two SD cards, 5 mm) | Two cards joined as one disk, no mirror | - |
| iFlash Quad (up to four microSD, 2 mm) | Cards joined as one disk | - |
| ZIF-to-CompactFlash adapter, CF-to-SD adapter, SD card | SD up to 64 GB. The ZIF adapter needs trimming to fit | Reported |
| mSATA SSD on an mSATA-to-ZIF adapter | Large and fast. Check height clearance | - |
| 1.8-inch ZIF SSD (40-pin, 64 and 128 GB) | Hard to find | Reported |
| Another 1.8-inch hard drive (Toshiba MK1634GAL 160 GB, MK1231GAL 120 GB, MK8022GAA 80 GB) | Moving parts, old stock | - |

## Requirements

- Maximum size 128 GiB (about 137 GB).
- Back up first: [The Device page](device-page.md).
- After the swap the Pacemaker shows "Restore hard drive?" then "Please upgrade firmware". Follow [Upgrading the storage to a microSD card](storage-upgrade.md) and [Installing firmware with the app](install-firmware.md).
- Card and opening steps: [Upgrading the storage to a microSD card](storage-upgrade.md).

## Handling

- Disconnect only the drive end of the ZIF ribbon. It tears easily.
- Replacement ribbons can be longer or have contacts on the other side. Compare with yours.

See also [Troubleshooting and FAQ](troubleshooting.md).
