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

When a Pacemaker is plugged in, a prompt offers to open it: [Libraries](libraries.md#pacemaker-connected).

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

Shows the firmware number on the Pacemaker, **Included with the app** (when there is one), the software and bootloader numbers.

| Control | What it does |
|---|---|
| **Update to** (number) | Shown with "Update available" when the included firmware is newer. Opens the warning, then copies the included file |
| **Install firmware...** | Checks a `.pfw` file you choose, then copies it |
| **Recovery file** | Shows "On the card" or "Not on the card". **Add recovery file** creates `.Pacemaker/revival-no-preload`, **Remove recovery file** deletes it |

Revival firmware always opens a warning first. **Copy firmware** stays disabled until both boxes are ticked:

- A verified backup of this Pacemaker (the last backup this session is shown, or **Go to the Backup card**).
- Install at your own risk.

The warning also lists what to do if the Pacemaker does not start. See [Installing firmware with the app](install-firmware.md).

While the recovery file exists the Pacemaker starts without Revival's add-ons (today the Vocal filter). It applies at the next start.

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
- Every firmware install deletes it. See [Installing firmware with the app](install-firmware.md).

## Settings presets

Save the current settings under a name, apply a saved preset to a Pacemaker, and import or export preset files.

See also [Troubleshooting and FAQ](troubleshooting.md).
