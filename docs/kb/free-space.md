---
id: free-space
title: Free space
topic: Library tools
order: 9
keywords: free space, storage, full, archive, remove, planner, make room, card, disk full, candidates, protect
summary: Plans which tracks to archive or remove so the music fits on a Pacemaker. Only offered for a Pacemaker.
---

# Free space

**Free space** (under Tools) only appears while a Pacemaker is open. A library on this computer is your master copy, so nothing is archived or removed from it.

## What it does

The planner scans the Pacemaker and works out how much must go to meet your target: either keep a number of GB free, or fit the library on a card of a size you give. It nominates **candidates**, each with a reason (for example a track never played, or one that repeats the audio of another track). Under Settings, Device, **Free-space planner** you choose which reasons are on, limit candidates (for example to tracks that are in no case or in no mix), protect tracks (such as those in Favourites), pick the order of the list and say where archives go.

## Steps

1. Press **Scan again** if the card changed. The headline says what the library takes and how much must be freed.
2. Review the list. **Tick suggested** ticks the nominated tracks, **Tick shown** ticks what the filter box shows, and **Untick all** clears. The footer says how much space that frees.
3. Choose what to do with the ticked tracks:
   - **Archive...** copies the tracks to a new folder on this computer, reads each copy back to check it, and writes a checksum list (`MANIFEST.sha256`) and a tag list (`ARCHIVE.csv`). Adding the files to a Pacemaker again restores the tracks. The tracks stay on the Pacemaker.
   - **Remove...** takes them off the Pacemaker.
   - **Archive and remove...** does both, so nothing is lost. A track in a mix can be archived but not removed.
4. Confirm. The result says what was done and links to the archive folder.

Back up the Pacemaker first if you are unsure: see [The Device page](device-page.md). To get a bigger store instead, see [Upgrading the storage to a microSD card](storage-upgrade.md).
