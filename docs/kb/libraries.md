---
id: libraries
title: Libraries (two panes)
topic: Libraries and sync
order: 2
keywords: libraries, pacemaker connected, plug in, popup, two libraries, drag, drop, copy, two panes, left, right, swap, into case, tracks, mixes, cases
summary: See two libraries side by side and drag tracks, mixes or cases from one to the other. A drag only ever copies.
---

# Libraries (two panes)

**Libraries** (in the sidebar) shows the open library on the left and another library on the right.

## Panes

Each pane has a library picker, **Tracks**, **Mixes** and **Cases** tabs, and a search box.

- Click to pick a row, Shift-click for a range, Ctrl-click to add or remove one.
- The arrows button swaps the libraries.

## Copy items

1. Pick rows.
2. Drag them to the other pane, or use **Copy to ...** or the row menu.
3. Review the dialog. Untick items to leave them out.
4. Confirm.

A copy only adds. Nothing on the target is removed or overwritten. If the copy does not fit, free space or pick fewer tracks.

## Copy tracks into a case

Drop tracks on a case row in the **Cases** tab, or use **Into case...** and choose a case.

## Compare whole libraries

To compare whole libraries, bring plays across, or choose what happens to items on both sides, use [Sync](sync.md).

## Pacemaker connected

While the app runs it looks every few seconds for a connected Pacemaker. When one appears that is not the open library, a small prompt shows the drive:

- **Open it** opens that Pacemaker as choosing it in the library picker does (it asks first if you have unsaved changes), then shows the Device page.
- **Not now** closes the prompt until the drive is unplugged and plugged in again.
- **Don't ask again** turns the prompt off.

Looking only checks which drives are there. Nothing on the Pacemaker is opened or read until you click **Open it**.

Both choices are in **Settings > Library > Pacemaker connected**: the prompt, and how often to look (0 = never look). A Pacemaker that was already plugged in when the app started is not announced.
