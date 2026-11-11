//! Device-level operations: storage, health, firmware files, backup snapshots, the unreferenced-file
//! clean-up, `settings.dat`, eject (owner: device area).
//!
//! Everything that writes to the device volume does so only on an explicit call here and only for
//! the paths the caller names; the scans are read-only. Nothing in this module touches a library's
//! `music.db` except to read it.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::library::{Error, Library};
use crate::{naming, pfw, strfile};

fn invalid<T>(msg: impl Into<String>) -> Result<T, Error> {
    Err(Error::Invalid(msg.into()))
}

/// Start of the message of an operation stopped by the user (the UI shows it without "Failed").
pub const CANCELLED: &str = "Cancelled";

fn pacemaker_dir(root: &Path) -> PathBuf {
    root.join(".Pacemaker")
}

fn music_dir(root: &Path) -> PathBuf {
    root.join(naming::MUSIC_DIR)
}

// ---------------------------------------------------------------------------------------------
// SHA-256 (FIPS 180-4). Written out here because the backup manifest is the only user and a
// dependency would touch the shared lock file; checked against the NIST vectors below.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
    0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
    0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
    0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

/// Incremental SHA-256.
pub struct Sha256 {
    h: [u32; 8],
    buf: [u8; 64],
    fill: usize,
    total: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Sha256 {
            h: [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19],
            buf: [0; 64],
            fill: 0,
            total: 0,
        }
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Self::default()
    }

    fn block(&mut self, b: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (slot, c) in w.iter_mut().zip(b.chunks(4)) {
            *slot = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b_, mut c, mut d, mut e, mut f, mut g, mut h] = self.h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b_) ^ (a & c) ^ (b_ & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b_;
            b_ = a;
            a = t1.wrapping_add(t2);
        }
        for (s, v) in self.h.iter_mut().zip([a, b_, c, d, e, f, g, h]) {
            *s = s.wrapping_add(v);
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.total += data.len() as u64;
        if self.fill > 0 {
            let take = (64 - self.fill).min(data.len());
            self.buf[self.fill..self.fill + take].copy_from_slice(&data[..take]);
            self.fill += take;
            data = &data[take..];
            if self.fill == 64 {
                let b = self.buf;
                self.block(&b);
                self.fill = 0;
            }
        }
        while data.len() >= 64 {
            let (head, rest) = data.split_at(64);
            self.block(head.try_into().unwrap());
            data = rest;
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.fill = data.len();
        }
    }

    /// Lower-case hex digest.
    pub fn finish_hex(mut self) -> String {
        let bits = self.total.wrapping_mul(8);
        let mut pad = vec![0x80u8];
        let rem = (self.fill + 1) % 64;
        pad.resize(1 + if rem <= 56 { 56 - rem } else { 120 - rem }, 0);
        pad.extend_from_slice(&bits.to_be_bytes());
        let total = self.total;
        self.update(&pad);
        self.total = total;
        debug_assert_eq!(self.fill, 0);
        self.h.iter().map(|v| format!("{v:08x}")).collect()
    }
}

/// Hex SHA-256 of a byte slice.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.finish_hex()
}

const CHUNK: usize = 1 << 20;

/// Hex SHA-256 of a file, streamed.
pub fn sha256_file(path: &Path) -> Result<String, Error> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finish_hex())
}

// ---------------------------------------------------------------------------------------------
// Storage

/// Where the space on the volume goes. `other_bytes` is whatever the volume uses that is neither
/// music nor the library data (other files, file-system overhead).
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Storage {
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// Audio files under `.Pacemaker/Music`.
    pub music_bytes: u64,
    /// `.str` beat grids plus `music.db` (which holds the mixes and cases), the mix document and
    /// the settings and system files.
    pub grids_mixes_bytes: u64,
    pub other_bytes: u64,
}

fn walk(dir: &Path, out: &mut Vec<(PathBuf, u64)>) -> std::io::Result<()> {
    let rd = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for e in rd {
        let e = e?;
        let ft = e.file_type()?;
        if ft.is_dir() {
            walk(&e.path(), out)?;
        } else if ft.is_file() {
            out.push((e.path(), e.metadata()?.len()));
        }
    }
    Ok(())
}

fn is_str(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("str"))
}

/// Path relative to `base`, '/'-separated.
fn rel_string(base: &Path, p: &Path) -> String {
    p.strip_prefix(base)
        .unwrap_or(p)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Total and free bytes of the volume holding `path`.
pub fn volume_space(path: &Path) -> Result<(u64, u64), Error> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        unsafe extern "system" {
            fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        // SAFETY: `wide` is NUL-terminated and the three out-pointers are valid for the call.
        let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
        if ok == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok((total, avail))
    }
    #[cfg(not(windows))]
    {
        let out = std::process::Command::new("df").arg("-Pk").arg(path).output()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().nth(1).ok_or_else(|| Error::Invalid("df printed nothing".into()))?;
        let cols: Vec<&str> = line.split_whitespace().collect();
        let kib = |i: usize| cols.get(i).and_then(|c| c.parse::<u64>().ok());
        match (kib(1), kib(3)) {
            (Some(t), Some(a)) => Ok((t * 1024, a * 1024)),
            _ => invalid("could not read df output"),
        }
    }
}

/// Reads the storage picture of the volume. A volume whose free space cannot be read reports
/// zeros for total and free rather than failing the whole view.
pub fn storage(root: &Path) -> Result<Storage, Error> {
    let mut files = Vec::new();
    walk(&music_dir(root), &mut files)?;
    let mut s = Storage::default();
    for (p, n) in &files {
        if is_str(p) {
            s.grids_mixes_bytes += n;
        } else {
            s.music_bytes += n;
        }
    }
    for name in ["music.db", "music.db-journal", "settings.dat", "system.xml", "mixdocument.xml"] {
        if let Ok(m) = fs::metadata(pacemaker_dir(root).join(name)) {
            s.grids_mixes_bytes += m.len();
        }
    }
    if let Ok((total, free)) = volume_space(root) {
        s.total_bytes = total;
        s.free_bytes = free;
        s.other_bytes = total.saturating_sub(free).saturating_sub(s.music_bytes + s.grids_mixes_bytes);
    }
    Ok(s)
}

// ---------------------------------------------------------------------------------------------
// system.xml and firmware

/// What `.Pacemaker/system.xml` says about the device. The serial number is masked here and the
/// full value never leaves this module.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct FirmwareInfo {
    pub serial_masked: Option<String>,
    pub sw_version: Option<String>,
    pub fw_version: Option<String>,
    pub bootloader_version: Option<String>,
    pub editor_compatible_flag: Option<String>,
    /// `.pfw` files already waiting in `.Pacemaker`.
    pub pending_pfw: Vec<String>,
}

/// Value of `NAME="..."` in an XML-ish text, only where NAME starts a token.
fn xml_attr(text: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = text[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let starts_token = at == 0 || !text.as_bytes()[at - 1].is_ascii_alphanumeric() && text.as_bytes()[at - 1] != b'_';
        if !starts_token {
            continue;
        }
        let rest = text[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else { continue };
        let body = &rest[1..];
        if let Some(end) = body.find(q) {
            return Some(body[..end].to_string());
        }
    }
    None
}

/// "***1234": everything but the last four characters hidden (all hidden when four or fewer).
pub fn mask_serial(s: &str) -> String {
    let n = s.chars().count();
    if n <= 4 {
        return "*".repeat(n);
    }
    let tail: String = s.chars().skip(n - 4).collect();
    format!("{}{tail}", "*".repeat(n - 4))
}

/// Parses the text of a `system.xml`.
pub fn parse_system_xml(text: &str) -> FirmwareInfo {
    let get = |n: &str| xml_attr(text, n).filter(|v| !v.is_empty());
    FirmwareInfo {
        serial_masked: get("SERIALNUMBER").map(|s| mask_serial(&s)),
        sw_version: get("SWVERSION"),
        fw_version: get("FWVERSION"),
        bootloader_version: get("BOOTLOADERVERSION"),
        editor_compatible_flag: get("EDITOR_COMPATIBLE_FLAG"),
        pending_pfw: Vec::new(),
    }
}

fn pending_pfw(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(pacemaker_dir(root))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("pfw")))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// Installed versions from `system.xml` (missing file: all `None`) and the `.pfw` files waiting.
pub fn firmware_info(root: &Path) -> FirmwareInfo {
    let text = fs::read(pacemaker_dir(root).join("system.xml")).map(|b| String::from_utf8_lossy(&b).into_owned());
    let mut info = text.map(|t| parse_system_xml(&t)).unwrap_or_default();
    info.pending_pfw = pending_pfw(root);
    info
}

/// Exact size of every known `.pfw`: a 50,000 KiB ext3 image plus the 512-byte trailer.
pub const PFW_EXPECTED_SIZE: u64 = 50_000 * 1024 + pfw::TRAILER as u64;
/// The device refuses an image whose firmware or bootloader number is not above this
/// (`prepareSwUpgrade`, notes/ssd-swap.md).
const PFW_MIN_VERSION: u64 = 12837;

/// What a `.pfw` file is, and whether the device would accept it.
#[derive(Debug, Clone, Serialize)]
pub struct PfwInfo {
    pub file_name: String,
    pub size: u64,
    pub tag: String,
    pub sw_version: String,
    pub fw_version: String,
    pub bootloader_version: String,
    pub md5_ok: bool,
    /// Reasons the device would refuse it or it is damaged. Empty = installable.
    pub problems: Vec<String>,
    /// Oddities that do not stop an install (an unusual size).
    pub warnings: Vec<String>,
}

fn inspect_pfw_bytes(file_name: &str, data: &[u8]) -> PfwInfo {
    let mut info = PfwInfo {
        file_name: file_name.to_string(),
        size: data.len() as u64,
        tag: String::new(),
        sw_version: String::new(),
        fw_version: String::new(),
        bootloader_version: String::new(),
        md5_ok: false,
        problems: Vec::new(),
        warnings: Vec::new(),
    };
    match pfw::parse(data) {
        Err(pfw::Error::TooShort) => info.problems.push("the file is too short to be a firmware image".into()),
        Err(pfw::Error::BadField(f)) => info.problems.push(format!("the trailer field {f} is unreadable: not a Pacemaker firmware file")),
        Ok(t) => {
            info.tag = t.tag.clone();
            info.sw_version = t.sw_version;
            info.fw_version = t.fw_version;
            info.bootloader_version = t.bootloader_version;
            if t.ver.parse::<u64>() != Ok(1) {
                info.problems.push(format!("the trailer version is {:?}, not 1: the device ignores images whose trailer version is not 1", t.ver));
            }
            info.md5_ok = pfw::verify(data).unwrap_or(false);
            if !info.md5_ok {
                info.problems.push("the checksum in the trailer does not match the file: it is damaged or incomplete".into());
            }
            for (label, v) in [("firmware", &info.fw_version), ("bootloader", &info.bootloader_version)] {
                match v.parse::<u64>() {
                    Ok(n) if n > PFW_MIN_VERSION => {}
                    Ok(n) => info.problems.push(format!("the {label} number {n} is too low: the device only accepts numbers above {PFW_MIN_VERSION}")),
                    Err(_) => info.problems.push(format!("the {label} number {v:?} is not a number")),
                }
            }
        }
    }
    if info.size != PFW_EXPECTED_SIZE {
        info.warnings.push(format!("the size is {} bytes; every known image is {PFW_EXPECTED_SIZE}", info.size));
    }
    info
}

/// Reads a `.pfw` and checks its trailer, checksum and the device's version gate.
pub fn inspect_pfw(path: &Path) -> Result<PfwInfo, Error> {
    let data = fs::read(path)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(inspect_pfw_bytes(&name, &data))
}

/// Result of [`install_pfw`].
#[derive(Debug, Clone, Serialize)]
pub struct InstallReport {
    /// Name of the file now in `.Pacemaker`.
    pub dest_name: String,
    pub size: u64,
    /// The image's firmware number equals the installed one: the device then flashes nothing
    /// (notes/ssd-swap.md section 2, step 5).
    pub same_version_as_installed: bool,
}

/// A safe file name for the copy: ASCII letters, digits, `-`, `_`, `.`, ending in `.pfw`.
fn pfw_dest_name(src: &Path) -> String {
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let clean: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect();
    format!("{}.pfw", if clean.is_empty() { "firmware".to_string() } else { clean })
}

/// Copies a verified `.pfw` into `.Pacemaker` of the volume (the Editor's own destination:
/// the device scans that folder for `*.pfw` when it next starts). The copy goes to a `.part`
/// file first, is read back and verified, and only then renamed, so the device never sees a
/// half-written image. `progress(done_mib, total_mib)` is called after each megabyte chunk;
/// `cancel()` is polled before each chunk. On every failure or cancel the `.part` file is removed,
/// and a stale one left by an earlier aborted copy is removed before starting.
pub fn install_pfw(
    root: &Path,
    src: &Path,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &dyn Fn() -> bool,
) -> Result<InstallReport, Error> {
    let data = fs::read(src)?;
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let info = inspect_pfw_bytes(&name, &data);
    if !info.problems.is_empty() {
        return invalid(format!("not installing {name}: {}", info.problems.join("; ")));
    }
    let dir = pacemaker_dir(root);
    if !dir.is_dir() {
        return Err(Error::NotALibrary(root.to_path_buf()));
    }
    let dest_name = pfw_dest_name(src);
    if let Some(other) = pending_pfw(root).into_iter().find(|n| !n.eq_ignore_ascii_case(&dest_name)) {
        return invalid(format!("another firmware file ({other}) is already waiting in .Pacemaker; remove it first so the device does not pick the wrong one"));
    }
    if let Ok((_, free)) = volume_space(root) {
        if free < data.len() as u64 + (1 << 20) {
            return invalid(format!("not enough free space on the device: {} bytes needed", data.len()));
        }
    }
    let part = dir.join(format!("{dest_name}.part"));
    let total_mib = (data.len() as u64).div_ceil(CHUNK as u64);
    let _ = fs::remove_file(&part);
    let copied = (|| -> Result<(), Error> {
        let mut f = File::create(&part)?;
        for (i, c) in data.chunks(CHUNK).enumerate() {
            if cancel() {
                return invalid(format!("{CANCELLED}: the firmware copy was stopped and nothing was installed"));
            }
            f.write_all(c)?;
            progress(i as u64 + 1, total_mib);
        }
        f.sync_all()?;
        drop(f);
        if fs::read(&part)? != data {
            return invalid("the copy on the device does not match the source (read back differs); nothing was installed");
        }
        Ok(())
    })();
    if let Err(e) = copied {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    let dest = dir.join(&dest_name);
    // rename replaces an existing file in one step (on Windows too), so the old firmware file is
    // never deleted first: if the rename fails, it is still there
    if let Err(e) = fs::rename(&part, &dest) {
        let _ = fs::remove_file(&part);
        return Err(e.into());
    }
    let installed = firmware_info(root).fw_version;
    Ok(InstallReport {
        dest_name,
        size: data.len() as u64,
        same_version_as_installed: installed.as_deref() == Some(info.fw_version.as_str()),
    })
}

// ---------------------------------------------------------------------------------------------
// Library health and the unreferenced-file scan

const AUDIO_EXTS: [&str; 11] = ["mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "aif", "aiff", "wma", "mp4"];

/// A file under `.Pacemaker/Music` that no track row refers to.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UnreferencedFile {
    /// Path relative to `.Pacemaker/Music`, '/'-separated.
    pub path: String,
    pub size: u64,
    /// "audio" (no track row), "grid" (a `.str` whose track is gone) or "other".
    pub kind: String,
}

/// A track whose audio file is not on the device.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MissingAudio {
    pub track_id: i64,
    pub location: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Health {
    pub compatibility_problems: Vec<String>,
    pub missing_audio: Vec<MissingAudio>,
    /// False when the scan was skipped (see `warnings`).
    pub unreferenced_checked: bool,
    pub unreferenced: Vec<UnreferencedFile>,
    pub unreferenced_bytes: u64,
    pub warnings: Vec<String>,
}

fn scan_unreferenced(root: &Path, locations: &[String]) -> Result<Vec<UnreferencedFile>, Error> {
    let mut keep: HashSet<String> = HashSet::new();
    for loc in locations {
        if let Some(rel) = loc.strip_prefix(naming::LOCATION_PREFIX) {
            keep.insert(rel.to_lowercase());
            keep.insert(naming::str_path(rel).to_lowercase());
        }
    }
    let base = music_dir(root);
    let mut files = Vec::new();
    walk(&base, &mut files)?;
    let mut out = Vec::new();
    for (p, size) in files {
        let rel = rel_string(&base, &p);
        // FAT is case-insensitive, so the comparison is too
        if keep.contains(&rel.to_lowercase()) {
            continue;
        }
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let kind = if ext == "str" {
            "grid"
        } else if AUDIO_EXTS.contains(&ext.as_str()) {
            "audio"
        } else {
            "other"
        };
        out.push(UnreferencedFile { path: rel, size, kind: kind.into() });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Library health: compatibility problems, tracks whose audio is gone, files nothing uses.
/// Read-only. The unreferenced scan is skipped when the database is not device-compatible,
/// because rows that cannot be read would make every file look unused.
pub fn health(root: &Path) -> Result<Health, Error> {
    let lib = Library::open_read_only(root)?;
    let mut h = Health { compatibility_problems: lib.compatibility_problems()?, ..Health::default() };
    let tracks = match lib.tracks() {
        Ok(t) => t,
        Err(e) if !h.compatibility_problems.is_empty() => {
            h.warnings.push(format!("the track table could not be read: {e}"));
            return Ok(h);
        }
        Err(e) => return Err(e),
    };
    for t in &tracks {
        if let Some((audio, _)) = lib.track_paths(&t.location) {
            if !audio.is_file() {
                h.missing_audio.push(MissingAudio { track_id: t.track_id, location: t.location.clone() });
            }
        }
    }
    if !h.compatibility_problems.is_empty() {
        h.warnings.push("Unused files were not looked for: the library is not device-compatible, so its rows cannot be trusted.".into());
        return Ok(h);
    }
    let locations: Vec<String> = tracks.iter().map(|t| t.location.clone()).collect();
    h.unreferenced = scan_unreferenced(root, &locations)?;
    h.unreferenced_checked = true;
    h.unreferenced_bytes = h.unreferenced.iter().map(|f| f.size).sum();
    if tracks.is_empty() && !h.unreferenced.is_empty() {
        h.warnings.push("The library has no tracks at all, so every file under Music counts as unused. Check that this is the right drive.".into());
    }
    Ok(h)
}

/// Outcome of [`delete_unreferenced`].
#[derive(Debug, Clone, Default, Serialize)]
pub struct DeleteReport {
    pub deleted: usize,
    pub bytes: u64,
    /// Asked for, but no longer unreferenced (a track now uses it) or already gone: left alone.
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
    /// The user stopped it: the files after the last one deleted were not touched.
    pub cancelled: bool,
}

/// Deletes the named files, but only those a fresh scan still finds unreferenced, so a stale
/// list can never remove a file a track now uses. `progress(done, total, path)` per file;
/// `cancel()` is polled before each file.
pub fn delete_unreferenced(
    root: &Path,
    requested: &[String],
    progress: &mut dyn FnMut(usize, usize, &str),
    cancel: &dyn Fn() -> bool,
) -> Result<DeleteReport, Error> {
    let lib = Library::open_read_only(root)?;
    let problems = lib.compatibility_problems()?;
    if !problems.is_empty() {
        return Err(Error::Incompatible(problems));
    }
    let locations: Vec<String> = lib.tracks()?.into_iter().map(|t| t.location).collect();
    let fresh = scan_unreferenced(root, &locations)?;
    let base = music_dir(root);
    let mut report = DeleteReport::default();
    for (i, path) in requested.iter().enumerate() {
        if cancel() {
            report.cancelled = true;
            break;
        }
        progress(i, requested.len(), path);
        let Some(f) = fresh.iter().find(|f| &f.path == path) else {
            report.skipped.push(path.clone());
            continue;
        };
        match fs::remove_file(base.join(&f.path)) {
            Ok(()) => {
                report.deleted += 1;
                report.bytes += f.size;
            }
            Err(e) => report.failed.push(format!("{path}: {e}")),
        }
    }
    progress(requested.len(), requested.len(), "");
    Ok(report)
}

// ---------------------------------------------------------------------------------------------
// Backup snapshot

pub const MANIFEST_NAME: &str = "MANIFEST.sha256";

/// Result of [`backup_snapshot`].
#[derive(Debug, Clone, Serialize)]
pub struct BackupReport {
    pub folder: String,
    pub files: usize,
    pub bytes: u64,
    pub manifest: String,
    pub includes_audio: bool,
}

/// `(path relative to the volume root, '/'-separated)` of everything a snapshot holds.
fn backup_list(root: &Path, include_audio: bool) -> Result<Vec<String>, Error> {
    let mut rels = Vec::new();
    for name in ["music.db", "settings.dat", "system.xml", "mixdocument.xml"] {
        if pacemaker_dir(root).join(name).is_file() {
            rels.push(format!(".Pacemaker/{name}"));
        }
    }
    let base = music_dir(root);
    let mut files = Vec::new();
    walk(&base, &mut files)?;
    for (p, _) in files {
        if include_audio || is_str(&p) {
            rels.push(format!("{}/{}", naming::MUSIC_DIR, rel_string(&base, &p)));
        }
    }
    rels.sort();
    rels.dedup();
    Ok(rels)
}

/// "YYYYMMDDTHHMMSSZ" for a Unix time (UTC), by Hinnant's civil-from-days.
pub fn utc_stamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Copies `src` to `dst` hashing the bytes read, then reads `dst` back and checks the hash.
fn copy_hashed(src: &Path, dst: &Path) -> Result<(String, u64), Error> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut input = File::open(src)?;
    let mut out = File::create(dst)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut n_total = 0u64;
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        out.write_all(&buf[..n])?;
        n_total += n as u64;
    }
    out.sync_all()?;
    let sum = h.finish_hex();
    if sha256_file(dst)? != sum {
        return invalid(format!("{} did not copy correctly (read-back checksum differs)", dst.display()));
    }
    Ok((sum, n_total))
}

fn inside(parent: &Path, child: &Path) -> bool {
    match (fs::canonicalize(parent), fs::canonicalize(child)) {
        (Ok(p), Ok(c)) => c.starts_with(p),
        _ => false,
    }
}

/// Copies the library's data (music.db, settings.dat, system.xml, the mix document, every `.str`
/// and, only when asked, the audio) into a new folder `pacemaker-backup-<UTC stamp>` inside
/// `dest_dir`, each file read back and checked, and writes `MANIFEST.sha256` in `sha256sum`
/// format. Reads the device only. `progress(done, total, path)` is called per file; `cancel()` is
/// polled before each file. The copy is built in `<name>.partial` and renamed to `<name>` only
/// once the manifest is written, so a folder with the final name is always complete; on any
/// failure or cancel the partial folder is removed.
pub fn backup_snapshot(
    root: &Path,
    dest_dir: &Path,
    include_audio: bool,
    progress: &mut dyn FnMut(usize, usize, &str),
    cancel: &dyn Fn() -> bool,
) -> Result<BackupReport, Error> {
    if !dest_dir.is_dir() {
        return invalid(format!("{} is not a folder", dest_dir.display()));
    }
    if inside(root, dest_dir) {
        return invalid("choose a folder on another drive: a backup on the device itself is lost with the device");
    }
    let rels = backup_list(root, include_audio)?;
    if !rels.iter().any(|r| r.ends_with("/music.db")) {
        return Err(Error::NotALibrary(root.to_path_buf()));
    }
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = format!("pacemaker-backup-{}", utc_stamp(secs));
    let folder = dest_dir.join(&name);
    let partial = dest_dir.join(format!("{name}.partial"));
    if folder.exists() || partial.exists() {
        return invalid(format!("cannot create {}: it already exists", folder.display()));
    }
    fs::create_dir(&partial).map_err(|e| Error::Invalid(format!("cannot create {}: {e}", folder.display())))?;
    let built = (|| -> Result<u64, Error> {
        let mut manifest = String::new();
        let mut bytes = 0u64;
        for (i, rel) in rels.iter().enumerate() {
            if cancel() {
                return invalid(format!("{CANCELLED}: the backup was stopped and the partial copy was removed"));
            }
            progress(i, rels.len(), rel);
            let mut dst = partial.clone();
            dst.extend(rel.split('/'));
            let (sum, n) = copy_hashed(&root.join(rel), &dst)?;
            manifest.push_str(&format!("{sum}  {rel}
"));
            bytes += n;
        }
        fs::write(partial.join(MANIFEST_NAME), &manifest)?;
        fs::rename(&partial, &folder)?;
        Ok(bytes)
    })();
    let bytes = match built {
        Ok(v) => v,
        Err(e) => {
            let _ = fs::remove_dir_all(&partial);
            return Err(e);
        }
    };
    progress(rels.len(), rels.len(), "");
    Ok(BackupReport {
        folder: folder.display().to_string(),
        files: rels.len(),
        bytes,
        manifest: folder.join(MANIFEST_NAME).display().to_string(),
        includes_audio: include_audio,
    })
}

/// Checks a backup folder against its manifest. Returns the problems found (empty = intact).
pub fn verify_manifest(folder: &Path) -> Result<Vec<String>, Error> {
    let text = fs::read_to_string(folder.join(MANIFEST_NAME))?;
    let mut problems = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let Some((sum, rel)) = line.split_once("  ") else {
            problems.push(format!("unreadable manifest line: {line}"));
            continue;
        };
        if rel.split('/').any(|c| c == ".." || c.is_empty()) {
            problems.push(format!("unsafe path in manifest: {rel}"));
            continue;
        }
        let mut p = folder.to_path_buf();
        p.extend(rel.split('/'));
        match sha256_file(&p) {
            Ok(got) if got == sum => {}
            Ok(_) => problems.push(format!("{rel}: checksum differs")),
            Err(_) => problems.push(format!("{rel}: missing")),
        }
    }
    Ok(problems)
}

// ---------------------------------------------------------------------------------------------
// Automatic safety copies of music.db

/// Name prefix of an automatic snapshot folder (`music-db-<UTC stamp>`).
pub const SNAPSHOT_PREFIX: &str = "music-db-";

/// How many automatic snapshots of one library the app keeps.
pub const SNAPSHOTS_KEPT: usize = 10;

/// One automatic snapshot on disk.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SnapshotEntry {
    pub name: String,
    pub folder: String,
    /// Files in it, e.g. `music.db`, `settings.dat`.
    pub files: Vec<String>,
}

fn is_snapshot_dir(name: &str) -> bool {
    name.starts_with(SNAPSHOT_PREFIX) && !name.ends_with(".partial")
}

/// Sort key of a snapshot folder name: the UTC stamp, then the same-second counter (`-2`, `-3`,
/// ... `-10`; none = 1). Compared as a number, so `-10` is newer than `-2`.
fn snapshot_order(name: &str) -> (String, u64) {
    let rest = name.strip_prefix(SNAPSHOT_PREFIX).unwrap_or(name);
    match rest.split_once('-') {
        Some((stamp, n)) => (stamp.to_string(), n.parse().unwrap_or(1)),
        None => (rest.to_string(), 1),
    }
}

/// The snapshots in `dest_dir`, newest first (by the stamp in the name, then the same-second
/// counter).
pub fn list_snapshots(dest_dir: &Path) -> Vec<SnapshotEntry> {
    let mut out: Vec<SnapshotEntry> = fs::read_dir(dest_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !is_snapshot_dir(&name) {
                return None;
            }
            let mut files: Vec<String> =
                fs::read_dir(e.path()).into_iter().flatten().flatten().map(|f| f.file_name().to_string_lossy().into_owned()).collect();
            files.sort();
            Some(SnapshotEntry { folder: e.path().display().to_string(), name, files })
        })
        .collect();
    out.sort_by_key(|e| std::cmp::Reverse(snapshot_order(&e.name)));
    out
}

/// Copies the library's `music.db` (and `settings.dat`, and a leftover `music.db-journal`, which
/// a restore needs next to the database) from the device into a new folder `music-db-<UTC
/// stamp>` inside `dest_dir`, each file read back and checked, and keeps only the newest `keep`
/// snapshots there. `dest_dir` must not be on the device itself: the point is a copy that
/// survives a bad write to it. The folder is built as `<name>.partial` and renamed when complete,
/// so a folder with the final name is always whole. Reads the device only. Call it before the
/// first write to a library and do not write if it fails.
pub fn snapshot_library(root: &Path, dest_dir: &Path, keep: usize) -> Result<SnapshotEntry, Error> {
    let pm = pacemaker_dir(root);
    if !pm.join("music.db").is_file() {
        return Err(Error::NotALibrary(root.to_path_buf()));
    }
    fs::create_dir_all(dest_dir)?;
    if inside(root, dest_dir) {
        return invalid("the snapshot folder is on the device itself; a copy there is lost with the device");
    }
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let base = format!("{SNAPSHOT_PREFIX}{}", utc_stamp(secs));
    let mut name = base.clone();
    let mut n = 1;
    while dest_dir.join(&name).exists() {
        n += 1;
        name = format!("{base}-{n}");
    }
    let folder = dest_dir.join(&name);
    let partial = dest_dir.join(format!("{name}.partial"));
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir(&partial)?;
    let built = (|| -> Result<Vec<String>, Error> {
        let mut files = Vec::new();
        for f in ["music.db", "music.db-journal", "settings.dat"] {
            let src = pm.join(f);
            if src.is_file() {
                copy_hashed(&src, &partial.join(f))?;
                files.push(f.to_string());
            }
        }
        fs::rename(&partial, &folder)?;
        Ok(files)
    })();
    let files = match built {
        Ok(f) => f,
        Err(e) => {
            let _ = fs::remove_dir_all(&partial);
            return Err(e);
        }
    };
    // retention: oldest first out, never the one just made; stale half-built folders go too
    let all = list_snapshots(dest_dir);
    for old in all.iter().skip(keep.max(1)) {
        let _ = fs::remove_dir_all(&old.folder);
    }
    for e in fs::read_dir(dest_dir).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with(SNAPSHOT_PREFIX) && n.ends_with(".partial") {
            let _ = fs::remove_dir_all(e.path());
        }
    }
    Ok(SnapshotEntry { name, folder: folder.display().to_string(), files })
}

// ---------------------------------------------------------------------------------------------
// Eject

/// Asks the OS to eject the volume (Windows: the shell's Eject verb, as Explorer does). Returns
/// true once the library has disappeared from the volume. Where nothing can be verified this
/// returns false rather than claiming success.
pub fn eject(root: &Path) -> Result<bool, Error> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let s = root.to_string_lossy();
        let b = s.as_bytes();
        let drive_only = b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && s[2..].chars().all(|c| c == '\\' || c == '/');
        if !drive_only {
            return invalid("eject works on a drive letter such as P:");
        }
        let script = format!(
            "(New-Object -comObject Shell.Application).Namespace(17).ParseName('{}:').InvokeVerb('Eject')",
            b[0] as char
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .status()?;
        if !status.success() {
            return invalid("the eject command failed");
        }
    }
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("diskutil").arg("eject").arg(root).status()?;
        if !status.success() {
            return invalid("diskutil could not eject the volume");
        }
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let status = std::process::Command::new("umount").arg(root).status()?;
        if !status.success() {
            return invalid("umount could not unmount the volume");
        }
    }
    for _ in 0..20 {
        if !crate::library::db_path(root).exists() {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    Ok(false)
}

// ---------------------------------------------------------------------------------------------
// settings.dat: a boost::serialization text archive of `SettingRepository` (notes/settings-dat.md;
// port of scripts/settingsdat.py). Parse then emit is byte-identical.

const SIGNATURE: &str = "22 serialization::archive";
/// Pointer-class ids boost assigns, in registration order.
const BOOL: u8 = 1;
const FLOAT: u8 = 2;
const FLOATLIST: u8 = 3;
const PARENT: u8 = 4;
const STRING: u8 = 5;
/// The only repository class version the firmware (180096069) reads; `serialize` throws on any other.
pub const SETTINGS_REPO_VERSION: i64 = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    None,
    Bool(bool),
    Index(i64),
    Double(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub cls: u8,
    pub id: i64,
    pub parent: i64,
    /// "Most recently used" stamp.
    pub mru: i64,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Archive {
    pub lib_version: i64,
    pub repo_tracking: i64,
    pub repo_version: i64,
    pub vec_tracking: i64,
    pub vec_version: i64,
    pub item_version: i64,
    /// Class info (tracking, version) per pointer class id, as written at first use.
    pub class_info: Vec<(u8, i64, i64)>,
    pub setting_tracking: i64,
    pub setting_version: i64,
    pub items: Vec<Item>,
    pub mru_counter: i64,
}

fn bad<T>(m: impl Into<String>) -> Result<T, Error> {
    Err(Error::Invalid(format!("settings.dat: {}", m.into())))
}

fn double_text(v: f64) -> String {
    // libstdc++ at precision 17: "%.17g"; the shared formatter folds -0 into "0"
    if v == 0.0 && v.is_sign_negative() { "-0".into() } else { strfile::fmt_g(v, 17) }
}

/// Parses the text of a `settings.dat`.
pub fn parse_settings(text: &str) -> Result<Archive, Error> {
    let mut toks = text.split_ascii_whitespace();
    if toks.next() != Some("22") || toks.next() != Some("serialization::archive") {
        return bad("not a boost text archive");
    }
    let toks = std::cell::RefCell::new(toks);
    let next_int = |what: &str| -> Result<i64, Error> {
        match toks.borrow_mut().next() {
            None => bad(format!("truncated archive (expected {what})")),
            Some(t) => t.parse::<i64>().or_else(|_| bad(format!("{what}: {t:?} is not an integer"))),
        }
    };
    let mut a = Archive {
        lib_version: next_int("library version")?,
        repo_tracking: next_int("repository tracking")?,
        repo_version: next_int("repository version")?,
        vec_tracking: next_int("vector tracking")?,
        vec_version: next_int("vector version")?,
        item_version: 0,
        class_info: Vec::new(),
        setting_tracking: 0,
        setting_version: 0,
        items: Vec::new(),
        mru_counter: 0,
    };
    let count = next_int("item count")?;
    a.item_version = next_int("item version")?;
    if count < 0 {
        return bad("negative item count");
    }
    for n in 0..count {
        let cid = next_int("class id")?;
        let cls = match u8::try_from(cid) {
            Ok(c) if (BOOL..=STRING).contains(&c) => c,
            _ => return bad(format!("unknown class id {cid} at item {n}")),
        };
        if !a.class_info.iter().any(|(c, _, _)| *c == cls) {
            let t = next_int("class tracking")?;
            let v = next_int("class version")?;
            a.class_info.push((cls, t, v));
        }
        let oid = next_int("object id")?;
        if oid != n {
            return bad(format!("object id {oid} at item {n}"));
        }
        if n == 0 {
            a.setting_tracking = next_int("setting tracking")?;
            a.setting_version = next_int("setting version")?;
        }
        let id = next_int("setting id")?;
        let parent = next_int("parent id")?;
        let mru = next_int("mru stamp")?;
        let value = match cls {
            BOOL => match next_int("bool")? {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                x => return bad(format!("bool token {x}")),
            },
            FLOATLIST => Value::Index(next_int("list index")?),
            FLOAT => match toks.borrow_mut().next() {
                None => return bad("truncated archive (expected double)"),
                Some(t) => match t.parse::<f64>() {
                    Ok(v) if v.is_finite() => Value::Double(v),
                    _ => return bad(format!("double token {t:?}")),
                },
            },
            _ => {
                debug_assert!(cls == PARENT || cls == STRING);
                Value::None
            }
        };
        a.items.push(Item { cls, id, parent, mru, value });
    }
    a.mru_counter = next_int("mru counter")?;
    if toks.borrow_mut().next().is_some() {
        return bad("trailing tokens");
    }
    Ok(a)
}

/// Writes an archive exactly as the firmware does: space-separated tokens, a new line before
/// every object id, class info at the first use of each class, no trailing newline.
pub fn emit_settings(a: &Archive) -> String {
    let mut cur = format!(
        "{SIGNATURE} {} {} {} {} {} {} {}",
        a.lib_version,
        a.repo_tracking,
        a.repo_version,
        a.vec_tracking,
        a.vec_version,
        a.items.len(),
        a.item_version
    );
    let mut lines: Vec<String> = Vec::with_capacity(a.items.len() + 1);
    let mut seen: Vec<u8> = Vec::new();
    for (n, it) in a.items.iter().enumerate() {
        if seen.contains(&it.cls) {
            cur.push_str(&format!(" {}", it.cls));
        } else {
            let (t, v) = a.class_info.iter().find(|(c, _, _)| *c == it.cls).map(|(_, t, v)| (*t, *v)).unwrap_or((1, 0));
            cur.push_str(&format!(" {} {t} {v}", it.cls));
            seen.push(it.cls);
        }
        lines.push(std::mem::take(&mut cur));
        cur = n.to_string();
        if n == 0 {
            cur.push_str(&format!(" {} {}", a.setting_tracking, a.setting_version));
        }
        cur.push_str(&format!(" {} {} {}", it.id, it.parent, it.mru));
        match it.value {
            Value::Bool(b) => cur.push_str(if b { " 1" } else { " 0" }),
            Value::Index(i) => cur.push_str(&format!(" {i}")),
            Value::Double(d) => {
                cur.push(' ');
                cur.push_str(&double_text(d));
            }
            Value::None => {}
        }
    }
    lines.push(format!("{cur} {}", a.mru_counter));
    lines.join("\n")
}

enum Kind {
    Bool(&'static str, &'static str),
    List(&'static [&'static str]),
    Float,
}

struct Meta {
    id: i64,
    group: &'static str,
    title: &'static str,
    kind: Kind,
    default: f64,
}

const FX_SLOTS: &[&str] = &["Gain", "Mid", "Treble", "Bass", "Reverb", "Echo", "Roll", "Hi-lo cut", "Crush", "Trans", "Wah", "Delay", "Key"];
const LOOP_LENGTHS: &[&str] = &["Off", "1/64", "1/32", "1/16", "1/8", "1/4", "1/2", "1", "2", "4"];
const JUMP_LENGTHS: &[&str] = &["1/16", "1/8", "1/4", "1/2", "1", "2", "4", "8"];

macro_rules! b {
    ($id:expr, $g:expr, $t:expr, $d:expr) => {
        Meta { id: $id, group: $g, title: $t, kind: Kind::Bool("Off", "On"), default: if $d { 1.0 } else { 0.0 } }
    };
    ($id:expr, $g:expr, $t:expr, $f:expr, $tr:expr, $d:expr) => {
        Meta { id: $id, group: $g, title: $t, kind: Kind::Bool($f, $tr), default: if $d { 1.0 } else { 0.0 } }
    };
}
macro_rules! l {
    ($id:expr, $g:expr, $t:expr, $opts:expr, $d:expr) => {
        Meta { id: $id, group: $g, title: $t, kind: Kind::List($opts), default: $d as f64 }
    };
}
macro_rules! f {
    ($id:expr, $g:expr, $t:expr, $d:expr) => {
        Meta { id: $id, group: $g, title: $t, kind: Kind::Float, default: $d }
    };
}

/// The settings the firmware (180096069) registers, with titles and option labels from its
/// `english.lang` (notes/settings-dat.md). Action rows, read-outs, folders and the hidden
/// "Mix save" flag are left out: none of them has a stored value worth editing.
const KNOWN: &[Meta] = &[
    l!(800, "Playback", "Repeat", &["Off", "One", "All"], 0),
    b!(801, "Playback", "Shuffle", false),
    b!(1300, "Mix control", "Autocue", true),
    l!(1301, "Mix control", "Fixed loop length", LOOP_LENGTHS, 0),
    b!(1302, "Mix control", "Snap loops", true),
    l!(1303, "Mix control", "Beat jump length", JUMP_LENGTHS, 3),
    b!(1304, "Mix control", "Loop travel", true),
    b!(1305, "Mix control", "Time stretch", true),
    b!(1306, "Mix control", "Vinyl mode", false),
    l!(1307, "Mix control", "Effect x-fader", &["Off", "Bass", "Filter"], 0),
    b!(1308, "Mix control", "Phones x-fader", "Relative", "Normal", true),
    l!(1309, "Mix control", "Beat graph", &["Off", "On"], 1),
    b!(1310, "Mix control", "Lineout warning", true),
    b!(1600, "Audio", "Normalize", true),
    l!(100, "Audio", "Phones range", &["Low impedance", "High impedance"], 0),
    b!(301, "Audio", "Digital output (play mode)", false),
    b!(500, "Track list", "Layout", "Linked", "Split", true),
    b!(1700, "Track list filters", "Mix", true),
    b!(1701, "Track list filters", "Case", true),
    b!(1702, "Track list filters", "Album", true),
    b!(1703, "Track list filters", "Artist", true),
    b!(1704, "Track list filters", "Genre", true),
    b!(1705, "Track list filters", "Label", false),
    b!(1706, "Track list filters", "BPM", true),
    b!(1707, "Track list filters", "Key", false),
    b!(1708, "Track list filters", "Added", false),
    b!(1709, "Track list filters", "Rating", false),
    b!(1710, "Track list filters", "Type", false),
    b!(1711, "Track list filters", "Bitrate", false),
    b!(1712, "Track list filters", "Year", false),
    b!(1713, "Track list filters", "Remix", false),
    b!(1800, "Track time", "Display", "Left", "Played", true),
    b!(1801, "Track time", "End alert (mix)", true),
    l!(1900, "FX pads", "North", FX_SLOTS, 2),
    l!(1901, "FX pads", "East", FX_SLOTS, 1),
    l!(1902, "FX pads", "South", FX_SLOTS, 3),
    l!(1903, "FX pads", "West", FX_SLOTS, 0),
    l!(1904, "FX pads", "P-up / North", FX_SLOTS, 4),
    l!(1905, "FX pads", "P-up / East", FX_SLOTS, 6),
    l!(1906, "FX pads", "P-up / South", FX_SLOTS, 5),
    l!(1907, "FX pads", "P-up / West", FX_SLOTS, 7),
    l!(2000, "Power", "Standby", &["Off", "1 min", "5 min"], 2),
    l!(2001, "Power", "Backlight intensity", &["Low", "Medium", "Bright"], 1),
    l!(2002, "Power", "Backlight timeout", &["Off", "10 sec", "30 sec"], 2),
    b!(2003, "Power", "Nightlight", false),
    b!(2004, "Power", "Buttons blink", true),
    // raw audio-engine parameters: units and ranges are not known (notes/settings-dat.md, low
    // confidence), so these are shown but not editable
    f!(101, "Levels (read-only)", "Phones volume limit", 6.2831854820251465),
    f!(1400, "Levels (read-only)", "Phones EQ treble", 0.0),
    f!(1401, "Levels (read-only)", "Phones EQ mid", 0.0),
    f!(1402, "Levels (read-only)", "Phones EQ bass", 0.0),
    f!(1500, "Levels (read-only)", "Lineout EQ treble", 0.0),
    f!(1501, "Levels (read-only)", "Lineout EQ mid", 0.0),
    f!(1502, "Levels (read-only)", "Lineout EQ bass", 0.0),
    f!(300, "Levels (read-only)", "Lineout volume (mix)", 2.284794807434082),
    f!(901, "Levels (read-only)", "Volume (mix)", 2.284794807434082),
];

/// One setting as the UI shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SettingView {
    pub id: i64,
    pub group: String,
    pub title: String,
    /// "bool", "list" or "float".
    pub kind: String,
    /// bool: 0/1; list: the option index; float: the value.
    pub value: f64,
    pub default: f64,
    /// bool: [label for off, label for on]; list: every option; float: empty.
    pub options: Vec<String>,
    pub editable: bool,
    /// Whether the file holds this setting (an absent one keeps the device default).
    pub present: bool,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsView {
    pub settings: Vec<SettingView>,
    /// Items in the file that this table does not describe.
    pub other_items: usize,
    pub total_items: usize,
}

fn meta_cls(k: &Kind) -> u8 {
    match k {
        Kind::Bool(..) => BOOL,
        Kind::List(_) => FLOATLIST,
        Kind::Float => FLOAT,
    }
}

fn item_number(v: Value) -> Option<f64> {
    match v {
        Value::Bool(b) => Some(if b { 1.0 } else { 0.0 }),
        Value::Index(i) => Some(i as f64),
        Value::Double(d) => Some(d),
        Value::None => None,
    }
}

/// Describes the known settings of an archive for display.
pub fn describe_settings(a: &Archive) -> SettingsView {
    let mut settings = Vec::new();
    let mut described = 0;
    for m in KNOWN {
        let item = a.items.iter().find(|i| i.id == m.id && i.cls == meta_cls(&m.kind));
        if item.is_some() {
            described += 1;
        }
        let value = item.and_then(|i| item_number(i.value)).unwrap_or(m.default);
        let (kind, options) = match &m.kind {
            Kind::Bool(f, t) => ("bool", vec![f.to_string(), t.to_string()]),
            Kind::List(o) => ("list", o.iter().map(|s| s.to_string()).collect()),
            Kind::Float => ("float", Vec::new()),
        };
        settings.push(SettingView {
            id: m.id,
            group: m.group.into(),
            title: m.title.into(),
            kind: kind.into(),
            value,
            default: m.default,
            options,
            editable: item.is_some() && !matches!(m.kind, Kind::Float),
            present: item.is_some(),
            changed: item.is_some() && value != m.default,
        });
    }
    SettingsView { settings, other_items: a.items.len() - described, total_items: a.items.len() }
}

/// Sets values by setting id. Each change is checked against the table: the setting must be in
/// the file, editable, and the value a valid option. A changed setting gets the device's own
/// "recently used" stamp (counter + 1), as if it had been changed on the device. Nothing is
/// applied if any change is invalid.
pub fn apply_setting_changes(a: &mut Archive, changes: &[(i64, f64)]) -> Result<usize, Error> {
    let mut staged: Vec<(usize, Value)> = Vec::new();
    for &(id, v) in changes {
        let Some(m) = KNOWN.iter().find(|m| m.id == id) else { return bad(format!("setting {id} is not one this editor knows")) };
        let Some(pos) = a.items.iter().position(|i| i.id == id && i.cls == meta_cls(&m.kind)) else {
            return bad(format!("setting {id} ({}) is not in the file", m.title));
        };
        let value = match &m.kind {
            Kind::Bool(..) if v == 0.0 || v == 1.0 => Value::Bool(v == 1.0),
            Kind::List(o) if v.fract() == 0.0 && v >= 0.0 && (v as usize) < o.len() => Value::Index(v as i64),
            Kind::Float => return bad(format!("{} is read-only here", m.title)),
            _ => return bad(format!("{v} is not a valid value for {}", m.title)),
        };
        staged.push((pos, value));
    }
    let mut changed = 0;
    for (pos, value) in staged {
        if a.items[pos].value != value {
            a.items[pos].value = value;
            a.mru_counter += 1;
            a.items[pos].mru = a.mru_counter;
            changed += 1;
        }
    }
    Ok(changed)
}

fn settings_path(root: &Path) -> PathBuf {
    pacemaker_dir(root).join("settings.dat")
}

/// Reads the device's settings, or `None` when the file does not exist yet (the device creates
/// it on its first shutdown).
pub fn read_settings(root: &Path) -> Result<Option<SettingsView>, Error> {
    let bytes = match fs::read(settings_path(root)) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let text = String::from_utf8(bytes).or_else(|_| bad("not ASCII text"))?;
    Ok(Some(describe_settings(&parse_settings(&text)?)))
}

/// Applies changes to `settings.dat` and writes it back. Refuses unless the file parses, is the
/// repository version the firmware reads, and re-emits byte-identical (so only the changed
/// values can differ). Returns how many settings actually changed.
pub fn write_settings(root: &Path, changes: &[(i64, f64)]) -> Result<usize, Error> {
    let path = settings_path(root);
    let original = fs::read(&path)?;
    let text = String::from_utf8(original.clone()).or_else(|_| bad("not ASCII text"))?;
    let mut a = parse_settings(&text)?;
    if a.repo_version != SETTINGS_REPO_VERSION {
        return bad(format!("repository version {} (the firmware reads {SETTINGS_REPO_VERSION}); not rewriting it", a.repo_version));
    }
    if emit_settings(&a).as_bytes() != original.as_slice() {
        return bad("this file does not re-emit byte-identical; not rewriting it");
    }
    let changed = apply_setting_changes(&mut a, changes)?;
    if changed == 0 {
        return Ok(0);
    }
    let out = emit_settings(&a);
    debug_assert!(parse_settings(&out).is_ok());
    // temp file beside it, fsynced, renamed; the temp file is removed on every failure path
    // (a write that runs out of space or an unplug between the steps), see library::atomic_write
    crate::library::atomic_write(&path, out.as_bytes())?;
    Ok(changed)
}

// ---------------------------------------------------------------------------------------------
// Vocal filter flag (Revival firmware 180096073 and later)
// ---------------------------------------------------------------------------------------------

/// The flag file that switches the vocal filter on: `.Pacemaker/revival-vocal`. The names are
/// constants on purpose: nothing a caller passes (or the database holds) ever becomes part of
/// the path.
const VOCAL_FLAG: &str = "revival-vocal";
/// The kill switch: while `.Pacemaker/revival-no-preload` exists the device runs its stock Wah
/// whatever the vocal flag says. This module only ever reads it.
const NO_PRELOAD_FLAG: &str = "revival-no-preload";

/// What the vocal flag file asks of the device.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VocalMode {
    /// No flag file: the stock Wah.
    Off,
    /// The flag file exists: Wah pad right keeps the vocals, left cuts them.
    Vocal,
    /// The flag file starts with `i`: the two directions swapped.
    Inverted,
}

/// The state of the vocal filter on a card.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct VocalState {
    /// What the flag file says.
    pub mode: VocalMode,
    /// `revival-no-preload` exists: the device ignores the flag file and plays the stock Wah.
    pub kill_switch: bool,
    /// What the device will actually play: `mode`, or `Off` under the kill switch. Previews and
    /// exports of mixes use this.
    pub effective: VocalMode,
}

/// Reads the vocal flag the way the device does at boot: the file's existence switches the
/// filter on, a first byte of `i` (or `I`) inverts it. Read-only.
pub fn read_vocal(root: &Path) -> Result<VocalState, Error> {
    let dir = pacemaker_dir(root);
    let mode = match File::open(dir.join(VOCAL_FLAG)) {
        Ok(mut f) => {
            let mut first = [0u8; 1];
            let n = f.read(&mut first)?;
            if n == 1 && (first[0] == b'i' || first[0] == b'I') { VocalMode::Inverted } else { VocalMode::Vocal }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => VocalMode::Off,
        Err(e) => return Err(e.into()),
    };
    let kill_switch = dir.join(NO_PRELOAD_FLAG).exists();
    Ok(VocalState { mode, kill_switch, effective: if kill_switch { VocalMode::Off } else { mode } })
}

/// Sets the vocal flag: Off deletes `revival-vocal`, Vocal writes it empty, Inverted writes `i`.
/// Atomic (temp file, fsync, rename), and a no-op when the card already says so. Touches no other
/// file, `revival-no-preload` included. Returns the state read back from the card.
pub fn write_vocal(root: &Path, mode: VocalMode) -> Result<VocalState, Error> {
    let dir = pacemaker_dir(root);
    if !dir.is_dir() {
        return Err(Error::NotALibrary(root.to_path_buf()));
    }
    let before = read_vocal(root)?;
    if before.mode == mode {
        return Ok(before);
    }
    let path = dir.join(VOCAL_FLAG);
    match mode {
        VocalMode::Off => match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        },
        VocalMode::Vocal => crate::library::atomic_write(&path, b"")?,
        VocalMode::Inverted => crate::library::atomic_write(&path, b"i")?,
    }
    let now = read_vocal(root)?;
    if now.mode != mode {
        return invalid(format!("the flag file reads back as {:?} after writing {:?}", now.mode, mode));
    }
    Ok(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("pm-device-test-{tag}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    // ---- sha256 ----

    #[test]
    fn sha256_nist_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha256_incremental_and_long() {
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let whole = sha256_hex(&data);
        let mut h = Sha256::new();
        for c in data.chunks(97) {
            h.update(c);
        }
        assert_eq!(h.finish_hex(), whole);
        // one million 'a' (FIPS 180-4 example)
        let mut h = Sha256::new();
        for _ in 0..1000 {
            h.update(&[b'a'; 1000]);
        }
        assert_eq!(h.finish_hex(), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
        // padding edge: 55, 56 and 64 byte messages
        for n in [55usize, 56, 63, 64, 65] {
            let d = vec![b'x'; n];
            let mut h = Sha256::new();
            h.update(&d[..n / 2]);
            h.update(&d[n / 2..]);
            assert_eq!(h.finish_hex(), sha256_hex(&d));
        }
    }

    // ---- system.xml ----

    const SYSTEM_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\n<PACE>\n  \t<PRODUCTINFORMATION \n  \t\tSERIALNUMBER=\"SN-TEST-0001234\" \n  \t\tSWVERSION=\"180096069\"\n  \t\tFWVERSION=\"180096070\"\n\t        BOOTLOADERVERSION=\"12838\"\n\t        EDITOR_COMPATIBLE_FLAG=\"002\"\n\t/>\n</PACE>\n";

    #[test]
    fn system_xml_masks_serial() {
        let i = parse_system_xml(SYSTEM_XML);
        assert_eq!(i.serial_masked.as_deref(), Some("***********1234"));
        assert_eq!(i.sw_version.as_deref(), Some("180096069"));
        assert_eq!(i.fw_version.as_deref(), Some("180096070"));
        assert_eq!(i.bootloader_version.as_deref(), Some("12838"));
        assert_eq!(i.editor_compatible_flag.as_deref(), Some("002"));
        assert!(!format!("{i:?}").contains("SN-TEST-0001234"));
        assert_eq!(mask_serial("abc"), "***");
        assert_eq!(parse_system_xml("<PACE/>"), FirmwareInfo::default());
    }

    // ---- pfw ----

    fn fake_pfw(fw: &str, boot: &str) -> Vec<u8> {
        pfw::build(&vec![7u8; 4096], fw, fw, boot, "00000000-0000-0000-0000-000000000000")
    }

    #[test]
    fn pfw_inspection_gate() {
        let ok = inspect_pfw_bytes("a.pfw", &fake_pfw("180096069", "12838"));
        assert!(ok.problems.is_empty(), "{:?}", ok.problems);
        assert!(ok.md5_ok);
        assert_eq!(ok.fw_version, "180096069");
        assert!(!ok.warnings.is_empty(), "a 4 KiB image is not the usual size");

        let low = inspect_pfw_bytes("a.pfw", &fake_pfw("180096069", "100"));
        assert!(low.problems.iter().any(|p| p.contains("bootloader")));

        let mut damaged = fake_pfw("180096069", "12838");
        damaged[10] ^= 1;
        assert!(inspect_pfw_bytes("a.pfw", &damaged).problems.iter().any(|p| p.contains("checksum")));

        assert!(!inspect_pfw_bytes("a.pfw", b"short").problems.is_empty());
        assert!(!inspect_pfw_bytes("a.pfw", &vec![0u8; 2000]).problems.is_empty());
    }

    #[test]
    fn pfw_trailer_version_must_be_one() {
        let mut f = fake_pfw("180096069", "12838");
        let t0 = f.len() - pfw::TRAILER;
        assert_eq!(&f[t0..t0 + 4], b"0201");
        f[t0 + 3] = b'2';
        // keep the checksum valid so only the version is wrong
        let body = f.len() - 34;
        let sum = format!("{:x}", md5::compute(&f[..body]));
        let blk = format!("{:02X}{sum}", sum.len());
        f[body..body + blk.len()].copy_from_slice(blk.as_bytes());
        let info = inspect_pfw_bytes("a.pfw", &f);
        assert!(info.md5_ok);
        assert_eq!(info.problems.len(), 1, "{:?}", info.problems);
        assert!(info.problems[0].contains("trailer version"));
    }

    #[test]
    fn pfw_install_cancel_and_stale_part_leave_nothing() {
        let root = scratch("pfw-cancel");
        fs::create_dir_all(root.join(".Pacemaker")).unwrap();
        let src_dir = scratch("pfw-cancel-src");
        let src = src_dir.join("fw.pfw");
        fs::write(&src, fake_pfw("180096070", "12838")).unwrap();
        // a stale part from an aborted copy is replaced; a cancel removes the part again
        fs::write(root.join(".Pacemaker/fw.pfw.part"), b"stale").unwrap();
        let e = install_pfw(&root, &src, &mut |_, _| {}, &|| true).unwrap_err().to_string();
        assert!(e.contains(CANCELLED), "{e}");
        assert!(fs::read_dir(root.join(".Pacemaker")).unwrap().next().is_none());
        install_pfw(&root, &src, &mut |_, _| {}, &|| false).unwrap();
        assert!(root.join(".Pacemaker/fw.pfw").is_file());
        assert!(!root.join(".Pacemaker/fw.pfw.part").exists());
    }

    #[test]
    fn pfw_install_copies_and_guards() {
        let root = scratch("pfw");
        fs::create_dir_all(root.join(".Pacemaker")).unwrap();
        fs::write(root.join(".Pacemaker/system.xml"), SYSTEM_XML).unwrap();
        let src_dir = scratch("pfw-src");
        let src = src_dir.join("my fw (1).pfw");
        fs::write(&src, fake_pfw("180096070", "12838")).unwrap();

        let mut calls = Vec::new();
        let rep = install_pfw(&root, &src, &mut |d, t| calls.push((d, t)), &|| false).unwrap();
        assert_eq!(rep.dest_name, "my_fw__1_.pfw");
        assert!(rep.same_version_as_installed, "system.xml says FWVERSION 180096070");
        assert_eq!(calls.last(), Some(&(1, 1)));
        assert_eq!(fs::read(root.join(".Pacemaker/my_fw__1_.pfw")).unwrap(), fs::read(&src).unwrap());
        assert!(!root.join(".Pacemaker/my_fw__1_.pfw.part").exists());
        assert_eq!(firmware_info(&root).pending_pfw, vec!["my_fw__1_.pfw".to_string()]);

        // the same name may be replaced
        install_pfw(&root, &src, &mut |_, _| {}, &|| false).unwrap();
        // a different one is refused while another waits
        let other = src_dir.join("other.pfw");
        fs::write(&other, fake_pfw("180096071", "12838")).unwrap();
        let e = install_pfw(&root, &other, &mut |_, _| {}, &|| false).unwrap_err().to_string();
        assert!(e.contains("already waiting"), "{e}");
        // a damaged image is refused and leaves nothing behind
        let root2 = scratch("pfw2");
        fs::create_dir_all(root2.join(".Pacemaker")).unwrap();
        let mut bad_bytes = fake_pfw("180096070", "12838");
        bad_bytes[5] ^= 0xff;
        let bad_src = src_dir.join("bad.pfw");
        fs::write(&bad_src, bad_bytes).unwrap();
        assert!(install_pfw(&root2, &bad_src, &mut |_, _| {}, &|| false).is_err());
        assert!(firmware_info(&root2).pending_pfw.is_empty());
        assert!(fs::read_dir(root2.join(".Pacemaker")).unwrap().next().is_none());
    }

    // ---- a synthetic device volume ----

    /// A volume with a real library: three tracks (one whose audio is missing), their `.str`
    /// files, plus stray files.
    fn device_volume() -> (PathBuf, Vec<(String, PathBuf)>) {
        use crate::library::TrackMeta;
        let root = scratch("vol");
        let mut lib = Library::create(&root).unwrap();
        let src = scratch("src");
        let mut tracks = Vec::new();
        for (i, name) in ["alpha one.mp3", "beta two.mp3", "gamma three.mp3"].iter().enumerate() {
            let p = src.join(name);
            fs::write(&p, vec![i as u8; 1000 + i * 100]).unwrap();
            let meta = TrackMeta { title: format!("T{i}"), format: "mp3".into(), play_time_secs: 60, ..Default::default() };
            let an = strfile::Analysis { normalize_factor: 1.0, beat_start_offset: 0, beats: vec![strfile::Beat { pos: 0, bpm: 128.0 }], first_downbeat: Some(0) };
            let id = lib.add_track(&p, &meta, &an).unwrap();
            let loc = lib.tracks().unwrap().into_iter().find(|t| t.track_id == id).unwrap().location;
            tracks.push((loc.clone(), lib.track_paths(&loc).unwrap().0));
        }
        (root, tracks)
    }

    #[test]
    fn health_and_unreferenced_scan() {
        let (root, tracks) = device_volume();
        let h = health(&root).unwrap();
        assert!(h.compatibility_problems.is_empty(), "{:?}", h.compatibility_problems);
        assert!(h.unreferenced.is_empty() && h.missing_audio.is_empty() && h.unreferenced_checked);

        // strays: an unused audio file, an orphan .str, an odd file; and one track loses its audio
        let music = root.join(".Pacemaker/Music");
        fs::create_dir_all(music.join("0/0")).unwrap();
        fs::write(music.join("0/0/stray.mp3"), vec![1u8; 500]).unwrap();
        fs::write(music.join("0/0/stray.str"), b"x").unwrap();
        fs::write(music.join("0/0/Thumbs.db"), b"yy").unwrap();
        fs::remove_file(&tracks[1].1).unwrap();

        let h = health(&root).unwrap();
        let by_path: Vec<(&str, &str)> = h.unreferenced.iter().map(|f| (f.path.as_str(), f.kind.as_str())).collect();
        // the removed track's own .str is now unreferenced? No: its row still exists, so it is kept
        assert_eq!(by_path, vec![("0/0/Thumbs.db", "other"), ("0/0/stray.mp3", "audio"), ("0/0/stray.str", "grid")]);
        assert_eq!(h.unreferenced_bytes, 503);
        assert_eq!(h.missing_audio, vec![MissingAudio { track_id: h.missing_audio[0].track_id, location: tracks[1].0.clone() }]);
        assert_eq!(h.missing_audio.len(), 1);

        // case differences do not make a referenced file look unused (FAT is case-insensitive)
        let upper: Vec<String> = tracks.iter().map(|t| t.0.to_uppercase().replace("/PMDB_TRACKS/", "/pmdb_tracks/")).collect();
        assert_eq!(scan_unreferenced(&root, &upper).unwrap().len(), 3);
    }

    #[test]
    fn delete_only_what_is_still_unreferenced() {
        let (root, tracks) = device_volume();
        let music = root.join(".Pacemaker/Music");
        fs::create_dir_all(music.join("0/0")).unwrap();
        fs::write(music.join("0/0/stray.mp3"), vec![1u8; 500]).unwrap();
        fs::write(music.join("0/0/keep.mp3"), vec![1u8; 10]).unwrap();
        let referenced = tracks[0].0.strip_prefix(naming::LOCATION_PREFIX).unwrap().to_string();

        let mut seen = Vec::new();
        let rep = delete_unreferenced(
            &root,
            &["0/0/stray.mp3".into(), referenced.clone(), "../music.db".into(), "0/0/gone.mp3".into()],
            &mut |d, t, p| seen.push((d, t, p.to_string())),
            &|| false,
        )
        .unwrap();
        assert_eq!(rep.deleted, 1);
        assert_eq!(rep.bytes, 500);
        assert_eq!(rep.skipped, vec![referenced.clone(), "../music.db".to_string(), "0/0/gone.mp3".to_string()]);
        assert!(!music.join("0/0/stray.mp3").exists());
        assert!(music.join("0/0/keep.mp3").exists(), "not requested, not deleted");
        assert!(tracks[0].1.exists(), "a referenced file is never deleted");
        assert!(root.join(".Pacemaker/music.db").exists());
        assert_eq!(seen.last().unwrap().0, 4);

        // a cancel before the first file deletes nothing
        fs::write(music.join("0/0/stray2.mp3"), vec![1u8; 20]).unwrap();
        let r = delete_unreferenced(&root, &["0/0/stray2.mp3".into()], &mut |_, _, _| {}, &|| true).unwrap();
        assert!(r.cancelled);
        assert_eq!(r.deleted, 0);
        assert!(music.join("0/0/stray2.mp3").exists());
    }

    #[test]
    fn storage_counts_music_and_grids() {
        let (root, tracks) = device_volume();
        let s = storage(&root).unwrap();
        let audio: u64 = tracks.iter().map(|t| fs::metadata(&t.1).unwrap().len()).sum();
        assert_eq!(s.music_bytes, audio);
        assert!(s.grids_mixes_bytes > 0);
        assert!(s.total_bytes == 0 || s.total_bytes >= s.free_bytes);
    }

    // ---- backup ----

    #[test]
    fn backup_writes_files_and_manifest() {
        let (root, tracks) = device_volume();
        fs::write(root.join(".Pacemaker/settings.dat"), "settings").unwrap();
        fs::write(root.join(".Pacemaker/system.xml"), SYSTEM_XML).unwrap();
        let dest = scratch("backup");

        let mut ticks = 0;
        let r = backup_snapshot(&root, &dest, false, &mut |_, _, _| ticks += 1, &|| false).unwrap();
        assert!(ticks > 3);
        let folder = PathBuf::from(&r.folder);
        assert!(folder.file_name().unwrap().to_string_lossy().starts_with("pacemaker-backup-"));
        assert!(folder.join(".Pacemaker/music.db").is_file());
        assert!(folder.join(".Pacemaker/settings.dat").is_file());
        // 3 .str + music.db + settings.dat + system.xml, and no audio
        assert_eq!(r.files, 6);
        assert!(!r.includes_audio);
        let audio_rel = tracks[0].0.strip_prefix(naming::LOCATION_PREFIX).unwrap();
        assert!(!folder.join(naming::MUSIC_DIR).join(audio_rel).exists());

        let manifest = fs::read_to_string(&r.manifest).unwrap();
        assert_eq!(manifest.lines().count(), 6);
        let line = manifest.lines().find(|l| l.ends_with("  .Pacemaker/settings.dat")).unwrap();
        assert_eq!(line, format!("{}  .Pacemaker/settings.dat", sha256_hex(b"settings")));
        assert!(manifest.lines().all(|l| l.len() > 66 && l.as_bytes()[64] == b' ' && l.as_bytes()[65] == b' '));
        assert!(verify_manifest(&folder).unwrap().is_empty());

        // tampering and loss are reported
        fs::write(folder.join(".Pacemaker/settings.dat"), "tampered").unwrap();
        fs::remove_file(folder.join(".Pacemaker/system.xml")).unwrap();
        let problems = verify_manifest(&folder).unwrap();
        assert_eq!(problems.len(), 2, "{problems:?}");

        // with audio, the audio files are in too
        let r2 = backup_snapshot(&root, &dest, true, &mut |_, _, _| {}, &|| false);
        // the stamp has one-second resolution: a second snapshot in the same second is refused, not merged
        let r2 = match r2 {
            Ok(r) => r,
            Err(e) => {
                assert!(e.to_string().contains("cannot create"), "{e}");
                std::thread::sleep(std::time::Duration::from_millis(1100));
                backup_snapshot(&root, &dest, true, &mut |_, _, _| {}, &|| false).unwrap()
            }
        };
        assert_eq!(r2.files, 9);
        assert!(PathBuf::from(&r2.folder).join(naming::MUSIC_DIR).join(audio_rel).is_file());
        assert!(verify_manifest(Path::new(&r2.folder)).unwrap().is_empty());
        // the device is untouched
        assert!(tracks[0].1.is_file());
    }

    #[test]
    fn backup_cancel_and_failure_leave_no_folder() {
        let (root, tracks) = device_volume();
        let dest = scratch("backup-cancel");
        let calls = std::cell::Cell::new(0);
        let e = backup_snapshot(&root, &dest, false, &mut |_, _, _| {}, &|| {
            calls.set(calls.get() + 1);
            calls.get() > 2
        })
        .unwrap_err()
        .to_string();
        assert!(e.contains(CANCELLED), "{e}");
        assert!(fs::read_dir(&dest).unwrap().next().is_none(), "no partial folder is left");

        // a file that vanishes mid-way fails the backup and removes the partial copy
        let db = root.join(".Pacemaker/music.db");
        let e = backup_snapshot(&root, &dest, false, &mut |i, _, _| {
            if i == 0 {
                fs::remove_file(&db).unwrap();
            }
        }, &|| false);
        assert!(e.is_err());
        assert!(fs::read_dir(&dest).unwrap().next().is_none(), "no partial folder is left");
        assert!(tracks[0].1.is_file(), "the device is untouched");
    }

    #[test]
    fn backup_refuses_the_device_itself_and_non_libraries() {
        let (root, _) = device_volume();
        let inside_dev = root.join("backups");
        fs::create_dir_all(&inside_dev).unwrap();
        assert!(backup_snapshot(&root, &inside_dev, false, &mut |_, _, _| {}, &|| false).unwrap_err().to_string().contains("another drive"));
        let empty = scratch("empty");
        let dest = scratch("dest");
        assert!(backup_snapshot(&empty, &dest, false, &mut |_, _, _| {}, &|| false).is_err());
        assert!(fs::read_dir(&dest).unwrap().next().is_none(), "nothing is created for a non-library");
        assert!(backup_snapshot(&root, &dest.join("nope"), false, &mut |_, _, _| {}, &|| false).is_err());
    }

    #[test]
    fn utc_stamp_known_dates() {
        assert_eq!(utc_stamp(0), "19700101T000000Z");
        assert_eq!(utc_stamp(951_782_400 + 3661), "20000229T010101Z");
        assert_eq!(utc_stamp(1_781_000_000), "20260609T101320Z");
    }

    // ---- settings.dat ----

    /// Six items, one of each class, with class info at first use, hand-written in the shape the
    /// firmware's dump has (notes/settings-dat.md).
    const SAMPLE: &str = "22 serialization::archive 4 0 16 0 0 6 0 1 1 0\n0 0 0 2004 604 0 1 1\n1 2003 604 9 0 3 1 0\n2 2000 604 0 2 4 1 0\n3 604 5 0 2 1 0\n4 1400 200 0 0.021000000000000001 5 1 0\n5 1100 4 0 4983";

    #[test]
    fn settings_round_trip_is_byte_exact() {
        let a = parse_settings(SAMPLE).unwrap();
        assert_eq!(a.items.len(), 6);
        assert_eq!(a.repo_version, 16);
        assert_eq!(a.mru_counter, 4983);
        assert_eq!(a.items[2].value, Value::Index(2));
        assert_eq!(a.items[4].value, Value::Double(0.021));
        assert_eq!(emit_settings(&a), SAMPLE);
    }

    #[test]
    fn settings_round_trip_awkward_doubles_and_order() {
        for d in [0.0, -0.0, 6.2831854820251465, 2.284794807434082, 3.1415927410125732, -0.0099, 1e-5, 1.5e300, -123456.789] {
            let mut a = parse_settings(SAMPLE).unwrap();
            a.items[4].value = Value::Double(d);
            let text = emit_settings(&a);
            let b = parse_settings(&text).unwrap();
            assert_eq!(b.items[4].value, Value::Double(d), "{text}");
            assert_eq!(emit_settings(&b), text);
        }
        // any item order re-emits a valid archive: class info moves with first use
        let mut a = parse_settings(SAMPLE).unwrap();
        a.items.reverse();
        let text = emit_settings(&a);
        // reversed: the last item (a string) now comes first, so its class id 5 carries the info
        let b = parse_settings(&text).unwrap();
        assert_eq!(b.items.iter().map(|i| i.id).collect::<Vec<_>>(), vec![1100, 1400, 604, 2000, 2003, 2004]);
        assert_eq!(emit_settings(&b), text);
    }

    #[test]
    fn settings_parse_rejects_damage() {
        assert!(parse_settings("garbage").is_err());
        assert!(parse_settings(&SAMPLE[..SAMPLE.len() - 5]).is_err(), "truncated");
        assert!(parse_settings(&format!("{SAMPLE} 7")).is_err(), "trailing tokens");
        assert!(parse_settings(&SAMPLE.replace("\n1 2003", "\n9 2003")).is_err(), "object id out of order");
        assert!(parse_settings(&SAMPLE.replace("2004 604 0 1 1", "2004 604 0 2 1")).is_err(), "bool token 2");
        assert!(parse_settings(&SAMPLE.replace("0.021000000000000001", "inf")).is_err());
        assert!(parse_settings(&SAMPLE.replace(" 2 4 1 0\n3 604", " 9 4 1 0\n3 604")).is_ok(), "any list index is a number");
    }

    /// A larger archive with real ids so the describe/apply path has something to chew on.
    fn known_archive() -> String {
        let mut a = parse_settings(SAMPLE).unwrap();
        a.items.clear();
        for (n, m) in KNOWN.iter().enumerate() {
            let mru = n as i64 + 1;
            let value = match m.kind {
                Kind::Bool(..) => Value::Bool(m.default == 1.0),
                Kind::List(_) => Value::Index(m.default as i64),
                Kind::Float => Value::Double(m.default),
            };
            a.items.push(Item { cls: meta_cls(&m.kind), id: m.id, parent: 0, mru: if m.id == 2003 { mru } else { 0 }, value });
        }
        a.items.push(Item { cls: PARENT, id: 604, parent: 5, mru: 0, value: Value::None });
        a.items.push(Item { cls: STRING, id: 1100, parent: 4, mru: 0, value: Value::None });
        a.mru_counter = 60;
        emit_settings(&a)
    }

    #[test]
    fn known_table_is_consistent() {
        let mut ids: Vec<i64> = KNOWN.iter().map(|m| m.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), KNOWN.len(), "duplicate ids");
        for m in KNOWN {
            if let Kind::List(o) = &m.kind {
                assert!((m.default as usize) < o.len(), "{}", m.title);
            }
        }
    }

    #[test]
    fn describe_and_apply() {
        let text = known_archive();
        let mut a = parse_settings(&text).unwrap();
        assert_eq!(emit_settings(&a), text);
        let v = describe_settings(&a);
        assert_eq!(v.other_items, 2);
        assert!(v.settings.iter().all(|s| s.present && !s.changed));
        let backlight = v.settings.iter().find(|s| s.id == 2001).unwrap();
        assert_eq!((backlight.kind.as_str(), backlight.value, backlight.options.len()), ("list", 1.0, 3));
        assert!(!v.settings.iter().find(|s| s.id == 1400).unwrap().editable, "floats are read-only");

        let n = apply_setting_changes(&mut a, &[(2001, 2.0), (2003, 1.0), (2004, 1.0)]).unwrap();
        assert_eq!(n, 2, "2004 is already on");
        assert_eq!(a.mru_counter, 62);
        let it = |id| a.items.iter().find(|i| i.id == id).unwrap().clone();
        assert_eq!(it(2001).value, Value::Index(2));
        assert_eq!(it(2001).mru, 61);
        assert_eq!(it(2003).value, Value::Bool(true));
        assert_eq!(it(2004).mru, 0, "an unchanged setting keeps its stamp");
        let after = describe_settings(&a);
        assert!(after.settings.iter().find(|s| s.id == 2001).unwrap().changed);
        assert_eq!(parse_settings(&emit_settings(&a)).unwrap(), a);

        // invalid changes apply nothing
        let before = a.clone();
        for bad in [(2001, 3.0), (2001, 0.5), (2001, -1.0), (2003, 2.0), (1400, 0.1), (9999, 1.0), (2005, 1.0)] {
            assert!(apply_setting_changes(&mut a, &[(2004, 0.0), bad]).is_err(), "{bad:?}");
            assert_eq!(a, before);
        }
    }

    fn tmp_in(dir: &Path) -> Vec<String> {
        fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".tmp")).collect()
    }

    #[test]
    fn settings_write_leaves_no_temp_file() {
        let root = scratch("settings-tmp");
        let pm = root.join(".Pacemaker");
        fs::create_dir_all(&pm).unwrap();
        fs::write(pm.join("settings.dat"), known_archive()).unwrap();
        assert_eq!(write_settings(&root, &[(2001, 0.0)]).unwrap(), 1);
        assert!(tmp_in(&pm).is_empty(), "{:?}", tmp_in(&pm));
        // refused writes (bad value, unchanged value) never create one either
        assert!(write_settings(&root, &[(2001, 7.0)]).is_err());
        assert_eq!(write_settings(&root, &[(2001, 0.0)]).unwrap(), 0);
        assert!(tmp_in(&pm).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn snapshot_copies_db_and_settings_and_keeps_the_newest() {
        let root = scratch("snap-root");
        let dest = scratch("snap-dest").join("snaps");
        let pm = root.join(".Pacemaker");
        fs::create_dir_all(&pm).unwrap();
        fs::write(pm.join("music.db"), b"database bytes").unwrap();
        fs::write(pm.join("settings.dat"), b"settings bytes").unwrap();

        let s = snapshot_library(&root, &dest, 3).unwrap();
        assert!(s.name.starts_with(SNAPSHOT_PREFIX));
        assert_eq!(s.files, ["music.db", "settings.dat"]);
        assert_eq!(fs::read(Path::new(&s.folder).join("music.db")).unwrap(), b"database bytes");
        assert_eq!(fs::read(Path::new(&s.folder).join("settings.dat")).unwrap(), b"settings bytes");
        // without settings.dat only the database is copied; the device is not touched
        fs::remove_file(pm.join("settings.dat")).unwrap();
        let s2 = snapshot_library(&root, &dest, 3).unwrap();
        assert_eq!(s2.files, ["music.db"]);
        assert_ne!(s.name, s2.name, "two snapshots in the same second get distinct names");
        assert_eq!(fs::read(pm.join("music.db")).unwrap(), b"database bytes");

        // retention: only the newest `keep` stay, and a stale half-built folder is swept
        fs::create_dir_all(dest.join(format!("{SNAPSHOT_PREFIX}19990101T000000Z.partial"))).unwrap();
        for _ in 0..4 {
            snapshot_library(&root, &dest, 3).unwrap();
        }
        let left = list_snapshots(&dest);
        assert_eq!(left.len(), 3);
        assert!(snapshot_order(&left[0].name) > snapshot_order(&left[1].name) && snapshot_order(&left[1].name) > snapshot_order(&left[2].name), "newest first");
        assert!(fs::read_dir(&dest).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".partial")));
        assert!(!s.folder.is_empty() && !Path::new(&s.folder).exists(), "the oldest was pruned");

        // refuses a folder on the device, and a volume with no library
        assert!(snapshot_library(&root, &root.join("inside"), 3).unwrap_err().to_string().contains("device itself"));
        let empty = scratch("snap-empty");
        assert!(snapshot_library(&empty, &dest, 3).is_err());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(dest.parent().unwrap());
        let _ = fs::remove_dir_all(&empty);
    }

    #[test]
    fn snapshots_made_in_one_second_order_by_number_not_by_text() {
        let stamp = "music-db-20260609T101320Z";
        let mut names: Vec<String> = ["", "-2", "-9", "-10", "-11"].iter().map(|n| format!("{stamp}{n}")).collect();
        names.push("music-db-20260609T101321Z".to_string());
        names.push("music-db-20260609T101319Z-12".to_string());
        names.sort_by_key(|n| std::cmp::Reverse(snapshot_order(n)));
        assert_eq!(names[0], "music-db-20260609T101321Z", "a later second beats any counter");
        assert_eq!(names[1], format!("{stamp}-11"));
        assert_eq!(names[2], format!("{stamp}-10"));
        assert_eq!(names[5], stamp);
        assert_eq!(names[6], "music-db-20260609T101319Z-12");
    }

    #[test]
    fn settings_file_read_and_write() {
        let root = scratch("settings");
        fs::create_dir_all(root.join(".Pacemaker")).unwrap();
        assert!(read_settings(&root).unwrap().is_none());
        let path = root.join(".Pacemaker/settings.dat");
        let text = known_archive();
        fs::write(&path, &text).unwrap();
        assert_eq!(read_settings(&root).unwrap().unwrap().total_items, KNOWN.len() + 2);

        assert_eq!(write_settings(&root, &[(2001, 1.0)]).unwrap(), 0, "no change, no write");
        assert_eq!(fs::read_to_string(&path).unwrap(), text);

        assert_eq!(write_settings(&root, &[(2001, 0.0), (2004, 0.0)]).unwrap(), 2);
        let written = fs::read_to_string(&path).unwrap();
        let a = parse_settings(&written).unwrap();
        let mut expect = parse_settings(&text).unwrap();
        apply_setting_changes(&mut expect, &[(2001, 0.0), (2004, 0.0)]).unwrap();
        assert_eq!(written, emit_settings(&expect));
        assert_eq!(a.mru_counter, 62);
        let leftovers: Vec<_> = fs::read_dir(root.join(".Pacemaker")).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");

        // refuses a file it would not reproduce exactly, or another repository version
        fs::write(&path, format!("{text}\n")).unwrap();
        assert!(write_settings(&root, &[(2001, 0.0)]).is_err());
        fs::write(&path, text.replacen(" 0 16 ", " 0 13 ", 1)).unwrap();
        assert!(write_settings(&root, &[(2001, 0.0)]).unwrap_err().to_string().contains("version 13"));
        // an invalid value leaves the file alone
        fs::write(&path, &text).unwrap();
        assert!(write_settings(&root, &[(2001, 7.0)]).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    /// Optional: set PM_SETTINGS_DUMP to a real settings.dat copy to check the byte-exact round
    /// trip and the id table against it. Skipped when unset (no device data lives in the repo).
    #[test]
    fn real_dump_round_trip_if_provided() {
        let Ok(path) = std::env::var("PM_SETTINGS_DUMP") else { return };
        let raw = fs::read(path).unwrap();
        let a = parse_settings(std::str::from_utf8(&raw).unwrap()).unwrap();
        assert_eq!(emit_settings(&a).as_bytes(), raw.as_slice());
        let v = describe_settings(&a);
        assert!(v.settings.iter().all(|s| s.present), "every known id is in a real dump: {:?}", v.settings.iter().filter(|s| !s.present).map(|s| s.id).collect::<Vec<_>>());
    }

    #[test]
    fn vocal_flag_round_trip_and_the_files_it_must_not_touch() {
        let root = scratch("vocal");
        let pm = root.join(".Pacemaker");
        fs::create_dir_all(&pm).unwrap();
        fs::write(pm.join("music.db"), b"db").unwrap();
        fs::write(pm.join("settings.dat"), b"settings").unwrap();
        let st = |m, k, e| VocalState { mode: m, kill_switch: k, effective: e };

        assert_eq!(read_vocal(&root).unwrap(), st(VocalMode::Off, false, VocalMode::Off));
        // Vocal = an empty file
        assert_eq!(write_vocal(&root, VocalMode::Vocal).unwrap(), st(VocalMode::Vocal, false, VocalMode::Vocal));
        assert_eq!(fs::read(pm.join("revival-vocal")).unwrap(), b"");
        // Inverted = "i"
        assert_eq!(write_vocal(&root, VocalMode::Inverted).unwrap(), st(VocalMode::Inverted, false, VocalMode::Inverted));
        assert_eq!(fs::read(pm.join("revival-vocal")).unwrap(), b"i");
        // back to Vocal replaces the content
        assert_eq!(write_vocal(&root, VocalMode::Vocal).unwrap().mode, VocalMode::Vocal);
        assert_eq!(fs::read(pm.join("revival-vocal")).unwrap(), b"");
        // Off deletes, twice is fine
        assert_eq!(write_vocal(&root, VocalMode::Off).unwrap(), st(VocalMode::Off, false, VocalMode::Off));
        assert!(!pm.join("revival-vocal").exists());
        assert_eq!(write_vocal(&root, VocalMode::Off).unwrap().mode, VocalMode::Off);

        // the device only looks at the first letter; hand-made files still read right
        for (content, want) in [("", VocalMode::Vocal), ("invert\n", VocalMode::Inverted), ("I", VocalMode::Inverted), ("yes\n", VocalMode::Vocal), ("x i", VocalMode::Vocal)] {
            fs::write(pm.join("revival-vocal"), content).unwrap();
            assert_eq!(read_vocal(&root).unwrap().mode, want, "{content:?}");
        }
        // a card that already says so is not rewritten (a hand-made 'invert' file stays as it is)
        fs::write(pm.join("revival-vocal"), "invert\n").unwrap();
        write_vocal(&root, VocalMode::Inverted).unwrap();
        assert_eq!(fs::read(pm.join("revival-vocal")).unwrap(), b"invert\n");

        // the kill switch is reported, overrides the effective mode, and is never touched
        fs::write(pm.join("revival-no-preload"), b"keep me").unwrap();
        let s = read_vocal(&root).unwrap();
        assert_eq!((s.mode, s.kill_switch, s.effective), (VocalMode::Inverted, true, VocalMode::Off));
        for m in [VocalMode::Vocal, VocalMode::Inverted, VocalMode::Off, VocalMode::Vocal] {
            let s = write_vocal(&root, m).unwrap();
            assert!(s.kill_switch && s.effective == VocalMode::Off && s.mode == m);
        }
        assert_eq!(fs::read(pm.join("revival-no-preload")).unwrap(), b"keep me");

        // nothing else changed and no temp file was left
        assert_eq!(fs::read(pm.join("music.db")).unwrap(), b"db");
        assert_eq!(fs::read(pm.join("settings.dat")).unwrap(), b"settings");
        let mut names: Vec<String> = fs::read_dir(&pm).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["music.db", "revival-no-preload", "revival-vocal", "settings.dat"]);
    }

    #[test]
    fn vocal_flag_refuses_a_folder_that_is_not_a_card() {
        let root = scratch("vocal-nolib");
        assert_eq!(read_vocal(&root).unwrap().mode, VocalMode::Off);
        assert!(matches!(write_vocal(&root, VocalMode::Vocal), Err(Error::NotALibrary(_))));
        assert!(!root.join(".Pacemaker").exists(), "nothing may be created");
        // 'Off' on a non-card is refused too, rather than reporting success
        assert!(write_vocal(&root, VocalMode::Off).is_err());
    }
}
