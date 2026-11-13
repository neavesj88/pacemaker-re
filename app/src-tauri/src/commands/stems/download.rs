//! The one place the app touches the network: fetching a pinned file after the user consented.
//!
//! `fetch` writes `<dest>.part`, hashes while writing, and only after the size and SHA-256 match
//! the pinned values renames it to `dest`. On any failure, and on cancel, the `.part` file is
//! removed and `dest` is untouched. The caller (never this module) decides that consent was given.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// Removes the partial file unless the download completed.
struct PartGuard {
    path: PathBuf,
    keep: bool,
}

impl Drop for PartGuard {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// https always; plain http only to this machine (the tests' local server).
fn allowed_url(url: &str) -> bool {
    if url.starts_with("https://") {
        return true;
    }
    ["http://127.0.0.1", "http://localhost", "http://[::1]"].iter().any(|p| url.strip_prefix(p).is_some_and(|rest| rest.starts_with(':') || rest.starts_with('/')))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of a file, as lower-case hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

/// Downloads `url` to `dest`. `progress(done, total)` is called after every chunk (the caller
/// throttles); `cancel` is checked between chunks. `expect_size` of 0 skips the size check.
pub fn fetch(url: &str, expect_sha256: &str, expect_size: u64, dest: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
    if !allowed_url(url) {
        return Err(format!("{url} is not an https address; nothing was downloaded."));
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let config = ureq::Agent::config_builder()
        // checked at every redirect hop too: an https address must not lead to plain http (only
        // the tests' loopback server is plain http)
        .https_only(url.starts_with("https://"))
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .user_agent("pacemaker-revival")
        .build();
    let agent: ureq::Agent = config.into();
    // identity: the pinned size and hash are of the file as served, not of a compressed copy of it
    let mut resp = agent.get(url).header("Accept-Encoding", "identity").call().map_err(|e| format!("download failed: {e}"))?;
    let total = resp.body().content_length().unwrap_or(expect_size);
    if expect_size > 0 && total != expect_size {
        return Err(format!("the server announced {total} bytes but {expect_size} were expected; nothing was kept."));
    }
    let part = part_path(dest);
    let mut guard = PartGuard { path: part.clone(), keep: false };
    let mut out = std::fs::File::create(&part).map_err(|e| format!("cannot write {}: {e}", part.display()))?;
    let mut reader = resp.body_mut().as_reader();
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".into());
        }
        let n = reader.read(&mut buf).map_err(|e| format!("download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| format!("cannot write {}: {e}", part.display()))?;
        done += n as u64;
        if expect_size > 0 && done > expect_size {
            return Err(format!("the server sent more than the {expect_size} bytes expected; nothing was kept."));
        }
        progress(done, total);
    }
    out.flush().map_err(|e| e.to_string())?;
    out.sync_all().map_err(|e| e.to_string())?;
    drop(out);
    if expect_size > 0 && done != expect_size {
        return Err(format!("received {done} bytes of {expect_size}; nothing was kept."));
    }
    let got = hex(&hasher.finalize());
    if !got.eq_ignore_ascii_case(expect_sha256) {
        return Err(format!("the SHA-256 of the download ({}...) does not match the pinned value ({}...); the file was deleted.", &got[..12], &expect_sha256[..12.min(expect_sha256.len())]));
    }
    std::fs::rename(&part, dest).map_err(|e| format!("cannot move the download into place: {e}"))?;
    guard.keep = true;
    Ok(())
}

/// Lists the unpacked libraries and their SHA-256, written LAST by [`unzip_dlls`]: a folder
/// without it is an unpack that did not finish.
pub const UNPACK_MANIFEST: &str = "unpacked.sha256";

/// Unpacks the `.dll` files that sit directly in `<top-folder>/<dir_prefix>` of the zip into
/// `out_dir` (flat). Everything else in the zip (headers, debug symbols, notes) is left. File
/// names are taken from the zip entry's last component only. Returns the names written.
///
/// All or nothing: the libraries are written and hashed in a staging folder, moved into place
/// only when every one of them was written, and [`UNPACK_MANIFEST`] (name and SHA-256 of each)
/// goes in last. If anything fails, what was already moved is removed again, so no half-installed
/// runtime is left that looks installed.
pub fn unzip_dlls(zip_path: &Path, dir_prefix: &str, out_dir: &Path) -> Result<Vec<String>, String> {
    const MAX_ENTRY: u64 = 200 * 1024 * 1024;
    let file = std::fs::File::open(zip_path).map_err(|e| format!("cannot open {}: {e}", zip_path.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("not a readable zip: {e}"))?;
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    // a re-download starts from nothing: the old manifest must not vouch for new files
    let _ = std::fs::remove_file(out_dir.join(UNPACK_MANIFEST));
    let stage = out_dir.join("unpack.tmp");
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let staged = stage_dlls(&mut zip, dir_prefix, &stage, MAX_ENTRY);
    let result = staged.and_then(|files| install_staged(&stage, out_dir, &files));
    let _ = std::fs::remove_dir_all(&stage);
    result
}

/// Writes the wanted libraries into `stage`; returns (name, sha256) of each.
fn stage_dlls(zip: &mut zip::ZipArchive<std::fs::File>, dir_prefix: &str, stage: &Path, max_entry: u64) -> Result<Vec<(String, String)>, String> {
    let mut files: Vec<(String, String)> = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("zip entry {i}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let Ok(name) = entry.name().map(|n| n.replace('\\', "/")) else { continue };
        // <top>/<dir_prefix><file>.dll
        let Some((top, rest)) = name.split_once('/') else { continue };
        let Some(file_name) = rest.strip_prefix(dir_prefix) else { continue };
        let safe = !top.is_empty() && top != ".." && !file_name.is_empty() && !file_name.contains(['/', '\\', ':', '\0']) && file_name != ".." && file_name.to_ascii_lowercase().ends_with(".dll");
        if !safe {
            continue;
        }
        if entry.size() > max_entry {
            return Err(format!("{file_name} in the zip is unexpectedly large; nothing was unpacked."));
        }
        let target = stage.join(file_name);
        let mut out = std::fs::File::create(&target).map_err(|e| format!("cannot unpack {file_name}: {e}"))?;
        let mut hasher = Sha256::new();
        let mut reader = entry.by_ref().take(max_entry);
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = reader.read(&mut buf).map_err(|e| format!("cannot unpack {file_name}: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(|e| format!("cannot unpack {file_name}: {e}"))?;
        }
        out.sync_all().map_err(|e| format!("cannot unpack {file_name}: {e}"))?;
        files.push((file_name.to_string(), hex(&hasher.finalize())));
    }
    if files.is_empty() {
        return Err("the zip does not contain the expected libraries".into());
    }
    Ok(files)
}

/// Moves the staged libraries into `out_dir`, then writes the manifest. On any failure the
/// libraries already moved are removed again.
fn install_staged(stage: &Path, out_dir: &Path, files: &[(String, String)]) -> Result<Vec<String>, String> {
    let mut moved: Vec<PathBuf> = Vec::new();
    let undo = |moved: &[PathBuf]| {
        for m in moved {
            let _ = std::fs::remove_file(m);
        }
    };
    for (name, _) in files {
        let target = out_dir.join(name);
        if let Err(e) = std::fs::rename(stage.join(name), &target) {
            undo(&moved);
            return Err(format!("cannot unpack {name}: {e}"));
        }
        moved.push(target);
    }
    let manifest: String = files.iter().map(|(n, h)| format!("{h}  {n}\n")).collect();
    let tmp = out_dir.join(format!("{UNPACK_MANIFEST}.part"));
    let written = std::fs::write(&tmp, manifest).and_then(|()| std::fs::rename(&tmp, out_dir.join(UNPACK_MANIFEST)));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        undo(&moved);
        return Err(format!("cannot record the unpacked libraries: {e}"));
    }
    Ok(files.iter().map(|(n, _)| n.clone()).collect())
}

/// The libraries listed in a folder's [`UNPACK_MANIFEST`] as (name, sha256); `None` when there is
/// no manifest or it is not in the format [`unzip_dlls`] writes (no unpack finished here).
fn read_unpack_manifest(dir: &Path) -> Option<Vec<(String, String)>> {
    let text = std::fs::read_to_string(dir.join(UNPACK_MANIFEST)).ok()?;
    let mut out = Vec::new();
    for line in text.lines() {
        let (hash, name) = line.split_once("  ")?;
        let plain = !name.is_empty() && !name.contains(['/', '\\', ':', '\0']) && name != "..";
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) || !plain {
            return None;
        }
        out.push((name.to_string(), hash.to_ascii_lowercase()));
    }
    (!out.is_empty()).then_some(out)
}

/// Whether an unpack finished in `dir`: the manifest is there and every library it lists exists.
/// Cheap (no hashing); [`verify_unpacked`] is the check made before loading.
pub fn unpacked_complete(dir: &Path) -> bool {
    read_unpack_manifest(dir).is_some_and(|files| files.iter().all(|(n, _)| dir.join(n).is_file()))
}

/// Hashes every library of an unpack again and compares with the manifest written at unpack time.
/// Run before the libraries are loaded as native code: the folder is user-writable.
pub fn verify_unpacked(dir: &Path) -> Result<(), String> {
    let files = read_unpack_manifest(dir).ok_or("The downloaded ONNX Runtime has no record of its files. Remove it in Settings and download it again.")?;
    for (name, want) in files {
        let got = sha256_file(&dir.join(&name)).map_err(|e| format!("Cannot read {name} of the downloaded ONNX Runtime: {e}"))?;
        if got != want {
            return Err(format!("{name} of the downloaded ONNX Runtime has changed since it was unpacked. Remove it in Settings and download it again."));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::Arc;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-stems-dl-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Serves `body` once on a free local port, in 64 KiB pieces. `status` is the whole status line text.
    fn serve(body: Vec<u8>, status: &'static str, announce: Option<usize>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut r = BufReader::new(sock.try_clone().unwrap());
                let mut line = String::new();
                while r.read_line(&mut line).unwrap_or(0) > 2 {
                    line.clear();
                }
                let len = announce.unwrap_or(body.len());
                let _ = write!(sock, "HTTP/1.1 {status}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n");
                for chunk in body.chunks(65_536) {
                    if sock.write_all(chunk).is_err() {
                        break;
                    }
                    let _ = sock.flush();
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        });
        format!("http://127.0.0.1:{port}/file.bin")
    }

    fn sha(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    fn payload(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 31 % 251) as u8).collect()
    }

    #[test]
    fn a_good_download_lands_under_its_final_name_with_progress() {
        let dir = temp_dir("ok");
        let body = payload(300_000);
        let url = serve(body.clone(), "200 OK", None);
        let dest = dir.join("sub").join("m.bin");
        let mut seen = Vec::new();
        fetch(&url, &sha(&body), body.len() as u64, &dest, &AtomicBool::new(false), &mut |d, t| seen.push((d, t))).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert!(!part_path(&dest).exists());
        assert_eq!(seen.last(), Some(&(300_000, 300_000)));
        assert!(seen.windows(2).all(|w| w[0].0 < w[1].0), "progress only moves forward");
        assert_eq!(sha256_file(&dest).unwrap(), sha(&body));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_wrong_checksum_deletes_the_file_and_keeps_the_old_one() {
        let dir = temp_dir("sha");
        let body = payload(100_000);
        let url = serve(body.clone(), "200 OK", None);
        let dest = dir.join("m.bin");
        std::fs::write(&dest, b"previous").unwrap();
        let wrong = sha(b"something else");
        let e = fetch(&url, &wrong, body.len() as u64, &dest, &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
        assert!(e.contains("does not match") && e.contains("deleted"), "{e}");
        assert_eq!(std::fs::read(&dest).unwrap(), b"previous");
        assert!(!part_path(&dest).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn size_mismatch_error_status_and_other_schemes_are_refused() {
        let dir = temp_dir("bad");
        let dest = dir.join("m.bin");
        let body = payload(50_000);
        // the server announces a different size than pinned
        let url = serve(body.clone(), "200 OK", None);
        let e = fetch(&url, &sha(&body), 49_999, &dest, &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
        assert!(e.contains("expected"), "{e}");
        // a 404
        let url = serve(b"nope".to_vec(), "404 Not Found", None);
        let e = fetch(&url, &sha(&body), 50_000, &dest, &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
        assert!(e.contains("download failed"), "{e}");
        // the connection drops before the announced length
        let url = serve(body[..20_000].to_vec(), "200 OK", Some(50_000));
        let e = fetch(&url, &sha(&body), 50_000, &dest, &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
        assert!(e.contains("interrupted") || e.contains("received"), "{e}");
        // not https, not loopback
        for u in ["http://example.com/x", "ftp://example.com/x", "file:///C:/x", "http://127.0.0.1.evil.example/x", "http://localhostile/x"] {
            let e = fetch(u, &sha(&body), 1, &dest, &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
            assert!(e.contains("not an https address"), "{u}: {e}");
        }
        assert!(!dest.exists() && !part_path(&dest).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cancel_stops_mid_download_and_leaves_nothing() {
        let dir = temp_dir("cancel");
        let body = payload(2_000_000);
        let url = serve(body.clone(), "200 OK", None);
        let dest = dir.join("m.bin");
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        let e = fetch(&url, &sha(&body), body.len() as u64, &dest, &cancel, &mut |d, _| {
            if d > 300_000 {
                c2.store(true, Ordering::SeqCst);
            }
        })
        .unwrap_err();
        assert_eq!(e, "cancelled");
        assert!(!dest.exists() && !part_path(&dest).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn make_zip(path: &Path) {
        let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in [
            ("ort-1/lib/onnxruntime.dll", &b"DLL"[..]),
            ("ort-1/lib/onnxruntime_providers_shared.dll", b"SHARED"),
            ("ort-1/lib/onnxruntime.pdb", b"symbols"),
            ("ort-1/lib/sub/deep.dll", b"nested"),
            ("ort-1/include/a.h", b"header"),
            ("../evil.dll", b"escape"),
            ("ort-1/lib/..\\evil2.dll", b"escape too"),
            ("ort-1/libx/other.dll", b"wrong folder"),
        ] {
            z.start_file(name, opts).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn only_the_libraries_in_the_named_folder_are_unpacked() {
        let dir = temp_dir("zip");
        let zp = dir.join("ort.zip");
        make_zip(&zp);
        let out = dir.join("out");
        let mut got = unzip_dlls(&zp, "lib/", &out).unwrap();
        got.sort();
        assert_eq!(got, vec!["onnxruntime.dll", "onnxruntime_providers_shared.dll"]);
        assert_eq!(std::fs::read(out.join("onnxruntime.dll")).unwrap(), b"DLL");
        let mut left: Vec<_> = std::fs::read_dir(&out).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        let mut want = got.clone();
        want.push(UNPACK_MANIFEST.to_string());
        want.sort();
        assert_eq!(left, want, "no pdb, header, nested or escaping file, no staging folder and no .part left; the manifest is there");
        assert!(unpacked_complete(&out));
        verify_unpacked(&out).unwrap();
        assert!(!dir.join("evil.dll").exists() && !out.join("..").join("evil.dll").exists());

        let e = unzip_dlls(&zp, "bin/", &dir.join("out2")).unwrap_err();
        assert!(e.contains("does not contain"), "{e}");
        std::fs::write(dir.join("junk.zip"), b"not a zip").unwrap();
        assert!(unzip_dlls(&dir.join("junk.zip"), "lib/", &out).unwrap_err().contains("zip"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_runtime_that_was_changed_or_half_unpacked_is_not_trusted() {
        let dir = temp_dir("verify");
        let zp = dir.join("ort.zip");
        make_zip(&zp);
        let out = dir.join("out");
        unzip_dlls(&zp, "lib/", &out).unwrap();
        verify_unpacked(&out).unwrap();
        // a library swapped after the unpack
        std::fs::write(out.join("onnxruntime_providers_shared.dll"), b"TAMPERED").unwrap();
        assert!(verify_unpacked(&out).unwrap_err().contains("has changed"));
        // a library gone: not complete
        std::fs::remove_file(out.join("onnxruntime.dll")).unwrap();
        assert!(!unpacked_complete(&out));
        // a failed re-unpack leaves no manifest and no half set behind
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join(UNPACK_MANIFEST), "stale").unwrap();
        std::fs::create_dir_all(out.join("onnxruntime_providers_shared.dll")).unwrap(); // a folder in the way: that move fails
        assert!(unzip_dlls(&zp, "lib/", &out).is_err());
        assert!(!out.join(UNPACK_MANIFEST).exists() && !out.join("onnxruntime.dll").exists() && !out.join("unpack.tmp").exists());
        assert!(!unpacked_complete(&out));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn plain_http_is_refused_unless_it_is_this_machine() {
        let e = fetch("http://example.invalid/x", &"0".repeat(64), 0, &temp_dir("http").join("x"), &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
        assert!(e.contains("not an https address"), "{e}");
        assert!(allowed_url("http://127.0.0.1:8080/x") && !allowed_url("http://127.0.0.1.evil.test/x"));
    }
}
