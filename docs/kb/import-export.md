---
id: import-export
title: Import and export
topic: Library tools
order: 5
keywords: import, export, add tracks, drag and drop, watch folders, rekordbox, itunes, serato, traktor, m3u, m3u8, playlist, csv, xml, crate, virtualdj, setlist, print, collection
summary: Bring tracks and playlists in from files and other DJ software, watch folders for new music, and send cases, mixes and the library out as lists.
---

# Import and export

**Import** (under Tools) is the one place for getting music and lists in and out. Everything shows you a preview before it writes.

## Music files

**Add tracks...** (also at the top of every view) picks files. You can also drop files or folders anywhere in the window. Each is analysed, copied into the library and added with its beat grid. Adding is unavailable while another job runs, while the library is read-only, or after a Pacemaker was ejected.

## Watch folders

Folders on this computer the app checks for new music. Under Settings you choose how often it looks, how old a file must be before it counts (so a file still being copied is not picked up half-written) and whether it adds tracks by itself or waits for you. Results are listed below the folder list.

## Other DJ software

| Source | What comes in | What does not |
|---|---|---|
| rekordbox (XML) | Ratings, playlists and the first memory cue become ratings, cases and (if you choose) cue points. Export writes tracks with their tempo grids, ratings, keys and cases | Beat grids are not imported |
| iTunes / Music (Library.xml) | Ratings (0 to 100 become 0 to 5 stars) and playlists, as cases | |
| Serato | Crates become cases. Pick the `_Serato_` folder to read every crate in it, or a single `.crate` file. A crate holds only file paths, so tracks are matched to the library by path | |
| Traktor (collection.nml) | Ratings, playlists and the first cue of each track, as ratings, cases and (if you choose) cue points | Colours and beat grids |

Each import shows a plan first, and you can leave items out.

## Playlists and lists

- **Import playlist** reads an M3U or M3U8 playlist as a case (relative and absolute paths, and `#EXTINF` titles).
- Export a case, a smart case, a mix or the whole library as an **M3U8** playlist or a **CSV** list.

## Other exports

- **Cases:** Export this case on the case page, or **Export all** on the case list, for playlist files with the audio copied alongside if you like. See [Cases and smart cases](cases.md).
- **VirtualDJ:** exports a case or a mix as a VirtualDJ list, with copies of the audio if you want them, named and placed as you choose in the settings.
- **Setlist:** a printable list of a case or a mix, with columns, time and key styles, paper and text size set under Settings.
- **Mix audio and track lists:** see [Mix tools and audio export](mixes-tools-export.md).

To match a list of wanted tracks against what you own, see [Request list matcher](request-matcher.md).
