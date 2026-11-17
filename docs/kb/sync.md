---
id: sync
title: Sync
topic: Libraries and sync
order: 1
keywords: sync, send to pacemaker, get from pacemaker, copy, compare, plan, removals, both sides, conflict, newer, replace, keep both, merge, plays counters, labels, from, to
summary: Copy tracks, mixes and cases from one library to the other, either way, after seeing exactly what would change.
---

# Sync

**Sync** (in the sidebar) copies tracks, mixes and cases in one direction between the library on this computer and a Pacemaker or another library.

## Run a sync

1. Connect the Pacemaker (or put its storage in a card reader) and press **Rescan** if it is not listed. For a library in a folder, use **Open folder...**.
2. Choose **From** and **To**. The arrows button swaps them. **Send to Pacemaker** and **Get from Pacemaker** set the pair.
3. Press **Check what would change**. Nothing is written.
4. Read the plan, grouped into **Tracks**, **Mixes**, **Cases** and **Removals** (only if enabled). Untick unwanted items, or use **Tick all** / **Leave all out** per group.
5. Press the Sync button.

If the plan is reported out of date, check again.

## Sync settings

In the **Sync settings** section of the Sync page, and in Settings, Library.

| Situation | Choices |
|---|---|
| Track on both sides | Take the sending library's details when newer (default); always take them; leave both |
| Mix with the same name | Replace when the sending library's is newer; always replace; keep both (the new one is renamed); leave the existing one |
| Case with the same name | Merge (keep its order, add missing tracks); replace; leave the existing one |
| Play counts | Keep the higher count and later date; or leave them |
| Tags, colour labels, energy | Travel with copied tracks (on or off) |
| Removals | Off by default. When on, items missing from the sending library are listed under Removals and removed only if left ticked and confirmed |

Back up before enabling removals: see [The Device page](device-page.md).

## Bring plays into the computer library

Choose **Get from Pacemaker**. [History](history.md) uses the plays.

## Related

- [Background analysis](background-analysis.md) pauses while a plan is on screen.
- To copy a few items by drag and drop, use [Libraries](libraries.md).
