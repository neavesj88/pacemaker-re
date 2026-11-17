---
id: cases
title: Cases and smart cases
topic: Cases
order: 1
keywords: cases, case, playlist, crate, smart case, rules, smart, merge, duplicate, export, order, favourites, refresh, star case, add tracks, setlist, virtualdj
summary: Cases are the Pacemaker's playlists, made by hand or filled automatically by smart case rules.
---

# Cases and smart cases

A **case** is a Pacemaker playlist. A track can be in many cases.

## Create and edit cases

- **New case** makes an empty case. **From selection** uses the selected tracks.
- Add tracks with right-click, Add to case, by dragging rows onto a case in the sidebar, or with **Add tracks...** inside the case.
- Open a case to see its tracks. **Play** it, remove tracks, or reorder: select tracks, press Alt+Up or Alt+Down, then **Save this order to the case**.
- **Copy to** and **Move to** put selected tracks into another case.
- **Rename**, **Duplicate** and **Delete** are on each row.
- **Merge** combines cases into a new or existing case, without repeats.
- A case has its own name, genre, year, producer, remixer and comments. **Save** keeps changes, **Revert** drops them.
- **Tags and colour** label cases too.
- The Tracks view heart adds a track to the Favourites case.

## Use a case

- **Build a set** orders tracks by key and tempo. See [Set builder](set-builder.md).
- **Export this case** saves a playlist for other software, optionally with the audio copied.
- **Setlist** makes a printable list. **VirtualDJ** exports a VirtualDJ list. See [Import and export](import-export.md).
- **Export all**, or **Export N ticked**, saves one file per case.

## Smart cases

A smart case fills itself from rules and updates as the library changes.

1. Press **+ New** under Smart cases in the sidebar.
2. Add rules. Combine them with all (AND), any (OR) or none (NOT). Groups can be nested.
3. Choose the order. Optionally keep only the first N tracks.
4. Check the match count and matching tracks.
5. **Save**.

| Group | Rules |
|---|---|
| Text | Title, Artist, Album, Genre, Label, Comments or Remixer that contains, is, or starts with text |
| Numbers | BPM between, Key is, Key compatible with, Year between, Length between, Rating at least, Format is |
| Playing | Never played, Played at least, Played between, Last played within, Not played for (stale) |
| Library | Added within the last, Added more than ... ago, Favourites, Case membership, Analysed |
| Labels | Tags (has any of, has all of, has none of), Colour label, Energy between |

## Put a smart case on the Pacemaker

Switch on **Put on Pacemaker**, or **Keep a star case in the library** for a library on this computer, to write the matching tracks as a normal case the Pacemaker can play.

Set the refresh mode under **Smart case settings** on the Cases page.

| Mode | Behaviour |
|---|---|
| **Ask me** | Shows what would change and waits for confirmation |
| **Do it automatically** | Refreshes when the library changes |
| **Only when I press Refresh smart cases** | Refreshes on demand with **Refresh smart cases...** or **Refresh the case now** |

**Refresh smart cases...** shows what would change for every switched-on smart case, then applies it after confirmation.

Export options are on [The Settings page](settings-page.md).
