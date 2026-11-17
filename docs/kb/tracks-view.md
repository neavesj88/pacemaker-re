---
id: tracks-view
title: The Tracks view
topic: Tracks
order: 1
keywords: tracks, table, columns, sort, filter, genre, bpm, key, case, analysed, missing, suspect, favourite, heart, context menu, right click, saved views, presets, column chooser, waveform column, player, space, cue, double-click, shortcut
summary: Lists every track in the open library with sortable columns, filters, saved filters, a right-click menu and a player bar.
---

# The Tracks view

**Tracks** lists every track in the library. Click a row to show it in [the Inspector](track-inspector-labels.md). Double-click a row, or press Enter, to play it.

## Search and filter

- Search accepts words and queries such as `bpm:120-128`. See [Search syntax](search-syntax.md).
- **Filters**: genre, BPM minimum and maximum, key (one key, or all keys that mix with it), case, tags.
- **Analysed**: tracks with a beat grid. **Missing**: grid missing or unreadable. **Suspect**: grid looks wrong for the genre.
- **Rules...** on Suspect sets the tempo windows and previews the half or double fix.
- **Clear filters** resets all filters.

The saved filters menu stores a search, the filters and optionally the sort under a name.

## Columns and sort

| Column | Content |
|---|---|
| Heart | Favourite. Click to toggle |
| Colour | Colour label |
| Title, Artist | Always shown |
| BPM | Tempo. A dash means the track needs analysis |
| Key | Camelot code, coloured by key |
| Waveform | Small picture of the audio |
| Energy | Your 1 to 10 rating |
| Energy (auto) | Estimate from the audio, blank until analysed. See [Background analysis](background-analysis.md) |
| Rating | 0 to 5 stars |
| Plays | Times the Pacemaker played it |
| Time | Length |
| Genre, Tags | Genre and your tags |

1. Press **Columns** to add Album, Year, Label, Remixer, Composer, Comments, Date added, Last played, Format, Bitrate, Size, Cases or Case # (position in the case chosen in the filter bar).
2. Drag a heading to reorder. Drag its edge to resize.
3. Click a heading to sort. Click again to reverse. Shift-click adds a second sort.
4. **Reset columns** restores the defaults.

## Right-click menu

Right-click a row or a selection. Actions apply to all selected rows.

- Play, Edit metadata, Tags, colour and energy, Add to mix, Add to case, Favourite, Show in Explorer.
- Which cases hold this track.
- Build a set ([Set builder](set-builder.md)).
- Key: detect or set. BPM: set, halve or double. See [BPM, keys and waveforms](bpm-keys-waveforms.md).
- Re-analyse the beat grid. Beat grid tools previews fixes for many tracks.
- Energy (auto), Sections analysis, Edit grid ([The beat grid editor](grid-editor.md)).

Drag rows onto a case in the sidebar to add them.

## Needs-analysis note

A note at the bottom of the sidebar counts tracks with no usable beat grid.

- **Show them** filters the table to those tracks.
- **Analyse N...** starts analysis.
- **Review** appears for tracks read outside the BPM range. Buttons for energy, sections and keys appear when there is more to do.

See [Background analysis](background-analysis.md).

## Player bar

| Control | Action |
|---|---|
| Space | Play and pause |
| C | CUE while held |
| Double-click the waveform | Move the cue flag to that point |
| Click or drag the waveform | Move the playing position |

While a mix is loaded in the bar there is no cue to move.
