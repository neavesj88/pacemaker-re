# Mix document format

The `MixDocument` inside `mixes.structured_ct` and `.pmk` files, as modelled by
`crates/pm-formats/src/mix.rs` (typed model, parser, writer) and stored by `library.rs`
(`read_mix`, `create_mix`, `save_mix`, `delete_mix`). The zlib+XOR container is in
`mixdoc.rs` and FINDINGS section 4; this note is about the XML inside.

Evidence tags: **[V]** verified (byte-exact on a real Editor-written document, or read directly
from the decompiled Mac Editor 2.0.2.14170 i386 slice), **[I]** inferred from data or from code
that was read but not executed, **[H]** hypothesis, nothing contradicts it but nothing proves it.

## 0. Material and results

Real documents available locally: **2 distinct** mix documents, both the Editor's bundled
"Example Mix": the prefab `bin.pmk` (the same bytes as `work/inv/example.xml`) and the copy the
Editor stored in its library `music.db` after first start (differs only in the three `libraryID`s,
the three `FilePath`s, which become absolute resource paths, and a last-digit change of one
double, appearing twice, caused by the Editor's own load/save). The device dump (`work/device-dump/music.db`) has
no mixes, and no other `.pmk` exists in the Editor installs. Every claim below about shapes that
do not occur in the demo mix (Trans, Ignore, Reverse, tools before the first track tool, ...) is
therefore [I] or [H], and says so.

Round trip [V]: both documents parse into the model and `Mix::to_xml` reproduces the original
XML **byte for byte** (`tests/mix_fixtures.rs`; the test also checks both containers, and the
blob re-encode is byte-identical, as FINDINGS 4b already showed for the container). The same
test runs over any further document it finds, so new samples are checked automatically.

Decompiled with Ghidra from the thinned Mac Editor (`scripts/ghidra-decompile.sh`):
`oserializer<xml_oarchive, X>::save_object_data` for every mix class, `AudioClip::save`,
`AutomationItem::getEnvelopeAsBase64` / `setEnvelopeFromBase64` / `convertOldToNewFormat`,
`AudioClip::getKeyEnvelopeAsBase64` / `createKeyEnvelopeFromSpeedVector`,
`MixLibraryComponent::createMix` / `saveMixWithId` / `saveMixDocumentToDatabase` /
`updateTracksInMixTracks`, `getMixDocumentPlayTimeSecs`,
`ChannelLayoutComponent::getPreviewMarkerPositions`, `DirectAccessRepo::insertMixtrack`.

## 1. The XML dialect (boost `xml_oarchive`) [V]

    <?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
    <!DOCTYPE boost_serialization>
    <boost_serialization signature="serialization::archive" version="4">
    <MixDocument ...>          <- column 0, children indented one tab per level
    ...
    </boost_serialization>     <- LF line ends throughout, final newline present

Unlike the `.str` archive (FINDINGS 4), the closing tag is present and line ends are LF.

- An element with children is `<tag attrs>` + newline, children, `</tag>`. A leaf is one line,
  `<tag>text</tag>`; an empty string is `<tag></tag>`; a null pointer is
  `<Parent class_id="-1"></Parent>`.
- Text escapes: `& < > " '` as `&amp; &lt; &gt; &quot; &apos;` (boost's `xml_escape`). The
  writer escapes these five; the parser also accepts numeric entities. Strings are UTF-8.
- Doubles are printed with `%.17g`, floats with `%.8g` (the archive sets the stream precision to
  `digits10 + 2`: 17 and 8). Bools are `0`/`1`. `crate::strfile::fmt_g` does the formatting.
- Collections (`std::vector`, `std::map`) are `<count>N</count><item_version>0</item_version>`
  followed by `<item>` elements. `item_version` is 0 everywhere. Maps are `std::map`, so the
  items come **sorted by key, byte-wise** (`Roll-Time` < `Roll-Time ` < `Roll-Wet/Dry`).
- **Boost bookkeeping is a pure function of the document**, so the model does not store it and
  the writer regenerates it: the first object of a class that has class info gets
  `class_id="N" tracking_level="T" version="V"` (N = 0, 1, 2, ... in order of first
  appearance); an object saved through a base pointer additionally gets `class_name="Export"`
  on that first appearance and `class_id_reference="N"` afterwards; a tracked object
  (`tracking_level="1"`) gets `object_id="_K"` (K = 0, 1, ... in pre-order over all tracked
  objects of the archive). A pointer to an already-saved tracked object is
  `class_id_reference="N" object_id_reference="_K"` (only `LoopAudioProcess.Parent` does this).
  The demo mix uses class ids 0..31 and object ids 0..109, all reproduced exactly.
- `AudioProcessContainer` has no class info (tracked, never a class id), everything else the
  mix touches has. Class table (name, tracked, version), all [V] from the demo unless marked:

| class | tracked | ver | note |
|---|---|---|---|
| MixDocument | 1 | 1 | |
| AudioProcessMixer (`Tracks`, `Transitions`) | 1 | 0 | holds one `Processes` vector |
| `vector<AudioProcess*>` (`Processes`) | 0 | 0 | |
| Track (`AudioClip`, export "Track") | 1 | 2 | pointer item |
| StreamedAudioProcess | 1 | 0 | |
| AudioProcess (`Name`, `Bypassed`) | 0 | 0 | member of every process |
| AutomatedAudioProcess | 1 | 0 | pointer item (tool) and by-value member (`VolumeEnvelope`) |
| FxAudioProcess / Filter / Crush / Eq / Peq / Volume | 1 | 0 | pointer items |
| DelayAudioProcess | 1 | 1 | |
| LoopAudioProcess | 1 | 2 | |
| TransAudioProcess, IgnoreAudioProcess, ReversalAudioProcess | 1 | 0 | [H] no sample; tracked/version assumed like the other process classes |
| Transition | 0 | 3 | pointer item, not tracked |
| PositionedEvent, LocalContext, GlobalContext, FuzzyID, speed entry | 0 | 0 | |
| AutomationItem | 0 | 1 | the map value |
| every std::vector / std::pair / std::map | 0 | 0 | |

## 2. Schema

Positions and lengths are in **samples at 44 100 Hz** [I]: the Editor's play-time function
divides by the engine rate, and the demo's first track (302 s stored, tempo ratio 1.0081) has
`Length` 13 232 037.85 = 300.05 s.

### MixDocument (version 1) [V]
`DocumentName` (= `mixes.name`; the Editor sets it from the row on every save), `DeviceInfo`
(empty), `Genre`, `Artist` (empty in the demo), `Tracks`, `Transitions`.

### PositionedEvent [V order, types from `PositionedEvent::save`]
`Start`, `Length`, `LOffset`, `ROffset` (doubles), `LimitSize` (bool), `SessionEventId` (string).
`LOffset`/`ROffset` are trim offsets: 0 on tools, but the demo's second and third tracks have
`LOffset` 9 925 797.47 and 9 589 191 (the cue: audio hidden at the left edge). **Geometry [I,
fits all three demo tracks and both transitions]**: `Start` is the timeline position of the
file's first sample, so a track is audible on `[Start + LOffset, Start + Length - ROffset]`; the
two transitions start at the later tracks' `Start + LOffset` (10 913 614 and 15 206 987 vs the
transitions' 10 913 614.4 and 15 210 616) and end near the earlier track's end. Everything inside
a track (tools, loops, envelope nodes, `PitchEnvelope`/`KeyEnvelope` positions) is **track-local**:
samples of the stretched audio counted from the file start, so tool positions are relative to
`Start` (the third track's first tool starts exactly at its `LOffset`, 9 589 191). The tempo
curve maps track-local to source time: see `mix::TempoMap`. `LimitSize` is 1 on a track's own event
and 0 on tools, loops and the volume envelope. `SessionEventId` is a `juce::Uuid` (32 lowercase
hex) on every event except the volume envelope, where it is empty; the Editor registers it for
its clipboard/undo bookkeeping (`PositionedEventContainer::addPositionedEvent` when non-empty).

### Track (class "Track" = `AudioClip`, version 2) [V order]
1. `StreamedAudioProcess`: `AudioProcess{Name "AudioClip", Bypassed}` and an
   `AudioProcessContainer` whose `Processes` is the track's **process list** (below).
2. `PositionedEvent`: the clip on the timeline (start, length after tempo stretch).
3. `LocalContext.libraryID` (64-bit): the `tracks.track_id` in the library the mix was saved
   from; in a device copy it is the device's id.
4. `GlobalContext`: `toniumId` (empty), `otherIds` (map, keys `graceNoteId`, `musicDnsId`, empty
   values), `fuzzyId` = `artist`, `album`, `title`, `genre` (a fallback identity so a mix can be
   re-linked after a library change [I]).
5. `FilePath`: the audio file. Bare hashed name in the prefab (`<hash>.mp3`); the Editor's
   library copy has an absolute resource path. On a device mix we expect the `Music/A/B/<hash>`
   style name [H].
6. `Bpm` (double, the track's analysed tempo), `Channel` (u32, 0 or 1: the two decks the Editor
   alternates between, [I]), `Offset` (u32, 0), `NonInterpolated` (bool; pitch mode, FINDINGS
   inventory "Timestretch vs Pitch" [I]).
7. `PitchEnvelope`: vector of `{position (u32), speed (float)}`: the **tempo curve**. `speed` is
   the playback ratio (target BPM / `Bpm`; 128/126.99 = 1.0079 vs 1.0080842 seen). [I]
8. `KeyEnvelope`: one string, JUCE base64 of `u32 count` + `count` x 12-byte records
   `{u32 position, f32 cents, u32 flags}` (§3). [V]
9. `VolumeEnvelope`: an `AutomatedAudioProcess` **by value** (Name `noname`, one
   `VolumeAudioProcess`, `LimitSize` 0, empty `SessionEventId`, one key `Volume-Volume`).

### Process list items (children of the track's `Processes`) [V]
Polymorphic pointers in document order (tools and loops are interleaved; the demo's list is
not globally sorted by start):
- `AutomatedAudioProcess` (a **tool**, §4): `StreamedAudioProcess{AudioProcess{Name
  "ToolProcess"}, AudioProcessContainer{Processes: the effect(s)}}`, `PositionedEvent`,
  `AutomationMap`.
- `LoopAudioProcess` (v2): `AudioProcess{Name "LOOP"}`, `PositionedEvent`, `Length` (double, one
  iteration), `Iterations` (u32), `ReLoop` (bool), `Mode` (bool), `Parent`, `Offsets`.
  `Parent` is a pointer to another `LoopAudioProcess` of the same track: null, or the loop it
  continues (`object_id_reference`). The demo has a chain of two: the second has `ReLoop 1` and
  points at the first. `Offsets` is `vector<{u32 first; {float, float} second}>`: iteration
  number, then two floats of which the second is a sample offset (-42100.23, 258.6105 seen) and
  the first 0 [I]; `Length`/offset semantics follow `LoopAudioProcess::getOffsetForIteration`
  (not decompiled).
- `IgnoreAudioProcess` [I, no sample]: `AudioProcess`, `PositionedEvent`, `StartPos` (u32).
  Field name from the string pool next to "IgnoreAudioProcess"; layout from the decompiled writer.
- `ReversalAudioProcess` [I, no sample]: `AudioProcess`, `PositionedEvent`.

The Editor-side writer skips some members when the archive flag `0x200` is set (a lite form,
used for the clipboard [H]); mix files use the full form, which is the only one modelled.

### Effects (inside a tool's inner `Processes`) [V unless marked]
Every effect has `AudioProcess{Name, Bypassed}` first. Class, then extra members:

| class | Name seen | extra members |
|---|---|---|
| FxAudioProcess | Reverb | `Active` |
| FilterAudioProcess | FILTER | none |
| CrushAudioProcess | Crush | none |
| DelayAudioProcess (v1) | Echo, Roll | `Type` (Echo 1, Roll 2), `Active`, `SyncAgainstBeat` |
| EqAudioProcess | EQ | none |
| PeqAudioProcess | Wah | none |
| VolumeAudioProcess | Volume | none |
| TransAudioProcess | Trans | `StartBPM` (float), `Active` [I, no sample, decompiled writer; field names from the string pool] |

"Delay" (the tool menu entry) is a `DelayAudioProcess` too; `Type` 0 is its presumed value
(`setAsDelay` / `setAsEcho` / `setAsRoll` exist) [H].

### AutomationMap [V]
`map<string, AutomationItem>`, key `<EffectName>-<Parameter>`, value version 1 = a single
`<Envelope>` string (§3). Version 0 was a vector of XML nodes
(`automation_node`, converted by `convertOldToNewFormat`); it is rejected as unsupported.

| key | nodes | value range seen (unit) |
|---|---|---|
| `Reverb-Time`, `Reverb-Wet/Dry` | 2 | 0..50, 0..49.6 |
| `FILTER-CutOff` | 2-3 | -100..50.4 (bipolar: negative = low-pass side [H]) |
| `Wah-CutOff` | 3 | -33.4..49.6 |
| `Crush-Crush` | 2 | 0..65.9 |
| `EQ-Hi`, `EQ-Mid`, `EQ-Lo` | 1-5 | Hi 0..2, Mid 0..4, Lo -100..0 (-100 = kill [H]) |
| `Echo-Time`, `Roll-Time` | 1 | 1500 (ms, `"%d ms"` format string in the binary) |
| `Echo-Time `, `Roll-Time ` (**trailing space**) | 1-4 | -2..0.585 (beat division, `"%d nr"` [H]) |
| `Echo-Wet/Dry`, `Roll-Wet/Dry` | 2-3 | 0..84.7, 24.8..74.9 |
| `Volume-Volume` | 1-5 | -5.0..0 (dB [H]) |

The trailing-space keys are real (the Editor builds them from the string "Time "). Node values
are the parameter's real value, not normalised.

### Transition (version 3, not tracked) [V]
`AudioProcess{Name "Transition"}`, `PositionedEvent` (the overlap on the timeline), `BeatMatch`
(bool), `Envelope`, `CrossfadeType` (u32; 1 and 3 seen: "fade after 1/2", "after 3/4" from
`TransitionComponent::onMenuItemClicked` [I]). `Envelope` is **XML**, not base64: a
`vector<{u32 first; float second}>` with `first` normalised over the overlap, 0..4294967295, and
`second` the crossfader position 0..1 (seen 0, 0.34, 0.5, 0.85). There are N-1 transitions for N
overlapping tracks in the demo, but the document does not link a transition to its tracks
(positions only) [I].

## 3. Binary envelope encodings (the old TODO) [V]

All are JUCE `MemoryBlock::toBase64Encoding` (`juce_b64.rs`) of little-endian data.

- **Automation / tool envelope** (`getEnvelopeAsBase64` decompiled): `u32 count`, then `count` x
  `{u32 position, f32 value}`. `position` is a sample offset from the start of the owning event
  (`Start`), not an absolute position; the last node of a two-node envelope sits at
  `Length - 1` (the demo's reverb, `Length` 2 666 659, ends at node 2 666 658). The demo's 43
  envelopes (32 distinct) and 3 key envelopes decode and re-encode identically.
- **Key envelope** (`getKeyEnvelopeAsBase64` / `setKeyEnvelopeFromBase64`): `u32 count`, then
  `count` x `{u32 position, f32 cents, u32 flags}` (12 bytes, the in-memory
  `speed_entry_lite`). `createKeyEnvelopeFromSpeedVector` fills `flags` with the constant 1 and
  the second word with `log(speed)/log(2) * K`; all 3 demo records have `flags` 1. The XML form
  of the same vector (used when archive flag bit 10 is set) writes only position and value.
  `cents` unit is [I]: -300.0 and +100.37 occur.

## 4. Tool lane

A tool = one `AutomatedAudioProcess`: a time range (`PositionedEvent`) plus one effect plus one
envelope per automated parameter. Tools of the same kind do not overlap
(`ToolOperations::adjustNotAllowedToOverlappTools`); EQ and FILTER tools can start at the same
time as another effect (the demo has Reverb+EQ pairs with identical position). Tool names in
the document: Reverb, FILTER, Crush, Echo, Roll, Wah, EQ (+ Trans, Delay unseen); LOOP,
IGNORE, REVERSE are process-list items, not tools.

## 5. Relation to `mixes` / `mixtracks` (verified by decompilation; one real row set)

Editor save path (`saveMixWithId` -> `saveMixDocumentToDatabase` -> `updateTracksInMixTracks`):

1. Set the document title from the row name (`MixDocument::setTitle`).
2. `Repo::modifyMix`: `name`, `play_time_secs` (`getMixDocumentPlayTimeSecs`),
   `preview_markers` (`getPreviewMarkerPositions`), `date_modified` = now (Unix ms). Other
   columns (genre, year, rating, producer, ...) are edited in the metadata dialog only.
3. `Repo::modifyMixContent`: `structured_ct` = the blob.
4. `updateTracksInMixTracks`: delete the mix's `mixtracks` rows, then for each clip in document
   order `INSERT INTO mixtracks VALUES (mix_id, libraryID, NULL, NULL, NULL, NULL, index,
   cue_point)`. `track_index` is the position in the track list (0-based).

Matches the demo's rows [V]: 3 `mixtracks` rows, `dup_*` NULL, `track_index` 0, 1, 2, in
document order (`libraryID` 1, 3, 2).

`createMix`: row with `date_created` = now (ms), `year` = current year, counters 0,
`creator_id` = `Tonium;Editor;<ver>;<mac>`, name from `mix_name_start` + `%.2u` + `mix_name_end`
(next free number); then an empty document is saved. Save As refuses an existing name and an
empty document (`save_mix_no_tracks`). `deleteMix`: sleeve image row, `mixtracks` rows, the mix
row. A track removed from the library turns its `mixtracks.track_id` negative and fills
`dup_title/artist/album/play_time_secs` (FINDINGS 3); `save_mix` keeps those rows when the
document still holds the negative id (the Editor re-derives them on load, which we do not model).

`preview_markers`: `juce::XmlElement` `<tracks>` with one `<previewmarkers position="N"/>` per
entry of the layout's marker list, N = -1 while "Show Preview Markers" is off. The demo row
(`<tracks><previewmarkers position="-1"/><previewmarkers position="-1"/></tracks>`, two
elements, two transitions) is consistent with one per transition [I]. `Mix::preview_markers_xml`
writes that, and `<tracks/>` for none.

## 6. API (crates/pm-formats)

`mix`: `Mix`, `Track`, `Transition`, `Tool`, `Effect`/`EffectKind`, `LoopProc`, `IgnoreProc`,
`ReverseProc`, `Process`, `Position`, `FuzzyId`, `Envelope`/`Node`, `SpeedNode`, `KeyNode`;
`Mix::parse`, `parse_exact`, `to_xml`, `from_blob`/`to_blob`, `from_pmk`/`to_pmk`,
`play_time_secs`, `preview_markers_xml`; constructors `Track::new`, `Tool::flat`, `Effect::*`,
`Transition::new`, `Envelope::flat`. The parser is strict (unknown element, unknown class,
version other than the known one = error) so a successful parse never silently drops data.

`library`: `Library::{mix_infos, mix_info, mix_track_rows, read_mix, create_mix, save_mix,
delete_mix}`, `MixInfo`, `MixTrackRow`.

## 7. Open questions

1. **Class registration when the first `AutomatedAudioProcess` is the by-value
   `VolumeEnvelope`** (a first track with no tools, followed by a track with tools). Our writer
   gives the member the class info and later tool items `class_id_reference`, the mechanical
   boost rule as we know it; the Editor must produce something it can reload, but the demo never
   exercises it and boost's `basic_oarchive_impl` is not a decompilable symbol in the binary.
   Needs a sample or a test with the Editor.
2. **`play_time_secs`.** The demo row says 600; `getMixDocumentPlayTimeSecs` (max track end /
   sample rate, truncated) gives 607 for the same document. Either the prefab
   import path writes a round value or the virtual it calls (slot 0x2c on `AudioClip`) is not
   `start + length`. We write the formula.
3. **`mixtracks.cue_point`.** Demo rows hold 0, 10 166 523, 9 589 191. Resolved for constant
   tempo curves: the value is the **source position of the left trim**, `TempoMap::source_at(LOffset)`
   (9 925 797.47 x 1.0242525 = 10 166 523; speed 1 for the third track), and `Library::save_mix`
   writes that now. For a tempo curve that is not constant this is a hypothesis (the curve is
   assumed piecewise linear). The device reads this column (`getCuePositionFromMixTrackId`); not
   decompiled on the pmplayer side yet.
4. Trans / Ignore / Reverse / Delay type 0: layout from the writer only; class versions and
   tracking assumed; automation key names for Trans, Ignore, Reverse and Delay are unknown.
5. Units of each envelope value, `Channel` and `Offset` semantics, `CrossfadeType` values other
   than 1 and 3, `Loop.Mode`.
6. Hardware: no document we write has been loaded by the Editor or a Pacemaker. Unchanged
   documents are byte-identical to the original, so they are safe; new structure is not tested.
7. Older class versions (Track < 2, Transition < 3, Loop < 2, AutomationItem 0) have loaders in
   the Editor and are rejected here.
8. Mix sleeve (`mix_image`, `miximages` JUCE-base64 PNG) and the row's genre/year/rating fields
   are not part of the document and not modelled by `save_mix`.
