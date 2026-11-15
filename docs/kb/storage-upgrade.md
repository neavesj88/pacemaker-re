---
id: storage-upgrade
title: Upgrading the storage to a microSD card
topic: Firmware and hardware
order: 2
keywords: storage upgrade, microsd, sd card, adapter, zif, ce, ipod classic, iflash, 64 gb, capacity, 8 gb, 8.46, hard drive replacement, partition, fat32, restore hard drive, format, u3, uhs, a1, fake card, dual slot, sintech, lba, pio, dma, memory upgrade, memory card, sdcard, bigger drive, more space, upgrade the drive
summary: Replace the Pacemaker's 1.8-inch hard drive with a microSD card on a ZIF adapter. What to buy (in general terms), what we used, what works, what to avoid, and why firmware decides how much of the card you can use.
---

# Upgrading the storage to a microSD card

The Pacemaker's original storage is a small 1.8-inch hard drive with a flat ribbon connector, no longer made. A **microSD card on an adapter board** can take its place. It is silent, has no moving parts, and holds far more. This article is a guide to the idea, not a parts list: parts of this kind vary from seller to seller.

For other ways to replace the drive (a solid-state drive, a CompactFlash route, another hard drive), see [Other ways to upgrade the storage](storage-alternatives.md).

> **Back up first.** The Pacemaker's own "Restore hard drive" erases everything on the storage. Use the Backup card on [the Device page](device-page.md) and run "Verify a backup" before you touch anything. A backup of the library is the only copy of irreplaceable data. The system files can always be put back from a firmware file.

## What you need (in general terms)

- **An adapter** that turns a microSD card into the Pacemaker's drive: a **microSD-to-1.8-inch 40-pin ZIF/CE adapter of the iPod Classic type**. These are sold for the iPod Video and iPod Classic, and many are clones of a well-known board. They come with one or two card slots.
- **A microSD card.**
- A **PC with a card reader** (or a microSD-to-USB reader).
- A **small screwdriver** for the Pacemaker's back screws. Sources disagree on the Torx size (T5 or T7), so check your own screws before buying a driver.
- A **firmware file** (`.pfw`) for the unit: see [Installing firmware with the app](install-firmware.md).

> **What we used.** A Sintech microSD TF to 1.8-inch 40-pin ZIF CE adapter (the dual-slot kind, with the card in slot 1) and a 64 GB UHS Speed Class 3 (U3) microSD card. This combination worked on the project owner's Pacemaker. It is **one** unit, one adapter and one card: other combinations may behave differently. Check each step on your own unit.

## What works

- A **genuine card from a well-known maker**, bought from a seller that gets stock from the maker. Keep the receipt.
- **UHS Speed Class U1 or U3**, and ideally also **A1** (Application Performance class). Speed matters because the Pacemaker reads while it plays and writes the library. The U1/U3 meaning is from the SD Association; the A1/A2 advice is general knowledge and was not checked here.
- A **sensible size, 32 to 128 GB.** The Pacemaker's disk driver addresses the disk with 28-bit sector numbers (the adapter this guide used reports no 48-bit support), which tops out at 128 GiB (about 137 GB), so anything above that is not usable. 64 GB is a good fit. The old drives were 60 or 120 GB, and nothing larger than that has been tested.
- **One card in one slot.** On a dual-slot adapter, use slot 1 only.
- **Testing the card before you trust it** with a capacity test such as H2testw, which catches fakes.
- A **FAT32** library. The Pacemaker's code mounts FAT32 (vfat) only. exFAT has not been tested, so do not use it.

## What to avoid

- **Cards with a suspiciously large size** (for example 512 GB or 1 TB for a few dollars). Fakes are common: they claim a large capacity and are really 12 to 56 GB. They corrupt data silently.
- **No-name cards, and very cheap cards from marketplaces.** One card is cheap compared with losing a library.
- **Mixing cards in a dual-slot adapter.** On adapters of this kind, two cards are normally joined into one disk (not a mirror). One dead card can lose the whole library, and how the Pacemaker partitions a joined size is untested.
- **Cards above 128 GiB.** Not addressable here.
- **Assuming another adapter behaves like ours.** Genuine iFlash boards are sold only for iPods. The Pacemaker is not on their lists, so compatibility is unproven. Check each unit yourself.
- **Forcing the ribbon.** The ZIF ribbon tears easily. Leave the end on the main board alone, lift the black latch at the drive end only, and seat the new one gently. A loose ribbon gives the "Hard drive needs checking" error.
- **Pressing Restore before you have a backup.**

## Why the size matters: stock firmware and Revival firmware

Every drive or adapter tells the Pacemaker its model name and size when the Pacemaker asks. The adapter we used reports a name **longer than the space the Pacemaker reserves for it**, so it spills into neighbouring fields. The stock firmware then misreads the adapter as an old drive and uses only about **8.46 GB**, whatever size the card is. It also reads and writes slowly, in a basic mode. The size and speed the adapter reports are right, but the stock firmware ignores them.

**Revival firmware 180096073 and later fixes this** with a one-instruction change to the disk driver, so the whole card shows up. This was verified on the project owner's unit: its boot log reports 125,042,688 sectors (64 GB) and the card shows up as a 64 GB disk. The earlier Revival test build 180096072 also has the fix. Drives that already report their size correctly (the original hard drive) behave exactly as before.

**Changing the size the Pacemaker sees does not grow existing partitions.** A card laid out when only 8.46 GB was visible keeps its small partitions until the card is partitioned again. Either let the Pacemaker's own Restore format it (which erases it), or prepare it on a PC with the layout below.

## The layout the Pacemaker expects

Two partitions on a normal (MBR) card:

| Partition | Use | Format |
|---|---|---|
| Second on the card (`hda2`) | The system: the firmware file, a copy of it and a swap file | ext3, about 500 MiB |
| First on the card (`hda1`) | Your music and library, in a `.Pacemaker` folder | FAT32, label `Pacemaker`, the rest of the card |

The Pacemaker mounts these by number. Windows will not make a FAT32 volume larger than 32 GB, and cannot make ext3, so the practical routes are the Pacemaker's own Restore, or a Linux machine (or a tool that can make FAT32 on large disks). Which route the project owner used for the card is not recorded.

## The steps

1. **Back up** the library with the Device page, and keep a copy of the stock firmware and a Revival firmware.
2. **Open the Pacemaker** (back screws, lift the back plate straight up). Lift the black latch on the ZIF connector and slide the ribbon out **at the drive end only**. Take photos as you go.
3. **Fit the adapter** with the microSD card in slot 1, in place of the drive. Keep the rubber mounts the same way up as the old ones. Check that the ribbon clears the other parts when the case closes.
4. **Switch on.** Because the new storage is empty or unrecognised, the Pacemaker fails to mount it five times and shows "Sorry friend... hard drive needs checking". Press **Continue**, then **Restore** at "Restore hard drive?". **Restore erases both partitions.** The Pacemaker formats the storage, restarts, and shows "Hard drive restored. Please upgrade firmware."
   - On **stock firmware** the Pacemaker sees only about 8.46 GB of the card, so Restore may size the partitions from that rather than from the real size. This has **not been observed** (see the last section). After the restore, check the sizes on the Device page's Storage card, and partition from a PC if they are too small.
   - On Revival firmware, or after a PC partitioning, the full size is visible.
5. **Put a firmware file on the storage.** Take the card out, put it in the card reader and open it in the app, then follow [Installing firmware with the app](install-firmware.md). Use Revival firmware 180096073 or later for the full capacity. Put the card back and power on **with USB unplugged**.
6. **Check the size.** Open the Device page and look at the **Storage** card. For a 64 GB card it should show most of the 64 GB (the system partition and the units used by Windows take some of it).
7. **Put the library back.** Copy it from your backup. The project's `restore-library.py` script does this with checks: see [Ways to install firmware](firmware-methods.md). A default backup has no audio files in it, so for the music to come back the backup must have been made with **Include the audio files** ticked; otherwise copy the music over from its original folders. Settings are reset by the firmware install, so also restore or re-apply them: the `--settings-only` option of the script does it, and the Settings presets card does it from the app.
8. Open the library in the app and run **Library health**.

## What is verified and what is not

- **Verified on the project owner's unit:** the stock firmware's 8.46 GB limit with this adapter, the full 64 GB with Revival 180096073, the adapter's report of its name and size, the Restore dialogs on a blank drive (from the firmware's code), and that a firmware install works through the normal route.
- **Not recorded or not verified:** exactly how the owner's card was made full size in the first place (Restore on Revival firmware, or a PC partitioning). Whether the Pacemaker's built-in partitioner copes with a card that has no partition table at all, and whether it sizes the first partition from the real or the old size, are unverified. If the first partition ends near 33 GB instead of 63 GB, partition the card from a PC.
- **Experimental:** a faster transfer mode (DMA) for this kind of adapter. It is off by default and, in one test on the owner's unit, took buffered reads from about 7 MB/s to about 21 MB/s. See the [Troubleshooting article](troubleshooting.md) if loading is slow.

If anything goes wrong, see [Troubleshooting and FAQ](troubleshooting.md) and [Button combinations and recovery](pm-button-combos-recovery.md).
