---
id: tracks-view
title: The Tracks view
topic: Tracks
order: 1
keywords: tracks, table, columns, sort, filter, genre, bpm, key, case, analysed, missing, suspect, favourite, heart, context menu, right click, saved views, presets, column chooser, waveform column
summary: The table of every track in the open library, its columns, filters, sorting, saved filters and the right-click menu.
---

# The Tracks view

**Tracks** (top of the sidebar) is the table of every track in the open library. The number beside it is how many tracks there are. Click a row to select it and see its details in [the Inspector](track-inspector-labels.md) on the right. Double-click a row, or press Enter, to play it.

## Searching and filtering

- The search box at the top takes plain words and a small query language (`bpm:120-128`, `key:~8A`, `-genre:techno`, `a OR b`). See [Search syntax](search-syntax.md).
- The **Filters** button opens genre, BPM minimum and maximum, key (one key, or all that mix with a key), case and tag filters.
- Three quick buttons: **Analysed** (only tracks with a beat grid), **Missing** (tracks whose beat grid is missing or unreadable) and **Suspect** (tracks whose grid looks wrong for their genre). Suspect has a **Rules...** button for the tempo windows and a preview of the suggested half or double fix.
- **Clear filters** puts everything back.
- The saved filters menu keeps a search plus the filters (and, if you ask, the sort) under a name. They are kept by this app, not on the Pacemaker.

## Columns

| Column | What it shows |
|---|---|
| Heart | Favourite. Click to toggle. Favourites are kept as a case called Favourites |
| Colour | A colour label (red, orange, yellow, green, teal, blue, purple, pink). Kept by this app |
| Title, Artist | Always shown |
| BPM | Tempo. A dash means the track needs analysis |
| Key | Musical key, written as a Camelot code such as 8A, coloured by key |
| Waveform | A small picture of the audio |
| Energy | Your own 1 to 10 rating of how intense the track is |
| Energy (auto) | An estimate from the audio, blank until analysed. See [Background analysis](background-analysis.md) |
| Rating | 0 to 5 stars |
| Plays | How many times the Pacemaker played it |
| Time | Length |
| Genre, Tags | Genre, and your tags |

More columns are available from the **Columns** button: Album, Year, Label, Remixer, Composer, Comments, Date added, Last played, Format, Bitrate, Size, Cases and Case # (the position in the case chosen in the filter bar). **Reset columns** puts the default set back.

Drag a column heading to reorder it and drag its edge to resize. Click a heading to sort by it, click again to reverse, and Shift-click to add a second sort. By default Title and Artist share spare width on a wide window; a column you drag keeps the width you gave it. The sort, the columns and the filters are remembered for each library.

## The right-click menu

Right-click a row (or several selected rows) for:

- Play, Edit metadata, Tags, colour and energy, Add to mix, Add to case, Which cases hold this track, Build a set (see [Set builder](set-builder.md)), Favourite.
- Key: detect or set. BPM: set, halve or double. See [BPM, keys and waveforms](bpm-keys-waveforms.md).
- Re-analyse the beat grid, beat grid tools (a preview of fixes for many tracks at once).
- Energy (auto) and Sections analysis. Edit grid (see [The beat grid editor](grid-editor.md)). Show in Explorer.

Rows can be dragged onto a case in the sidebar. Ticking several rows first applies the action to all of them.

## The needs-analysis note

At the bottom of the sidebar a note says how many tracks need analysis (they have no usable beat grid, and their BPM shows a dash), with **Show them** to filter the table to those tracks and **Analyse N...** to start. Other buttons appear there when there is more to do: tracks read outside the BPM range (**Review**), energy, sections and keys. See [Background analysis](background-analysis.md).
