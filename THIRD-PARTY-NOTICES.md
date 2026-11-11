# Third-party notices

Pacemaker Revival is GPL-3.0-or-later (see `LICENSE`). This file lists what the stem-separation feature
(`crates/pm-stems`, the Stems view) uses that is not ours. Licences below were read from the crates'
own `Cargo.toml` / bundled `LICENSE` files in this build, or from the upstream pages named in
`app/src-tauri/src/commands/stems/manifest.rs`. Dependencies of the rest of the app are not listed here yet.

## Linked into the app

| Component | Version | Licence | Used for |
| --- | --- | --- | --- |
| LAME (bundled by `mp3lame-sys`, wrapper `mp3lame-encoder`) | LAME 4.0, mp3lame-sys 0.2.0, mp3lame-encoder 0.3.0 | LGPL-3.0 | Writing MP3 stem files |
| Vorbis encoder (`vorbis_rs`, built on the aoTuV/libvorbis sources) | 0.5.6 | BSD-3-Clause | Writing Ogg Vorbis stem files |
| `rustfft` | 6.4.1 | MIT OR Apache-2.0 | STFT / inverse STFT |
| `ort` (Rust bindings for ONNX Runtime, `load-dynamic`) | 2.0.0-rc.15 | MIT OR Apache-2.0 | Running the model; the ONNX Runtime library itself is loaded at run time, not linked |

LAME is statically linked. LGPL-3.0 asks that a recipient can replace the LAME code and relink: the
complete source of this app is published under GPL-3.0-or-later, so the whole program can be rebuilt with
another LAME. The LAME licence text is in the `mp3lame-sys` crate (`lame-4.0/COPYING`).

## Downloaded by the user, never bundled

These are fetched only after the user accepts the notice shown in the Stems view, from the address in the
manifest, and checked against a pinned SHA-256.

| Component | Licence | Note |
| --- | --- | --- |
| HTDemucs 4-stem model, ONNX Runtime format (third-party conversion on Hugging Face by `gentij`) | Demucs code: MIT. Trained weights: **licence not clear** (the weights' author says so, because the MUSDB18 training data includes research-only material). The conversion has no licence of its own | Not hosted or redistributed by this project. Personal music preparation at the user's own risk; do not assume commercial use is allowed |
| ONNX Runtime (Microsoft), CPU build | MIT (the release zip carries its own third-party notices) | Only the libraries the app needs are unpacked |

See `notes/vocal-isolation-pc.md` for the research behind these choices and what is unverified.
