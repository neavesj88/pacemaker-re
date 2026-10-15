# pacemaker-re

Reverse engineering toward an open-source, modern replacement for the **Tonium Pacemaker
Editor** (last release 2.0.2.14170, 2010). The goal is a cross-platform Editor (Windows, Linux,
macOS) for owners of the Pacemaker pocket DJ device, with library management and track analysis
on par with current DJ software.

The original Editor is closed source and has been unmaintained since Tonium went bankrupt in
2011. This repository documents the device's formats and protocols so that a new Editor can
interoperate with existing devices and libraries.

## Status

The device-side contract is mapped and mostly verified against the shipped binaries. See
**[notes/FINDINGS.md](notes/FINDINGS.md)**.

| Area | State |
|---|---|
| Transport | USB mass storage (FAT32). No custom protocol. |
| Library DB `.Pacemaker/music.db` | Full SQLite schema recovered ([schema](notes/schema-14142.sql)), `user_version` 677, identical across all firmwares |
| Track files and `tracks.location` | Editor naming scheme recovered (`Music/A/B/<hash>.<ext>`, `/pmdb_tracks/…`) |
| Beat grid / gain analysis (`.str`) | Format recovered from both the device reader and the Editor writer |
| Mix documents / `.pmk` | boost XML; obfuscation (zlib + XOR) reversed |
| Firmware `.pfw` | Container fully specified; all 7 known images rebuild byte-identical |
| Restore / unbrick | Stock `dfu-util` to `5e1d:a7a4`; DFU entry = hold Play+Cue+Forward+Back at power-on |

Every codec is byte-exact against files written by the original Editor. What remains are a few
device-side details, listed at the end of
FINDINGS.

## The new Editor (Rust, in progress)

A cross-platform replacement Editor is being built in Rust, with a Tauri UI to follow.

| Crate | Purpose |
|---|---|
| `crates/pm-formats` | Device formats: `music.db` create/check, `.str` beat grids, mix blobs and `.pmk`, track naming, `.pfw` firmware. Tested byte-exact against original-Editor output and all 7 firmware images (those inputs are local-only, so those tests skip without them). |
| `crates/pm-analysis` | Our own tempo and beat-grid analyser: spectral-flux onsets, autocorrelation tempo candidates, Ellis DP beat tracking, a whole-track locked-grid search with BPM snapping, and a configurable BPM range. Beat placement is within +-0.5 ms on synthetic kicks. |
| `crates/pm-bench` | Scores the analyser on a folder of tracks against BPM tags and the original Editor's beat grids. |

    cargo test --workspace --release
    cargo run --release -p pm-bench -- <tracks dir> --editor-str "%APPDATA%/Tonium/Pacemaker/str"

## Reverse-engineering tools

| Script | Purpose |
|---|---|
| `scripts/fetch.sh` | Download the original firmware, Editor and GPL drops into `vendor/` (pinned SHA-256) |
| `scripts/unpack-firmware.sh` | Unpack `.pfw` images (ext2) into `work/fw-<ver>/` |
| `scripts/pfw-trailer.py` | Parse, verify and rebuild `.pfw` trailers |
| `scripts/pmdb.py` | Create an empty device-compatible `music.db`, or check one against the device schema |
| `scripts/strfile.py` | Read/write `.str` beat-grid sidecars |
| `scripts/pmname.py` | Device file naming and `tracks.location` |
| `scripts/pmk.py` | Decode/encode obfuscated `.pmk` mixes |
| `scripts/gen-schema.py` | Regenerate the schema from a firmware's `pmplayer` |
| `scripts/isx.py`, `carve-msi.py`, `thin-macho.py` | Unpack the Editor installers statically |
| `scripts/ghidra-decompile.sh` | Headless Ghidra decompilation by name, address or callers |

Requirements: Python 3, 7-Zip, and Ghidra 12.1.x + JDK 21 for decompilation.
[rea](https://github.com/morluto/rea) is used for the .NET/PE lanes.

## Legal

This is independent interoperability work. Nothing here is affiliated with Tonium or Pacemaker
Music AB. Their firmware, Editor and restore tools are copyrighted and are **not** in this
repository: `scripts/fetch.sh` downloads them from the community mirrors, and `vendor/` and
`work/` are gitignored. The GPL components (Linux BSP, RedBoot, build scripts) are referenced
from Tonium's own SourceForge release.

## Licence

GPL-3.0-or-later (see `LICENSE`). Chosen so the project can use established GPL audio-analysis
code (for example Mixxx's analysers, aubio, libKeyFinder).
