---
id: storage-upgrade
title: Upgrading the storage to a microSD card
topic: Firmware and hardware
order: 2
keywords: storage upgrade, microsd, sd card, adapter, zif, ce, ipod classic, iflash, 64 gb, capacity, 8 gb, 8.46, hard drive replacement, partition, fat32, restore hard drive, format, u3, uhs, a1, fake card, dual slot, sintech, lba, pio, dma, memory upgrade, memory card, sdcard, bigger drive, more space, upgrade the drive
summary: Replace the Pacemaker's 1.8-inch hard drive with a microSD card on a ZIF adapter, including parts, card requirements and install steps.
---

# Upgrading the storage to a microSD card

A microSD card on a 1.8-inch 40-pin ZIF adapter replaces the hard drive. Other options: [Other ways to upgrade the storage](storage-alternatives.md).

> Restore hard drive erases everything on the storage. Back up first with the Backup card on [the Device page](device-page.md) and run Verify a backup.

## Parts and tools

- microSD-to-1.8-inch 40-pin ZIF/CE adapter (iPod Classic type).
- microSD card and a PC card reader.
- Small Torx screwdriver (T5 or T7).
- Firmware file (`.pfw`): see [Installing firmware with the app](install-firmware.md).

## Tested parts

- Adapter: Sintech microSD TF to 1.8-inch 40-pin ZIF CE, dual-slot, card in slot 1.
- Card: 64 GB UHS Speed Class 3 (U3) microSD.

## What works

| Item | Requirement |
|---|---|
| Card | Genuine, from a known maker and a reputable seller |
| Speed | UHS Speed Class U1 or U3, ideally A1 |
| Size | 32 to 128 GB (maximum usable 128 GiB, about 137 GB) |
| Slots | One card, in slot 1 on a dual-slot adapter |
| Library format | FAT32 |
| Firmware | Revival 180096073 or later shows the full card. Stock firmware shows about 8.46 GB |

## What to avoid

- Fake or no-name cards. Test new cards with H2testw.
- Cards above 128 GiB.
- Two cards on a dual-slot adapter: they join as one disk.

## Install steps

1. Back up the library with the Device page. Keep the stock firmware and a Revival firmware file.
2. Remove the back screws, lift the back plate straight up and disconnect the drive ribbon (lift the black ZIF latch at the drive end only).
3. Fit the adapter with the card in slot 1 in place of the drive. Seat the ribbon gently, keep it clear of other parts and refit the rubber mounts the same way up. A loose ribbon shows "hard drive needs checking": reseat it.
4. Switch on. At "Sorry friend... hard drive needs checking" press **Continue**, then **Restore** at "Restore hard drive?". The unit restarts and shows "Hard drive restored. Please upgrade firmware."
   - Check the partition sizes on the Device page **Storage** card.
   - If the first partition is too small, repartition the card on a PC (see the partition table below).
5. Remove the card and open it in the app. Install Revival firmware 180096073 or later: see [Installing firmware with the app](install-firmware.md). Put the card back and power on with USB unplugged.
6. Check the **Storage** card on the Device page shows most of the card's capacity.
7. Copy the library back from the backup with `restore-library.py` ([Ways to install firmware](firmware-methods.md)). A backup contains audio only if **Include the audio files** was ticked. The firmware install resets settings: re-apply them with the Settings presets card or the script's `--settings-only` option.
8. Open the library in the app and run **Library health**.

## Partition layout

| Partition | Use | Format |
|---|---|---|
| Second (`hda2`) | System | ext3, about 500 MiB |
| First (`hda1`) | Music and library, label `Pacemaker` | FAT32, rest of the card |

Windows cannot make ext3 or FAT32 above 32 GB. Use the Pacemaker's Restore or a Linux machine.

## Slow loading

See [Troubleshooting and FAQ](troubleshooting.md) for the optional DMA mode.

## Related

- [Troubleshooting and FAQ](troubleshooting.md)
- [Button combinations and recovery](pm-button-combos-recovery.md)
