//! What the app may download for stem separation, pinned: URL, SHA-256, size and the licence
//! notice shown before anything is fetched. Nothing is downloaded from anywhere else, and
//! nothing without the user's explicit consent to the notice of that version (see `download`).
//!
//! The checksums are the ones the hosts publish as metadata (Hugging Face's LFS record, GitHub's
//! release-asset digest), read WITHOUT downloading the files. They pin what the host serves
//! today; they are not an independent audit of the content (see `notes/vocal-isolation-pc.md`).

use serde::Serialize;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Model,
    Runtime,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LicenceStatus {
    /// The licence of the thing is stated and permits the use here.
    Clear,
    /// The rights holder says the licence is not clear: the UI shows a warning, not a badge.
    Unclear,
}

#[derive(Serialize, Clone, Debug)]
pub struct Component {
    pub id: &'static str,
    pub kind: Kind,
    pub name: &'static str,
    pub summary: &'static str,
    /// Exactly where the file comes from.
    pub url: &'static str,
    /// A page a person can read about it.
    pub page_url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    /// File name it is stored under (models) or the zip it is unpacked from (runtimes).
    pub file: &'static str,
    /// For a zip: the folder inside it whose `.dll` files are kept.
    #[serde(skip)]
    pub unzip_dir: Option<&'static str>,
    pub licence_name: &'static str,
    pub licence_status: LicenceStatus,
    pub licence_text: &'static str,
    /// Bump when the notice changes: consent given to an older notice does not count.
    pub notice_version: u32,
    /// Whether this download exists for the platform the app runs on.
    pub available_here: bool,
}

const HTDEMUCS_NOTICE: &str = "\
HTDemucs separates a song into drums, bass, other and vocals; this app keeps the vocals and the sum of the rest.\n\n\
Licence status: UNCLEAR. The Demucs code is MIT-licensed. The author of the trained weights says their licence is not clear, because the MUSDB18 training set includes material released for research use only. \
This file is a third-party conversion of those weights (Hugging Face user gentij) and has no licence of its own.\n\n\
This project does not host or bundle it. If you agree, the app downloads it from the address above to your computer and checks its SHA-256 before using it. \
Use it for your own music preparation at your own risk, and do not assume commercial use is allowed. \
Quality limits: reverb tails and backing vocals leak between the two results, and bass-heavy music smears.";

const ORT_NOTICE: &str = "\
ONNX Runtime is Microsoft's open-source engine for running the model. Licence: MIT (the zip also carries third-party notices).\n\n\
This is the CPU build, version 1.31.1, from Microsoft's GitHub release page. It is large (about 83 MB) because it includes debugging symbols; only the two libraries the app needs are unpacked. \
GPU use needs a build with DirectML or CUDA: choose its onnxruntime library under Settings instead.";

pub fn all() -> Vec<Component> {
    vec![
        Component {
            id: "htdemucs-ort",
            kind: Kind::Model,
            name: "HTDemucs 4-stem model (ONNX Runtime format)",
            summary: "Vocals and instrumental from one model. About 210 MB.",
            url: "https://huggingface.co/gentij/htdemucs-ort/resolve/main/htdemucs.ort",
            page_url: "https://huggingface.co/gentij/htdemucs-ort",
            sha256: "09dc165512d8ef7480bcb2cacea9dda82d571f8dbf421d8c44a2ca5568bec729",
            size_bytes: 209_884_896,
            file: "htdemucs.ort",
            unzip_dir: None,
            licence_name: "Weights licence not clear (code: MIT)",
            licence_status: LicenceStatus::Unclear,
            licence_text: HTDEMUCS_NOTICE,
            notice_version: 1,
            available_here: true,
        },
        Component {
            id: "onnxruntime-win-x64",
            kind: Kind::Runtime,
            name: "ONNX Runtime 1.31.1 (Windows x64, CPU)",
            summary: "The engine that runs the model. About 83 MB to download.",
            url: "https://github.com/microsoft/onnxruntime/releases/download/v1.31.1/onnxruntime-win-x64-1.31.1.zip",
            page_url: "https://github.com/microsoft/onnxruntime/releases/tag/v1.31.1",
            sha256: "5869e26bb25314375f2d5ef1877e3ea06b1543d6c710e6e4a1ec2f7177ab4b2c",
            size_bytes: 82_929_944,
            file: "onnxruntime-win-x64-1.31.1.zip",
            unzip_dir: Some("lib/"),
            licence_name: "MIT",
            licence_status: LicenceStatus::Clear,
            licence_text: ORT_NOTICE,
            notice_version: 1,
            available_here: cfg!(all(windows, target_arch = "x86_64")),
        },
    ]
}

pub fn find(id: &str) -> Option<Component> {
    all().into_iter().find(|c| c.id == id)
}

/// The file name of the onnxruntime library on this platform.
pub const RUNTIME_LIB: &str = if cfg!(windows) {
    "onnxruntime.dll"
} else if cfg!(target_os = "macos") {
    "libonnxruntime.dylib"
} else {
    "libonnxruntime.so"
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_entries_are_pinned_and_https() {
        let all = all();
        assert_eq!(all.iter().filter(|c| c.kind == Kind::Model).count(), 1);
        for c in &all {
            assert!(c.url.starts_with("https://"), "{}: only https sources", c.id);
            assert!(c.page_url.starts_with("https://"));
            assert_eq!(c.sha256.len(), 64, "{}", c.id);
            assert!(c.sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
            assert!(c.size_bytes > 1_000_000);
            assert!(c.notice_version >= 1);
            assert!(!c.licence_text.is_empty());
            assert!(c.file.chars().all(|ch| ch.is_ascii_alphanumeric() || ".-_".contains(ch)), "{}: file name is a plain name", c.id);
        }
        let ids: std::collections::HashSet<_> = all.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), all.len(), "ids are unique");
    }

    #[test]
    fn the_unclear_licence_is_labelled_unclear() {
        let m = find("htdemucs-ort").unwrap();
        assert_eq!(m.licence_status, LicenceStatus::Unclear);
        assert!(m.licence_text.contains("UNCLEAR"));
        assert!(m.licence_text.contains("does not host or bundle"));
        assert!(find("nope").is_none());
    }
}
