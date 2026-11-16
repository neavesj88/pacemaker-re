---
id: mixes-building
title: Building a mix
topic: Mixes
order: 1
keywords: mix, mixes, mix editor, timeline, clip, track table, add tracks, drag, playhead, snap, zoom, play, preview, live play, save, save as, undo, pmk, import, setlist, track list, cue sheet, mix tempo, unsaved
summary: Make a mix from the track table under the timeline, move and trim clips, play it live, and save it.
---

# Building a mix

A **mix** is a set of tracks laid out on a timeline with crossfades and tools, saved in a file the Pacemaker can play. **Mixes** in the sidebar lists them. The Mix editor is where you build one. The small **?** beside the mix name opens this article.

## The mix list

- **New mix** starts an empty one. **Open**, **Rename**, **Duplicate** and **Delete** work on the picked mixes.
- **Import .pmk** brings in a mix file made with the original Tonium Editor or the Pacemaker. **Export .pmk** writes saved mixes as .pmk files.
- **Setlist** makes a printable list of a mix. **Tags and colour** labels mixes (kept by this app only).
- The search box finds mixes by name (and, if you turn it on, genre, producer, remixer and comments).

## Putting tracks in

Open a mix and press **Tracks** in the toolbar to show the track table under the timeline. It is the real Tracks table, with its own search box, filters (including the case filter), columns and sort, remembered separately from the Tracks view. Drag the grip on its top edge to make it taller or shorter.

Add tracks any of these ways:

- Drag rows from the table onto the mix bar.
- Double-click a row, or press Enter, to add it at the playhead.
- Select rows and press **Add at playhead** or **Add to end**.
- From anywhere, press **Add to mix** on a track in [the Tracks view](tracks-view.md).

Tracks are matched to the mix tempo as they are added if that setting is on: see [Crossfades, tempo and Match all](mixes-crossfades-tempo.md).

## Moving around the timeline

- Clips sit on one bar in their own colours. Drag a clip by its name to move it. Drag a clip edge to trim it. Where two clips overlap they are drawn over each other, each in its own colour and see-through, with the crossfade lines over both. A click in the overlap takes the clip that comes in last; reach the earlier one on its own part, by its edge, or with Tab.
- **Snap** (tick box, on by default) lines a clip up with its neighbour's beat grid when you drag it over, or up against, another clip: downbeat on downbeat for Bar, phrase start on phrase start for Phrase, any beat for Beat. The box next to it chooses Beat, Bar (the default), Phrase (8 bars) or Section (where detected sections begin). Hold **Alt** while dragging to move freely. Both choices are remembered. A green line shows the beat it lined up on.
- **Beat-align** (toolbar, the clip panel, and the right-click menu) does the same in one step for the selected clip, or for the incoming clip of a selected crossfade, and Undo puts it back. It moves the clip to the nearest place where the grids agree and does not change its tempo, so use **Match all** first when the tempos differ. A clip whose track has no beat grid is not moved; the message says so.
- **Zoom** and **Fit** change the scale. Ctrl and the wheel zooms at the pointer.
- The **sections** tick draws the detected sections on the clips.
- **Curves** (Gain, Tempo, Key) draw one kind of envelope over the bar so you can edit it: click on the selected clip to add a node, drag to move it, right-click to remove it.
- Click a clip, a crossfade or a tool block to see its panel on the right. With nothing selected, the panel is the mix itself: name, mix tempo, genre and comments.

Delete removes the selected clips after asking, and Ctrl+Z brings them back until you save and close. Ctrl+Z and Ctrl+Y undo and redo anything in the mix.

## Playing the mix

The transport at the bottom of the editor plays the mix live from the computer.

- **Play** or the Space bar plays and pauses. The first and last buttons jump to the start and the end.
- **Point at the timeline and press Space** to move the playhead to that spot. A thin line with a time shows where it will go. If the mix is playing it carries on from there; if it is stopped, only the playhead moves. With **Snap** on, the playhead goes to the nearest beat (when the track under it has a beat grid); hold **Alt** while pressing Space to put it exactly under the pointer. With the pointer anywhere else, Space plays and pauses.
- **follow playhead** scrolls the timeline with the playhead. **Master** is the preview volume only (it is not saved in the mix).
- The preview plays the crossfades, tools and tempo changes of the mix. When you have set loudness for the export, the preview plays at the level the last analysis found, and says "export level".
- A long mix reads the tracks nearest the playhead first. How much decoded audio is kept in memory is the Playback memory setting: by default the computer decides (an eighth of its memory, from 256 to 1024 MB, so a small computer keeps fewer tracks decoded), and you can type your own number instead.

## Saving

- **Save** (Ctrl+S) writes the mix to the library. The button is greyed until something changed.
- **Save as** (Ctrl+Shift+S) saves a new mix under another name and leaves this one as last saved.
- A coloured dot beside the name means unsaved changes. Leaving with unsaved changes asks whether to save, discard or keep editing.
- If the toolbar says "N problems blocking save", hover it for the list. Typical problems are two tools of the same kind overlapping on one clip.
- If a restore or Sync changed the mix in the library while you had it open, you can Reload (drop your edits) or Keep my edits.

> The **mix tempo** is not stored in the mix file. When a mix is opened again it starts at its first track's tempo, and the toolbar says so.

## Getting the mix out

- **Track list** gives the start times of the tracks as text, CSV, YouTube chapters or a CUE sheet.
- **Export audio** renders the mix to a file: see [Mix tools and audio export](mixes-tools-export.md).
- **Setlist** prints a list.
