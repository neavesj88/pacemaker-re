//! The library on this computer: a normal folder with the exact device layout (made by
//! `Library::create`), so every command that takes a root works on it unchanged. This file holds
//! the two app-side settings (where it is, whether it opens at start) and the commands that read,
//! change and create it. Settings live in the app config folder, never on a device.
//!
//! Changing the path never moves or deletes the library at the old path.

use std::path::{Path, PathBuf};

use pm_formats::library::{Library, db_path};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::kind::{PACEMAKER, library_kind};
use crate::error::{AppError, AppResult};
use crate::store::config_dir;

pub const FILE: &str = "computer-library.json";
/// The folder made inside the Music folder when no path is chosen.
pub const FOLDER_NAME: &str = "Pacemaker Revival";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ComputerSettings {
    /// "" = the default (`<Music>/Pacemaker Revival`, resolved each time it is needed), else the
    /// absolute folder chosen.
    pub path: String,
    /// Open the library on this computer when the app starts (a Pacemaker is never opened by itself).
    pub open_at_start: bool,
}

impl Default for ComputerSettings {
    fn default() -> Self {
        ComputerSettings { path: String::new(), open_at_start: true }
    }
}

impl ComputerSettings {
    /// Trims the path and checks it. A library on this computer is a folder the app owns, so a
    /// drive root (which would count as a Pacemaker) and a file are refused.
    pub fn validated(mut self) -> Result<ComputerSettings, String> {
        self.path = self.path.trim().to_string();
        if self.path.is_empty() {
            return Ok(self);
        }
        if self.path.contains('\0') {
            return Err("That folder name is not valid.".into());
        }
        if library_kind(&self.path) == PACEMAKER {
            return Err("That is a whole drive, which counts as a Pacemaker. Choose a folder on it instead.".into());
        }
        if !Path::new(&self.path).is_absolute() {
            return Err("Give the full path of the folder, for example C:\\Users\\you\\Music\\Pacemaker Revival.".into());
        }
        if Path::new(&self.path).is_file() {
            return Err("That is a file, not a folder.".into());
        }
        Ok(self)
    }
}

/// `<Music>/Pacemaker Revival`, from the OS Music folder, else `<home>/Music`.
pub fn default_under(music: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    let base = music.or_else(|| home.map(|h| h.join("Music")))?;
    Some(base.join(FOLDER_NAME))
}

fn default_path(app: &AppHandle) -> AppResult<PathBuf> {
    let p = app.path();
    default_under(p.audio_dir().ok(), p.home_dir().ok()).ok_or_else(|| AppError::from("Could not find your Music folder. Choose a folder for the library in Settings."))
}

/// The folder the library lives in: the chosen one, else the default.
pub fn resolve(settings: &ComputerSettings, default: &Path) -> PathBuf {
    if settings.path.is_empty() { default.to_path_buf() } else { PathBuf::from(&settings.path) }
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// Reads the settings. A missing file is the defaults; an unreadable or invalid one is also the
/// defaults, with a warning to show (the file is left alone until the next save). The flag says
/// the file exists but could not be used.
pub fn load(dir: &Path) -> (ComputerSettings, Option<String>, bool) {
    let p = path(dir);
    match std::fs::read_to_string(&p) {
        Ok(text) => match serde_json::from_str::<ComputerSettings>(&text) {
            Ok(s) => match s.validated() {
                Ok(v) => (v, None, false),
                Err(e) => (ComputerSettings::default(), Some(format!("{} has an invalid value ({e}); defaults are used until you save.", p.display())), true),
            },
            Err(e) => (ComputerSettings::default(), Some(format!("{} is not readable ({e}); defaults are used until you save.", p.display())), true),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (ComputerSettings::default(), None, false),
        Err(e) => (ComputerSettings::default(), Some(format!("{}: {e}; defaults are used.", p.display())), true),
    }
}

/// Validates and saves. An unreadable file is copied to `.bad` first (once), as every settings file is.
pub fn save(dir: &Path, incoming: ComputerSettings) -> Result<ComputerSettings, String> {
    let next = incoming.validated()?;
    let (_, _, unreadable) = load(dir);
    crate::store::save_keeping_bad(&path(dir), &next, unreadable)?;
    Ok(next)
}

/// Creates the library at `root` if there is none (the folders and an empty `music.db`); one that
/// is already there is opened and left as it is, so calling this twice is the same as once.
pub fn create_at(root: &Path) -> Result<(), String> {
    Library::create(root).map(|_| ()).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct ComputerLibraryInfo {
    settings: ComputerSettings,
    /// Where the library is (or will be made): the chosen folder, else the default.
    path: String,
    default_path: String,
    /// A library (`.Pacemaker/music.db`) is there now.
    exists: bool,
    warning: Option<String>,
    /// The settings file is there but could not be used, so `settings` and `path` are only the
    /// defaults. The app opens nothing by itself then: the user decides (see `startupRoot`).
    unreadable: bool,
}

/// What the settings say, read only: nothing is created, opened or rewritten.
fn info_for(dir: &Path, default: &Path) -> ComputerLibraryInfo {
    let (settings, warning, unreadable) = load(dir);
    let at = resolve(&settings, default);
    ComputerLibraryInfo { exists: db_path(&at).is_file(), path: at.display().to_string(), default_path: default.display().to_string(), settings, warning, unreadable }
}

fn info(app: &AppHandle) -> AppResult<ComputerLibraryInfo> {
    Ok(info_for(&config_dir(app)?, &default_path(app)?))
}

/// Where "make the library" would put it. Refused while the settings file is unreadable, because
/// the path would then be the default instead of the one the file was meant to hold.
fn create_target(dir: &Path, default: &Path) -> Result<PathBuf, String> {
    let i = info_for(dir, default);
    if i.unreadable {
        return Err(format!(
            "{} could not be read, so no library was made. {} Use the default location in Settings (a copy of the damaged file is kept) or choose a folder first.",
            path(dir).display(),
            i.warning.unwrap_or_default()
        ));
    }
    Ok(PathBuf::from(i.path))
}

/// The settings, the resolved path and whether a library exists there.
#[tauri::command]
pub async fn computer_library_get(app: AppHandle) -> AppResult<ComputerLibraryInfo> {
    tauri::async_runtime::spawn_blocking(move || info(&app)).await?
}

/// Saves the settings (validated). The library at the old path stays where it is.
#[tauri::command]
pub async fn computer_library_set(app: AppHandle, settings: ComputerSettings) -> AppResult<ComputerLibraryInfo> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<ComputerLibraryInfo> {
        save(&config_dir(&app)?, settings)?;
        info(&app)
    })
    .await?
}

/// Makes the library at the configured path if it is not there yet and returns its root. Safe to
/// call again: an existing library is not touched.
#[tauri::command]
pub async fn computer_library_create(app: AppHandle) -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<String> {
        let at = create_target(&config_dir(&app)?, &default_path(&app)?)?;
        create_at(&at)?;
        Ok(at.display().to_string())
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-computer-lib-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn defaults_are_valid_open_at_start_and_the_default_path() {
        let d = ComputerSettings::default();
        assert_eq!(d.clone().validated().unwrap(), d);
        assert!(d.open_at_start && d.path.is_empty());
        assert_eq!(resolve(&d, Path::new("/m/Pacemaker Revival")), PathBuf::from("/m/Pacemaker Revival"));
        assert_eq!(resolve(&ComputerSettings { path: "/x".into(), ..d }, Path::new("/m")), PathBuf::from("/x"));
    }

    #[test]
    fn the_default_folder_is_in_music_else_in_home_music() {
        assert_eq!(default_under(Some("/u/Music".into()), Some("/u".into())), Some(PathBuf::from("/u/Music").join(FOLDER_NAME)));
        assert_eq!(default_under(None, Some("/u".into())), Some(PathBuf::from("/u").join("Music").join(FOLDER_NAME)));
        assert_eq!(default_under(None, None), None);
    }

    #[test]
    fn bad_paths_are_refused_with_a_reason() {
        let bad = |p: &str| ComputerSettings { path: p.into(), open_at_start: true }.validated().unwrap_err();
        assert!(bad("Music/lib").contains("full path"));
        assert!(bad("a\0b").contains("not valid"));
        assert!(bad("G:\\").contains("whole drive"));
        assert!(bad("g:").contains("whole drive"));
        if cfg!(windows) {
            let ok = ComputerSettings { path: "  C:\\Users\\me\\Music\\Lib ".into(), open_at_start: false }.validated().unwrap();
            assert_eq!(ok.path, "C:\\Users\\me\\Music\\Lib", "trimmed");
        } else {
            assert!(bad("/mnt/usb").contains("whole drive"));
            assert_eq!(ComputerSettings { path: " /home/me/Lib ".into(), open_at_start: false }.validated().unwrap().path, "/home/me/Lib");
        }
    }

    #[test]
    fn a_file_is_not_a_library_folder() {
        let dir = temp_dir("file");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.txt");
        std::fs::write(&f, "x").unwrap();
        assert!(ComputerSettings { path: f.display().to_string(), open_at_start: true }.validated().unwrap_err().contains("file"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_load_round_trip() {
        let dir = temp_dir("rt");
        let (s, warn, bad) = load(&dir);
        assert!(warn.is_none() && !bad && s == ComputerSettings::default(), "no file is the defaults");
        let lib = dir.join("lib");
        let saved = save(&dir, ComputerSettings { path: lib.display().to_string(), open_at_start: false }).unwrap();
        let (back, warn, bad) = load(&dir);
        assert!(warn.is_none() && !bad);
        assert_eq!(back, saved);
        assert!(!back.open_at_start);
        // a refused save changes nothing
        assert!(save(&dir, ComputerSettings { path: "relative".into(), open_at_start: true }).is_err());
        assert_eq!(load(&dir).0, saved);
        // missing fields read as the defaults
        std::fs::write(path(&dir), "{}").unwrap();
        assert_eq!(load(&dir).0, ComputerSettings::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_corrupt_file_gives_defaults_and_a_warning_and_is_kept_as_bad_when_saved_over() {
        let dir = temp_dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(path(&dir), "not json").unwrap();
        let (s, warn, bad) = load(&dir);
        assert_eq!(s, ComputerSettings::default());
        assert!(warn.unwrap().contains("not readable") && bad);
        assert_eq!(std::fs::read_to_string(path(&dir)).unwrap(), "not json", "loading never rewrites it");
        save(&dir, ComputerSettings { path: String::new(), open_at_start: false }).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(format!("{FILE}.bad"))).unwrap(), "not json");
        assert!(!load(&dir).0.open_at_start);
        // a hand-edited bad value is also reported, not silently used
        std::fs::write(path(&dir), r#"{"path":"relative"}"#).unwrap();
        let (s, warn, bad) = load(&dir);
        assert!(warn.unwrap().contains("invalid value") && bad && s == ComputerSettings::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreadable_settings_file_is_reported_and_nothing_is_made_or_rewritten() {
        let root = temp_dir("unreadable");
        let cfg = root.join("cfg");
        let default = root.join("Music").join(FOLDER_NAME);
        std::fs::create_dir_all(&cfg).unwrap();
        // an owner's library is at the default path: a corrupt file must not lead the app to it
        create_at(&default).unwrap();
        std::fs::write(path(&cfg), "{ not json").unwrap();
        let i = info_for(&cfg, &default);
        assert!(i.unreadable && i.warning.as_deref().unwrap().contains(FILE), "the file is named in the warning");
        assert!(i.exists, "the default library is there, but the frontend must not open it by itself");
        assert_eq!(std::fs::read_to_string(path(&cfg)).unwrap(), "{ not json", "reading never rewrites the file");
        assert!(!cfg.join(format!("{FILE}.bad")).exists());
        let err = create_target(&cfg, &default).unwrap_err();
        assert!(err.contains(FILE) && err.contains("no library was made"), "{err}");
        // a good file is not flagged
        std::fs::write(path(&cfg), r#"{"open_at_start":false}"#).unwrap();
        let i = info_for(&cfg, &default);
        assert!(!i.unreadable && i.warning.is_none() && !i.settings.open_at_start);
        assert_eq!(create_target(&cfg, &default).unwrap(), default);
        drop(Library::open_read_only(&default));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_path_that_does_not_exist_is_reported_and_not_created() {
        let root = temp_dir("missing");
        let cfg = root.join("cfg");
        std::fs::create_dir_all(&cfg).unwrap();
        let gone = root.join("unplugged").join("Lib");
        let default = root.join("Music").join(FOLDER_NAME);
        std::fs::write(path(&cfg), serde_json::json!({ "path": gone.display().to_string(), "open_at_start": true }).to_string()).unwrap();
        let i = info_for(&cfg, &default);
        assert!(!i.exists && !i.unreadable && i.path == gone.display().to_string(), "the configured path is kept, not swapped for the default");
        assert!(!gone.exists() && !default.exists(), "looking at the settings creates nothing");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn creating_twice_is_the_same_as_once_and_the_layout_is_the_device_layout() {
        let root = temp_dir("create").join("Pacemaker Revival");
        assert!(!db_path(&root).exists());
        create_at(&root).unwrap();
        assert!(db_path(&root).is_file() && root.join(".Pacemaker").join("Music").is_dir());
        let before = std::fs::read(db_path(&root)).unwrap();
        create_at(&root).unwrap();
        assert_eq!(std::fs::read(db_path(&root)).unwrap(), before, "an existing library is left alone");
        let lib = Library::open_read_only(&root).unwrap();
        assert!(lib.compatibility_problems().unwrap().is_empty() && lib.tracks().unwrap().is_empty());
        drop(lib); // Windows cannot delete a folder with an open database in it
        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }
}
