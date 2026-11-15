---
id: device-page
title: The Device page
topic: Device page
order: 1
keywords: device page, backup, verify backup, library health, missing audio, find files, fix, clean up, unused files, restore snapshot, edit history, undo, device settings, settings.dat, presets, eject, storage, firmware, vocal filter, safety, snapshot
summary: Every card on the Device page: storage, library health, firmware, vocal filter, backup, restore, undo, safety and the Pacemaker's own settings, and how to eject.
---

# The Device page

**Device** (under Tools) is the control room for a Pacemaker that is open in the app. For a library on this computer the page is called **This library** and shows only the cards that are about the library itself: Library health, Restore from a snapshot, Edit history and Safety settings. The cards for storage, firmware, the vocal filter, settings.dat and eject need a real Pacemaker.

## Eject

The **Eject** button at the top asks Windows to eject the Pacemaker safely. Always eject before unplugging, and wait for "Do not disconnect" on the Pacemaker's screen to clear. After an eject the page says the drive was ejected and it is safe to unplug. Jobs that write wait for each other, so eject is unavailable while one is running.

## The cards

### Storage

A bar shows how much space music, grids and mixes, and other files take, and how much is free. Below it are the number of tracks, mixes and cases. **Clean up N unused files...** reviews files under Music that no track uses, and **Free up space...** opens [Free space](free-space.md). To get a bigger store inside the Pacemaker, see [Upgrading the storage to a microSD card](storage-upgrade.md).

### Library health

Four checks, each with a tick or a cross:

- The database matches what the Pacemaker requires. If not, the library stays read-only until fixed.
- Every track has its audio file. If some do not, **Show tracks**, **Find files...** (looks for the files elsewhere, optionally matching by tags and length) or **Fix...** (removes rows with no audio; rows a mix uses are kept).
- Every track has a usable beat grid. If not, **Analyse...** or **Fix...**.
- No unused files under Music. If there are, **Review...** lists every one by name. Beat grids of removed tracks are ticked, audio and other files are your call, and only ticked files are deleted after you confirm. Deleting audio is permanent.

### Firmware

Shows the firmware, software and bootloader numbers and the masked serial number. **Install firmware...** copies a firmware file to the Pacemaker. See [Installing firmware with the app](install-firmware.md). The small **?** on this card opens that article.

### Vocal filter

Three choices for the Wah effect: **Off** (stock Wah), **Vocal** (Wah pad right keeps the vocals, left cuts them) and **Vocal inverted** (the other way round). It needs Revival firmware 180096073 or later and takes effect at the next Pacemaker start. If a file named `revival-no-preload` is on the card, the Pacemaker plays stock Wah whatever you choose, and the card says so. See [Effects on the Pacemaker](pm-effects.md).

### Backup

**Back up...** makes a snapshot of the library in a new folder called pacemaker-backup and the date, inside a folder you choose: the database (tracks, mixes, cases), the Pacemaker settings, the system file and every beat grid. Audio is left out unless you tick **Include the audio files**. Each file is compared with its source right after writing, and a SHA-256 list is saved beside the copy. Nothing on the Pacemaker is changed.

**Verify a backup...** re-reads every file of a backup folder against its SHA-256 list, so you can trust a backup before you need it. The system may answer the first check from its cache, so run Verify again later to read from the disk. **Back up before you change storage, install firmware or sync with removals.**

### Restore from a snapshot

Before the first change to a library in each session, the app copies the database (music.db) to its own data folder (never onto the Pacemaker) and keeps the last 10. If the copy fails, nothing is written. **Restore** puts one back: tracks, cases and mixes return to how they were then. Audio files, beat grids and the Pacemaker settings are not touched, so a bulk tempo or grid edit is not undone by it. This is **not** the Pacemaker's own "Restore hard drive" function.

### Edit history

Lists your recent edits (a rating, a tag, a heart) so you can undo back to one. Keeps the last N edits (a safety setting), and **Clear** empties it. A short Undo prompt also appears after an edit.

### Safety settings

How many edits are kept to undo (0 switches the history off), how long the undo prompt stays, and an extra check that refuses a fix that would remove more than a set percentage of tracks. Saved on this computer.

### Device settings

Reads the Pacemaker's `settings.dat` and lets you edit its settings (the same ones as in [The settings menu](pm-settings-menu.md)) here, grouped by folder. Changed values are marked "changed". **Write N changes...** shows each change before rewriting the file. Levels and EQ are raw engine values, so they are shown but not editable. If the card says the drive has no settings file, the Pacemaker creates it the first time it is shut down. Do not write while the Pacemaker is running: eject, then power it on. A new firmware number deletes this file, so settings reset: see [Installing firmware with the app](install-firmware.md).

### Settings presets

Saves the current settings under a name, applies a saved preset to a Pacemaker, and imports or exports preset files. Handy after a firmware install resets the settings.

## If something is wrong

See [Troubleshooting and FAQ](troubleshooting.md).
