//! Pre-images of beat grids for the bulk grid writes (tempo edits, batch grid operations).
//!
//! `Library::save_grid` replaces a `.str` in place and the music.db snapshot does not hold the
//! `.str` files, so without a copy a bulk overwrite loses a hand-tuned grid or a variable-tempo
//! map for good. Before a job overwrites a track's `.str` it is copied here, once per run, into
//! `<snapshots>/<volume>/grids-<UTC stamp>/<path of the .str below .Pacemaker/Music>`. That is
//! off the device, next to the music.db snapshots (which only look at `music-db-*` folders, so
//! the two never disturb each other). A run that fails to copy a grid does not overwrite it.
//!
//! The copy is the way back by hand: put the file back at the same relative path below
//! `.Pacemaker/Music`. The row's BPM is not rewritten by that (it is derived from the grid when
//! a track is saved), so open the track in the grid editor and save it once afterwards.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pm_formats::naming::MUSIC_DIR;
use pm_formats::ops_device::utc_stamp;
use pm_formats::ops_fix::checked_track_paths;
use tauri::AppHandle;

use crate::error::AppResult;

/// Folder name prefix of one run's saved grids.
pub const GRIDS_PREFIX: &str = "grids-";

/// How many runs' folders are kept per volume; older ones are deleted when a new one is made.
pub const GRID_RUNS_KEPT: usize = 20;

/// The folder a run saves the previous grids into. Created on the first grid kept.
pub struct GridBackup {
    dir: PathBuf,
    kept: usize,
}

impl GridBackup {
    /// A backup for a run starting now, below the snapshot folder of the library at `root`.
    pub fn for_run(app: &AppHandle, root: &Path) -> AppResult<Self> {
        let base = crate::state::snapshots_dir(app, root)?;
        Ok(Self::new_in(&base))
    }

    /// A backup in a new `grids-<stamp>` folder (not created yet) below `base`.
    pub fn new_in(base: &Path) -> Self {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let stem = format!("{GRIDS_PREFIX}{}", utc_stamp(secs));
        let mut dir = base.join(&stem);
        let mut n = 1;
        while dir.exists() {
            n += 1;
            dir = base.join(format!("{stem}-{n}"));
        }
        Self { dir, kept: 0 }
    }

    /// Copies the `.str` of the track at `location` (a `tracks.location`) into the run's folder,
    /// before anything overwrites it. A track without a `.str` has nothing to keep. An error
    /// means the grid must not be overwritten.
    pub fn keep(&mut self, root: &Path, location: &str) -> AppResult<()> {
        let (_, sidecar) = checked_track_paths(root, location).ok_or("That track location is not valid.")?;
        if !sidecar.is_file() {
            return Ok(());
        }
        let rel = sidecar.strip_prefix(root.join(MUSIC_DIR)).map_err(|_| "That track location is not valid.")?;
        let dst = self.dir.join(rel);
        if dst.exists() {
            return Ok(());
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Cannot save the previous beat grid first: {e}"))?;
        }
        let part = dst.with_extension("part");
        fs::copy(&sidecar, &part).map_err(|e| format!("Cannot save the previous beat grid first: {e}"))?;
        // read back: the copy is the only way back, so it must be the same bytes
        let same = fs::read(&sidecar).ok() == fs::read(&part).ok();
        if !same {
            let _ = fs::remove_file(&part);
            return Err("The saved copy of the previous beat grid does not match; the grid was left as it is.".into());
        }
        fs::rename(&part, &dst).map_err(|e| format!("Cannot save the previous beat grid first: {e}"))?;
        if self.kept == 0 {
            self.prune();
        }
        self.kept += 1;
        Ok(())
    }

    /// The folder holding the saved grids, once at least one was kept.
    pub fn folder(&self) -> Option<&Path> {
        (self.kept > 0).then_some(self.dir.as_path())
    }

    /// A sentence for the job summary: where the previous grids went.
    pub fn note(&self) -> Option<String> {
        self.folder().map(|d| {
            format!(
                "The {} previous beat grid{} saved in {}; Restore does not bring grids back, that folder does",
                self.kept,
                if self.kept == 1 { " is" } else { "s are" },
                d.display()
            )
        })
    }

    /// Deletes the oldest `grids-*` folders beside this run's so only [`GRID_RUNS_KEPT`] remain
    /// (this run's folder exists by then and is counted).
    fn prune(&self) {
        let Some(base) = self.dir.parent() else { return };
        let mut runs: Vec<String> = fs::read_dir(base)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(GRIDS_PREFIX))
            .collect();
        // the stamp sorts as text; the same-second counter only matters for ties, which are rare
        // enough that a wrong order there deletes nothing important (the newest are kept)
        runs.sort();
        let this = self.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let excess = runs.len().saturating_sub(GRID_RUNS_KEPT);
        for old in runs.iter().filter(|n| **n != this).take(excess) {
            let _ = fs::remove_dir_all(base.join(old));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-app-gridbackup-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_grid_is_copied_to_the_same_relative_path_and_a_missing_one_is_nothing_to_keep() {
        let base = temp_dir("keep");
        let root = base.join("vol");
        let music = root.join(MUSIC_DIR);
        fs::create_dir_all(music.join("sub")).unwrap();
        fs::write(music.join("sub/a.str"), b"grid-a").unwrap();
        let snaps = base.join("snaps");
        let mut b = GridBackup::new_in(&snaps);
        assert!(b.note().is_none(), "nothing kept yet");
        b.keep(&root, "/pmdb_tracks/sub/a.mp3").unwrap();
        b.keep(&root, "/pmdb_tracks/sub/missing.mp3").unwrap();
        assert_eq!(b.kept, 1);
        let saved = b.folder().unwrap().join("sub/a.str");
        assert_eq!(fs::read(saved).unwrap(), b"grid-a");
        assert!(b.note().unwrap().contains("Restore does not bring grids back"));
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn a_location_that_climbs_out_of_the_music_folder_is_refused() {
        let base = temp_dir("dots");
        let root = base.join("vol");
        fs::create_dir_all(root.join(MUSIC_DIR)).unwrap();
        let mut b = GridBackup::new_in(&base.join("snaps"));
        assert!(b.keep(&root, "/pmdb_tracks/../../x.mp3").is_err());
        assert!(b.keep(&root, "/elsewhere/x.mp3").is_err());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn old_runs_are_pruned_to_the_cap() {
        let base = temp_dir("prune");
        let root = base.join("vol");
        let music = root.join(MUSIC_DIR);
        fs::create_dir_all(&music).unwrap();
        fs::write(music.join("a.str"), b"x").unwrap();
        let snaps = base.join("snaps");
        for i in 0..(GRID_RUNS_KEPT + 3) {
            fs::create_dir_all(snaps.join(format!("{GRIDS_PREFIX}2020-01-01T00-00-{i:02}Z"))).unwrap();
        }
        fs::create_dir_all(snaps.join("music-db-2020")).unwrap();
        let mut b = GridBackup::new_in(&snaps);
        b.keep(&root, "/pmdb_tracks/a.mp3").unwrap();
        let n = fs::read_dir(&snaps).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(GRIDS_PREFIX)).count();
        assert_eq!(n, GRID_RUNS_KEPT);
        assert!(snaps.join("music-db-2020").exists(), "snapshots are not touched");
        assert!(b.folder().unwrap().exists());
        fs::remove_dir_all(base).unwrap();
    }
}
