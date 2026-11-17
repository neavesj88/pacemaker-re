---
id: mixes-building
title: Building a mix
topic: Mixes
order: 1
keywords: view menu, clip height, whole mix strip, overview, shrink, mix, mixes, mix editor, timeline, clip, track table, add tracks, drag, playhead, snap, zoom, follow, scroll, double-click, cue, play, preview, live play, save, save as, undo, pmk, import, setlist, track list, cue sheet, mix tempo, unsaved
summary: Build a mix from the track table, arrange and trim clips on the timeline, play it live and save it.
---

# Building a mix

Open **Mixes** in the sidebar to list mixes.

## The mix list

| Control | Action |
|---|---|
| **New mix** | Start an empty mix |
| **Open**, **Rename**, **Duplicate**, **Delete** | Act on the picked mixes |
| **Import .pmk** | Import a mix file from the original Tonium Editor or the Pacemaker |
| **Export .pmk** | Write saved mixes as .pmk files |
| **Setlist** | Print a list of a mix |
| Search box | Find mixes by name; optionally genre, producer, remixer, comments |
| **Tags and colour** | Label mixes |

With nothing selected, the right panel edits the mix: name, mix tempo, genre, comments.

## Add tracks

Press **Tracks** in the toolbar to show the track table under the timeline. Drag its top edge to resize it.

- Drag rows onto the mix bar.
- Double-click a row, or press Enter, to add it at the playhead.
- Select rows and press **Add at playhead** or **Add to end**.
- Press **Add to mix** on a track in [the Tracks view](tracks-view.md).

Tempo on add: [Crossfades, tempo and Match all](mixes-crossfades-tempo.md).

## Move and trim clips

- Drag a clip by its name to move it. Drag a clip edge to trim it.
- In an overlap, a click selects the later clip. Select the earlier one with Tab.
- Crossfades and transition tools follow a clip you move, trim, Beat-align or retime. Setting: **Settings > Mixes > Auto transitions > When a clip moves** ([details](mixes-crossfades-tempo.md)).
- **Delete** removes the selected clips after a prompt.
- **Ctrl+Z** and **Ctrl+Y** undo and redo any mix edit.

## Snap

The **Snap** tick box (on by default) lines a dragged clip up with its neighbour's beat grid. The box beside it sets the unit:

| Unit | Lines up |
|---|---|
| Beat | Any beat |
| Bar (default) | Downbeat on downbeat |
| Phrase | Phrase start on phrase start (8 bars) |
| Section | Section starts |

Hold **Alt** while dragging to move freely.

## Beat-align

**Beat-align** (toolbar, clip panel, right-click menu) moves the selected clip, or the incoming clip of a selected crossfade, to the nearest place where the beat grids agree. Tempo is unchanged; use **Match all** first when tempos differ.

## Zoom and scroll

**Zoom** and **Fit** set the scale. **Ctrl** + wheel zooms at the pointer; **Shift** + wheel scrolls sideways.

## Timeline view

**View** (toolbar) options are also in **Settings > Mixes > Mix editor view**.

| Option | Effect |
|---|---|
| Clip height: *Shrinks as you zoom out* (default) | Clips shrink as you zoom out; the track table gets the room |
| Clip height: *Always full height* | Zoomed-in height at every zoom |
| Whole-mix strip (on by default) | Strip above the ruler showing the whole mix. Click to centre the view; drag the box to scroll |
| Sections on the clips | Show detected sections |
| Follow the playhead while playing | *Smoothly* or *A page at a time* (also **Settings > Mixes > Following the playhead**) |

## Curves

**Curves** (Gain, Tempo, Key) draws one envelope kind over the bar. Click the selected clip to add a node, drag to move it, right-click to remove it.

## Play the mix

The transport at the bottom plays the mix live.

| Control | Action |
|---|---|
| **Play** / Space | Play and pause |
| First / last buttons | Jump to start / end |
| Double-click ruler, clip or empty timeline | Move the playhead there (to the nearest snap point when Snap is on) |
| **Alt** + double-click | Put the playhead exactly under the pointer |
| **follow playhead** | Scroll the timeline with the playhead |
| **Follow again** | Resume following after you scrolled |
| **Master** | Preview volume |

- **Playback memory** setting: decoded audio held in memory. Default: an eighth of the computer's memory, 256 to 1024 MB; type a number to override.

## Save

| Action | Shortcut | Result |
|---|---|---|
| **Save** | Ctrl+S | Write the mix to the library |
| **Save as** | Ctrl+Shift+S | Save under another name |

- A dot beside the name marks unsaved changes. Closing asks whether to save, discard or keep editing.
- "N problems blocking save": hover for the list (usually two tools of one kind overlapping on a clip).
- If the mix changed in the library while open (restore or Sync), choose **Reload** or **Keep my edits**.

> The mix tempo is not saved. A reopened mix starts at its first track's tempo.

## Get the mix out

- **Track list**: track start times as text, CSV, YouTube chapters or a CUE sheet.
- **Export audio**: see [Mix tools and audio export](mixes-tools-export.md).
- **Setlist**: print a list.
