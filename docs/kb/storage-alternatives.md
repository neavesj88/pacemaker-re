---
id: storage-alternatives
title: Other ways to upgrade the storage
topic: Firmware and hardware
order: 3
keywords: storage alternatives, ssd, zif ssd, kingspec, compactflash, cf adapter, msata, iflash solo, iflash dual, iflash quad, replacement hard drive, toshiba mk1634gal, pata, 1.8 inch, comparison, lba28, 128 gib
summary: A comparison of the ways to replace the 1.8-inch hard drive (microSD adapters, iFlash boards, CompactFlash, mSATA, a ZIF SSD, another hard drive), with what is verified and what is only reported.
---

# Other ways to upgrade the storage

The Pacemaker takes a **1.8-inch PATA drive with a 40-pin ZIF ribbon**. It does not take SATA. Anything that looks like that drive to the Pacemaker will do. The main guide is [Upgrading the storage to a microSD card](storage-upgrade.md). This article compares it with the alternatives.

> **How to read the table.** "Verified" means it was done on a Pacemaker and recorded in this project. "Reported" means a person wrote that it worked, usually years ago, and nothing here could confirm it. "Unverified" means there is no Pacemaker report at all, only the product's own pages or a similar device (an iPod).

| Option | Good | Not so good | Evidence |
|---|---|---|---|
| **microSD on a ZIF adapter** (an iPod Classic style board, one or two slots) | Cheap, silent, large. Said to draw less power than a hard drive (not measured here). A 64 GB card is easy | Stock firmware sees only about 8.46 GB with the adapter we used, so Revival firmware 180096073 or later is needed for the full size. Starts in a slow mode. Fake cards are common. Other adapters may differ | **Verified** on the project owner's unit with one Sintech dual-slot adapter and a 64 GB U3 card |
| **Genuine iFlash boards** (Solo: one SD card, about US$33. Dual: two SD cards, 5 mm thick like the original drive, about US$40, out of stock on 15 Nov 2026. Quad: up to four microSD, 2 mm thick, about US$42) | The original design. A known-good card list. Dual is the same thickness as the old drive | Sold and listed only for iPods, not the Pacemaker. More than one card is joined without protection (JBOD, not a mirror) | **Unverified** for the Pacemaker (prices and stock from the maker's pages on 15 Nov 2026) |
| **ZIF-to-CompactFlash adapter plus a CF-to-SD adapter plus an SD card** | A community write-up exists for the Pacemaker. SD sizes up to 64 GB | The ZIF adapter has to be trimmed with a saw to fit. More parts, more contacts | **Reported** (community guide, years old) |
| **mSATA SSD on an mSATA-to-ZIF adapter** (or the iFlash-Sata board) | Large, fast, solid state. Used in iPod Classic builds | Height clearance inside the Pacemaker is untested. Some buyers report poor contact. A Windows format must be redone as FAT32 | **Unverified** for the Pacemaker |
| **A 1.8-inch ZIF SSD** (40-pin, sold by a few makers in 64 and 128 GB) | The community's recommendation. One part. Reported much better battery life and slightly quicker menus (not measured) | Hard to find now. Quality varies. An open-source ZIF-IDE SSD design exists, but no ready-made product | **Reported** (one 64 GB MLC unit worked after a firmware update, 2013) |
| **Another 1.8-inch hard drive** (for example Toshiba MK1634GAL, 160 GB, 5 mm; MK1231GAL, 120 GB; MK8022GAA, 80 GB) | Same kind of part as the original | Moving parts, old stock, unknown health. One listing says a model does not work in one iPod, so it depends on the board | **Unverified** on a Pacemaker. The original in the owner's unit was a Samsung 120 GB drive |

## Things that apply to every option

- **Backup first.** Back up the library with the Device page and verify it: see [The Device page](device-page.md).
- **Size limit.** The Pacemaker's disk driver uses 28-bit addresses (with the adapter we used, which reports no 48-bit support), which is **128 GiB** (about 137 GB) at the most. Anything bigger is not usable. The old drive was 120 GB and nothing larger has been tried.
- **FAT32.** The Pacemaker's code mounts a FAT32 (vfat) library. exFAT and NTFS have not been tested. The system partition is ext3.
- **After the swap**, the Pacemaker shows "Restore hard drive?" (and then "Please upgrade firmware"). That is normal: it needs a partition layout and a firmware file. See [Upgrading the storage to a microSD card](storage-upgrade.md) and [Installing firmware with the app](install-firmware.md).
- **The ZIF ribbon tears easily.** Disconnect only the drive end. Replacement ribbons from shops can be longer and have contacts on a different side. The ribbon width (two widths exist) is not settled, so look at yours.
- **Opening the case.** The back plate has screws that sources say are T5 or T7 Torx. Check yours. Lift the back plate straight up so the power and button boards do not snag.
- **Cards for adapters.** If an adapter takes SD or microSD, buy genuine cards with a UHS rating, test them for fakes (a capacity test such as H2testw), and use one card.

## Which to pick

- **Safest, best reported:** a plain 1.8-inch ZIF SSD, if you can find one, or a used genuine hard drive.
- **Cheapest and the one this project has tried:** a microSD adapter, with Revival firmware.
- **Worth trying if you like to tinker:** CompactFlash or mSATA, knowing that nobody here has tested them.

Nothing here has measured the effect of any option on battery life or heat. See also [Troubleshooting and FAQ](troubleshooting.md).
