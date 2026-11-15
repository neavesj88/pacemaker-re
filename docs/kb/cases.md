---
id: cases
title: Cases and smart cases
topic: Cases
order: 1
keywords: cases, case, playlist, crate, smart case, rules, smart, merge, duplicate, export, order, favourites, refresh, star case, add tracks, setlist, virtualdj
summary: Cases are the Pacemaker's playlists. Make them by hand, or let a smart case fill itself from rules.
---

# Cases and smart cases

A **case** is the Pacemaker's word for a playlist. The **Cases** entry in the sidebar lists them, and the number beside it is how many there are.

## Making and changing cases

- **New case** makes an empty one, and **From selection** makes one from the tracks selected in the Tracks view. Add tracks from the Tracks view (right-click, Add to case, or drag rows onto a case in the sidebar) or with **Add tracks...** inside the case.
- Open a case to see its tracks. You can **Play** it, remove tracks, and move the selected ones up or down (Alt+Up and Alt+Down) to change the order, then **Save this order to the case**.
- **Copy to** and **Move to** put selected tracks into another case.
- **Rename**, **Duplicate** and **Delete** are on each row. **Merge** combines several cases into a new or an existing one, without repeats.
- A case has its own name, genre, year, producer, remixer and comments. Press **Save** to keep changes and **Revert** to drop them.
- **Tags and colour** labels cases too (kept by this app only).
- A track can be in many cases. Right-click a track and choose Which cases hold this track to see them.
- Favourites is a case too. The heart in the Tracks view adds a track to it.

## Using a case

- **Build a set** orders the tracks by key and tempo: see [Set builder](set-builder.md).
- **Export this case** saves a playlist or list for other software, with the audio copied alongside if you like. **Setlist** makes a printable list, and **VirtualDJ** exports a VirtualDJ list. See [Import and export](import-export.md).
- **Export all** (or **Export N ticked** once you tick cases on the list) saves several cases in one go, one file per case.

## Smart cases

A smart case fills itself from **rules**, so it stays up to date as the library changes.

1. In the sidebar, press **+ New** under Smart cases.
2. Add rules. Each rule is one condition, and you choose how they combine: all of them (AND), any of them (OR) or none of them (NOT). Groups can be nested.
3. Choose how to order the result, and optionally keep only the first N tracks.
4. The count on the right shows how many tracks match as you edit. A list of the matching tracks is shown below.
5. **Save**.

The rules you can use:

| Group | Rules |
|---|---|
| Text | Title, Artist, Album, Genre, Label, Comments or Remixer that contains, is, or starts with some text |
| Numbers | BPM between, Key is, Key compatible with, Year between, Length between, Rating at least, Format is |
| Playing | Never played, Played at least, Played between, Last played within, Not played for (stale) |
| Library | Added within the last, Added more than ... ago, Favourites, Case membership, Analysed |
| Labels | Tags (has any of, has all of, has none of), Colour label, Energy between |

A smart case lives only in this app until you switch on **Put on Pacemaker** (or **Keep a star case in the library**, for a library on this computer). That writes a normal case with the matching tracks, so the Pacemaker can play it. The case is refreshed:

- after you are asked (**Ask me**, the choice that shows what would change and waits for a yes),
- by itself when the library changes (**Do it automatically**), or
- only when you press **Refresh smart cases** or **Refresh the case now** (**Only when I press Refresh smart cases**).

The choice is under **Smart case settings** on the Cases page. **Refresh smart cases...** shows what rewriting every switched-on smart case would change, then does it if you agree. A case made by a smart case is marked "smart", and its name is set by the smart case.

The Settings page has the smart case and case export settings: see [The Settings page](settings-page.md).
