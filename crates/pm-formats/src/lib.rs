//! Readers and writers for the Tonium Pacemaker's on-device formats.
//!
//! Every format here was recovered from the device firmware and the original Editor (see
//! notes/FINDINGS.md) and is tested byte-exact against files the original Editor wrote, where
//! those are available locally (tests/editor_fixtures.rs).

pub mod db;
pub mod interop;
pub mod juce_b64;
pub mod library;
pub mod mix;
pub mod mixdoc;
pub mod naming;
pub mod ops_device;
pub mod ops_grid;
pub mod ops_history;
pub mod ops_metadata;
pub mod pfw;
pub mod strfile;
