# Pacemaker Editor 2.0.2.14170: feature inventory, Revival gap analysis, roadmap

PROVISIONAL (written under time pressure, 10 Nov 2026). Evidence is from the Mac Editor i386 slice
(class/method names from `work/editor-mac.strings`, listing in `work/inv/classes.txt` and
`work/inv/methods.txt`, Ghidra output in `work/inv/dc-menu/` and `work/inv/dc-2/`), and the shared
`english.txt` language file (identical on Windows and Mac). "Inferred" = from symbol names only,
not decompiled. No personal data is included. Sections marked TODO are not yet inventoried.

Revival status basis: `app/src-tauri/src/lib.rs` exposes only list_volumes, open_library, case_tracks,
expand_audio_paths, add_tracks, reanalyse_tracks. `crates/pm-formats/src/library.rs` additionally has
`set_track_rating`, `create_case`, `set_case_tracks`, `delete_case` (not yet exposed by the app).
`crates/pm-formats/src/mixdoc.rs` only handles the zlib+XOR container, there is no MixDocument model.

## 1. Application structure

Main window with a menu bar (File, Edit, View, Controls, Community, Window, Help; `getMenuBarNames`).
Two top-level modes: Player view (`PlayerComponent`, single-track sequencer with a playlist) and Mix view
(`MixComponent`, multi-track sequencer), toggled by "Change Mix/Player View" (shortcut e). A library pane
(`LibraryComponent`) has Editor (computer) and Device sides (`ComputerLibraryComponent`,
`DeviceLibraryComponent`), each switching between Tracks, Mixes (`mixes_button_text`) and Cases. A
collapsible "mini window" stack (`MiniWindowList/Container/Header`) shows info panels
(`LibraryInfoWindow`, `DeviceInfoWindow`, `MixInfoWindow`, `CaseInfoWindow`, `SequencerInfoWindow`).
Undo/redo via `JuceUndoManager` (transaction limit constructed with 30000, 30; inferred meaning).

## 2. Library and browse

| Feature | Evidence | Revival | Backing code |
|---|---|---|---|
| Track table with sortable, resizable, saved columns: Title, Genre, Artist, Album, BPM, Time, Label, Track Number, Year, Times Played, Last Played, Location, Composer, Comments, File Size, Format, Rating, Bit Rate, Sample Rate, Cue Point, Date Added/Modified, Disc Number, Key, Producer, Remixer, File Type | `TrackListHeader` strings, `TrackTableComponent::createColumns`, `CustomTableListComponent::saveColumnSizes/loadColumnSizes` | partial (table exists; column set and persistence unverified) | `library.rs Track` |
| Free-text search box | `LibraryComponent::freeTextSearch`, `ControlToolBarComponent::updateSearchText`, string "Search" | check app (unverified) | none |
| Column filters / reset filters | `TrackLibraryComponent::getQueryFilter/resetFilters/removeNonExistingFilterValues` | missing | none |
| Type-to-find in table (1 s timer, space excluded) | `TrackTableComponent::keyPressed` | missing | none |
| "Show Not Analysed Tracks" view | `LibraryComponent::showNotAnalyzedTracks`, `setShowingUnanalyzedTracks` | partial (re-analyse of missing grids exists) | `reanalyse_tracks` |
| "Show Unavailable Tracks" (files missing on disk) + Preferences toggle | `menu_item_title_show_unavailable_tracks`, `flagTracksAsMissingOrFound` | missing | none |
| Shuffle, play next/previous in list | `TrackLibraryComponent::shuffle/playNextTrack` | missing | none |
| Remove from library, optional move files to trash/recycle bin, warning if track is used by a mix or case | `DeleteTrackDialog`, `referenced_track_remove_*` strings | missing | negative `track_id` convention in FINDINGS 3 |
| Show in Finder/Explorer, Get Info (i), Edit Track | menu strings | missing | none |
| Copy/Cut/Paste of tracks and mix items, Select All/None | `CopyManager`, `XClipBoard` | missing | none |
| Drag tracks between Editor and Device, into cases and mixes | `ComputerMixLibraryComponent::itemDropped`, `getDragSourceDescription` | partial (file drop to add) | none |
| Info panels: track/mix/case counts, analysed vs analysing count, GB used/available, firmware version | `info_window_*`, `tracks_info_text`, `size_info_text` | missing | none |
| Context menu per track: Analyse Now, Get Info, Delete, Export, Rating 0..5, Show in Finder; per mix/case: Delete from Mix/Case | `english.txt` Context Menu, `TrackTableComponent::mouseUp` (FINDINGS 4d) | missing (rating writer exists) | `set_track_rating` |

## 3. Track metadata

| Feature | Evidence | Revival | Backing code |
|---|---|---|---|
| Edit metadata dialog, single and multi-select ("Multiple Item Information", Previous/Next) | `EditTrackMetaDataComponent::editSingleTrack/editMultipleTracks`, `metadata_*` strings | missing | `tracks` columns known; no update API except rating |
| Edit mix and case metadata (name, genre, year, rating, producer, remixer, comments) | `EditMixMetaDataComponent`, `EditCaseMetaDataComponent` | missing | schema known |
| Rating 0..5 | FINDINGS 4d | missing in UI | `set_track_rating` |
| Tag read at import (artist, album, title, genre, BPM tag, artwork not used) | `TrackImporter::IImport_tagMap`, `enricher::ITag`, container/decoder dylibs (mp3, aac/mpeg4, flac, ogg/vorbis, wav/pcm, aiff/aifc, snd/au) | done for the add path (formats supported by our decode crate unverified) | `pm-analysis decode/ingest` |
| Key column and key field | `tracks.key VARCHAR(4)`, column "Key" | missing. Editor has no key detection (inferred: only Key Envelope for pitch shifting) | none |

## 4. Analysis and beat grids

| Feature | Evidence | Revival | Backing code |
|---|---|---|---|
| Background analysis queue with priority (analyse selected first), progress, cancel | `TrackAnalyzerQueue::prioritizeTracks/populate`, `TrackAnalyzer`, "Analyse Now" | partial (job with progress) | `pm-analysis` |
| BPM + beat grid -> `.str` | `BpmAnalysis`, `TrackAnalysedData` | done | `strfile.rs`, `pm-analysis` |
| Peak/gain normalise factor | `PeakLevelAnalysis`, `normalizeFactor` | done (verify) | `strfile.rs` |
| Track length analysis | `TrackLengthAnalysis` | done via decode | `decode.rs` |
| Waveform peak cache (`PeakCache/*.dat`) and WaveComponent drawing | `PeakDataCache`, `PeakReader`, `WaveComponent::redrawWaveImage` | missing (`.dat` format not decoded; recomputable) | none |
| Re-analyse already analysed tracks prompt | `reanalyze_already_analyzed_tracks_*`, `checkAndReanalyzeTracks` | partial | `reanalyse_tracks` |
| Preference "Track analyse" (auto-analyse on import) | `PreferencesNativeDialog::initDialog` | TODO verify | none |
| Beat grid editing | no UI found in the Editor (inferred absent) | missing (and a rekordbox-class gap) | `strfile.rs` |

## 5. Cue points and loops

| Feature | Evidence | Revival | Backing code |
|---|---|---|---|
| One cue point per track, set in the player view, stored in `tracks.cue_point` (REAL 2^64 = unset on device, -1 in Editor library) | `CueComponent::setCuePointToDatabase/getCuePointFromDatabase/removeCuePoint`, Cue Point column | missing | column known (FINDINGS 4c) |
| Per-track loop_in/loop_out columns | schema `tracks.loop_in/loop_out` (device writes -1) | missing | schema |
| Preview markers (`mixes.preview_markers`, "Show Preview Markers" view and preference) | `MarkerComponent`, `MarkerPositionedEvent`, `mixes.preview_markers` | missing; format of the column TODO | none |
| Mix cue per mixtrack | `mixtracks.cue_point` | missing | schema |

## 6. Mixes: create, edit, play, record, export (owner's top priority)

What a mix document contains (decoded from the Editor's bundled demo; `work/inv/example.xml`, boost XML,
`MixDocument` class version 1): DocumentName, DeviceInfo, Genre, Artist, then `Tracks` (N `Track`
items, class version 2) and `Transitions` (N `Transition` items, class version 3).

Per `Track`: a `StreamedAudioProcess` container holding a list of `AutomatedAudioProcess` tool events
(names seen: `ToolProcess` wrapping `FxAudioProcess` Reverb/Echo/Roll/Wah, `DelayAudioProcess`,
`CrushAudioProcess`, `FilterAudioProcess` "FILTER", `EqAudioProcess` / `PeqAudioProcess` "EQ" with Lo/Mid/Hi
envelopes, `LoopAudioProcess` "LOOP" with Length, Iterations, ReLoop, Mode, Offsets). Each tool event has a
`PositionedEvent` (Start, Length, LOffset, ROffset, LimitSize, SessionEventId; units appear to be sample
frames, possibly fractional) and an `AutomationMap` of named parameters (e.g. `Reverb-Time`,
`Reverb-Wet/Dry`, `FILTER-CutOff`, `EQ-Lo`, `Crush-Crush`, `Echo-Time`, `Volume-Volume`), each value an
`Envelope` string in JUCE base64 (node list, format not yet decoded). The track itself has
`PositionedEvent` (timeline start/length), `LocalContext.libraryID`, `GlobalContext` (toniumId, otherIds,
`fuzzyId` artist/album/title/genre), `FilePath` (hashed device name), `Bpm`, `Channel`, `Offset`,
`NonInterpolated`, `PitchEnvelope` (list of position/speed pairs = tempo/pitch), `KeyEnvelope` (JUCE base64,
semitone shift) and `VolumeEnvelope` (gain automation). Each `Transition` has a `PositionedEvent` (the
overlap region), `BeatMatch` flag, `CrossfadeType` (1 = fade after 1/2, 3 = fade after 3/4 by
`TransitionComponent::onMenuItemClicked`, inferred 0/other = beat match crossfade) and an `Envelope` of
(position uint32 0..4294967295, value) nodes for the crossfade curve. Tracks sit on channels (`Channel`);
transitions connect overlapping tracks.

| Feature | Evidence | Revival | Backing code |
|---|---|---|---|
| Mix library list (Title, Date Created, Times Played, Genre, Rating, Year, Producer, Remixer, Date Modified, Time, Comments, Last Played) | `MixListHeader`, `MixTableComponent` | partial (list is read-only) | `library.rs mixes()` |
| New / Open / Save / Save As / Rename mix; unsaved-changes prompts; name collision prompts | menu strings, `MixLibraryComponent::saveMixAs/renameAndSaveMix/checkAndSaveCurrent` | missing | container only (`mixdoc.rs`) |
| Mix document model: parse + write the XML | `MixDocument::loadDocumentFromBlob/dumpDocumentBlobToXMLStream` | missing | `mixdoc.rs` (container), `scripts/pmk.py` |
| Multi-track timeline sequencer: drag tracks from library onto channels, move/resize, snapping, zoom (px per sample), auto-scroll, time bar | `MixSequencerComponent::addTrackAtPosition`, `SequencerComponent::*`, `ChannelComponent`, `TimeBarComponent`, `Auto Scroll` | missing | none |
| Beat matching of overlapping tracks (tempo envelope aligns B to A), automatic transitions, "Timestretch vs Pitch" mode for tempo curve (Edit menu) | `TrackOperations::doBeatMatching/getBeatMatchPositionFromTracks`, `TrackComponent::updateBeatMatching`, `setAutomaticTransitions`, `TempoEnvelopeImpl`, `m_tsModeForTempoCurve`, `TimeStretch::generateBeatVector` | missing (beat grids exist) | `.str` reader/writer, `pm-analysis` |
| Per-track envelopes selectable from track menu: Key Transpose, Key Tune, Tempo/Pitch, Key, Gain | `TrackComponent::getPopupMenu`, `track_menu_*` strings, `KeyEnvelopeImpl` (snap to semitones) | missing | none |
| Tool lane per track: X-FADE, EQ, COLOUR FX (Filter, Hi-Lo cut), BEAT FX (Reverb, Echo, Roll, Wah, Crush, Trans, Delay), LOOP, REVERSE, IGNORE; each draggable/resizable, with envelope editing (add/drag/delete nodes, double click), Mute Tool, Move to Back, sync to beat | `ToolComponent`, `EqToolComponent`, `FxToolComponent`, `FilterToolComponent`, `LoopToolComponent`, `ReversalToolComponent`, `IgnoreToolComponent`, `EnvelopeComponent`, `ToolOperations::adjustNotAllowedToOverlappTools`, tool strings | missing | none |
| Loop tool: Re-loop, Length, Repeats | `loop_tool_menu_*` | missing | none |
| Undo/redo, cut/copy/paste of timeline items (clipboard is XML) | `PositionedEventUndoableAction` family, `XClipBoard` | missing | none |
| Mix playback in the Editor (real-time engine, mute per channel, volume, rewind/forward, play marker) | `AudioManager`, `MixAudioFormatReader`, `VolumeMuteComponent`, `AudioClip`, `TimeStretch`, `CrossfadeAudioProcess` | missing | none |
| Render/export mix to Ogg Vorbis ("Export Mix...", progress) | `MixExportManager::doExport` (extension "ogg", file name "render"), `MixRenderingImpl`, `OggWriter` | missing | none |
| Export / import mix with tracks (obfuscated `.pmk`-style bundle with audio, dev-gated menu `show_export_xml_menu_items`) | `Utils::exportObfuscatedMixWithTracks`, `TrackOperations::exportMixWithTracks/importMixWithTracks`, `ZipFileProgressHolder` | missing | `mixdoc.rs decode_pmk/encode_pmk` |
| Prefab demo mix imported at first start | `startupImportPrefabMix`, `bin.pmk` | missing | `mixdoc.rs` |
| Mix artwork (sleeve): drag image, show/hide, stored in `miximages` as JUCE-base64 PNG | `SequencerInfoWindow::filesDropped`, `MixComponent::setSleeve`, FINDINGS 4b | missing | schema |
| Missing-track handling when opening a mix: locate dialog, relocate related files automatically, "unavailable tracks" warning | `MissingTrackDialog`, `fileRelocator::onRelocate`, `locate_*` and `mix_open_*` strings, `TrackOperations::findAndRelocateTracksOnComputerDisc` | missing | negative id placeholder convention |
| Publish mix to pacemaker.net community (sign in, upload tracks, pause/resume, webcasting rule check: 5+ songs, max 60 min, max 3 per artist, max 2 per album) | `Communication::publishMix`, `PublishMix`, `PublishRules`, `paceStartupData.xml` URLs | out of scope: servers are dead | none |
| Mix "record" | no record feature found; the Editor renders offline (inferred) | n/a | n/a |

## 7. Cases

| Feature | Evidence | Revival | Backing code |
|---|---|---|---|
| Case list with columns (Title, Producer, Remixer, Genre, Rating, Date Modified, Year, Time, Times Played, Last Played, Date Created, Comments) | `CaseListHeader`, `CaseTableComponent` | partial (read-only list) | `library.rs cases()` |
| New Case, New Case from Selection, rename, delete, drag tracks in, reorder, remove track | `CaseLibraryComponent::createCase/removeCases`, `CaseListComponent::addTracksToCase`, FINDINGS 4d | missing in UI | `create_case`, `set_case_tracks`, `delete_case` (done in crate) |
| Case metadata edit | `EditCaseMetaDataComponent` | missing | none |

## 8. Device sync and firmware

| Feature | Evidence | Revival | Backing code |
|---|---|---|---|
| Device detection (volume added/removed), compatibility check (`EDITOR_COMPATIBLE_FLAG`), serial number, product info | `DeviceCommunication::*`, `fileSystemVolumeAdded` | done (volume find, compatibility) | `library.rs` |
| Sync to Pacemaker: copy tracks, `.str`, mixes, cases both directions; progress; cancel; failed-copy list; space check; "already transferred"; require analysed tracks | `TrackLibrarySyncer::*`, `sync_*` strings | partial (add tracks to device; no two-library sync, no mixes) | `add_track` |
| Transfer from device to computer (`transferTrackToComputer`) | same | missing | none |
| Delete unreferenced files on device | `DeviceUnreferencedFilesDeleter` | missing | none |
| Eject (Windows/Mac, with AppleScript on Mac) | `unmountPacemakerDevice`, `EjectVolume` | missing | none |
| Disc ID: `.PaceMakerDiscId.txt` identifies the volume so tracks can be found after drive-letter change | `DiscIdManager`, `disc_id_file_comment` | missing | none |
| Firmware upgrade from file, download latest (servers dead), copy to device, reboot wait | `updateFirmwareFromFile`, `Communication::downloadLatestFirmwareAndUpgrade` | missing (pfw tooling exists) | `pfw.rs`, `scripts/` |
| Pioneer MEP-7000 compatibility (SURPRISE): flag file `._mep_7000_compatible` at the device root; on sync the Editor generates a Pioneer PRO DJ database from the library (functions `generatePdbl/Pddb/Pdtl/Pdbi/Pdbas`, `writePdst`, `tl::generateProDJDB`, `removeProDJDB`, UTF-16BE strings, sort by title/artist/album/genre). This almost certainly explains the `.PDBL.DB/.PDDB.DB/.PDTL.DB` files left open in FINDINGS section 2 (inferred from names, not decompiled) | `mep7000::*`, `prodjdb::*`, preference "Pioneer MEP-7000 Compatibility" | missing | none |

## 9. Import

| Feature | Evidence | Revival | Backing code |
|---|---|---|---|
| Import music files / directory (recursive), drag and drop, one import at a time | `TrackImportManager`, `importDirectory`, `import_already_running_*` | done (file picker + drop; folder expand) | `expand_audio_paths`, `add_tracks` |
| iTunes library import and re-import (tracks, playlists -> cases), look at startup, 4 re-import policies (ask, never, tracks, tracks+playlists) | `iTunesImportManager`, `iTunesImporter`, `ImportITunesDialog`, `trackmap` table | missing | `trackmap` schema |
| Duplicate detection on import (same size/hash path) | `existing_track` in our crate; Editor `trackmap.tag_hash/location_hash` | partial | `library.rs` |

## 10. Preview playback and waveform display

| Feature | Evidence | Revival | Backing code |
|---|---|---|---|
| Player view: play single track from library, playlist (`PlayList`), cue button press/release, position label, volume up/down | `PlayerComponent`, `PlayerSequencerComponent::cueButtonPressed`, `Controls` menu | missing | none |
| Waveform in sequencer (grey peak envelope, beat markers `BeatMarkerComponent`, cue flags) | `WaveComponent`, `BeatMarkerComponent`, resources `cueflag_bar_and_shadow.png` | missing | `.str` beats |
| Vinyl mode, Normalize audio (playback preference) | menu strings, Preferences | missing | none |
| Equalizer view (`menu_item_title_eq` "Show Equalizer") | strings | TODO not inventoried | none |

## 11. Settings, preferences, shortcuts

Preferences dialog (`PreferencesNativeDialog::initDialog`, decompiled): language (English, Japanese,
Korean), look for iTunes library at startup, iTunes re-import policy, auto scroll, track analyse,
normalize audio, vinyl mode, MEP-7000 compatibility, show unavailable tracks, show preview markers. Hidden
settings keys: `show_export_xml_menu_items`, `show_test_dialogs`.

Menu structure from `getMenuForIndex` and `addMenuCommands` (command ids 0x2000..0x2410): File (New Mix,
Save, Save As, edit/rename, Export, New Case, New Case from Selection, Close, Import Music Directory/Files,
Get Info, Show in Finder, Eject, Upgrade Firmware), Edit (Timestretch/Pitch mode, Undo, Redo, Cut, Copy,
Paste, Delete, Select All/None), View (Change Mix/Player View, Show Mix Artwork, Show Preview Markers),
Controls (Play/Pause, Next, Previous, Shuffle), Community (Publish, Sign Out, Pause/Resume Upload), Window
(Minimize), Help (web help, about, software and firmware update checks).

Keyboard shortcuts (from `english.txt`, with the platform command key): n new mix, s save, o import music
files, w close, i get info, r reveal file, l show current track, z undo, y redo, x/c/v cut/copy/paste, a
select all, comma preferences, ? help, e change view, q quit. Space plays/pauses (inferred from the
`spaceKey` check in `TrackTableComponent::keyPressed`). Revival status: missing.

## 12. Other

TODO: not yet inventoried: sign-in dialog details, About box, the Equalizer view, drag-out of tracks to the
OS, exact `.pmm` extension use, `PeakCache` binary layout, envelope node binary layout (JUCE base64), and
decompilation of the pmplayer-side mix playback (the device renders the same `MixDocument`).

## 13. Rekordbox-class additions

| Feature | Device can use it? | Notes |
|---|---|---|
| Colour-by-frequency waveforms (overview + zoomed) | app-only | needs own peak cache; the Editor's `.dat` is just a peak overview |
| Multiple hot cues / memory cues | no: device has one `cue_point` and one loop pair | app-only, or write only the primary cue to `tracks.cue_point` |
| Beat grid editor and tempo map (nudge, set downbeat, halve/double) | yes: `.str` beat list carries per-beat `bpm` and `firstDownBeat` | high value, format already writable |
| Key detection + Camelot display and filter | partial: `tracks.key VARCHAR(4)` exists, device display unverified | GPL libKeyFinder is licence-compatible |
| Preview player with waveform | app-only | |
| Playlists, smart playlists | cases are the device playlists (plain, ordered by rowid); smart rules app-only, can materialise into cases | |
| History | app-only (device never writes play counts back, FINDINGS 4c) | |
| Ratings, colours, tags | rating yes (0..5); colours and tags app-only | |
| Duplicate detection, collection health, relocation | app-only, but also needed for parity (missing-track locate) | |
| Import/export rekordbox XML, Serato, Traktor, iTunes | app-only converters; iTunes is Editor parity | |
| Search, BPM and key range filters | app-only | `ind_bpm` bucket exists on device |
| Auto-gain | yes: `normalizeFactor` in `.str` | already produced by analysis |
| Cloud backup / library snapshot | app-only; cheap version = copy `.Pacemaker` DB plus `.str` | |
| Waveform/phrase analysis, stems, AI mix suggestions | app-only | later |

## 14. Provisional top 15 missing features (priority order)

1. Mix document model (parse/write `MixDocument` XML round-trip; envelope node codec). Blocks everything in mixes.
2. Mix list actions: new, rename, delete, edit metadata, sleeve image.
3. Multi-track timeline editor: place tracks, move, trim, zoom, snap.
4. Transitions: crossfade types, envelope editing, beat matching using `.str` beats.
5. Tool lane: EQ, Filter, Reverb/Echo/Roll/Wah/Crush/Trans/Delay, Loop, Reverse with envelope editing.
6. Per-track tempo/pitch, key and gain envelopes.
7. Real-time playback engine and mix preview (time-stretch, mixing of tools).
8. Undo/redo, copy/paste, and the dirty-state prompts for all the above.
9. Mix export to Ogg Vorbis (and mix-with-tracks bundle).
10. Cases UI (crate support exists): create, add, reorder, delete, metadata.
11. Track metadata editor (single and multi), rating UI, context menu.
12. Two-way sync including mixes and cases, transfer back to computer, delete unreferenced, eject.
13. Single-track player and waveform with cue point, cue setting.
14. Missing-track locate/relocate and unavailable-track view; remove-from-library with reference warning.
15. iTunes import, MEP-7000 Pro DJ DB export, firmware-from-file, search/filter/type-to-find.

## 15. Phased plan (complexity: S small, M medium, L large)

Phase 1, Editor parity, mixes first: mix model + codec (M), mix list CRUD (S), timeline + transitions (L),
tool lane and envelopes (L), tempo/key/gain envelopes (M), playback engine with stretch and tools (L),
undo/redo and clipboard (M), Ogg export (M), mix bundles (S), cases UI (S), metadata and rating UI (M),
two-way sync, locate, eject (M), player + waveform + cue (M).
Phase 2, completeness: iTunes import (M), MEP-7000 export (L, needs decompilation), firmware from file (S),
preferences and shortcuts (S), column persistence and filters (S).
Phase 3, rekordbox-class: beat grid editor (M), colour waveforms (M), key detection (M), BPM/key filters
(S), duplicate and health tools (M), rekordbox XML/Serato/Traktor import-export (M), hot cues app-only
(M), smart playlists and history (M), backup (S).
