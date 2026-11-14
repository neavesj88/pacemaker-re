# Pacemaker Revival: app architecture

The app is split so that many feature agents can work at once, each inside its own files. This
document is the contract. A file is **owned** by one area (only that area edits it) or **shared**
(changed only in the integration step, never by a feature agent in parallel).

Areas, each with its own folder / files in every layer: `tracks`, `mixes`, `cases`, `history`,
`grid`, `device`, `import`, `playback`, and the feature-only areas `metadata` (tag and rating
editor), `analysis` (key detection and other analysis jobs), `export` (playlists and library
listings out) and `stems` (vocal / instrumental separation into a PC folder). `library` is the session plumbing (`commands/library.rs`, `api/library.ts`,
`devmock/library.ts`, the `LibraryView` payload) and belongs to **integration**: it is not a
feature area. "Integration" means the single step that merges feature work and may touch shared
files.

## File map

### Frontend (`app/src`)

| Path | Purpose | Owner |
| --- | --- | --- |
| `routes/+page.svelte` | Thin shell: header, banners, job strip, sidebar, active view, player slot, overlays | integration |
| `routes/+layout.svelte`, `+layout.ts` | Global CSS; SPA mode; installs devmock outside Tauri | integration |
| `lib/styles/tokens.css`, `base.css` | The device palette (do not change colours); element defaults, `.muted` / `.num` / `.empty` | integration |
| `lib/format.ts` | `mmss`, `baseName`, `volumeName` | integration |
| `lib/shell/Header`, `Sidebar`, `Banners`, `Welcome`, `JobStrip` (`.svelte`) | App chrome; `JobStrip` shows any job | integration |
| `lib/shell/overlays.ts`, `startup.ts` | Auto-discover `views/*/Overlays.svelte` and `views/*/init.ts` (see below) | integration |
| `lib/views/registry.ts`, `ids.ts` | Auto-discovers `views/*/view.ts`; `ViewDef` type; `ViewId` is a string | integration |
| `lib/views/<area>/view.ts` | One row for the area's sidebar entry (id, label, section, order, component, count) | that area |
| `lib/views/tracks/` (`TracksView`, `trackFilter.ts`, `NeedsAnalysis`) | Tracks view, the sidebar "need analysis" block | tracks |
| `lib/views/tracks/query.ts`, `columns.ts`, `viewState.ts`, `trackLogic.ts` | Pure logic, run by plain node (`npm test`): the typed search language (`field:value`, ranges, comparisons, `key:~8A`, `-not`, `OR`; every help example is compiled by the tests); columns, their order and the multi-key sort prefs (browser storage); what a filter preset / remembered view holds and how it is read back safely; multi-key sort. Backend: `commands/tracks/views.rs`, one JSON file per library in the app config dir (presets + the remembered search / filters / sort), same safety rules as the tag store; wrappers in `lib/api/tracks.ts`. `trackFilter.tracksViewRows` is the one pipeline (search, toolbar filters, label filter, sort) behind both the Tracks table and the player's next / previous | tracks |
| `lib/views/mixes/` (`MixesView`, `MixEditor`, `MixList`, `Timeline`, `flow.ts`, `preview.svelte.ts`, `init.ts`, ...) | Mix list and editor; plays previews through the shared player (`playback.previewMix`); follows the `mix-export` job in `init.ts`. Backend: `commands/mixes.rs` plus `mixes/{doc,timing,info}.rs`; `lib/api/mixes.ts`; store `lib/state/mixes.svelte.ts` (`mixes`); pure edit logic `lib/state/mixedit.ts`; sections in mixing (W2): snap mode "section" and `clipSectionPoints` / `clipSectionSpans` in `mixedit.ts`, section markers on clips (`ClipSections.svelte`, toggle in `view.svelte.ts`), and the auto-transition option that moves the window onto section boundaries (`lib/state/sectionTransition.ts`, settings `sectionEdge` / `sectionReachBars` in `transitionRecipes.ts`); see `notes/section-detection.md`; tests `app/scripts/test-mixedit.mjs` (`npm run test:mixedit`, Node >= 22.6). Audio engine: `crates/pm-mixer`, commands in `commands/export.rs` (the part of the mix to export in `export/range.rs`, loudness policy in `export/loudness.rs`; export dialog `views/mixes/ExportDialog.svelte`, store `lib/state/mixexport.svelte.ts`, pure settings `lib/state/exportprefs.ts` with `exportprefs.test.mjs`) | mixes |
| `lib/views/cases/` (`CasesView`, `SmartCases`) | Cases; the sidebar "Smart cases" section | cases |
| `lib/views/cases/` (organising) | Case organisation: pointer-event track drag into cases (`lib/state/trackDrag.svelte.ts`, `CaseDrops`; HTML5 DnD does not work on Windows, so the whole app uses pointer events for in-page drags: the mix picker -> timeline lane drag reuses `trackDrag` with scope "mix", and row reordering in the set builder and an open case uses `lib/state/pointerReorder.svelte.ts`; pure arithmetic in `lib/state/dragLogic.ts`, run by `npm test`. Only the OS file drop in `api/import.ts` is a Tauri event, and `tauri.conf.json` keeps its drag-drop handler on for it), duplicate, merge (`MergeDialog`), copy / move (`TransferDialog`), bulk delete, "which cases hold this track" (`HoldersDialog`, `lib/state/holders.svelte.ts`, chips in the Tracks inspector via `CaseChips`). Smart cases: nested ALL / ANY / NONE rule groups, order and limit (`SmartRuleGroup`, `SmartRuleRow`, `SmartPreview`, pure `lib/state/smartRules.ts`), auto-refresh of their Pacemaker copies (`SmartRefreshDialog`, `lib/state/smartRefresh.svelte.ts`), settings (`SmartSettingsDialog`). Export of cases and smart cases to a folder or as a playlist (`ExportDialog`, `lib/state/caseExport.svelte.ts`). Pure logic run by `npm test` (`caseLogic`, `smartRules`, `smartRefresh`, `exportLogic`, `holdersLogic`; they import `.ts`, so they run with `--experimental-strip-types`) | cases |
| `lib/views/duplicates/` (`DuplicatesView`, `DupGroupCard`, `MergeDialog`, `DupSettingsDialog`, pure `lib/state/dupeLogic.ts`) | Duplicate finder (matching tags, size + length, or sound fingerprint) with a merge that keeps one track and moves cases, tags and stats across. Backend: `commands/cases/dupes*.rs`, `crates/pm-formats/src/library/dupes.rs`, `crates/pm-analysis/src/fingerprint.rs`; wrappers `lib/api/duplicates.ts` | cases |
| `commands/cases/` (`smart/rules.rs`, `smart_sync.rs`, `smart_export.rs`, `smart_settings.rs`, `export_bulk.rs`, `export_prefs.rs`, `dupes*.rs`) | Smart-case rule engine and store, device copy refresh, case / smart-case export (also the rekordbox library XML with smart cases, called from `import.rs`), duplicate finder. Bulk case operations are in `crates/pm-formats/src/library/case_bulk.rs`; `library/track_extra.rs` holds the read-only extra track columns the smart rules use | cases |
| `lib/views/setbuilder/` (`SetBuilderView`, `SetSettings`, `CreateMixDialog`, `SaveCaseDialog`, `solver.ts`, `buildMix.ts`, `createMix.ts`, `energy.ts`: the energy resolver, see "Labels everywhere" below) | Set builder: orders a case or the selected tracks into a set (Camelot moves, tempo limits, optional energy curve, target length), saves it as a new case or lays it out as a new mix with the existing beat-match + auto-transition code. The setting "Prefer leaving a track on its outro and arriving on an intro" (`preferSections`, default off; minimum length `sectionMinBars`, default 8) is a small search-cost tie-breaker from the tracks' detected sections (`sections.of`, `mixSides`), and mix creation moves each transition onto section boundaries when the Auto transition setting asks for it. Pure solver and mix layout (`solver.ts`, `buildMix.ts`) are tested by `solver.test.mjs`, run from `npm run test:mixedit`. Opened with `nav.go("setbuilder", { "setbuilder.caseId": id })` or `{ "setbuilder.trackIds": [...] }`. No backend commands of its own: it uses the `cases` and `mixes` commands | setbuilder |
| `lib/views/stems/` (`StemsView`, `SetupCard`, `MakeCard`, `SettingsCard`, `PlanModal`, `LicenceModal`, `ResultCard`, `logic.ts`, `init.ts`) | Stems view: vocal and instrumental files for chosen tracks into a folder on this computer; model download behind a licence notice. Store `lib/state/stems.svelte.ts` (`stems`); `lib/api/stems.ts`; `lib/devmock/stems.ts`; tests `logic.test.mjs` (in `npm test`). Engine: `crates/pm-stems` (provider trait, STFT, overlap-add, HTDemucs via ONNX Runtime, file writers) | stems |
| `lib/views/history/`, `grid/`, `device/` | History, beat grid editor, device views (placeholders) | that area |
| `lib/views/insights/` (`GapMapView`, `Heatmap`, `CellPanel`, `ThinSummary`, `gapLogic.ts`, `gapState.svelte.ts`) | Library gap map (sidebar "Gap map"): BPM band x key, BPM band x genre, key x energy (tab shown only when tracks carry `energy`). Pure binning / thin-area maths in `gapLogic.ts`, tested by `gapLogic.test.mjs` (in `npm test`). Frontend only: reads `session.library`, writes nothing; a cell opens the Tracks filters via `tracksState` | insights |
| `lib/views/analysis/` (`EnergyCell`, `SectionBand`, `SectionList`, `SectionsPanel`, `Overlays.svelte`, `BpmRangeFields`, `BpmRangeOffer`, pure `energyLogic.ts`, `sectionLogic.ts`), `lib/state/energy.svelte.ts`, `sections.svelte.ts` | App-side audio analyses, nothing written to the device: "Energy (auto)" 1-10 estimate (own Tracks column `autoenergy`, sort field `auto_energy`; separate from the user-set energy label in `labels.ts`) and section detection (intro / build / drop / breakdown / outro markers on waveforms, grid overview and a section list). Both run as confirmed jobs. Backend: `commands/analysis/energy.rs`, `sections.rs`, `crates/pm-analysis/src/energy.rs`, `sections.rs`; wrappers `lib/api/analysis.ts`; notes `notes/section-detection.md`. `Overlays.svelte` also mounts the key-tool dialogs | analysis |
| `lib/views/import/` (`ImportView`, `ConfirmModal`, `DropOverlay`, `Overlays.svelte`, `init.ts`) | Import view; write confirmation and drop overlay; job-event and file-drop subscriptions | import |
| `lib/views/import/` (interop importers), `crates/pm-formats/src/interop/` (`m3u.rs`, `serato.rs`, `traktor.rs`, `paths.rs`) | More library importers next to rekordbox / iTunes: M3U / M3U8 playlists, Serato crates, Traktor NML, each previewed before it is applied; opt-in cue import (`importLogic.ts`, run by `npm test`). Paths read from foreign files are untrusted: every probe goes through `paths.rs` (plain absolute local paths only, never a network or device path) | import |
| `lib/views/matcher/` (`MatcherView`, `PickTrackModal`, `SaveCaseModal`, `matcher.svelte.ts`, pure `matchLogic.ts`, `parseList.ts`), `commands/import/request.rs` | Track matcher (sidebar tools, "Track matcher"): matches a request list (text, CSV or playlist file) against the library, saves the owned tracks as a case and exports the missing list. Matching runs in the UI; the two backend commands (`matcher_read_list`, `matcher_save_text`) only move text, and a save refuses a destination inside the library volume. Wrappers `lib/api/matcher.ts` | import |
| `lib/views/setlist/` (`SetlistModal`, `Overlays.svelte`, `setlist.svelte.ts`, pure `setlistLogic.ts`), `commands/export/setlist.rs` | Printable setlist of a case or a mix (buttons "Setlist..." in the case page, mix list and mix editor): print-ready HTML built by the UI so preview, print and saved file are one text; event details, a note and an energy rating per track live app-side in one JSON file per library (nothing on the device). Wrappers `lib/api/setlist.ts`. The mix track list / CUE sheet is a separate feature in `commands/export/tracklist.rs` | export |
| `lib/views/vdj/` (`VdjExportModal`, `Overlays.svelte`, `init.ts`, `vdj.svelte.ts`, pure `vdjLogic.ts`), `commands/export/vdj.rs` | VirtualDJ list export of a case ("VirtualDJ..." in the case page), optionally with copies of the audio in a folder the user picks; one plan computation behind the preview and the run, nothing written to the device, audio found by the plain-names-only rule rather than `Library::track_paths`. Why this format and not OneLibrary: `notes/onelibrary-export.md`. Wrappers `lib/api/vdj.ts` | export |
| `lib/views/settings/` (`SettingsView`, `registry.ts`, `types.ts`, `areas.ts`, pure `settingsLogic.ts`), `views/<area>/settings.ts` | Settings view (sidebar tools, "Settings"): every feature setting grouped by area, with a search box. It owns no control: each area adds a `views/<area>/settings.ts` (a list of entries, found by glob like `view.ts`) that points at the feature's own component or state, and the control stays where it is used as well. An entry is `inline` (the feature's own settings component, shown in place), `dialog` (its own dialog, which takes `onclose`) or `open` (its always-mounted panel or dialog, opened through its own opener). There is no link-only kind: where the choices sit inside a flow (the mix export and track list dialogs, the setlist, VirtualDJ, matcher and case export dialogs, the missing-audio search, the gap map and tempo panels, the tag write page), the option fields are extracted into a component that the dialog and the Settings entry both show, on the same state. A new feature's settings appear by adding that file. `settingsLogic.test.mjs` (in `npm test`) checks every `settings.ts` is well formed that no entry is only a link, that every settings file the backend keeps (`*-settings.json`, `energy-source.json`, ...) has an entry, and that every `pm.*` / `pacemaker*` browser-storage key is either an entry or listed as view state (track, case and mix list columns and sorting, marker toggles, the last grid nudge), so a new setting without an entry fails the test | settings |
| `lib/views/metadata/` (`TidyView`, `CleanupPreview`, `CleanupRecipes`, `CleanupSettingsPane`, `cleanupLogic.ts`, `cleanupState.svelte.ts`; `TagWritePanel`, `TagWriteFields`, `tagwriteState.svelte.ts`), `lib/api/metacleanup.ts`, `lib/devmock/cleanupEngine.ts` (+ `cleanupGolden.json`), `commands/metadata/{cleanup,tagwrite,audiohash}.rs` | Tidy view (the `metatidy` view): metadata clean-up recipes with a full before / after preview computed by the same engine as the write (golden vectors in `cleanup.test.mjs`), and writing the library's tags back into the audio files themselves (a write to the audio on the card, not to `music.db`). The engine twin for the browser mock is `devmock/cleanupEngine.ts` | metadata |
| `lib/views/space/` (`SpaceView`, `SettingsPanel`, `RunModal`, `space.svelte.ts`, pure `spaceLogic.ts`), `lib/api/device-space.ts`, `commands/device/space.rs`, `crates/pm-formats/src/ops_space.rs` | "Free space" view: plans which tracks to archive (verified copy to a folder on this computer) and/or remove from the card to reach a target, ranked by play history. The plan the confirmation shows and the run use one function; a track a mix uses is never removed | device |
| `lib/views/device/` (`PresetsCard`, `PresetApplyModal`, `PresetNameModal`, `presetLogic.ts`; `RelinkModal`, `RelinkOptionsFields`, `RelinkSettings`, `relink.svelte.ts`, `relinkLogic.ts`; `SafetyCard`, `VerifyModal`, `FixModal`, `UndoCard`, `UndoModal`, `trustLogic.ts`; `RestoreCard`, `RestoreModal`; `VocalCard`; `WideModal`, `JobProgress`), `lib/state/undo.svelte.ts`, `lib/state/vocal.svelte.ts`, `lib/api/device-{preset,relink,trust}.ts`, `lib/devmock/device-{preset,relink,trust}.ts`, `commands/device/{preset,relink,trust,undo}.rs` | The device area's later cards: `settings.dat` presets, relinking tracks whose audio moved, the "trust pack" (verify the library, fix dead rows and grids, undo of rating / tag / favourite edits with a pill after each edit), restore of `music.db` from a snapshot (below) and the Vocal filter flag | device |
| `views/libraries/`, `lib/devmock/libraries.ts`, `commands/library/pane.rs`, `ops_sync/selection.rs` | The Libraries view (nav id `libraries`, shown when a computer library exists or is open or a second library is on hand): the Editor's two panes. Left = the open library (`session.library`), right = another one from a picker ("This computer", each detected Pacemaker, "Open folder..."), defaulting to the Pacemaker when a computer library is open and the reverse. A detected Pacemaker is read only by the button "Read the Pacemaker (G:)" (`library_pane`: tracks, mixes and cases from music.db alone, read-only, no .str scan); a library on this computer is read when chosen. Each pane has Tracks / Mixes / Cases tabs, a search, a windowed list (30 000 tracks draw only the rows in view plus a few of overscan, a few dozen), click / Shift / Ctrl selection with arrow keys, "on both" markers (location sets for tracks, trimmed lower-case names for mixes and cases) and a status line. **Drag between panes** is a pointer drag (`trackDrag` scope `lib`, `from` = `lib:<side>:<tab>`, so it never reaches case strips or the mix timeline and theirs never reach the panes; the pill, threshold, Escape and blur cancel are the shared ones): drop on the other pane to copy the selection there, drop tracks on a case row of its Cases tab to copy them and add them to that case (`into_case`), a drop on the pane it came from does nothing. The click routes are "Copy to <side> ->" in each pane and the row menu. A drop plans `sync_plan_selection` and opens a dialog naming every item (Copy, Update, Skip: already there, Needed by mix X, Add to case X), the size and the space line (blocks when it does not fit); Copy runs `sync_apply_selection`, the same write job `device-sync` (same write gate, snapshot, progress and Cancel as `sync_apply`; `run_job` in `commands/device/sync.rs` is shared), then the written library is read again. What the right pane read (`loaded`) is dropped by `libs.invalidate()` on a different open library, Rescan, any other finished job and coming back to the view; a library on this computer is then read again at once, a Pacemaker waits for its button (`readsAtOnce`, tested), and "Read again" is a deliberate click. The engine is `ops_sync::plan_selection` / `apply_selection` (selection = track locations, mix names, case names, optional `into_case`): only the picked items, the tracks they need (`needed_by`) and one case item are planned, each decided by the same helpers as the full plan (`track_item`, `mix_item`, `case_item`) under `drag_settings`, the saved Sync settings clamped so that a drag only adds: a track already on the target is left as it is, a same-named mix is kept beside the target's ("keep both", unless the setting skips), a same-named case is merged into (unless the setting skips); found by indexed lookups (location index, track id, target by location) instead of reading every row; it never removes or overwrites anything on the target (a case made from tracks of which one failed to copy reports Failed naming it), and its digest holds the picked items' own stamps, so an unrelated change is not stale but a change to a picked item (or the target row it meets) is. Pure parts (`librariesLogic.ts`: search, on-both, selection, drop resolution, dialog text) run in `npm test`; `?big=1` gives the devmock Pacemaker 30 000 tracks. | libraries |
| `lib/api/sync.ts`, `lib/devmock/device-sync.ts`, `commands/device/sync.rs`, `crates/pm-formats/src/ops_sync.rs`, `pm sync` | Library sync, one engine for both directions (a library on this computer and the Pacemaker hold the same layout): `ops_sync::plan` decides per track, mix and case (copy, update, replace, copy as new, merge, skip with a reason, remove only when the setting is on), `apply` re-plans, refuses a stale digest, drops unticked items, re-checks what mixes need, then runs tracks, mixes, cases, removals with a cancel check between items; `copy_labels` moves app-side labels by `global_id`. App layer: settings file `sync.json` (`sync_settings_get/set`), `sync_plan`, `sync_apply` as the write job `device-sync` on the target (snapshot first). The devmock simulates "This computer" next to the mock device. Plays counted on one side come over even when nothing else differs (action `update_plays`: the higher count and later last-played; the setting "Play counts"). Smart cases are never synced. UI: `views/sync/` (nav id `sync`, shown when a computer library exists or is open; `syncLogic.ts` is the pure part, `sync.svelte.ts` the store, `SyncSettingsFields.svelte` is shown in the view and in Settings > Library). The plan is only made by "Check what would change" and apply sends its digest plus the left-out keys; a header button "Send to Pacemaker" deep-links in | device |
| `lib/views/history/` (`PlayLog`, `PlayLogSettings`, `TrackLogModal`, `playlogLogic.ts`), `lib/state/playlog.svelte.ts`, `commands/history/{playlog,logcmds}.rs` | Play log: a snapshot of the play counters each time a library is read, kept in an app-side file (never on the device), turned into plays, sessions and rankings; per-track history in `TrackLogModal`. Bounded (event cap, retention, folding into a baseline) | history |
| `lib/views/import/` (`WatchFolders`, `WatchResults`, `watchLogic.ts`), `lib/state/watch.svelte.ts`, `commands/import/watch.rs` | Watch folders: scans chosen folders for audio the library does not hold (by name and size, then size and title, then tags) and hands what it finds to the ordinary add flow, which asks first unless the user chose automatic | import |
| `lib/views/grid/` (`BatchDialog`, `SuspectDialog`, `GridDiff`, `TempoPanel`, `TempoStrip`, `TempoSettingsFields`, `vtempo.svelte.ts`, `vtempoLogic.ts`), `lib/state/{tempo.svelte.ts,tempoLogic.ts,gridBatch.svelte.ts,batchLogic.ts,gridSuspects.svelte.ts,suspectDefaults.json,keyLogic.ts,mixNote.ts}`, `lib/devmock/{gridBatch,gridVtempo}.ts`, `commands/grid/{backup,batch,suspect,tempo,vtempo}.rs`, `crates/pm-formats/src/ops_{grid,vtempo}.rs` | Beat grids in bulk: tempo edits (halve / double / set) on many tracks, batch nudge / halve / double / suggested fix / re-analyse with a diff of every grid before anything is written, suspect-grid rules, and variable-tempo editing of one grid. A bulk write copies each track's previous `.str` off the device first (`grid/backup.rs`) | grid |
| `lib/views/tracks/` (`QueryHelp`, `PresetsMenu`, `tableRows.svelte.ts`, `peakCache.ts`) | The "?" search help, saved filter presets, the one cached list of table rows (shared with the player) and the waveform request queue | tracks |
| `lib/views/mixes/` (`TracklistDialog`, `TracklistFields`, `RecipeSettings`), `lib/state/{tracklist.svelte.ts,tracklistprefs.ts,sameaudio.ts,mixexport.svelte.ts}` | The mix track list / CUE sheet dialog, the transition recipe settings, and `sameaudio.ts` (whether two mix documents sound the same: when a preview or a loudness analysis is out of date) | mixes |
| `lib/views/settings/` (`Licences.svelte`, `settings.ts`), bundled `THIRD-PARTY-NOTICES.md` | The "About" area of Settings: the licences of everything the app contains, read from the file bundled as a Tauri resource (`app_notices` in `commands/library.rs`). The file is generated by `scripts/gen-third-party.py` from the dependency tree; regenerate it when a dependency changes | settings |
| `lib/state/prefStore.ts` | `loadPref` / `savePref`: the one way a view keeps a per-viewer preference in browser storage (reading never throws and always gives defaults at worst; a failed write loses only the memory of the choice). Settings that code outside the browser reads (smart-case rules) go through the backend instead, see "App-side files" below | integration |
| `lib/state/labels.ts`, `labels.svelte.ts` | Tags, colour labels (8 named, `--label-*` tokens) and energy 1-10, app-side only (nothing on the device). `labels.ts` is pure (types, ops mirror, filter predicate `matchesLabels` / `filterByLabels`, drafts) and safe for any area to reuse; `labels.svelte.ts` holds `labels` (by track id) and `labelEditor` (`edit(ids)`, `manage()`). Parity vectors `labels.parity.json` run against both this and the Rust store. UI in `views/metadata/` (`TagEditor`, `TagManager`, `LabelFields`, also a section of the metadata editor) and the Tracks columns / filter (`views/tracks/LabelFilterBar`) | metadata |
| `lib/views/<area>/Overlays.svelte`, `init.ts` | Optional: always-mounted overlays / startup subscriptions of that area | that area |
| `lib/player/Player.svelte` (+ optional `Overlays.svelte`, `init.ts`) | Bottom player bar and engine | playback |
| `lib/ui/*` | Generic primitives: `Button`, `Modal`, `Table`, `CollectionTable`, `Stars`, `Waveform`, `ContextMenu`, `Toast`, `Placeholder`, `RMark` | integration |
| `lib/state/session.svelte.ts` | Detected volumes, the open library, load / reload, error banner text, `onReset` hooks. The open library is one object per load whose `tracks`, `cases` and `mixes` are shallow (`$state.raw`) signals, typed read only: never write a field in place, use `session.patchTracks(ids, fields)` (new track objects, new array) or `session.setCases` / `session.setMixes` | integration |
| `lib/state/nav.svelte.ts` | Current view and the one-shot deep-link param bag | integration |
| `lib/state/filters.svelte.ts`, `selection.svelte.ts`, `toast.svelte.ts` | Header search / needs filter / sort; highlighted track ids; toasts | integration |
| `lib/state/jobs.svelte.ts` | Generic job runner: `jobs.ask(request)`, progress, completion; add / re-analyse adapters | import |
| `lib/state/cases.svelte.ts` | The open case and its track ids | cases |
| `lib/state/playback.svelte.ts` | The transport (loaded track, playing, position, mix preview). Others call its methods only | playback |
| `lib/state/<area>.svelte.ts` | An area's own store, created by that area when needed | that area |
| `lib/api/types.ts`, `events.ts` | Wire types shared across areas (Track, Collection, LibraryView, job events); event names, `subscribeAll` | integration |
| `lib/api/<area>.ts` | One typed `invoke()` wrapper module per area: `library`, `tracks`, `cases`, `import`, `mixes`, `history`, `grid`, `device` (plus `device-preset`, `device-relink`, `device-space`, `device-trust`), `playback`, `metadata`, `metacleanup`, `tags`, `analysis`, `export`, `duplicates`, `matcher`, `setlist`, `vdj`, `stems` | that area |
| `lib/devmock/index.ts` | Merges every `devmock/<area>.ts` automatically | integration |
| `lib/devmock/db.ts`, `jobs.ts`, `dialog.ts`, `types.ts` | Shared mock fixture (tracks, cases), job runner, dialog mock | integration |
| `lib/devmock/<area>.ts` | That area's mock command handlers and its own fixtures (the mix fixture is in `devmock/mixes.ts`) | that area |

#### Labels everywhere, and the one energy resolver

Tags and colour labels also belong to **cases and mixes** (not energy: a case or mix has none). The same store file and rules as tracks (`commands/tags.rs`, kind `case` / `mix`); a case or mix is keyed `<id>:<date_created>` because music.db has no UUID for them, so a rename keeps the labels and a reused row id never inherits another case's. Commands `tags_things_load` / `tags_things_apply`; stores `lib/state/thingLabels.svelte.ts` (`caseLabels`, `mixLabels`, `thingEditor`); the editor `views/metadata/ThingLabelEditor.svelte` (reuses `LabelFields` with its `get` / `vocabulary` / `thing` props), the row chips `ThingChips`, the list filter `ThingFilterBar`. Set / clear from the case list (row menu, ticked cases), the case page header and the mix list (row menu, picked mixes). Not a device write, so no write gate.

**BPM range is a setting.** Analysis doubles or halves every tempo into one window: 92-184 (default, drum & bass at ~174), 78-156 (the original Editor, ~87), 98-196, 68-136. Saved by the backend (`analysis_bpm_range_get` / `_set`, `bpm-range-settings.json` in the app config dir; `commands/analysis/bpm_range.rs`), read at the start of every analysis (add tracks, re-analyse, grid editor re-analyse, grid batch, `pm-cli`). Store: `lib/state/bpmRange.svelte.ts`; pure rules and the preset list (mirroring `BpmRange::PRESETS` in `pm-analysis/src/tempo.rs`): `bpmRangeLogic.ts`. Nothing is stored per track: a track whose BPM lies outside the current window counts as read with another range, and changing the setting (or Settings > Analysis > BPM range, or the sidebar "Review" button) offers to re-analyse those, naming each, through the normal re-analyse confirmation. Double / halve BPM (track menu, inspector) go through `tempo.askDouble` / `askHalve`.

**Energy has one resolver.** The setting "Energy source" (`mine`, `auto`, `mine_else_auto`, default `mine_else_auto`) says which of your own rating (`labels`) and the Energy (auto) estimate (`energy`) counts. Pure rule: `lib/state/energyResolve.ts` (`resolveLevel`, whole levels 1 to 10), mirrored by `resolve_level` in `commands/tags/energy_source.rs`; both run the vectors in `lib/state/energy.parity.json`. The choice is saved by the backend (`energy_source_get` / `energy_source_set`, `energy-source.json` in the app config dir) because smart-case rules read it when a Pacemaker copy is rewritten; the store `lib/state/energySource.svelte.ts` (`energySource.level(id)`, `.mine`, `.auto`, `.used`) is what every reader calls, and `views/metadata/EnergySourceSelect.svelte` is the control (Energy (auto) panel, gap map, set builder). Readers: the search box (`energy:`; `myenergy:` and `autoenergy:` ask for one source) and the label filter's energy range (`trackFilter.ts`), smart-case rules (below), the gap map's key x energy grid, the set builder (`setbuilder/energy.ts`), the printable setlist's energy column (typed-per-setlist value first, then the resolver). The Tracks table keeps both columns and dims the one the setting does not count.

**Smart-case label rules** (`commands/cases/smart/rules.rs`): `tags` (any / all / none, ignoring case), `colour` (is / is not one of several, `none` = no colour label), `energy_range` (resolver level). `Data::load(lib, queries, now, config_dir)` reads the label snapshot (`tags::snapshot`: labels by track id, auto energy, energy choice) only when a rule needs it; every caller with a config dir passes `Some(dir)` (`None` is for tests only: no labels exist). Twin for the browser mock: `lib/state/smartRules.ts` (`evaluateQuery`), tested against the same fixture.

**Traktor colours** (`INFO COLOR`, read as a bare index by `pm-formats` `interop/traktor.rs`; `ImportPlan.colours`) go to the label store, never the device, when the user ticks "Also set colour labels from the colours Traktor gave these tracks" in the import preview (`views/import/colourImport.ts`, `ImportPreview.svelte`). What Traktor calls each index is unverified, so the mapping table is shown and editable (default: palette order for 1 to 7), existing label colours are kept unless "Replace" is ticked, and every track it would colour is listed before the question.

### Backend (`app/src-tauri/src`)

| Path | Purpose | Owner |
| --- | --- | --- |
| `lib.rs` | Registration only: plugins, `setup`, `register_handlers!()` | integration |
| `commands/mod.rs` | Module list, `setup` chain, `register_handlers!` chain (all twelve areas are already in it) | integration |
| `commands/library.rs` | `list_volumes`, `open_library`, `LibraryView` | integration |
| `commands/tags.rs`, `commands/tags/store.rs` (re-exports `pm_formats::ops_labels`), `commands/tags/energy_source.rs` | Label store (the store itself is `pm_formats::ops_labels`, shared with `pm tags`): one JSON file per library volume in the app config dir, atomic writes, generic over kind (track, case, mix) and a stable key; tracks are keyed by `global_id` (fallbacks in the file header), cases and mixes by `<id>:<date_created>`. The Energy source setting and its resolver, and the label snapshot the smart-case rules read. Not a device write, so no write gate | metadata |
| `commands/<area>.rs` | Commands of that area. Every area has some now: `tracks`, `mixes` (`mix_list` ...), `cases`, `import` (`add_tracks` ...), `history` (9), `grid` (10), `device` (22), `playback` (5), `metadata` (24), `analysis` (21), `export` (a few dozen), `stems`. Bigger areas are split into submodules next to the area file: `commands/device/{preset,relink,space,trust,undo}.rs`, `commands/grid/{backup,batch,suspect,tempo,vtempo}.rs`, `commands/history/{playlog,logcmds}.rs`, `commands/metadata/{cleanup,tagwrite,audiohash}.rs`, `commands/mixes/{doc,timing,info,hygiene}.rs`, `commands/export/{encoding,loudness,range,render,setlist,tracklist,vdj}.rs`, `commands/analysis/{energy,sections}.rs`, `commands/import/{watch,request}.rs`, `commands/cases/...` (above) | that area |
| `state.rs` | State shared by several areas (`JobLock`); `setup` | integration |
| `store.rs` | Only what needs Tauri: `config_dir`. The rest of the app-side file plumbing is `pm_formats::appfile`, re-exported here: see "App-side files" below | integration |
| `jobs.rs` | Generic `spawn_job`, `Progress`, `JobOutcome`, `JobDone` | integration |
| `events.rs` | Shared event names (jobs) and `emit` | integration |
| `error.rs` | `AppError` (serialises to a string), `AppResult`, `.app()` | integration |

`commands/stems.rs` plus `commands/stems/{manifest,download,settings,plan,run}.rs` are the `stems` area: consented downloads of the model and engine into the app data folder, settings and consent records in the app config folder, a plan that the run recomputes with the same function, and a job that writes only into a folder on this computer (the card is reached through the ordinary add-tracks step, which asks first).

#### App-side files

Everything the app keeps for itself (labels, saved filters, play log, watch state, smart cases,
setlist notes, presets, settings, caches) is a JSON file in the app config folder, one per library
or one per setting, never on the device. The plumbing they share is `pm_formats::appfile`
(`crates/pm-formats/src/appfile.rs`), used by the app and by `pm`, so one file is read and written
one way everywhere:

- **Names.** `fnv1a` is the hash in every `<kind>-<16 hex>.json` name; `root_hash` is the hash of a
  library root spelled one way (case, slashes) and `volume_key` the key the label and saved-filter
  files use. Changing either orphans files already on users' disks; the tests pin them.
- **Writes.** `write_atomic` (an app-side file: `<name>.tmp`, fsync, rename, folder created) and
  `write_file` (a file the user chose: `<name>.part`, fsync, rename, the folder must exist), both
  removing the temp file on failure. `write_json` / `write_bytes` are the same with the error as text.
- **Unreadable files** are never overwritten or deleted: `set_aside` moves a damaged one to
  `<name>.corrupt` (then `.corrupt.1`, ...), `save_keeping_bad` copies a settings file once to
  `<name>.bad` before replacing it. A store keeps its own format, version check and wording.

The stores that use it include the fingerprint cache (`pm-analysis/src/fingerprint.rs`), the peaks
cache (`peaks.rs`) and the preset file names (`ops_preset::file_name_for`), which take `fnv1a` and
`write_atomic` from here rather than carrying their own copy. The same goes for text that goes into
a file: `pm_formats::textfmt` holds the one CSV cell writer (formula guard, quoting) that the track
list, the interop CSV and the archive listing share.

Two stores live in `pm_formats` entirely so `pm` can read what the app saved: the label store
(`ops_labels`, with `build_keys`, which keys a track by `global_id`) and the duplicate finder's
settings and matching rules that need no audio (`ops_dupes`; the sound check stays in the app,
`commands/cases/dupes_logic.rs`). `pm tags` and `pm duplicates` read them from the app's config
folder (`--config-dir`) and never write. Not merged on purpose: the two ways of copying a file
(`cases/export_bulk.rs` and `export/vdj.rs`; the VirtualDJ one re-checks that the source still lies
under the music folder), the tag writers (`commands/metadata/tagwrite.rs` rewrites tags in place
through lofty; `pm-mixer/src/tags.rs` builds the tag bytes of a file being rendered, with chapters),
the free-space planner's byte-identical duplicate check (`ops_space`, which decides what is safe to
remove) and the finder's looser matching, and the two "feat." readers (`dupes` strips them to
compare, `cleanup` rewrites them): each answers a different question, and merging them would change
what they find. The three XML attribute escapers are also kept apart because each is the rule of its
reader: `interop::xml_escape` (rekordbox-style XML: controls become spaces, tabs stay),
`export/vdj.rs::esc` (VirtualDJ: apostrophes escaped, controls dropped) and `mix.rs::escape` (the
Pacemaker's fixed mix format, written byte-exact). `cases/export_bulk.rs`'s `Fnv` is an incremental
plan fingerprint with separators, not a file name hash, so it is not `appfile::fnv1a`. `pm duplicates`
lists groups in the order the app does (`ops_dupes::display_order`: the suggested copy's artist,
title, then group id).

Crates in `crates/` are not part of the app layout; the app only calls them.

## Rules of ownership

1. Edit only files your area owns, plus new files you create inside your own folders.
2. A shared file needs a change only when you cross an area boundary; write the request in your
   hand-off note and let the integration step make it. Never edit a shared file in parallel.
3. A view may import another view's *exported component* only where listed here. Everything else
   goes through `lib/state` and `lib/api`. The cross-view imports that exist, all of them
   presentational pieces or pure logic, never another view's state:
   the Cases view uses `trackFilter.ts`; `tracks/ColourSwatch` is used by cases, import, metadata
   and tracks; `metadata/ThingChips` and `ThingFilterBar` by cases and mixes;
   `metadata/EnergySourceSelect` by analysis, insights and setbuilder; `analysis/SectionBand` and
   `SectionList` by grid and tracks; `analysis/EnergyCell` by tracks; `cases/CaseChips` by tracks;
   the space view uses `device/WideModal`, `JobProgress` and the `device` store. Stores may
   import a view's pure `*Logic` / `*Logic.ts` module (no component, no store): `state/energy`,
   `sections`, `smartRefresh` and `watch` do (from `views/analysis`, `cases`, `import`), `api/setlist.ts`
   takes a type from `views/setlist`, and `devmock/import.ts` and `mixes.ts` use view logic to build
   their fixtures. A new cross-view import goes in this list, or its piece moves to `lib/ui`.
4. Use the CSS variables in `tokens.css`; do not write raw colours. Keep the R mark and palette.
5. Never put real track, artist, case or mix names, or serial numbers, in code, mocks or tests.
6. **No dependencies mid-run.** `Cargo.lock` and `package-lock.json` are single files; parallel
   `cargo add` / `npm install` conflict. If you need a crate, package, Tauri feature or
   permission, write it in your hand-off note; the integration step adds it. Already decided:
   audio is streamed to an `<audio>` element over the asset protocol (`protocol-asset` is on,
   scope empty; playback adds the open library's files with
   `app.asset_protocol_scope().allow_file(..)` at run time); waveform peaks come from
   `pm-analysis` decode in Rust; tag and rating edits go through the `pm-formats` `Library` (no
   tag-writing crate); exports are plain text formats (M3U, CSV) written with `std::fs`.
7. **Public surfaces others depend on.** `trackFilter.ts` exports are
   frozen for existing callers: tracks may add exports only with defaults that keep current
   behaviour. `ui/Waveform.svelte` is integration-owned: an area that needs more (zoom, cues,
   editable grid lines) wraps it or copies it into its own folder (e.g. `views/grid/`), it does
   not edit it. The `Track` and `LibraryView` payloads are frozen: new per-track data comes from
   an area command (`<area>_track_detail(root, id)`, or a list command of your own like
   `mix_list`), never by widening `Track`. A field that truly must be added is an integration
   request.

## How to add things

### A view's content (your area already has a folder)

Edit the `<Area>View.svelte` in your folder. Split it into more components in the same folder as
it grows. The registry already shows it in the sidebar.

### A new top-level view

Create `lib/views/<area>/` with `<Area>View.svelte` and a `view.ts` (copy `views/mixes/view.ts`:
a default-exported `ViewDef` with `id`, `label`, `section`, `order`, `component`, optional
`count`). `registry.ts` and `ids.ts` are not edited. Navigate with `nav.go("<id>")`.

### A modal, drop zone or other always-mounted UI

Add `lib/views/<area>/Overlays.svelte` (or `lib/player/Overlays.svelte`). The shell renders every
such file once. Nothing shared is edited.

### A startup subscription (events, OS listeners)

Add `lib/views/<area>/init.ts` (or `lib/player/init.ts`) with
`export default function init(): Unlisten` (use `subscribeAll` from `lib/api/events.ts`). The
shell runs every such file when it mounts and calls the returned teardown on unmount.

### Opening a view with a target

`nav.go("grid", { "grid.trackId": 7 })`; the receiving view reads `nav.take<number>("grid.trackId")`
(removes it so it is acted on once). Prefix keys with the area name.

### A store

Create `lib/state/<area>.svelte.ts`. Use a class with `$state` fields and export one instance. If
it holds per-library state, register `session.onReset(() => store.clear())` at module level
(see `cases.svelte.ts`); `session.svelte.ts` is never edited.

```ts
class Mixes {
  openId = $state<number | null>(null);
  open(id: number) { this.openId = id; }
}
export const mixes = new Mixes();
```

Read library data from `session.library`; do not copy it into your store. Stores must not import
views (except the pure `*Logic` modules listed under rule 3). A store may import `lib/api/<area>.ts` and other stores, never a cycle.

### An API call

In `lib/api/<area>.ts`:

```ts
export const getHistory = (root: string) => invoke<Session[]>("history_sessions", { root });
```

Command names are the Rust function names; argument keys are camelCase (`track_ids` is `trackIds`).
Types used by one area stay in its api file; types several areas need go in `api/types.ts`
(integration step). Errors arrive as strings; `catch (e) { session.error = String(e) }`.

### A Tauri command

In `src-tauri/src/commands/<area>.rs`:

1. Write `#[tauri::command] pub async fn name(...) -> AppResult<T>`. Do blocking work in
   `tauri::async_runtime::spawn_blocking`. Open the library with `Library::open_read_only` to read,
   `Library::open` only for a command that writes, and write only after an explicit user action.
2. Add `$crate::commands::<area>::name,` to the list in that file's `<area>_commands!` macro.
   **This step is required**: a command that is not listed compiles but fails at run time with
   "command not found".
3. Add the wrapper in `lib/api/<area>.ts` and the mock in `lib/devmock/<area>.ts`.

`lib.rs` and `commands/mod.rs` are not touched. Errors: `?` works on `pm_formats::library::Error`,
`std::io::Error` and `tauri::Error`; for any other `Display` error use `result.app()?`
(`use crate::error::IntoApp`). Managed state used by only your area: `app.manage(MyState::default())`
in your file's `setup`. State shared by several areas goes in `state.rs` (integration step).

### A background job (any kind: key detection, export, sync, tag write)

Backend: call `jobs::spawn_job(app, &lock, "my-kind", |app| { ...; Ok(JobOutcome { ok, failed, summary }) })`
from a command that returns once the job has started. Emit per-item progress with
`events::emit(app, events::JOB_PROGRESS, Progress::new("my-kind", label, done, total, "my-status"))`
(set `.bpm`, `.detail` or `.error` as needed; status `"failed"` lists the item as failed). The
`summary` string is shown as is when the job finishes. `spawn_job` holds the shared `JobLock`, so
only one job runs at a time. Never reuse another area's kind string.

Frontend: nothing shared is edited. From your store or view call
`jobs.ask({ kind, root, count, names, title, description, confirmLabel, runningLabel, doneLabel, start })`
where `start` invokes your command. The confirmation, progress strip, failure list, completion and
library reload are handled by `state/jobs.svelte.ts`, `shell/JobStrip.svelte` and
`views/import/ConfirmModal.svelte`. Call `jobs.guard()` first if you need to check that writing is
possible before doing work. In devmock use `runJob(kind, labels, each)` from `devmock/jobs.ts`.

### An event

1. Backend: add `pub const MY_EVENT: &str = "my-event";` **to your own `commands/<area>.rs`** and
   emit with `events::emit(&app, MY_EVENT, payload)` (payload: `Serialize + Clone`). Only events
   that several areas listen to belong in `events.rs`.
2. Frontend: in `lib/api/<area>.ts`, export a typed wrapper
   `export const onMyEvent = (cb) => listen<Payload>("my-event", (e) => cb(e.payload))` (the event
   name string is declared once, there), and register it in your `init.ts` with
   `subscribeAll([...])` (it returns one teardown function and tolerates running in a plain browser).
3. In devmock, emit it with `emit("my-event", payload)` from `@tauri-apps/api/event`.

### A devmock entry

In `lib/devmock/<area>.ts` add to the exported `handlers` object, keyed by command name:

```ts
export const handlers: Handlers = {
  history_sessions: () => [],   // the same shape the Rust command returns; throw a string to fail
};
```

The file is merged by `devmock/index.ts` automatically (a command name defined twice throws at
start). Fixtures private to your area live in your file; `devmock/db.ts` holds only the shared
track and case fixture. The mock is invented placeholder data only.

## Writes to the device

Every change to the device volume goes through `state::WriteGate`, so that Eject, backup and
firmware install never run under a write, quick writes never fail because a long job is running,
and `music.db` always has a safety copy from before the session's first change.

| Kind | Examples | How |
| --- | --- | --- |
| Quick write | rating, tags, cases (including bulk delete, which deletes only a case still named as confirmed), smart-case device copy, mix new / save / rename / delete / import, history "save as case", grid save, undo apply (`device_undo_apply`), `settings.dat` (the editor and a preset apply, `device_preset_apply`), the Vocal filter flag file (`.Pacemaker/revival-vocal`, `device_vocal_set`) | `state::quick_write(app, root, \|\| ...)` on a blocking thread |
| Write job | add tracks, re-analyse, key detection, library import, bulk tempo edit (`tempo-edit`) and batch grid operations (`grid-batch`: each previous `.str` is copied to `<snapshots>/<volume>/grids-<UTC stamp>/` first, and a track whose grid changed since the preview is skipped), duplicate merge (`dupes-merge`, one transaction per group, deleting files only as the confirmed plan listed them), free-space archive and removal (`device-space`), tag write-back into audio files (`tag-write`), relink missing audio (`device-relink`: points `tracks.location` at a file under Music or copies it there first; a fresh `music.db` snapshot before its first change) | `jobs::spawn_write_job(app, &lock, kind, root, \|app\| ...)`: `JobLock` (one job at a time) plus a write ticket for the body |
| Exclusive operation | Eject, backup, firmware install, snapshot restore (`device_restore_snapshot`: job lock and gate held like Eject; a pinned safety copy of the current `music.db` first, the result read back and compared with the snapshot) | `state::take_exclusive` (backup, firmware: a job, so `spawn_job` holds the `JobLock`) or `state::take_exclusive_locked` (Eject: takes the `JobLock` itself), held until the operation ends |
| Device-deleting job | Device clean-up | `spawn_job` plus a `WriteTicket` inside the job, so it is refused during an eject / backup and gets the session snapshot |
| Not a device write | mix audio export (whole or a range), loudness analysis, M3U / CSV out, preview render | `spawn_job` with the `JobLock` only |

Rules:

1. A quick write and the body of a write job each hold a `WriteTicket`. Any number of tickets can
   exist at once; SQLite's own file lock orders the statements, and `Library::open` sets a 10 s
   busy timeout, so a rating saved while "add tracks" runs waits for the job's current insert
   instead of failing with "database is locked". A quick write is never refused because of
   `JobLock`.
2. An exclusive operation sets a flag first (new writes are refused at once with a clear message),
   then waits up to 5 s for tickets already out to be returned, then runs. If a write job holds
   `JobLock`, Eject and the others refuse immediately ("a job is still running"); if a quick write
   is stuck past 5 s they give up and release the gate. Eject also holds the `JobLock` until it
   returns, so no job (clean-up, export, add) can start under it. Backup is exclusive so that
   `music.db` and the `.str` files are not copied mid-write: for the whole backup (minutes with
   audio included) every quick write is refused with "the device is being backed up", not
   queued. That is deliberate (a rating saved mid-copy would leave the backup describing a state
   the device never had); the message names the operation so the user knows to wait.
3. `WriteGate::begin_write` also makes the safety copy: before the first write of the session to a
   library (keyed by volume root) it copies `music.db`, `music.db-journal` if present and
   `settings.dat` to `<app local data>/snapshots/<volume>/music-db-<UTC stamp>/` (never onto the
   device), read back and checksummed, and keeps the newest 10 per library
   (`pm_formats::ops_device::snapshot_library`; the volume key ignores a trailing separator and, on
   Windows, case, so `E:` and `E:\` are one library, and the folder name carries a hash of the
   key so `/mnt/a-b` and `/mnt/a_b` never share one). If the copy fails the write is refused with the
   reason, and the next attempt tries again. A successful Eject forgets the volume, so a different
   device that appears under the same drive letter is snapshotted on its first write. The Device
   view's Backup card lists the folder (`device_snapshots`, `device_snapshots_open`). Bulk
   operations that rewrite many rows (relink, free-space removal, the trust pack's fixes) take a
   fresh snapshot of their own before they start. The first-write copy of a session and the safety
   copy a restore makes are **pinned** (a `pinned` marker file in the folder): they are counted
   apart, newest 10 pinned besides the newest 10 ordinary, so a run of bulk operations cannot push
   out the "before anything changed" copy. **Restore from snapshot** (Device view,
   `ops_restore`, `device_snapshot_preview` / `device_restore_snapshot`) puts a snapshot's
   `music.db` back after a preview of what changes; it replaces `music.db` only: audio, beat
   grids (`.str`) and `settings.dat` stay as they are. A bulk grid write is therefore not undone
   by a restore; its previous grids are in the `grids-<UTC stamp>` folders beside the snapshots
   (newest 20 runs kept), to be copied back by hand.
4. Files are written temp + fsync + rename (`library::atomic_write*`, `install_pfw`'s `.part`,
   backup's `.partial` folder); every failure path removes the temp file, so an unplug leaves at
   worst a `*.tmp` (the Device clean-up lists those under `/pmdb_tracks`; the next write of the
   same file removes its own temps older than 10 minutes, which covers `.Pacemaker`), never a
   truncated file under a real name. `install_pfw` renames over an existing `.pfw` without
   deleting it first. A `.str` rewrite (`store_analysis`) restores the old file if the row update
   then fails, so file and row never describe different grids.
5. A new write command: use `quick_write` or `spawn_write_job`, never `Library::open` directly in
   a command. A new exclusive operation: take `take_exclusive` before it starts.

The Vocal filter (Revival firmware 180096073: the Wah tool becomes a vocal filter) is one flag file on the card.
`pm_formats::ops_device::{read_vocal, write_vocal}` own its fixed path; `revival-no-preload` (kill switch) is only ever read.
Mix previews and exports ask `export/render.rs::peq_mode(root)` for the same effective mode, so what is previewed is what is
exported; the mix editor names the Wah tool "Vocal" from the same flag (`state/vocal.svelte.ts`). `effective` is the flag
as the device will act on it: the kill switch, or a `system.xml` firmware known to be older than 180096073 (no hook there),
turn it off, so the editor, the preview and the export all show the stock Wah in those cases; an unreadable `system.xml`
version counts as unknown and leaves the flag in force. A rendered mix preview remembers the mode it was rendered with
(`views/mixes/previewState.ts::sameRender`); a change of the flag marks it out of date like an edit. `device_vocal_set`
treats an unreadable flag as unknown and writes anyway, so the card can be repaired from the app.

A preview window that starts after 0 is rendered from a block boundary about 190 ms earlier and the lead-in is dropped
(`pm-mixer` `engine.rs`, `PREROLL`), so filters, delays and the limiter are settled at the first kept frame and a mid-tool
start does not fade in from dry. Control-rate ticks (32 frames) are phased from the render's blocks, so a window matches a
render from 0 sample for sample but an export that starts at the first audible frame can differ from a preview by one
tick of control timing at a hard engage edge (measured about -34 dBFS); the DSP is the same.

Mix audio export is cancellable (`export_cancel` sets a flag the render's progress callback turns
into `false`; the engine removes its `.partial` file). Saving a beat grid for a track that mixes
use asks first (`grid_mixes_using` names them); the clips keep the old tempo until re-beatmatched.

## Shared and therefore integration-only

`routes/*`, `lib/styles/*`, `lib/format.ts`, `lib/shell/*`, `lib/ui/*`, `lib/views/ids.ts`,
`lib/views/registry.ts`, `lib/state/{session,nav,filters,selection,toast}.svelte.ts`,
`lib/api/{types,events}.ts`, `lib/devmock/{index,db,jobs,dialog,types}.ts`, `src-tauri/src/lib.rs`,
`commands/{mod,library}.rs`, `state.rs`, `jobs.rs`, `events.rs`, `error.rs`, `Cargo.toml`,
`package.json`, `tauri.conf.json`, `capabilities/*`.

Adding a dependency or a Tauri permission is also integration-only (rule 6).

## Checks

In `app/`: `npm run check`, `npm test` (the pure-logic tests, Node >= 22.6), `npm run test:mixedit`
and `npm run build`. In `app/src-tauri/`: `cargo test` (and `cargo build`, `cargo clippy --all-targets`).
From the repo root: `cargo test` (the whole workspace: `pm-formats`, `pm-analysis`, `pm-mixer`,
`pm-stems`, `pm-cli`).
`npm run dev` serves the UI in a browser against devmock (no Tauri needed). `svelte-check` covers
every file, so a half-finished component in one area shows up in everyone's check: keep your area
compiling, or stub the part you have not finished.

## Library on this computer

A library is `kind` "pacemaker" when its root is a volume root (a drive root, or a mount point `find_volumes` scans), else "computer" (`commands/library/kind.rs::library_kind`, which calls the pure text rule `pm_formats::library::is_volume_root` that `pm sync` uses too; mirrored by `lib/state/place.ts` for the browser preview; one table of paths in both tests). The library on this computer is a normal folder with the exact device layout, made by `Library::create`; settings (`path`, `open_at_start`) live in the app config folder (`commands/library/computer.rs`, commands `computer_library_get` / `_set` / `_create`). `session.start()` opens it at start when it exists and `open_at_start` is on; a Pacemaker is never opened by itself. Views say where a library is with `session.place` / `session.isComputer` (`lib/state/place.ts`): device-only controls (storage, eject, firmware, vocal filter, settings.dat, backup) are not shown for a computer library.
