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

/// Unpacks the `.dll` files that sit directly in `<top-folder>/<dir_prefix>` of the zip into
/// `out_dir` (flat). Everything else in the zip (headers, debug symbols, notes) is left. File
/// names are taken from the zip entry's last component only. Returns the names written.
pub fn unzip_dlls(zip_path: &Path, dir_prefix: &str, out_dir: &Path) -> Result<Vec<String>, String> {
    const MAX_ENTRY: u64 = 200 * 1024 * 1024;
    let file = std::fs::File::open(zip_path).map_err(|e| format!("cannot open {}: {e}", zip_path.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("not a readable zip: {e}"))?;
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let mut written = Vec::new();
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
        if entry.size() > MAX_ENTRY {
            return Err(format!("{file_name} in the zip is unexpectedly large; nothing was unpacked."));
        }
        let target = out_dir.join(file_name);
        let tmp = part_path(&target);
        let result = (|| -> std::io::Result<()> {
            let mut out = std::fs::File::create(&tmp)?;
            std::io::copy(&mut entry.by_ref().take(MAX_ENTRY), &mut out)?;
            out.sync_all()?;
            drop(out);
            std::fs::rename(&tmp, &target)
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("cannot unpack {file_name}: {e}"));
        }
        written.push(file_name.to_string());
    }
    if written.is_empty() {
        return Err("the zip does not contain the expected libraries".into());
    }
    Ok(written)
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
        assert_eq!(left, got, "no pdb, header, nested or escaping file, and no .part left");
        assert!(!dir.join("evil.dll").exists() && !out.join("..").join("evil.dll").exists());

        let e = unzip_dlls(&zp, "bin/", &dir.join("out2")).unwrap_err();
        assert!(e.contains("does not contain"), "{e}");
        std::fs::write(dir.join("junk.zip"), b"not a zip").unwrap();
        assert!(unzip_dlls(&dir.join("junk.zip"), "lib/", &out).unwrap_err().contains("zip"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
