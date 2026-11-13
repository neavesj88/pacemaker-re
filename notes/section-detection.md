# Section detection (phrase / section markers)

Status: built 11 Nov 2026 (step A3). Heuristic, checked on synthetic audio only. Not validated on a labelled set of real tracks.

## What it does

For each track it finds intro, build, drop, breakdown and outro sections, snaps their boundaries to 8 / 16 / 32 bar phrases, and shows them as coloured markers on the Tracks table's waveform and on the grid editor's overview (with a chip list under it). The device cannot store sections, so they live in an app-side file per library (`<app config dir>/sections-<hash of the library root>.json`) and never appear on the card. The settings are in `sections-settings.json` next to it.

Code: `crates/pm-analysis/src/sections.rs` (method), `app/src-tauri/src/commands/analysis/sections.rs` (job, cache, commands), `app/src/lib/views/analysis/` (`SectionBand`, `SectionList`, `SectionsPanel`, `sectionLogic.ts`), `app/src/lib/state/sections.svelte.ts` (store and the snap API).

## Method

1. Bars come from the stored beat grid: four beats per bar from the first downbeat; a grid that ends before the audio does is continued at its last beat period. A track with no usable grid or no first downbeat is skipped with that reason (it is not guessed).
2. Per bar (decoded mono audio, 2048-sample FFT, hop 1024): level in three bands (20-150 Hz, 150-2000 Hz, 2-10 kHz), total level, and spectral flux (how busy). These bar rows and the bar edge times are what the cache stores, with the audio's file size and the `.str` size and modified time.
3. Detection (`detect`) runs on the cached rows, so the settings apply with no new decode. Each feature is normalised to the track's own 5th to 95th percentile range (with a floor so near-constant features are not stretched). A Foote checkerboard kernel (half width = half the shortest section, 2 to 8 bars) slides along the similarity between bars; novelty peaks above a floor set by the sensitivity (0.17 to 0.05 on a 0 to 0.5 scale) and above 20% of the strongest peak are boundary candidates.
4. Phrase grid: with the length on "automatic", the largest of 32 / 16 / 8 bars on which at least 70% of the strong boundaries sit (within a bar) is used; if none fits there is no phrase grid and boundaries stay where the music changes. A boundary within the snap distance (default 2 bars) of a phrase line moves onto it. The grid starts at the first downbeat unless "best fit" is chosen, which moves the origin only for a clear gain.
5. Boundaries closer than the shortest section (default 8 bars) are dropped, strongest first.
6. Labels come from each section's mean level relative to the track's quietest and loudest bars: loud is a drop; otherwise the first section is an intro and the last an outro; a quiet section between others is a breakdown; anything in between is "section". A rising stretch (last third minus first third, at least a quarter of the level range) that leads into a clearly stronger drop is a build; a ramp at the end of a longer section is split off as a build of one or two phrases.
7. Confidence (0 to 1) combines the strength of the section's two boundaries (reduced for a boundary that is off the phrase grid), how decisively the label fits, and how much the track's level varies at all. It is a relative score for fading markers and ranking, not a probability.

## Snapping for the mix editor

`sections.snap(trackId, secs, { maxSecs, into, from, minConfidence, onPhraseOnly })` in `state/sections.svelte.ts` (pure form: `snapToBoundary` in `views/analysis/sectionLogic.ts`) returns the nearest section boundary within reach, ties to the earlier one, or null when the track has none. `sections.boundaries` and `sections.neighbour` list and step through them. Times are seconds in the beat grid's time base, i.e. the same source-time axis as the grid editor's bar lines. The Rust side has `snap_secs` and `sections_by_track` (read API, no caller yet). The mix editor, the auto-transition recipes and the set builder use it as described next.

## Sections in mixing (W2)

All of this is app-side and optional: a track without sections behaves exactly as before.

- **Snap mode "Section"** (Snap menu next to Beat / Bar / Phrase). Targets are the section boundaries of every other clip (`clipSectionPoints` in `state/mixedit.ts`, placed on the timeline through the clip's tempo curve like its beats), and the dragged clip's own boundaries are what land on them. A clip whose track has none contributes its phrase lines instead, so with no sections anywhere the mode is exactly "Phrase". Alt still switches snapping off for a drag.
- **Markers on clips** (checkbox "sections" in the editor toolbar, remembered in browser storage; also needs "show markers on waveforms" in the Sections panel's settings): `ClipSections.svelte` draws a coloured band and a dashed boundary per section on the audible part of a clip, fainter when the detection is less sure. It takes no pointer events.
- **Auto transitions** (setting "Sections" under the editor's Auto transition settings; default off; "Boundary within (bars)", default 8): `state/sectionTransition.ts` moves the start, the end or both of the backend's suggested window to the nearest section boundary of either clip inside the part where both are audible. A start that moves keeps the length (shortened to whole beats if it would run past either clip); an end that moves changes the length. Nothing within reach: the backend's window is kept and a note says so. The recipes then build on the moved window (a bass swap whose window moved gets its tools from the app, `RecipeOptions.moved`, since the backend's sit on the old window). The notes after building say what moved and by how much. The set builder's "Create mix" uses the same code and the same setting.
- **Set builder** (setting "Prefer leaving a track on its outro and arriving on an intro"; default off): a hop out of a track with an outro of at least 8 bars (`mixSides` in `views/analysis/sectionLogic.ts`) and into one with an 8-bar intro takes 2 points per side off the hop's search cost (`SECTION_BONUS`). It never changes the hop's score, and is far below the 60-point cost of a broken key or tempo limit, so it only chooses between hops that are otherwise alike. Tracks without sections get no bonus and no penalty. A hop's row says "leaves on an outro" / "arrives on an intro" / "outro into intro" when the preference is on and it applies.

Limits: the boundaries are estimates (see below), so a transition aligned to one is only as good as the detection; the notes and the markers show what the app believed, and Undo restores the plan before it.

## Accuracy limits (honest)

- Verified only on synthetic signals (generated tracks with a known structure, in the unit tests): boundaries found within a bar, labels right, a flat groove gives no sections, phrase grid chosen correctly on constructed cases. The thresholds were tuned while writing those tests, so the synthetic results say the code does what it is meant to, not that it is accurate on music.
- No evaluation on real tracks has been done. Expect it to work best on four-on-the-floor dance music with a steady, correct grid and clear level changes between sections.
- It depends on the grid: a wrong first downbeat shifts every bar and every phrase; tempo changes and half-time sections confuse the bar features.
- It sees level, spectrum and busyness, not harmony or melody. Sections that differ in notes but not in loudness are not found, and a loud breakdown reads as a drop.
- Builds are the least reliable label (a ramp has no step for the novelty to find; it is picked up by a separate rising-level test).
- Tracks that change little give no sections ("no clear sections at this sensitivity"); a higher sensitivity finds more, including false ones.
- A track's sections are measured against the grid it had at analysis time. The state command does not look at the card, so after editing a grid the markers keep the old bar lines until "Detect new or changed" (which does look at each `.str`'s size and modified time) measures that track again.
- Unverified: behaviour with MP3s with a gapless header (the grid already carries the device offset of 22 ms; at bar level this does not matter), variable-tempo grids on real music, very long files (a one hour mix is about 1900 bars; the code handles it, speed was not measured).
