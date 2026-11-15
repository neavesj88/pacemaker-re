---
id: troubleshooting
title: Troubleshooting and FAQ
topic: Problems and answers
order: 1
keywords: troubleshooting, faq, problem, not seen, not detected, drive not found, firmware did not install, hard drive needs checking, restore hard drive, slow loading, stutter, sd card slow, dma, half tempo, bpm half, double tempo, missing keys, no key, drum tracks, settings reset, plan out of date, cannot add tracks
summary: Fixes for the common problems: the Pacemaker not seen, firmware not installing, "hard drive needs checking", slow loading on an SD card, a half-tempo BPM, and missing keys on drum tracks.
---

# Troubleshooting and FAQ

## The Pacemaker is not seen by the app

1. Switch it on and, if it is only charging, press the **settings button** so the data connection turns on: [Connecting to a computer](pm-computer-connection.md).
2. Try another USB cable and port. Charge-only cables do not carry data.
3. Press **Rescan** in the app's top bar (or on the Welcome screen).
4. If it appears as a drive but not in the app, open the folder that contains `.Pacemaker` with **Open folder...**. Only a drive root (such as `G:`) counts as a Pacemaker. A copy of a card image in a folder opens as a library on this computer.
5. If the Pacemaker's storage is a card, put the card in a PC reader and open that.

## The firmware did not install

- **You started it with USB plugged in.** On the unit tested, that hung it after the kernel logo. Unplug the cable, switch off, then on. See [Installing firmware with the app](install-firmware.md).
- **The file had problems.** The Firmware card's check lists them and offers no Copy button until they are fixed. A checksum that "does not match" means the file is damaged.
- **Same number.** A file with the number already installed is accepted and flashes nothing. The app says so.
- **The file is in the wrong place.** It must be in the `.Pacemaker` folder of the FAT32 volume, not in a subfolder.
- **It looks like it did, but settings are gone.** A new firmware number deletes the settings file by design. Re-apply them from a preset: [The Device page](device-page.md).
- If it shows an error such as "FW Upgrade is NOT OK", the firmware number and the root system did not match. Use a matching build.
- Last resort: [Ways to install firmware](firmware-methods.md), method 5 (DFU).

## "Sorry friend... hard drive needs checking"

1. **Check the ribbon first.** A loose ZIF ribbon (or a storage adapter not fully seated) is the usual cause. Reseat it gently.
2. The Pacemaker gives up after failing to mount five times. **Pressing Continue and then Restore erases both partitions.** Back up first if you can read the storage on a PC.
3. A blank or new storage shows this by design. See [Upgrading the storage to a microSD card](storage-upgrade.md).
4. If the library is fine but the Pacemaker complains, run **Library health** in the app: [The Device page](device-page.md).

## Slow loading, or stutter, on an SD card

A storage adapter can leave the Pacemaker in a slow transfer mode (PIO), where the processor moves every byte. Loading a track and the screen can then stutter. The Pacemaker reports about 7 MB/s in this mode. It is not measured on every unit, and a **stutter-while-loading link is only a hypothesis**.

- **Use a good card**, UHS U3 and genuine: [Upgrading the storage to a microSD card](storage-upgrade.md).
- **Experimental: faster DMA mode.** Needs Revival firmware **180096073 or later**. It is **off by default**. It has run on one unit (with one adapter), where it took buffered reads from about 7.3 to about 21 MB/s, but it is **experimental** and a card fault could in theory corrupt the library.
  1. Back up first.
  2. Create an empty file called `revival-dma` in the `.Pacemaker` folder on the FAT32 volume. You can put one word in it: `udma0` to `udma5` or `mdma0` to `mdma2`. With no word, `udma2` is used.
  3. Start the Pacemaker. It tries the mode after mounting, with a time limit, a checksum of up to 64 MiB of the system image and a 4 MiB write-and-read-back test. On **any** problem it drops back to the slow mode and creates `revival-dma-failed`. That file stops all later attempts until you delete it. A leftover `revival-dma-trying` file means a hang or power loss, and DMA is also given up on.
  4. Every start writes `revival-diag.txt` in the same folder with timings and the verdict.
  - Other flag files: `revival-unmask` (interrupts during transfers) and `revival-nobench` (skips the timing test, which makes the first start about 6 seconds longer).
  - **Known gap:** after a DMA error the Pacemaker's own driver may switch DMA back on at a faster setting (UDMA3), so the "udma2" safety margin holds only until the first error.
  - **If unsure, leave DMA off.**

## The BPM looks half or double what I expected

The analysis folds every tempo into one window (the **BPM range** setting). A drum and bass track at 174 reads 87 under the original Editor's range of 78-156. See [BPM, keys and waveforms](bpm-keys-waveforms.md).

1. Pick the range that suits your music (Settings, Analysis, BPM range). The default 92-184 reads drum and bass at about 174.
2. Re-analyse the tracks that are outside the new range (the sidebar "Review" button), or use **Halve BPM** or **Double BPM** on single tracks.
3. In a mix, a track at the wrong octave: the clip panel has **half time** and **double time** buttons. See [Crossfades, tempo and Match all](mixes-crossfades-tempo.md).

## Tracks have no key (especially drum-only tracks)

A track with no steady pitch (drums only, silent or atonal) has no key to detect. The sidebar says "N tracks have no key to find". Set the key by hand (right-click, Key: set). A track that has a key you set is never overwritten. Tracks that could be keyed but were not may have been **held back by the confidence floor**: lower it under Settings, Analysis, Background analysis. See [Background analysis](background-analysis.md).

## Other questions

**Why does a track show "needs analysis"?** It has no usable beat grid (the `.str` file is missing, empty or unreadable). Analyse it from the sidebar or the right-click menu. See [The Tracks view](tracks-view.md).

**The beat grid is off.** [The beat grid editor](grid-editor.md).

**A Sync plan says it is out of date.** Something changed since you checked. Press **Check again**. See [Sync](sync.md).

**My mix has the wrong tempo when I open it.** The mix tempo is not saved in the file: it starts at the first track's tempo. See [Building a mix](mixes-building.md).

**Can I undo a change?** Edits are kept in the Edit history, and the app snapshots the database before the first change in each session: [The Device page](device-page.md).

**Where do I find a setting?** Press F1 and search, or use the search on [The Settings page](settings-page.md).
