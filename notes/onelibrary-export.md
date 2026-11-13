# OneLibrary / Device Library Plus export: research result and what was built instead

Researched 11 Nov 2026 (step I4 of the interop work). Question: is OneLibrary (AlphaTheta's name for
what rekordbox calls Device Library Plus, `exportLibrary.db` on the USB stick) publicly specified
well enough to write an exporter that "follows the documented format exactly"?

**Answer: no. It is reverse-engineered, not specified, and several things an exporter needs are
unknown or unusable here. No OneLibrary exporter was written. A VirtualDJ list export was built
instead, because that format is documented by its vendor.**

## What is known (with sources)

- It is one SQLite file, `exportLibrary.db`, encrypted with SQLCipher. pyrekordbox's format page
  says all Device Libraries use the same key, which is not licence or machine dependent. The page
  does not print the key; pyrekordbox's source keeps it as an obfuscated blob and de-obfuscates it
  at run time. This note does not repeat it.
- The schema is "a selection of tables from the main database". pyrekordbox documents 22 tables
  with column names and short descriptions (`album`, `artist`, `category`, `color`, `content`,
  `cue`, `genre`, `history`, `history_content`, `hotCueBankList`, `hotCueBankList_cue`, `image`,
  `key`, `label`, `menuItem`, `myTag`, `myTag_content`, `playlist`, `playlist_content`, `property`,
  `recommendedLike`, `sort`), and its SQLAlchemy models carry types and an enum for file and
  playlist types.
- AlphaTheta says OneLibrary and Device Library Plus are the same thing, and names Algoriddim,
  Native Instruments and VirtualDJ as partners. It publishes no technical specification on its
  OneLibrary page, and a web search for a developer or SDK document found none. Its FAQ is quoted
  by a trade site as not guaranteeing that every import and export works.
- Devices that read it (pyrekordbox, rbox): OPUS-QUAD, OMNIS-DUO, XDJ-AZ; rbox adds the CDJ-3000X.
  VirtualDJ now exports it (trade press, 15 Sep 2026).
- Two community libraries implement it: pyrekordbox (Python, MIT) and rbox (Rust).

## Why that is not enough to write an exporter

1. **The file cannot be written with what this project has.** The workspace uses plain bundled
   SQLite (`rusqlite` with `bundled`). SQLCipher needs a different build of SQLite (and, usually,
   OpenSSL). Adding it is a large dependency change, which the architecture forbids mid-run
   (`app/ARCHITECTURE.md`, rule 6) and which is out of proportion for a format that is not
   specified.
2. **The key is only available as an obfuscated constant in someone else's source.** Copying it
   into this repository is not "following a documented format"; it is reproducing a
   reverse-engineered secret. No vendor document grants the right to.
3. **Column names are not a format.** What is missing: the exact DDL (types, constraints, indexes,
   triggers), the SQLCipher parameters the device expects (pyrekordbox passes only the key), the
   values the player menus need in `menuItem`, `category`, `sort` and `property` for browsing to
   work, what `analysisDataFilePath` must point at (the device reads analysis files for waveforms
   and beat grids; their layout is a separate, also reverse-engineered, format), how `content.path`
   is resolved on the stick, and how a database that does not match is handled (the vendor's own
   FAQ gives no guarantee).
4. **Nothing can be tested.** No supported player (or a rekordbox install) is available to this
   project, so an exporter could only be checked against its own assumptions. A USB stick that a
   club player rejects, or half-loads, is a bad outcome for a feature whose purpose is playing out.
5. **A Pacemaker library has no beat-grid or cue data in the shape the format wants.** Tempo grids
   live in `.str` sidecars and the device holds one cue per track; mapping them to `cue` rows and
   ANLZ files would be inventing semantics.

If a vendor specification appears, or the project gains a way to test on supported hardware, this
is worth revisiting: the table and column names above would be the starting point, and the
SQLCipher question would need an explicit decision.

## What was built: VirtualDJ list export

Engine DJ's `m.db` was the other candidate. The vendor's own developer note says tools must not
change the schema of the files in `Database2` ("Engine DJ to refuse to load the database"), and
only community pages (Mixxx wiki, the endjine crate) describe the schema, some fields still marked
unknown. That is not a documented format either.

VirtualDJ publishes the structure of its lists on VDJPedia. Since VirtualDJ 2024 a playlist or
virtual folder is one XML file in the "My Lists" folder, named after the list:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<VirtualFolder noDuplicates="yes" singleDrive="no" ordered="yes">
  <song path="..." size="..." artist="..." title="..." songlength="123.0" idx="0" />
</VirtualFolder>
```

Documented there: root attributes `noDuplicates`, `singleDrive` (default no) and `ordered`; per
song `path` (the only required one, an absolute path to the file), `size` (recommended: bytes, so
VirtualDJ can match a song already in its database), and optional `artist`, `title`, `remix`,
`songlength` (seconds with one decimal), `bpm`, `key`, `idx` (from 0 in an ordered list),
`singer`, `keychange`, `playtime`. The page's own example writes `ordered` with typographic
quotes, a typo the exporter does not copy.

Implemented in `app/src-tauri/src/commands/export/vdj.rs`, with the dialog in
`app/src/lib/views/vdj/` (button "VirtualDJ..." on an open case):

- One case, or the tracks selected in it, becomes `<folder>/<list name>/<list name>.xml`.
- Optional copy of the audio to `<folder>/<list name>/Audio/`, named "Artist - Title.ext" or as on
  the Pacemaker; the list then points at the copies by full path. The path can be given as the
  stick will appear on the DJ's computer (`path_base`), since the format needs absolute paths.
- Without copying, the list points at the files on the Pacemaker volume.
- Written: path, size (read from the file), artist, title, songlength, idx. **Not written: `bpm`
  and `key`**: the list page does not say in which unit `bpm` is stored (the database page says
  seconds between beats, the list page is silent), so writing it would be a guess. VirtualDJ
  reads both from the audio when it analyses the file.
- Nothing is written to the Pacemaker. Existing audio copies are never overwritten (same name and
  size, plus the first and last 64 kB, is reused, a different file gets a numbered name); the list file is replaced only after an
  explicit tick; the list is written last, so a stopped export leaves no list that points at
  missing files.

### Unverified

- No copy of VirtualDJ was available, so the file has not been opened in it. The shape follows the
  VDJPedia page; the tests check the shape, not VirtualDJ's reaction.
- Whether VirtualDJ accepts a list from outside its "My Lists" folder by drag and drop is not
  documented; the dialog says to copy the `.xml` into My Lists. The folder's exact name on disk
  (`My Lists` on the wiki, `MyLists` in forum posts) was not confirmed.
- Whether a path whose drive letter differs from the one VirtualDJ saw before is matched by the
  recommended `size`, and what `singleDrive` changes, are not documented in detail.
- Free space on the destination is not checked before copying; a full disk stops the export with
  the files already copied left in place and no list written.

## Sources

- pyrekordbox, "Rekordbox OneLibrary Format": https://github.com/dylanljones/pyrekordbox/blob/master/docs/source/formats/onelibrary.md (MIT licence; models in `pyrekordbox/onelibrary/models.py`, database handler in `database.py`)
- AlphaTheta OneLibrary page: https://alphatheta.com/en/onelibrary/
- rbox (Rust library for rekordbox data): https://github.com/dylanljones/rbox
- Digital DJ Tips, "OneLibrary: 5 things you need to know": https://www.digitaldjtips.com/alphatheta-onelibrary-5-things-you-need-to-know/
- VirtualDJ supports OneLibrary (export to USB): https://sonicstate.com/news/2026/09/15/-virtualdj-officially-supports-onelibrary/
- Engine DJ, support for third-party database tools: https://support.enginedj.com/en/support/solutions/articles/69000834165
- VDJPedia, Lists (the list XML): https://virtualdj.com/wiki/Lists.html
- VDJPedia, VDJ database (database.xml structure; its note advises against external writes): https://virtualdj.com/wiki/VDJ_database.html
- VirtualDJ manual, Smart Folders (what a List folder is): https://virtualdj.com/wiki/VirtualFolder.html
