# Vocal / instrumental stems on the PC (Question A)

Research date 2026-11-11. Nothing in this note was run or benchmarked by me; every number is
quoted from a source. "Unverified" marks anything I could not confirm from a primary source.
Test machine for later measurement: Ryzen 9 3900X (12C/24T), RTX 5080 (queried with CIM today).

## Verdict

Use **HTDemucs (`htdemucs`, 4 stems) run through the `ort` crate**, with STFT/iSTFT and the
overlap-add done in Rust. Instrumental = drums + bass + other (or mix minus vocals), acapella =
vocals. It is the best vocals quality that has a working Rust+ONNX route today and an 84 MB
weights file. The one real problem is the **weights licence, which the author says is unclear**.
So: do not bundle, do not host a copy. Download on first use, on explicit user consent, from the
upstream host, behind a "model provider" trait so a cleanly licensed model can be swapped in.

Fallback that is clean on paper: **Open-Unmix `umxhq` vocals** (weights MIT per its Zenodo
record), about 2 dB worse on vocals SDR and with audibly more bleed. Higher-quality option for
later: BS-/Mel-RoFormer or MDX23C checkpoints from the UVR community (about 10-11 dB vocals SDR),
but weight licences are unstated and I found no verified Rust route.

## Comparison

SDR figures are NOT like for like (different test subsets, mean vs median, extra training data).
Read them as ranking hints only.

| Model | Code licence | Weights licence | Vocals SDR (source) | Size | Rust route |
|---|---|---|---|---|---|
| HTDemucs (`htdemucs`) | MIT (archived upstream; fork adefossez/demucs) | **"not clear"** per the author | overall 9.00 dB on MUSDB-HQ (README, fine-tuned v4); vocals 8.24 on ZFTurbo's "Multisong" set | 84,025,440 B safetensors | `ort` + ORT/ONNX export; `burn` port; no candle/tract port found |
| HTDemucs FT (`htdemucs_ft`) | same | same | vocals-specialist 8.38 (ZFTurbo table) | 333 MB (demucs-rs README) | same, 4x slower per Demucs README |
| Open-Unmix `umxhq` | MIT | **MIT** (Zenodo 3370489 metadata), trained on MUSDB18-HQ | median 6.25 (README) | vocals.pth 35,637,796 B | ONNX export exists on HF (`edgetools/umx-hq`, vocals.onnx, tag MIT); STFT outside; no Rust example found |
| Open-Unmix `umxl` | MIT | **CC BY-NC-SA 4.0** | median 7.21 | 113 MB per target | avoid |
| Spleeter 2/4 stems | MIT | not stated in README (only code) | 6.55 mask / 6.86 MWF (4 stems, wiki) | 2stems.tar.gz 73 MB | TF models; no Rust route found |
| MDX-Net / MDX23C (UVR) | UVR GUI MIT; kuielab/mdx-net MIT | **unstated**; `TRvlvr/model_repo` has no licence file (GitHub API: null) | MDX23C vocals 10.17 (ZFTurbo table); KUIELAB-MDX-Net overall 7.5 (Demucs README) | tens of MB (size unverified) | native ONNX; `ort` works; a CoreML/ONNX walkthrough exists for `mdx23c_instvoc_hq2` |
| BS-RoFormer / Mel-RoFormer | lucidrains MIT, ZFTurbo training repo MIT | viperx/UVR ckpts unstated; Kimberley ckpt HF tag `mit` (tag only) | paper, MUSDB18HQ, no extra data: BS 10.78 (L6)/11.02 (L9), Mel 11.21/11.60; ZFTurbo viperx BS 10.87 | not measured | no verified ONNX or Rust route; heavy (see pitfalls) |

## Sources for the table

- Demucs README (archived): https://github.com/facebookresearch/demucs , fetched 2026-11-11: MIT code, archived 1 Jan 2025, 9.00 dB, "roughly 1.5 times the duration" on CPU, `htdemucs_ft` 4x time, segment max 7.8 s.
- Fork README, licence section (raw): https://raw.githubusercontent.com/adefossez/demucs/main/README.md , fetched 2026-11-11. Quote: "The license of the weights is not clear: MusDB contains some data that were only usable for research purpose." It also says the 6-source `htdemucs` was trained on Meta internal data, "likely" CC-NC-BY. Author declines to clarify.
- Official HF weights: https://huggingface.co/adefossez/HTDemucs , API 2026-11-11: no licence tag now, `955717e8.safetensors` 84,025,440 B. Discussion #1 (https://huggingface.co/adefossez/HTDemucs/discussions/1) asks whether an earlier `license: mit` tag was intended; no maintainer reply on the page.
- Open-Unmix README: https://raw.githubusercontent.com/sigsep/open-unmix-pytorch/master/README.md : repo MIT; `umxl` "only licensed for non-commercial use (CC BY-NC-SA 4.0)"; SDR table 6.32 / 6.25 / 7.21.
- Zenodo UMX-HQ record: https://zenodo.org/api/records/3370489 (licence id `mit-license`, files above); UMX-L: https://zenodo.org/api/records/5069601 (`cc-by-nc-sa-4.0`).
- Spleeter: https://github.com/deezer/spleeter (README: "The code of Spleeter is MIT-licensed"; GitHub licence MIT); wiki performance https://raw.githubusercontent.com/wiki/deezer/spleeter/Separation-Performances.md ; release v1.4.0 assets sizes via GitHub API 2026-11-11; Demucs README table says Spleeter trained on 25k songs, overall SDR 5.9.
- ZFTurbo pretrained table: https://raw.githubusercontent.com/ZFTurbo/Music-Source-Separation-Training/main/docs/pretrained_models.md , fetched 2026-11-11 (repo MIT). Which test set the "SDR vocals" column uses is not stated in the file I read: unverified.
- Mel-RoFormer paper numbers: https://arxiv.org/abs/2310.01809 (via search snippet 2026-11-11; HDemucs vocals 8.04 in the same table).
- UVR README: https://raw.githubusercontent.com/Anjok07/ultimatevocalremovergui/master/README.md : code MIT; asks third-party developers using its models to "honor the MIT license by providing credit". That is a request about the models but is not a licence file for each checkpoint.
- `TRvlvr/model_repo` licence null: https://api.github.com/repos/TRvlvr/model_repo , 2026-11-11.
- KimberleyJensen Mel-RoFormer HF card tag `license:mit`: https://huggingface.co/api/models/KimberleyJSN/melbandroformer , 2026-11-11 (tag only, training data not stated there).
- Patent note (unverified, flag for a lawyer if shipping RoFormer): a USPTO document 12542148 titled "Systems and methods for audio source separation using band-split transformers" exists (found in search 2026-11-11, PDF unreadable, assignee and claims NOT confirmed).

## Licence analysis (not legal advice)

GPL-3 applies to the app's code, not to data files it downloads. The questions are (a) may we
redistribute the weights and (b) may users run them.

- HTDemucs: code is MIT, weights unclear by the author's own words, driven by MUSDB18's research-only
  portion and Meta's internal data. Third-party model cards (kickvn, StemSplit, etc.) re-label the
  weights MIT; that is not the rights holder's statement. Treat as "ship nothing, link out".
  If commercial use is ever planned this needs a decision from Jared, not a default.
- Download-on-demand from the original host (`dl.fbaipublicfiles.com/demucs/...`, used by
  ZFTurbo's table for `htdemucs`: 955717e8-8726e21a.th) or the official HF repo means we do not
  redistribute. A conversion to ORT is a derivative; the community ORT files
  (`gentij/htdemucs-ort`, 209,884,896 B, HF tags none, no licence file) inherit the same
  uncertainty and add a trust issue (pickle-free but unaudited third-party binary). Prefer
  exporting ONNX ourselves from the official safetensors in CI and hosting nothing, or have the
  app convert on first run (needs Python, so not great), or verify the SHA-256 of a pinned
  third-party ORT file and say so in the UI.
- Open-Unmix `umxhq`: author states MIT on Zenodo, so bundling is permissible per that record. Residual
  doubt: trained on MUSDB18-HQ, which carries non-commercial terms; the Demucs author raises exactly
  that doubt. Still the most defensible choice for a bundled model.
- Bundling anything CC BY-NC-SA with a GPL-3 binary is a licence-compatibility problem; keep `umxl`
  and any NC checkpoint out of the tree.
- Show the model's licence status in the UI and ship THIRD-PARTY-NOTICES with attribution (UVR asks
  for credit; Demucs/MIT needs the notice).

## Speed (CPU, 5-minute track) - measurements found

No measurement on a known desktop CPU for the Rust path exists in what I found. Treat as a range.

- Mixxx GSoC 2025 (https://mixxx.org/news/2025-10-27-gsoc2025-demucs-to-onnx-dhunstack/ , 2026-11-11): 50 MUSDB tracks, 3.46 h audio. CPU: PyTorch 25.89 s per minute of input, C++ ONNX Runtime 21.24 s per minute. For 5 min: about 129 s and 106 s. Hardware not given. GPU Python ORT 1.86 s/min (they report ORT slower than PyTorch on GPU, 8.35%).
- Demucs README: "roughly equal to 1.5 times the duration" on CPU, so about 7.5 min for 5 min. Older and conservative.
- StemSplit vendor post (https://dev.to/stevecase430/best-free-ai-stem-splitters-in-2026-a-developers-benchmark-2dam): i7-12700, htdemucs about 3.8 min, htdemucs_ft about 4.5 min, UVR MDX-Net/VR about 2 min; track lengths not stated, author is a vendor. Low trust.
- stem-splitter-core says a runtime change gave "up to roughly 40% faster" with no data.
- Quality of the ONNX path equals PyTorch within noise (Mixxx table: vocals SI-SDR 7.84 vs 7.83).

Expect roughly 1.5-4 minutes per 5-minute track on a modern 8-12 core CPU (estimate from the
above, not measured). Do not promise a number in the UI; show progress and an ETA from the
first segment. DirectML on a GPU should be much faster but is an unmeasured claim here.
Open-Unmix is a BiLSTM on magnitude spectrograms and should be several times faster than
HTDemucs, but I found no cited CPU number: unverified. RoFormers are the slowest class on CPU;
no cited CPU figure found.

## Inference from Rust

- `ort` 2.0.0-rc.15 (crates.io, 2026-10-19, MIT OR Apache-2.0, ORT v1.31.0 per its README badge). Still a release candidate: pin the exact version. `load-dynamic` + `ORT_DYLIB_PATH` is documented as the recommended linking mode (https://github.com/pykeio/ort/blob/main/docs/content/setup/linking.mdx). Prebuilt-binary platform matrix is a JS widget I could not read, and ort.pyke.io returned 403: Windows x64 prebuilt availability is plausible but UNVERIFIED by me. DirectML/CUDA EPs exist as cargo features and need helper DLLs next to the exe (`copy-dylibs`, doc text in execution-providers.mdx). Do not rely on System32's old onnxruntime.dll.
- Working example with the same shape of problem: `stem-splitter-core` 1.2.0 (https://docs.rs/crate/stem-splitter-core/1.2.0 , MIT OR Apache-2.0, 3.1k downloads): htdemucs ORT, window 343,980 samples (about 7.8 s at 44.1 kHz), hop 171,990 (50% overlap), writes 16-bit 44.1 kHz stereo WAV, downloads from HF on first use and caches with SHA-256 check, EP order on Windows CUDA, DirectML, oneDNN, XNNPACK, CPU. Its docs list no Windows binary, so test the library on Windows before depending on it; copying its structure is lower risk than depending on it.
- Export recipe: https://github.com/sevagh/demucs.onnx (README, raw 2026-11-11) moves STFT/iSTFT out of the graph; core model takes `mix` (time) and `x` (complex-as-channels spectrogram, nfft 4096, hop 1024), returns `(x, xt)`; Rust must implement spec, magnitude, mask/CaC, ispec, padding. The ORT manifest in `gentij/htdemucs-ort` confirms shapes: inputs `input` [1,2,343980], `x` [1,4,2048,336], output [1,4,2,343980], opset 18.
- Alternative: Mixxx's fully self-contained ONNX (STFT rewritten as Conv1d) removes the Rust DSP, but I found no published weights file or merged export script yet: unverified whether the upstream PRs landed.
- `burn`: `demucs-rs` (https://github.com/nikhilunni/demucs-rs , Apache-2.0, pushed 2026-10-20, 162 stars) is a pure-Rust htdemucs/htdemucs_6s/htdemucs_ft on Burn; downloads weights from HF on first use; README lists GPU backends only (Metal, Vulkan, WebGPU), so no confirmed CPU path: unverified. Interesting if we later want to drop the ORT DLL.
- `candle`, `tract`: no Demucs or Open-Unmix port found in my searches. Not recommended.
- DSP crates: `rustfft` 6.4.1 / `realfft` 3.5.1 for STFT; `rubato` 5.0.1 for resampling; `symphonia` 0.6.1 (MPL-2.0) for decode. MPL-2.0 is GPL-3 compatible per MPL 2.0's secondary-license clause (my reading; confirm before release).

## Pitfalls

- **Sample rate**: HTDemucs is 44.1 kHz stereo. Resample 48k/32k/22.05k sources in, and resample (or keep 44.1k) out. Output the same length in samples as the input after trimming; stems must be sample-aligned with the source.
- **Mono sources**: duplicate to stereo.
- **Chunking**: transformer limit 7.8 s per segment; use 50% overlap with a triangular or Hann crossfade window (as in stem-splitter-core), normalise the mix before inference (the KICK export notes this) and undo it afterwards. Zero-pad the tail. Keep the overlap-add in f32, accumulate weights, divide.
- **Memory**: whole-track f32 stereo at 44.1 kHz is about 106 MB per 5 min; 4 stems output is about 423 MB. Stream segments to disk if tracks are long (DJ mixes can be an hour).
- **Level and clipping**: sum of stems can exceed full scale; apply the same gain to both outputs or limit, and write the instrumental as the true sum of non-vocal stems, or mix minus vocals (the latter keeps phase-consistency with the original but leaks any vocal estimation error, both are legitimate; offer one default).
- **Quality limits to tell the user**: reverb tails and backing vocals leak; bass-heavy tracks smear. Higher-SDR RoFormers sound cleaner but cost more CPU and have no Rust route.
- **Priming / grid alignment** (device-specific, see below): the biggest risk to "same beat grid".
- **RoFormer on CPU**: attention over long band sequences is slow; one search found only advice to cut overlap; no CPU timing is cited. Gate behind an "HQ (slow)" option if ever added.
- **ORT format**: `.ort` files are tied to the ORT build's operator set; if using the minimal-ORT-build trick (sevagh), the exact `.ort` must match. With the stock ORT DLL, use plain `.onnx`.

## Output format and the device

Facts from this repo: the device keeps tracks as `/pmdb_tracks/A/B/<hash>.mp3` with `A/B` taken
from a file-size hash, analysis in a sibling `.str` (`<name>.str`, FINDINGS.md section 4);
`.str` positions are frame positions, `beatStartOffset` and `normalizeFactor` included. The
Editor's importer lists decoders for mp3, aac/mpeg4, flac, ogg/vorbis, wav/pcm, aiff
(notes/editor-feature-inventory.md, row "Tag read at import"). The device image carries "codec
plugins" (FINDINGS.md line 281). Whether the device plays each of those: **unverified**; the
Editor list is computer-side. FINDINGS.md contains no explicit device codec list, so the first
task is to read the plugin list from the firmware image.

Recommendation:
1. Default output **MP3 320 kbps or 256 kbps CBR**, because every observed device track is mp3 and that is the one format proven on-device. Tag it (artist, title, "(Instrumental)" / "(Acapella)") so the library shows it.
2. Alignment: the encoder adds a priming delay (LAME writes about 1105 samples of delay plus padding, gapless info in the LAME tag; value from general knowledge, unverified here). If the device decoder ignores that tag, every `.str` position is off by about 25 ms. Probe first: encode a click-track stem, load it on the device, compare the cue/beat markers. If there is a shift, either prepend negative-offset compensation to `beatStartOffset` in the copied `.str` (semantics of that field: unverified) or switch to a format with no priming issue (FLAC or WAV, if the device plays them; Ogg Vorbis also has a decoder delay story, unverified).
3. Copy the source `.str` unchanged as the stem's sidecar only after the alignment probe passes; otherwise re-run beat detection on the stem (acapellas have weak beats, so prefer copying and offsetting). `normalizeFactor` should be recomputed for the stems: an acapella is quieter than the mix, and the sidecar gain affects loudness at the deck. Meaning of the field is from FINDINGS.md; the loudness rule is my inference.
4. Encoders: LAME via `mp3lame-encoder` 0.3.0 is **LGPL-3.0** (crates.io 2026-11-03): fine to link into a GPL-3 app, mind the notices; `flacenc` 0.5.1 Apache-2.0; `vorbis_rs` 0.5.6 BSD-3-Clause; `hound` 3.5.1 Apache-2.0 for WAV.

## Integration plan

1. **Model provider trait** `StemModel { id, license_status, sample_rate, run(segment)->stems }` with two entries: `htdemucs` (default, download) and `umxhq-vocals` (small, MIT, optional second choice). Ship the ORT DLL via `load-dynamic` or the crate's download strategy; test a clean Windows VM.
2. **Weights source**: pinned manifest in the app (name, URL, SHA-256, size, licence text, licence status). First use shows a consent dialog with the licence status ("upstream author states the licence is unclear") and a link; download to `%LOCALAPPDATA%\<app>\models\` with resumable download and hash check, as stem-splitter-core does. Do not commit or bundle weights to the repo (the repo is public GPL-3).
3. **Cache**: key = SHA-256 of decoded PCM plus model id plus model hash plus pipeline version. Store the 4 float stems once as FLAC in a cache dir so changing the output mode or bitrate does not re-run inference.
4. **Job queue**: one inference at a time (ORT already uses all cores via intra-op threads; set `intra_op_threads` to cores minus 1 to keep the UI alive); cancel token checked per segment; progress per segment; run in a Tauri async task, emit events; persist a partial-result marker so a crash does not leave a half stem on the device.
5. **Pipeline**: decode (symphonia, trim encoder delay) -> to 44.1k stereo f32 -> normalise -> segment 343,980 with hop 171,990 -> STFT (4096/1024, CaC) -> ORT -> mask/iSTFT -> windowed overlap-add -> denormalise -> sum non-vocal stems -> encode -> tag -> write `.str` sidecar -> add to library as a new track (new hash path) with a "derived from" link to the original row. Writing to the device follows the existing add path (`pm add`, FINDINGS.md); never write to a mounted device from the worker thread without the existing safety checks.
6. **Acceptance test**: three tracks of different genres; null test (vocals + instrumental vs original, RMS error); beat-marker comparison on the device or in the Editor; run time per 5 min recorded on the 3900X and one laptop and written back into this note.

## Open items for Jared

- Decide the licence posture for HTDemucs (download-only versus not offering it). Everything else is mechanical.
- Confirm which codecs the device really plays (read the codec plugins from the firmware image) and run the priming probe.
- Run the first real CPU benchmark (htdemucs ORT, one 5-minute track) and replace the range above.
- Verify a Windows x64 prebuilt ONNX Runtime exists for the chosen `ort` rc, or plan on shipping `onnxruntime.dll` ourselves.
