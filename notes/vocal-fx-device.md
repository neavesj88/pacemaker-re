# On-device "VOCAL" FX for the Pacemaker: research and hook plan

Static analysis only. Nothing run on a device, nothing built, no code outside this file changed. Tags: **[V]** verified in
decompiled code / ELF bytes this session, **[I]** inferred from verified facts, **[H]** hypothesis, **[U]** unverified (from
memory or not checked). Target: firmware 180096069, `work/fw-180096069/bin/pmplayer`, Ghidra project
`work/ghidra/fw-180096069-pmplayer`; addresses are ELF virtual addresses. Scratch (symbol dump, decompiles `dc1`..`dc6`,
`rw.py` word reader) is in `work/vfx/`.

## Short answer

Replace the `process` slot in the **vtable of `PeqAudioProcess` (the "Wah" FX)** with our own function from the
`LD_PRELOAD` library described in `favourite-button.md`. It is a one-word write into read-only data (after `mprotect`), not
a code patch, so no instruction-cache issue. The new function consumes the stock events (so the touch pad still drives the
stock parameter), then runs a time-domain mid/side centre-band filter on the deck's stereo float buffer. The bipolar pad
gives: centre = off, one direction = vocal focus (centre mid band kept, rest faded out), other direction = vocal reduction
(centre mid band subtracted). Relabel "Wah" to "Vocal" in `english.lang` (and the other .lang files). Adding a 14th FX
entry is possible but needs 5+ patch sites (section 6); not recommended first.

Realistic result: a centre-band emphasis/cut, not true isolation. Good on mono-centred vocals over drums/bass, poor on
stereo-widened or reverb-heavy EDM vocals (section 7). CPU cost of the time-domain version is negligible (section 5).

## 1. How the FX are built and selected

### 1.1 Engine structure [V]

- Per deck there is an `audiocore::MixChannel` (a `StreamedAudioProcess`; ctor 0x493538). `audiocore::MixEngine` (ctor
  0x470d40) owns two of them (at `MixEngine+0x20` and `+0x380`), `MixEngine::process` 0x470568.
- The MixChannel ctor creates these processes and `addBack`s them to its `AudioProcessContainer` (at `MixChannel+0x18`), in
  this order: **Crush** (`+0x338`), **Peq = "Wah"** (`+0x344`), **Filter** (`+0x328`), **Trans** (`+0x33c`), **Echo**
  (DelayAudioProcess, `+0x32c`), **Roll** (`+0x330`), **Delay** (`+0x340`), **Reverb** (FxAudioProcess, `+0x334`), **EQ**
  (`+0x2ac`). `StreamedAudioProcess::process` (0x4afab8) calls them front to back through vptr slot `+0x14`, first
  in->out, then out->out in place. `AudioProcessContainer::addBack` (0x497258) re-sorts the container after every add with `AudioProcessFilterSorter`
  (operator() 0x498dc4 compares ranks looked up by lower-cased `getName()`), and the rank table built in its ctor (0x499974;
  strings read from the ELF) is transitioncomponent 0, reverse 1, loop 2, crush 3, **peq 4**, filter 5, trans 6, echo 7,
  roll 8, delay 9, reverb 10, eq 11 [V, 11 Nov 2026]. The names given to the Crush and Peq constructors in
  `MixChannel::MixChannel` (0x493538) are the literals `crush` and `peq` (0x86de80 / 0x86de88), so the sort reproduces the
  constructor order. An unlisted name would get rank 0 from `operator[]` and sort first, but Crush (rank 3) is always in
  the container and sorts before Peq, so Peq is never at index 0.
  Wah therefore sits after Crush and before Filter.
- `MixChannel::processExt` (0x49176c): position/time-stretch process writes the deck signal to a "cue" buffer (`param_3`),
  then the FX chain runs `param_3 -> param_2` (or only EQ when `param_6` is true). The FX output goes on to the crossfader.
  So the FX are per deck, post time-stretch, pre-crossfader, pre-master-EQ, and the cue/headphone buffer looks FX-free [I].
- The same classes (`FilterAudioProcess`, `PeqAudioProcess`, `CrushAudioProcess`, ...) are also used by mix-document tools
  (`ToolProcess`, see `mix-format.md`), so a vtable patch changes mix playback of Wah tools too (section 8).

### 1.2 Selection and control on the UI [V unless tagged]

- Settings (`PMSettingsMixMode` ctor 0x33c6f0) hold 8 `FloatListSetting`s, one per pad direction: "North/East/South/West"
  (string ids 202..205) and the alternate set "P-up/North.." (198..201) (`english.lang`, `work/fw-180096069/bin/resources/fonts/`).
  Defaults 0,1,2,3 and 7,6,4,5. `PMRunningApp::sendFxMap` 0x33e4c0 packs the 8 indices into a `pecomms::FXMap` and sends it
  (`pecomms::ui2rt::SendActivateFXMap` 0x3e3480 -> `bob::Player::IUi2rt_activateFXMap` 0x3ee6f0).
- The choosable list (`FloatListSetting::setupValidValues` 0x35fefc, case 0x6e) has 13 entries, language ids
  `6b 69 6a 68 70 6d 6f 6c ea eb ec ed f6` = Gain, Mid, Treble, (104, absent from english.lang), Reverb, Echo, Roll,
  "Hi-lo cut" (= FILTER), Crush, Trans, **Wah (236, index 10)**, Delay, Key. So FXMap value: 0..3 EQ/gain, 4 Reverb,
  5 Echo, 6 Roll, 7 Filter, 8 Crush, 9 Trans, **10 Wah**, 11 Delay, 12 Key (`MixExtendedFunctionality::touchCommon` 0x3f4568
  switch and `CSIAFXParameter::SetFX` 0x3f1464 agree; they use effectType 3=Reverb, 1=Echo, 2=Roll, 0=Filter, 4=Crush, 5=Trans,
  6=Wah, 7=Delay).
- **The parameter control is the circular touch pad ("donut"), not a linear slider.** `CSIAFXParameter::CSIAMotion` (0x3f32f8)
  turns pad x/y motion into an angle with `atanf` and integrates the angle change. The ParamDesc objects `g_pdHiLoCut`,
  `g_pdCrush`, `g_pdPeq` (0xacf360/380/3a0, bases at 0xad0370/388/3a0) all share range -pi..+pi, default 0, transform
  `ParamTransformFilter::paramToValue` (0x3dba98), so Filter, Crush and Wah are all bipolar rotary controls with a centre
  neutral position. The linear slider (`SliderHandler`, `EventOutput::*Slider*`) is the crossfader strip.
- `UserInputs::commitFX` (0x3de440) / `CSIAFXParameter::SetFX` call `AudioCore::setEffectParameter(channel, effectType,
  index, float)` (0x415928, vtable +0x7c of `AudioCore`) with the normalised 0..1 value; that builds a `PeqParamChangeEvent`
  etc. (`MixChannel::createSetEffectParameterEvent` 0x41719c) and pushes it into the `EventBuffer` at `AudioCore+0x38`. The
  events are then consumed inside each process's own `process()` (e.g. `PeqAudioProcess::process` calls
  `setParameterFromEvent` 0x4f5118 for every event). **If `process` is replaced, the events must still be pumped** (section 4).
- Parameter scale: `PeqAudioProcess` ctor stores min -100, max +100, span 200 (floats at `this+0x54/0x58/0x5c`), normalisation
  factor 0.005 (`this+0x60`), clamped normalised value as a double at `this+0x40` (0..1, 0.5 = neutral); the real value is
  `((float(*)(void*,int))vptr[0x2c/4])(this+0x20, 1)` (`getValue(true)`), range -100..+100, 0 = off. This matches the mix
  document values (FILTER -100..50.4, Wah -33.4..49.6). The Wah path tests `getValue(1) == 0.0` for "off".
- The pad readout text for these three FX is `ParamTransformFilter::paramToString` 0x3db5ec = a plain number (2d-1), no unit,
  so a "Vocal" label does not conflict with it.

## 2. Existing Wah / Filter / Crush DSP (what we replace or copy) [V]

- Wah = `PeqAudioProcess` (object 0x90 bytes, vtable 0x877e48): copies in->out, consumes events, and if on, runs
  `processFilter` (0x4f4c90) over a scratch copy in 16-sample slices with coefficients recomputed per slice using double
  `exp/sin/cos/pow` (a swept peaking/band-pass), then adds it back with a gain ramp. It has an off counter (16 blocks of
  `getValue==0` before it stops). State block at `this+0x8c`: coeffs, `BiQuadFilterStereo`, `CriticalSection`, a scratch
  `AudioProcessBuffer` (2 ch).
- Filter = `FilterAudioProcess` (0x4d609c): two cascaded stereo biquads (HP or LP by sign), 20-block off counter, 1-pole
  cross-fade of coefficient changes. Crush (0x4e0278): sample-and-hold decimator (negative side) / high-pass blend.
- `abacus::BiQuadFilterStereo<float>::process(const float* const*, float* const*, uint)` (0x5c7e48) is a hand-written
  transposed-DF2 VFP loop (about 17 VFP ops per stereo sample, scalar vmla, FZ+DN set in FPSCR): reusable by our code
  (`float**` in/out, coefficient struct as in `BiQuadFilterCoeffs`: 5 floats, layout seen in `processFilter`:
  [0]=b0 [1]=b1 [2]=b2 [3]=-a1 [4]=-a2 [U exact sign convention, check in a log build]).

## 3. Audio format in the engine [V]

- Float32, **planar** (`float** channels`), stereo per deck. `AudioProcessBuffer` layout: +0x04 byte "silent" flag
  (1 = no audio, consumers skip and propagate it), +0x08/+0x0c = L/R channel pointers (up to 8), +0x28 float (speed/gain
  tag, copied on `copyFrom`), +0x2c owns-memory flag, +0x30 capacity frames, +0x34 channel count, +0x38 valid frame count
  (`AudioProcessBuffer::AudioProcessBuffer(float**,int,int)` 0x4ae280, `copyFrom` 0x4ae168, `clear` 0x4aee90, `setSize` 0x4af588).
- Block size **512 frames** (`AudioCore+0x20 = 0x200`, ctor 0x414a14; ALSA period size 512, `periods_near 4`,
  `AlsaAudioDevice` ctor 0x48fd18, `configureDevices` 0x4143a8). Block time 11.61 ms.
- Sample rate **44100 Hz** (the device open call passes the double 0x40e58880_00000000 = 44100.0; engine value in
  `AudioProcess::m_sampleRate` 0xad05e0, double, exported; `AudioProcess::getSampleRate()` 0x495c68).
- Device side: ALSA `RW_INTERLEAVED` (access 3), format 6 then fallback 2 (S24_LE then S16_LE if the numeric values follow
  alsa-lib's `snd_pcm_format_t` [U: from memory, check pcm.h]); float -> int via `abacus::interleaveFloatToInt32/Int16`.
  Output can also be 4 channels (split output) which is a different AudioProcessBuffer wrapper; the per-deck FX buffers
  are always 2 channels.
- ABI: `e_flags` 0x04000002 (EABI4, no hard-float flag), GCC 4.1.1, VFP instructions in use, doubles returned in r0/r1 from
  `sin()` => **softfp** (VFP code, integer-register argument passing). Build the library `-march=armv6 -mfpu=vfp
  -mfloat-abi=softfp`, not `-mfloat-abi=soft` as written in `favourite-button.md` section 3.2 (that was written before the
  ABI was known).

## 4. Concrete hook plan

Hook site (data, not code):

| item | value |
|---|---|
| vtable symbol | `_ZTVN7xponaut11audioengine15PeqAudioProcessE` @ 0x877e48 (80 bytes, in the R+X segment, i.e. read-only) |
| `process` entry | vtable +0x1c = **0x877e64**, currently 0x4f524c (`PeqAudioProcess::process`); vptr in objects = 0x877e50, call is `vptr[0x14]` |
| find at runtime | `dlsym(RTLD_DEFAULT, "_ZTVN7xponaut11audioengine15PeqAudioProcessE") + 0x1c`; check it equals `dlsym(..."_ZN7xponaut11audioengine15PeqAudioProcess7processERKNS0_18AudioProcessBufferERS2_RKN4juce10MidiBufferERNS0_11EventBufferE")`; otherwise do nothing |
| patch | `mprotect(page, RW)`, store pointer, `mprotect(page, R+X)`; run in the library constructor before `main` (no thread is running the old pointer) |
| alternative pair | Crush (vtable 0x875938) or Filter (0x8746c8) have the same slot layout; Wah chosen because it is the least musically valuable and has a bipolar pad control |

Replacement signature (plain C, ARM EABI, `this` in r0; no C++ types needed):
`void vocal_process(void *self, const char *in, char *out, const void *midi, void *events)` where `in`/`out` are
`AudioProcessBuffer*` used through the offsets in section 3.

Algorithm of `vocal_process` (all per call, 512 frames):

1. **Event pump through the original.** Set `in[4] = 1` (silent), call the saved original `PeqAudioProcess::process(self, in,
   out, midi, events)`, restore `in[4]`. With a silent input the original does `copyFrom(out,in)` (no audio copied), consumes
   all Peq events via `setParameterFromEvent`, skips the filter (the `param_1[4] != 0` branch), and stores the new value in
   `this+0x80`. [V from the decompile of 0x4f524c; the flag trick itself is untested.] This keeps the stock parameter plumbing
   and is why no `EventBuffer` internals are needed.
2. If `in[4]` is set: `out[4] = 1`, return. Else `AudioProcessBuffer::copyFrom(out, in)` (exported, 0x4ae168; `out[4]` was
   set by step 1, copyFrom clears it and copies the 2 channels and +0x28/+0x38).
3. `v = getValue(true)` (virtual `self+0x20`, vptr `+0x2c`), range -100..+100. If `|v| < 1` for 16+ blocks: bypass (leave
   the copy). Use a per-sample linear ramp of the wet amount across the 512-frame block to avoid zipper noise.
4. Per sample: `m = 0.5*(L+R)`, `s = 0.5*(L-R)`. `c = BP(m)`, a band-pass on the mid signal only, e.g. 2nd-order HP at about
   200-250 Hz + 2nd-order LP at about 6-8 kHz (two `BiQuadFilterStereo`-style sections; the stereo routine can process `m` as
   both "channels", or use a mono biquad `abacus::BiQuadFilter<float>::process` 0x5c7d74).
   - `v > 0` (focus): `k = v/100`; `outL = outR = (1-k)*orig + k*g*c`, plus optionally `+ k2*s*(1-k)`; the side signal is removed
     progressively so at full travel only the centre mid band remains.
   - `v < 0` (reduce): `k = -v/100`; `outL = L - k*c`, `outR = R - k*c` (subtract the centre mid band from both channels; low
     end below the band and all side content stay).
   Filter state persists across blocks; keep it in a small static table keyed by `self` (two live instances; see item 5).
5. **Only act on live deck instances.** Mix-document Wah tools share the vtable. Decide per call: read the parent with the
   virtual/inline `AudioProcess::getParentProcess()` (0x495cb0, field at the AudioProcess object, offset not taken from the
   decompile [U]) and compare its vptr to `_ZTVN9audiocore10MixChannelE + 8`; if it is not a MixChannel, call the original
   unchanged. The offset and the parent semantics (the container may store the owning `StreamedAudioProcess` rather than the
   MixChannel, see `AudioProcessContainer::updateParentProcessOnContainedProcesses` 0x4964f8) must be checked in a log-only
   build. Fallback: record the instance pointers from the MixChannel ctor (hook 0x493538 exit; the two Peq pointers at
   `MixChannel+0x344`), which needs an instruction patch.
6. Thread: runs on the audio thread (`AlsaMultiDeviceManager::run` 0x48f678 calls the device callback from `WriteAvail`). No
   allocation, no locks, no `malloc`/stdio, no exceptions in `vocal_process`.

Relabel: edit string 236 ("Wah") in `bin/resources/fonts/english.lang` (and `japanese.lang`, `korean.lang`) to "Vocal" when
building the `.pfw` (the strings are plain `id "text"` lines, loaded by id; the `.pfw` already carries this directory).

Wrapper: as in `favourite-button.md` 3.2 (`pmplayer.real`, shell wrapper, conditional `LD_PRELOAD`, kill-switch file). The same
library can contain both features.

## 5. CPU headroom

- Direct measurement is not possible offline. The player has a built-in meter: `AlsaMultiDeviceManager::run` times each
  `WriteAvail` (the engine callback) and `dumpStats` (0x48e418) logs through `duff_logf` "Maximum callback time: %fms",
  "Mean callback load: %lf%%", "Mean time between callbacks: %lfms" [V strings]. It prints when the device manager thread
  stops. Where `duff_logf` output goes by default was not traced (`duff_vlogf` 0x6a3bb4, `duff_setLog` 0x6a3afc) [U]. First
  device step: run stock firmware with logging, play two decks with FX on, record these three numbers (budget 11.6 ms per block),
  then repeat with the library.
- Existing work per deck is time-domain: decoder library, `TimeStretch::getSamples` (beat-vector splice + crossfade, 0x5844f0)
  and linear SRC; no FFT symbols in the player [V by symbol search]. The 9-process FX chain is mostly `copyFrom` (4 KB memcpy
  each) when idle.
- Estimate for the time-domain VOCAL: M/S + two mono biquads + mix = roughly 30-60 VFP ops per sample pair => at 44.1 kHz about
  2-3 M ops/s per deck, i.e. a few percent of the 532 MHz ARM11 even at poor VFP11 efficiency (latency-bound scalar code; VFP11
  FMAC latency figures are in ARM DDI 0274 and were not read here [U]). Two decks: under about 5 percent. Comparable to the
  stock Filter (two stereo biquads).
- An STFT version (Avendano-style per-bin panning mask, 1024-point FFT, hop 256, 2 forward + 1-2 inverse transforms per hop)
  would need on the order of 170 hops/s x 3 x ~0.2 Mcycles = ~100 Mcycles/s per deck [H, rough, no FFT code exists in the
  player and one would have to be written in softfp C], i.e. ~20 percent per deck, plus 1024 samples (23 ms) of latency that
  would put the FX deck out of beat alignment with the other deck (the engine has `getLatency()` virtuals, 0x495c98, but no
  evidence was seen that the chain compensates). Not recommended before the time-domain version is judged on the device.

## 6. Adding a new entry instead (why it is not the least invasive route)

Needs all of: (1) a 14th item in `setupValidValues` case 0x6e (vector push of a new language id, plus a string in each .lang);
(2) a new case in `MixExtendedFunctionality::touchCommon` and `CSIAFXParameter::SetFX`; (3) a `UserInputs` value slot and a
`commitFX` bit; (4) a new `g_pd` ParamDesc with colours/strings; (5) a new engine process and an `AudioCore::setEffectParameter`
effectType plus event class; (6) the `FXMap` consumers (`IUi2rt_activateFXMap` uses `1 << index` so indices up to 31 are fine).
Six coordinated instruction patches versus one data word. Reusing the Wah slot is the least invasive.

## 7. What the effect can realistically achieve

- Centre extraction/cancellation assumes the target is amplitude-panned dead centre and the interferers are not. Avendano's
  stereo unmixing paper states the panning index identifies sources only when they sit at different pans and do not overlap
  much in time-frequency, "rarely the case", so errors are expected
  (https://www.ee.columbia.edu/~dpwe/papers/Aven03-unmix.pdf, C. Avendano, WASPAA 2003, found via search 2026-11-11; centre
  = panning coefficient 0.7071 in that paper). Avendano & Jot, "A Frequency-Domain Approach to Multichannel Upmix", JAES 2004
  (title seen in search results, not opened). ADRess (Barry, Lawlor, Coyle, DAFx 2004) is the other classic panning-azimuth
  method [U, from memory, not fetched].
- Audacity's own manual for Vocal Reduction and Isolation says it works on audio common to both channels and that audio from
  other positions is always included, so the result is never a clean separation
  (https://manual.audacityteam.org/man/vocal_reduction_and_isolation.html, 2026-11-11; from 3.5.0 it is no longer bundled and the
  manual recommends the AI plugins). Third-party guides in the same search say centred bass, kick and snare are removed too, and
  that reverb/delay/chorus on a vocal spreads it off-centre; those are secondary sources.
- Applied to typical EDM [I, not measured]: kick, bass, snare/clap and the lead vocal are all usually centred, so plain
  S-only "karaoke" would hollow out the whole mix. The band-limited version (subtract only about 250 Hz-7 kHz of the mid signal)
  keeps kick/bass fundamentals and wide synths and removes the centred vocal body and the centred snare/clap/hat energy in that
  band. Focus mode mostly yields "centre mid band, mono" (vocal + snare + mono stabs), not a clean vocal. Wide/doubled/ping-pong
  vocals, vocal reverb tails and hard-panned chops will stay in the side signal. Expect: usable as a DJ-style "pull the
  vocal forward / drop it out for a moment" effect on centred vocals, with audible artefacts (comb-like mono collapse) on
  others. A source-separation network (Spleeter, Demucs) would do far better but is not feasible on an ARM11 at 532 MHz in real
  time and would have to be run offline in the Editor, which is a separate feature.
- Mono-compatibility side effect: focus mode outputs near-mono, so a deck in focus loses width by design.

## 8. Risks

- Shared vtable: mix-document Wah tools and any already-saved mixes using Wah will sound different (instance check in 4.5
  avoids this for non-MixChannel owners if it works).
- The silent-flag event pump is the weakest assumption; if the original `process` touches `param_1` beyond `[4]` or the
  `copyFrom` assumptions differ, the parameter may not update. Mitigation: log-only build that prints `v` per block to a file.
- Cache/permission: vtable is in the R+X segment; `mprotect` on that page must succeed. A data store needs no icache flush;
  the library code itself is loaded normally.
- Crash = player abort (`pwrApplication` reaction not analysed, see `favourite-button.md` section 4). Mitigations: byte/pointer
  checks before patching, kill-switch file, build log-only first, never call into player code that allocates from `vocal_process`.
- Denormals: the player sets FZ and DN in FPSCR inside `abacus` routines and restores them; our code runs with the thread's
  FPSCR as left by the caller [U]; add a tiny DC offset or set FZ locally.
- Latency: time-domain version has under 1 ms (IIR). Do not use linear-phase FIR.
- Firmware specific: pointer and offsets are for 180096069 only; the pointer comparison in section 4 makes it inert elsewhere.

## 9. Plan

1. Log-only build of `libvocalfx.so`: patch the vtable, call the original, print `v`, `in[4]`, parent vptr check, frames per
   call, and elapsed time per call (`clock_gettime`) to a file on `/mnt/public/.Pacemaker/`. Also capture the stock
   "Mean callback load" numbers.
2. Confirm item 5 (instance identification) and the event pump from that log.
3. Enable the DSP with the kill-switch available; check output on the headphones/line out with a test mix (centre vocal,
   stereo synth, kick/bass); tune corner frequencies, makeup gain and the ramp.
4. Relabel the .lang strings in the image build; document removal (stock `.pfw`, or kill-switch file).
5. Only if the time-domain result is judged too weak: prototype an offline STFT centre mask in the Editor/Rust first
   (`app/` is being edited by another workflow, so not touched here), measure on real tracks, then decide on-device.

## 10. Status: built and tested off-device (11 Nov 2026)

Code, build, tests and install text are in `device/vocalfx/` (README there). Changes to the plan above, all from a
second reading of the decompile (`work/vfx/dc1/004f524c.c`, `004afab8.c`, `004ae168.c`) and from measurement:

- **Peq is the second FX in the chain, so it is always called with `in == out`** (sorter ranks checked, section 1.1) (`StreamedAudioProcess::process`
  passes `(param_2, param_2)` from the second process on). `copyFrom(x, x)` is a no-op, and the silent-flag event pump of
  section 4 is safe: set `in[4]`, call the original, restore `in[4]`. For `in != out` (not the live chain) the library
  calls the original untouched, because with a silent input `copyFrom` does not copy audio into `out` and does not set
  its frame count.
- The stock process also tests a byte at `this+4` (skip when set) and stores the value at `this+0x80`, which is the
  value the library reads (no virtual call, no extra lock).
- **"Subtract the band-passed centre" does not cut**: any band-pass has phase, measured -2.4 dB on a voice and -7 dB at
  1 kHz (HP2 150 Hz + LP4 4 kHz). The DSP is therefore a phase-coherent three-band split (LR4 crossovers at 150 Hz and
  4 kHz, all-pass on the low band and on the side signal) with per-band gains: -48 dB at 1 kHz for a full cut, no notches
  at partial settings. 11 biquads per frame instead of the 3 estimated in section 5; estimate 1.5 - 2.5 % of the block
  time per deck (unmeasured on the device, the library logs it).
- `PeqAudioProcess` objects: the vtable symbol, `process`, both destructors and `m_sampleRate` are exported by name in
  the player's `.dynsym` (checked in `work/vfx/allsyms.txt`), and the vtable words at 0x877e48 were re-read from the
  ELF: `[2] = 0x4f5730`, `[3] = 0x4f58d8`, `[7] (+0x1c) = 0x4f524c`. The library checks all of them before it writes.
- The preload library must be linked against the device's own libs: the compiler's default `_TIME_BITS=64` makes
  references glibc 2.4 does not have; `build.sh` removes it and audits every undefined symbol version (all `GLIBC_2.4`).
- Review round (11 Nov 2026): hook page is mapped read/write only for the store (the vtable is in `.rodata`, no code on the
  page); audio path has no syscall unless `revival-vocal-timing` exists; library carries `.ARM.exidx`; log traffic after
  five minutes only on milestone changes; engage with hysteresis (on at 1.0, off below 0.5); input sanitising; knee 0.97.
- Not yet done: any run on the device. The first boot is observer-only (flag file absent) so the hook, the 512-frame
  call size and the stock Wah cost can be read from `revival-vocal.log` before the DSP is enabled.
