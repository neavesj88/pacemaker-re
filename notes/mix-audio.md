# Mix audio engine

How a mix (`pm_formats::mix::Mix`, the document of `notes/mix-format.md`) becomes audio, for the
editor's preview and for export. Code: `crates/pm-mixer`; app glue: `app/src-tauri/src/commands/export.rs`
and `export/render.rs`; frontend contract: `app/src/lib/api/export.ts`.

**Tonium's own DSP is unknown.** The Editor's decompiled code was only read for the document
format, not for the audio path. Every algorithm and parameter mapping below is a choice made here:
musical, documented, and deliberately simple to change (each effect is one struct in `fx.rs`).
Anything that is a guess about the meaning of a document field is labelled **[H]**.

## 1. Pipeline

```
file -> decode (stereo, 44.1 kHz)
     -> tempo curve + key curve  (identity | WSOLA stretch [+ pitch read-out] | vinyl resample)
     -> loop / reverse / ignore regions
     -> tools (EQ, FILTER, Reverb, Echo, Roll, Wah, Crush, Trans, Volume) with tails
     -> clip gain envelope (dB)
     -> audible window [LOffset, Length - ROffset] with 32-sample edges
     -> crossfader gain from the transitions
     -> sum of all clips -> look-ahead limiter -> WAV (16-bit) | Ogg Vorbis
```

The timeline is processed in blocks of 4096 frames. A clip is decoded when its first block is
reached and dropped after its last, so memory holds the clips playing now (a 6-minute decoded track
is about 64 MB; at most about three at once in a normal mix, plus a 3-entry decode cache in the app
for repeated previews). One thread, no randomness: the same mix renders to the same bits every time
(tested). A full export of a 3-hour mix streams to disk; it never holds the mix in memory.

Units: every position in the document is a sample at 44.1 kHz. Positions of tools, loops and
envelopes are **clip-local** (counted from the file's first sample, after tempo stretch); the clip's
own `Start` places it on the timeline, so clip-local `p` plays at timeline `Start + p`. The audible
part of a clip is local `[LOffset, Length - ROffset]`.

## 2. Decoding

`pm_analysis::decode` returns a **mono** mix-down at the file's rate, which is right for analysis and
wrong for a stereo render, so `audio.rs` decodes with symphonia directly (same crate and version,
so no new codec dependency): the first two channels are kept, mono becomes dual-mono, a corrupt
packet costs one frame. Other rates are converted to 44.1 kHz with the windowed-sinc resampler
(`sinc.rs`: 16 taps, Blackman-Harris, kernel stretched when down-sampling so nothing folds back).

Audio files are found **through the library** (`mixtracks.libraryID` -> `tracks.location` -> file
under `Music/`), never from the document's `FilePath`, as `api/mixes.ts` requires. A clip whose track
was removed or whose file is missing is skipped with a warning (the rest renders).

## 3. Tempo and key (`clip.rs`, `stretch.rs`)

The tempo curve is `pm_formats::mix::TempoMap` (piecewise linear speed, `source_at(local)` = source
sample played at local time `local`). Three base signals:

- **Identity**: every speed node is 1 and the key curve is flat. The file is copied bit-exactly.
- **Time-stretch** (default): WSOLA. Frames of 2048 samples (Hann) with a synthesis hop of 512
  (75 % overlap, constant gain). For frame *k* the source position is `S(local)` from the tempo map,
  then moved by up to +-256 samples (5.8 ms) to the position whose waveform best continues the
  previous frame (normalised cross-correlation of the overlap, small penalty on the shift). The
  search is two-stage (coarse on a 4x decimated mono, then +-4 samples at full rate), so it costs a
  few hundred operations per output sample. With speed exactly 1 the natural continuation wins and
  the input is reproduced exactly. The tempo curve is followed continuously, so a ramp is a
  smooth tempo change; there is no drift (each frame is placed from the map, not from the last
  frame). Measured on a synthetic kick train: beat onsets stay within **2.1 ms** of the ideal position at
  +6 %, -7 %, and over a 1.0 -> 1.1 tempo ramp (tests assert < 4-5 ms). No special handling of
  transients beyond the small search radius; a hard attack can smear by the frame length when the
  stretch is large (> 15 %).
- **Pitch (vinyl)**: `NonInterpolated` clips are resampled at the speed ratio with the sinc kernel,
  so tempo and pitch change together. **[H]** The `KeyEnvelope` is ignored for these clips: the
  Editor's `createKeyEnvelopeFromSpeedVector` builds the key envelope *from* the speed vector, which
  suggests the key curve of a pitch-mode clip already encodes the pitch change the resampling makes;
  applying both would shift twice.

**Key (pitch shift).** `KeyNode.cents` is read as cents (100 = a semitone; the demo's -300 and +100.37
fit; the inventory calls it "semitone shift"), joined linearly between nodes, flat outside. A shifted
clip is rendered as: WSOLA time-stretch to a time axis `u` where `du/dt = 2^(cents/1200)`, then read
out at that rate with the band-limited interpolator. Duration and tempo curve are unchanged. Shifts up
to about +-5 semitones sound clean; larger shifts widen the stretch the WSOLA stage has to do and
get gritty. Tested: +-1200 and +100 cents give 880 / 220 / 466.2 Hz from a 440 Hz sine within 1 %,
alone and together with a 1.1x tempo change.

## 4. Edits (loop, reverse, ignore)

- **Loop** `[Start, Start + min(Length, Iterations * IterationLength))`: the slice
  `[root.Start, root.Start + root.IterationLength)` of the stretched audio is repeated; the output
  at local `t` is `slice[(t - root.Start) mod IterationLength]`. A loop that continues another
  (`Parent`, "Re-loop") follows the chain to the first loop and keeps its slice and phase. **[H]**
  The region replaces the audio inside it and the clip does not get longer (the document's `Length`
  of a clip is bounded by its file, `LimitSize`); `Mode`, `Offsets` and per-iteration offsets are not
  interpreted. The slice is captured when playback passes its start, so a preview that starts in the
  middle of a loop begins early enough to see it.
- **Reverse** `[Start, Start + Length)`: the same region played backwards (sample `a + b - 1 - t`).
- **Ignore**: muted, with 64-sample fades. **[H]** `StartPos` is not used.

## 5. Tools (`fx.rs`)

A tool is active for its clip-local range; envelopes are read every 32 samples at the tick centre,
node positions are relative to the tool's start, values are linear between nodes and flat outside. A
tool fades its wet signal in and out over 128 samples at its edges (dry + ramp * (fx - dry)), a
bypassed tool or effect is skipped. Reverb and Echo keep ringing after the tool ends (their input
becomes silence and the tail is added to the dry signal), but never past the end of the clip or
with more than 10 s (Echo) / 5 s (Reverb) tail. Missing envelopes fall back to the default in the
table. `Wet/Dry` is a percentage; the mix law of the beat effects is `(1 - w) dry + w wet`.

All knob ranges are **[H]**: the document stores the raw value of the Editor's slider, whose range is
unknown. The ranges seen in the demo mix (Reverb 0..50, FILTER -100..50, EQ Lo -100..0, ...) fit
-100..100 bipolar and 0..100 unipolar sliders, which is what is assumed.

| Tool | Keys | Mapping |
|---|---|---|
| **EQ** | `EQ-Lo/Mid/Hi` | Isolator: LR4 crossovers at 250 Hz and 2.5 kHz; the low band goes through an all-pass at 2.5 kHz so all gains = 1 is flat. `v <= 0`: gain `(1 + v/100)^2` (-100 = kill, -50 = -12 dB); `v > 0`: up to +6 dB at +100. Gains glide in about 2 ms. Default 0. Measured: kill removes > 35 dB (Lo at 50 Hz, Hi at 9 kHz) and > 25 dB (Mid at 1 kHz, the bands overlap), the other bands move < 1 dB, flat EQ < 0.05 dB. |
| **FILTER** | `FILTER-CutOff` | -100..100. Negative: low-pass from 18 kHz (just below 0) to 80 Hz (-100). Positive: high-pass from 25 Hz to 6 kHz (+100). \|v\| < 0.5: off. Two cascaded 12 dB/oct state-variable filters (24 dB/oct), Q 0.8. Measured: -70 takes 8 kHz down > 30 dB and leaves 60 Hz within 1.5 dB; +70 the reverse. |
| **Wah** (`Peq`) | `Wah-CutOff` | Resonant band-pass (Q 3), centre 400 Hz..3 kHz on a log scale over -100..100, added to the dry: `0.4 dry + 1.8 bandpass`. |
| **Crush** | `Crush-Crush` | 0..100 = a. Word length `16 - 10a` bits (fractional level counts), sample-and-hold factor `1 + 11a^2`, no dither. Off below 0.5. |
| **Reverb** (`Fx`) | `Reverb-Time`, `Reverb-Wet/Dry` | Two series Schroeder all-passes per channel feed an 8-line feedback delay network (23..66 ms, orthonormal Hadamard mixing, one-pole damping in the loop). RT60 = `0.3 + 4.7 * Time/100` s (defaults 30 -> 1.7 s, wet 30 %). |
| **Echo** (`Delay` type 1, also type 0) | `Echo-Time` (ms), `Echo-Time ` (beat division, **trailing space**), `Echo-Wet/Dry` | Stereo delay, feedback 0.45 cross-fed between the channels, low-pass in the loop. With `SyncAgainstBeat` the delay is `2^d` beats (`d` = `Time ` value: -2 = a 16th, 0 = a beat, 0.585 = 1.5 beats; the demo's range -2..0.585 fits) at the clip's tempo at that point (`Bpm * speed`); otherwise `Time` ms (1500 seen). Delay changes glide in about 100 ms (tape-style sweep). Defaults: one beat, wet 35 %. |
| **Roll** (`Delay` type 2) | `Roll-Time`, `Roll-Time `, `Roll-Wet/Dry` | Beat repeat: records one slice while it plays through, then loops it; same time parameters as Echo (default 1/2 beat); a new slice is captured when the length changes by more than 5 %; 32-sample fades at the loop edge. Default wet 50 %. |
| **Trans** | `StartBPM`, optional `Trans-Wet/Dry` | Tempo-synced gate, 8th-note period, open for the first half, 2 ms edges, phase locked to the tool start. The period follows `StartBPM`, or the clip's tempo at the tool start when that is 0. No sample exists, so the gate shape is a guess. |
| **Volume** | `Volume-Volume` | dB, glides. |

The clip's own **gain envelope** is the `Volume-Volume` envelope of the track's volume tool (node
positions relative to the volume event's start, usually 0; dB; linear in dB between nodes, ramped
per 32 samples); a bypassed volume tool is ignored. A bypassed track (`clip_bypassed`) is silent.

## 6. Transitions and the master bus (`engine.rs`, `master.rs`)

The document does not say which two tracks a transition joins. The rule used: of the clips audible
during the transition, the one that starts last is the **incoming** clip, and of the rest the one that
ends last is the **outgoing** clip. The transition envelope is the crossfader position `v(x)` (x = 0
at the start of the transition, 1 at its end; 0 = outgoing only, 1 = incoming only). Gains are
equal-power: outgoing `cos(v pi/2)`, incoming `sin(v pi/2)`; the incoming clip sits at `v(0)` before
the transition and the outgoing clip is untouched before it and stays at `v(1)` after it. Measured
with two uncorrelated sines and the default linear curve: the level stays within 1 dB of a solo clip
throughout the fade. `BeatMatch` and `CrossfadeType` do not change the audio (the envelope already
holds the shape). **[H]** Equal-power is the usual crossfader law, not something read from the
Editor. Clips that overlap without a transition simply add.

**Master**: a look-ahead peak limiter. Ceiling 0.98 (-0.18 dBFS); 3 ms (132 frame) look-ahead with a
linear attack ramp (the mean of the sliding minimum of the required gain, which guarantees the
output stays under the ceiling), 100 ms release. Below the ceiling the audio passes bit-exact, apart
from the delay (the engine compensates, so output length equals timeline length). Non-finite samples
are replaced by zero before the limiter. The output is therefore never above 0.98.

## 7. Output

- **WAV**: 16-bit stereo 44.1 kHz with deterministic TPDF dither. Written as `<dest>.partial` and
  renamed when complete, so a failed or cancelled export never leaves a half file.
- **Ogg Vorbis** (what the original Editor exports): `vorbis_rs` 0.5.6 (BSD-3, GPL-compatible),
  which builds the vendored aoTuV/Lancer libvorbis and libogg C sources with the `cc` crate: **no
  system libraries**, it compiled on Windows/MSVC next to the bundled SQLite. Quality VBR 0.6
  (about 160 kbit/s for music), fixed stream serial (so the same mix gives the same file),
  `TITLE` = mix name, `ENCODER` = Pacemaker Revival. FLAC is not offered.
- Previews are 16-bit WAV in the temp dir (`pacemaker-revival-preview/mix-preview-0..3.wav`,
  rotating so the file the player is streaming is never overwritten).
- Exports start at the **first audible frame** (leading silence is cut); previews keep timeline
  positions, so `fromSec` in the file is time 0.

## 8. Commands

`export_render_preview(root, mix, fromSec, toSec) -> { path, durationSecs, warnings }` and
`export_mix_audio(root, mix, dest, format) -> { path }` (see `api/export.ts`). `mix` is the editor's
`MixDoc`, converted with `MixDoc::to_mix`; the saved mix is not read, so unsaved edits are heard.

The export is a job (`mix-export`) on the shared `JobLock`: the command returns at once, then
`job-progress` (`done` / `total` in seconds of audio, at most 4 per second) and `job-done` follow. A
destination inside the device volume is refused (audio written there would not be in the library),
as is a folder or a folder that does not exist. Previews are limited to 15 minutes.

## 9. Performance and tests

Release build, one core: **149x real time** on a 3-clip, 6-minute mix with time-stretch, one key
shift, reverb, EQ, echo and two crossfades (`cargo test -p pm-mixer --release -- --ignored
--nocapture benchmark`; target was 10x). The stretcher alone is several hundred times real time.

`crates/pm-mixer/tests/render.rs` (synthetic float WAV clips in a temp dir): unit speed is a bit-exact
copy with the right length; clip offsets and trims; beat onsets after constant and ramped stretch;
pitch kept by the stretcher and moved by vinyl mode; key shifts; crossfade energy; no clipping or
NaN with three loud clips; bit-identical repeat renders; preview window equals the full render;
EQ/FILTER response; every tool kind renders and changes only its own range; reverb and echo tails
decay; crush levels; reverse, ignore and loop edits; gain envelope in dB; WAV and Ogg round trip
(decoded again and measured); cancellation leaves no file; missing file is a warning. Unit tests:
limiter, envelopes, sinc interpolation and resampling, EQ gain law. App side
(`export/render.rs`): library lookup against a temp volume, preview and Ogg export end to end,
destination checks, removed-track warning, preview file rotation.

## 10. Open questions

1. What each knob value means in the Editor (ranges, curves, EQ boost scale, filter frequencies). The
   mappings here are plausible, not matched; if a Pacemaker or the Editor is ever recorded playing a
   known mix, fit the curves in `fx.rs` against it.
2. Whether a pitch-mode clip's key envelope is additional or already contains the speed's pitch.
3. Loop semantics (does the loop add time? `Mode`, `Offsets`) and the Ignore `StartPos`.
4. Which clips a transition joins, and the true crossfader law.
5. `Trans` has no sample document; its gate shape and parameters are a guess.
6. WSOLA quality on very large stretches (> 15 %) or key shifts beyond +-5 semitones; a phase-vocoder
   or transient-preserving mode could be added behind the same `Base` interface.

## 11. Robustness rules (review pass)

- A render in which every clip failed to load is an error, not a silent file; no output is created and an existing destination is untouched. With some clips failed the export finishes but its summary says it is not the complete mix.
- The finished file replaces an existing one by rename only (no delete first).
- A corrupt packet is decoded as silence of the previous frame's length, so later audio keeps its place on the beat grid; the engine warns with the count.
- Key cents are clamped to +-4800 (non-finite = 0) when the pitch warp is built.
- The export destination must end in the format's extension; progress total is the audible span (first audible frame to last clip end) and a final done == total event is sent.
- Preview WAVs live in `pacemaker-revival-preview-<pid>` under the temp folder; folders untouched for 24 h are removed on the first preview of a run.
- Not done: cancelling an export (needs a cancel flag in the shared job layer; the engine already stops when the progress callback returns false) and a Preview/Export UI in the mix editor (integration work).
