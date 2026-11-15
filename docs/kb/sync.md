---
id: sync
title: Sync
topic: Libraries and sync
order: 1
keywords: sync, send to pacemaker, get from pacemaker, copy, compare, plan, removals, both sides, conflict, newer, replace, keep both, merge, plays counters, labels, from, to
summary: Copy tracks, mixes and cases from one library to the other, either way, after seeing exactly what would change.
---

# Sync

**Sync** copies tracks, mixes and cases between your library on this computer and a Pacemaker (or another library). It is in the sidebar once there is a library on this computer.

## Steps

1. Connect the Pacemaker (or put its storage in a card reader) and press **Rescan** if it is not listed. If its library is in a folder, use **Open folder...**.
2. Choose **From** and **To**. The arrows button swaps them. The shortcuts **Send to Pacemaker** and **Get from Pacemaker** set the pair for you.
3. Press **Check what would change**. Nothing is written yet.
4. Read the plan. It is grouped into **Tracks**, **Mixes**, **Cases** and (only if you turned it on) **Removals**. Each item says what will be done to it. Untick any you do not want, with **Tick all** or **Leave all out** per group.
5. Press the Sync button. A progress card shows the work, and the result says what happened and whether anything failed.

If the plan was made before something changed (another job ran, or a key was written), Sync says it is out of date and asks you to check again. It never applies a stale plan.

## What happens to things on both sides

The **Sync settings** (a section on the Sync page, and under Settings, Library) decide:

| Situation | Choices |
|---|---|
| A track is on both sides | Take the sending library's details when they are newer (default), always take the sending library's details, or leave both |
| A mix has the same name | Replace it when the sending library's is newer, always replace it, keep both (the new one gets a different name), or leave the one that is there |
| A case has the same name | Merge (keep its order and add the tracks it lacks), replace it, or leave the one that is there |
| Play counts | Keep the higher count and the later date, or leave them |
| Tags, colour labels and energy | Travel with the copied tracks (on or off) |
| Removals | **Off by default: nothing is ever removed.** When on, items the receiving library has and the sending library lacks are listed under Removals, each named, and removed only if you leave them ticked and confirm |

**Back up first** if you turn removals on: see [The Device page](device-page.md).

## Tips

- Sync is a copy from one side to the other, not a merge of both. To bring plays from a Pacemaker into your library on this computer, choose Get from Pacemaker: [History](history.md) uses them.
- Background analysis waits while a Sync plan is on screen, so a key written meanwhile cannot make the plan out of date. See [Background analysis](background-analysis.md).
- To copy a few items by hand with drag and drop, use [Libraries](libraries.md).
