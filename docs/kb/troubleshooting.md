---
id: troubleshooting
title: Troubleshooting and FAQ
topic: Problems and answers
order: 1
keywords: troubleshooting, recovery file, faq, problem, not seen, not detected, drive not found, firmware did not install, hard drive needs checking, restore hard drive, slow loading, stutter, sd card slow, dma, half tempo, bpm half, double tempo, missing keys, no key, drum tracks, settings reset, plan out of date, cannot add tracks
summary: Fixes for a Pacemaker not detected, failed firmware installs, the hard drive check message, slow SD card loading, wrong BPM octave and missing keys.
---

# Troubleshooting and FAQ

## Pacemaker not detected

1. Switch the unit on. If it is only charging, press the **settings button** to enable data: [Connecting to a computer](pm-computer-connection.md).
2. Use a data USB cable and try another port.
3. Press **Rescan**.
4. If the drive appears but is not listed, use **Open folder...** on the folder containing `.Pacemaker`. Only a drive root (such as `G:`) opens as a Pacemaker. A card image copied to a folder opens as a library on this computer.
5. For a card, use a PC card reader.

## Firmware did not install

| Cause | Fix |
|---|---|
| USB connected when the install started | Unplug, switch off, switch on: [Installing firmware with the app](install-firmware.md). |
| File has problems | Fix what the Firmware card check lists. A checksum that "does not match" means a damaged file. |
| Same firmware number as installed | Only the app image is replaced. Settings still reset. |
| File in the wrong place | Put it directly in the `.Pacemaker` folder of the FAT32 volume, not a subfolder. |
| Settings gone after install | Every install resets settings. Re-apply a preset: [The Device page](device-page.md). |
| "FW Upgrade is NOT OK" | Firmware number and root system do not match. Use a matching build. |

If a Revival firmware installed but the unit will not start, put `.Pacemaker/revival-no-preload` on the card (Device page, Firmware card, **Add recovery file**): [Button combinations and recovery](pm-button-combos-recovery.md).

Last resort: method 5 (DFU) in [Ways to install firmware](firmware-methods.md).

## "Sorry friend... hard drive needs checking"

1. Reseat the ZIF ribbon or storage adapter.
2. After five failed mounts the unit stops retrying. **Continue, then Restore, erases both partitions.** Back up the storage on a PC first if readable.
3. Blank or new storage shows this message: [Upgrading the storage to a microSD card](storage-upgrade.md).
4. If the library is intact, run **Library health**: [The Device page](device-page.md).

## Slow loading or stutter on an SD card

- Use a genuine UHS U3 card: [Upgrading the storage to a microSD card](storage-upgrade.md).
- Optional DMA mode needs firmware **180096073 or later**; **180096075** or later runs it at UDMA2 (earlier builds start at UDMA3). Off by default.

### Enable DMA mode

Experimental: a card fault can corrupt the library.

1. Back up the library.
2. Create an empty file `revival-dma` in the `.Pacemaker` folder. Optionally put one mode word in it: `udma0` to `udma5` or `mdma0` to `mdma2`. Default `udma2`.
3. Start the Pacemaker. It tests the mode after mounting.
4. On failure it returns to slow mode and creates `revival-dma-failed`. Delete that file to try again. A leftover `revival-dma-trying` file also disables DMA.
5. `revival-diag.txt` in the same folder holds timings and the result.

| Flag file | Effect |
|---|---|
| `revival-unmask` | Allows interrupts during transfers |
| `revival-nobench` | Skips the timing test (saves about 6 seconds at start) |

## BPM is half or double

Set the **BPM range** to match your music. Default 92-184 reads drum and bass at about 174. The original Editor range 78-156 reads it as 87. See [BPM, keys and waveforms](bpm-keys-waveforms.md).

1. Set Settings, Analysis, BPM range.
2. Re-analyse with the sidebar **Review** button, or use **Halve BPM** or **Double BPM** on a track.
3. In a mix, use the clip panel **half time** and **double time** buttons: [Crossfades, tempo and Match all](mixes-crossfades-tempo.md).

## Tracks have no key

Tracks without steady pitch (drums only, silent, atonal) have no key. The sidebar shows "N tracks have no key to find".

- Set one: right-click, Key: set. It is never overwritten.
- For keyable tracks left blank, lower the confidence floor: Settings, Analysis, Background analysis. See [Background analysis](background-analysis.md).

## Other questions

| Question | Answer |
|---|---|
| Track shows "needs analysis" | Analyse from the sidebar or right-click menu: [The Tracks view](tracks-view.md). |
| Beat grid is off | [The beat grid editor](grid-editor.md). |
| Sync plan is out of date | Press **Check again**: [Sync](sync.md). |
| Mix opens at the wrong tempo | It starts at the first track's tempo: [Building a mix](mixes-building.md). |
| Undo a change | Edit history, or the session snapshot: [The Device page](device-page.md). |
| Find a setting | F1 search or [The Settings page](settings-page.md). |
