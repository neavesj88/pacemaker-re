---
id: import-export
title: Import and export
topic: Library tools
order: 5
keywords: import, export, add tracks, drag and drop, watch folders, rekordbox, itunes, serato, traktor, m3u, m3u8, playlist, csv, xml, crate, virtualdj, setlist, print, collection
summary: Bring tracks and playlists in from files and other DJ software, watch folders for new music, and send cases, mixes and the library out as lists.
---

# Import and export

**Import** (under Tools) brings music and lists in and out. Every operation shows a preview first.

## Add music files

Press **Add tracks...** or drop files or folders anywhere in the window. Each file is analysed and added with its beat grid.

Adding is unavailable while another job runs, while the library is read-only, or after a Pacemaker was ejected.

## Watch folders

The app checks watch folders for new music. Settings: how often it looks, how old a file must be, and whether new tracks are added automatically or wait for you.

## Import from other DJ software

| Source | Imported | Not imported |
|---|---|---|
| rekordbox (XML) | Ratings, playlists (as cases), first memory cue (optional) | Beat grids |
| iTunes / Music (Library.xml) | Ratings (0 to 100 become 0 to 5 stars), playlists as cases | |
| Serato | Crates, as cases | |
| Traktor (collection.nml) | Ratings, playlists (as cases), first cue of each track (optional) | Colours, beat grids |

- Serato: pick the `_Serato_` folder for every crate, or one `.crate` file.
- Untick plan items to leave them out.
- **Import playlist** reads an M3U or M3U8 playlist as a case.

## Export

- Export a case, smart case, mix or the whole library as an **M3U8** playlist or **CSV** list. Playlist files can include copies of the audio.
- On the case page, **Export this case**. On the case list, **Export all**. See [Cases and smart cases](cases.md).
- rekordbox export writes tracks with tempo grids, ratings, keys and cases.
- VirtualDJ: exports a case or mix as a VirtualDJ list, optionally with audio copies. Naming and placement are in Settings.
- Print a setlist of a case or mix. Columns, time and key styles, paper size and text size are in Settings.

## Related

- [Mix tools and audio export](mixes-tools-export.md)
- [Request list matcher](request-matcher.md)
