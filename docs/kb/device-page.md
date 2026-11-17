---
id: device-page
title: The Device page
topic: Device page
order: 1
keywords: device page, backup, verify backup, library health, missing audio, find files, fix, clean up, unused files, restore snapshot, edit history, undo, device settings, settings.dat, presets, eject, storage, firmware, vocal filter, safety, snapshot
summary: The Device page shows storage, library health, firmware, backup, restore, undo and settings for a connected Pacemaker, and ejects it safely.
---

# The Device page

**Device** (under Tools) manages the connected Pacemaker. For a library on this computer it is called **This library** and shows only Library health, Restore from a snapshot, Edit history and Safety settings.

## Eject

1. Press **Eject**.
2. Wait for "Do not disconnect" to clear on the Pacemaker's screen.
3. Unplug when the page says it is safe.

## Storage

- **Clean up N unused files...** reviews files under Music that no track uses.
- **Free up space...** opens [Free space](free-space.md).

For a bigger store see [Upgrading the storage to a microSD card](storage-upgrade.md).

## Library health

| Check | Action when it fails |
| --- | --- |
| Database matches what the Pacemaker requires | Library is read-only until fixed |
| Every track has its audio file | **Show tracks**; **Find files...** searches elsewhere; **Fix...** removes rows with no audio |
| Every track has a usable beat grid | **Analyse...** or **Fix...** |
| No unused files under Music | **Review...** lists every file |

In **Review...** only ticked files are deleted, after you confirm. Deleting audio is permanent.

## Firmware

Shows the firmware, software and bootloader numbers. **Install firmware...** copies a firmware file to the Pacemaker. See [Installing firmware with the app](install-firmware.md).

## Vocal filter

Sets the Wah effect. Needs Revival firmware 180096073 or later; applies at the next Pacemaker start. See [Effects on the Pacemaker](pm-effects.md).

| Choice | Effect |
| --- | --- |
| **Off** | Stock Wah |
| **Vocal** | Wah pad right keeps vocals, left cuts them |
| **Vocal inverted** | Reverse of Vocal |

A file named `revival-no-preload` on the card forces stock Wah; the card says so.

## Backup

- **Back up...** copies the database, settings and beat grids into a new dated folder you choose. Tick **Include the audio files** to copy audio.
- **Verify a backup...** checks every file of a backup folder.

Back up before changing storage, installing firmware or syncing with removals.

## Restore from a snapshot

The app keeps the last 10 library snapshots, taken before the first change in each session. **Restore** returns tracks, cases and mixes to the chosen snapshot. Audio, beat grids and Pacemaker settings are not changed.

> This is not the Pacemaker's own "Restore hard drive" function.

## Edit history

Lists recent edits (rating, tag, heart) so you can undo back to one. **Clear** empties the list.

## Safety settings

- Number of edits kept for undo. 0 switches history off.
- How long the Undo prompt stays.
- A check that refuses a fix removing more than a set percentage of tracks.

## Device settings

Edits the Pacemaker's `settings.dat`. The settings match [The settings menu](pm-settings-menu.md).

1. Change values.
2. Press **Write N changes...**. Each change is shown before the file is rewritten.

- Levels and EQ are read-only.
- Eject and power on the Pacemaker before writing. Do not write while it runs.
- If there is no settings file, the Pacemaker creates it at its first shutdown.
- A new firmware number deletes it. See [Installing firmware with the app](install-firmware.md).

## Settings presets

Save the current settings under a name, apply a saved preset to a Pacemaker, and import or export preset files.

See also [Troubleshooting and FAQ](troubleshooting.md).
