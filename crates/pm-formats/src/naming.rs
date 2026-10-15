//! Device file naming and `tracks.location`, as the original Editor does it.
//!
//! A track is copied to `.Pacemaker/Music/A/B/C.<ext>` and `location` becomes
//! `/pmdb_tracks/A/B/C.<ext>`. A and B are the last hex digit of [`oaat`] and [`elf`] over the
//! decimal file size, and C is the hex of [`oaat`] over the filename stem
//! (`osal::getEncodedFilePath`). The hashes are the decompiled variants, which use arithmetic
//! shifts, and the bytes are UTF-8 treated as signed chars. Verified against names the Windows
//! Editor generated.

/// Device-side prefix of `tracks.location` (`before_url_device`); `/pmdb_tracks` is a rootfs
/// mount point the device maps onto `.Pacemaker/Music`.
pub const LOCATION_PREFIX: &str = "/pmdb_tracks/";
pub const MUSIC_DIR: &str = ".Pacemaker/Music";

fn signed_bytes(text: &str) -> impl Iterator<Item = i32> + '_ {
    text.bytes().map(|b| b as i8 as i32)
}

/// One-at-a-time hash variant from `osal::getEncodedFilePath`.
pub fn oaat(text: &str) -> u32 {
    let mut h: u32 = 0;
    let mut any = false;
    for c in signed_bytes(text) {
        any = true;
        h = h.wrapping_add(c as u32).wrapping_mul(0x401);
        h ^= ((h as i32) >> 6) as u32;
    }
    let t = if any { h.wrapping_mul(8) } else { 0 };
    let u = t.wrapping_add(h);
    (((u as i32) >> 11) as u32 ^ u).wrapping_mul(0x8001)
}

/// ELF-hash variant from `osal::getEncodedFilePath`.
pub fn elf(text: &str) -> u32 {
    let mut h: u32 = 0;
    for c in signed_bytes(text) {
        h = h.wrapping_mul(16).wrapping_add(c as u32);
        let g = h & 0xF000_0000;
        if g != 0 {
            h ^= ((g as i32) >> 24) as u32;
        }
        h &= !g;
    }
    h
}

/// `A/B/C` for a filename stem and file size.
pub fn encoded_name(stem: &str, file_size: u64) -> String {
    let s = file_size.to_string();
    let last = |v: u32| format!("{v:x}").chars().last().unwrap();
    format!("{}/{}/{:x}", last(oaat(&s)), last(elf(&s)), oaat(stem))
}

/// `(path relative to .Pacemaker/Music, tracks.location)` for a source file name and size.
/// The extension keeps its original spelling.
pub fn device_paths(file_name: &str, file_size: u64) -> (String, String) {
    let (stem, ext) = match file_name.rfind('.') {
        Some(i) if i > 0 => (&file_name[..i], &file_name[i..]),
        _ => (file_name, ""),
    };
    let rel = format!("{}{}", encoded_name(stem, file_size), ext);
    let loc = format!("{LOCATION_PREFIX}{rel}");
    (rel, loc)
}

/// Sidecar path for an audio path: everything up to the last `.` plus `.str`.
pub fn str_path(audio_path: &str) -> String {
    match audio_path.rfind('.') {
        Some(i) => format!("{}.str", &audio_path[..i]),
        None => format!("{audio_path}.str"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_textbook_oaat_for_short_ascii() {
        // the signed-shift variant only diverges once the high bit is set
        assert_eq!(oaat("a"), 0xca2e9442);
        assert_eq!(oaat("ab"), 0x45e61e58);
    }

    #[test]
    fn editor_demo_tracks() {
        // names the Windows Editor 2.0.2.14170 produced for its bundled demo tracks
        assert_eq!(encoded_name("364185d56662beb9b3ccadb34d084c94", 6051328), "6/8/722ef9a6");
        assert_eq!(encoded_name("37ff932057f7d378d024d31cd1e31aca", 9599856), "9/6/7651d2ee");
        assert_eq!(encoded_name("5d75e1d191cd7277d86f007d86f257f6", 8015703), "a/3/bc8a130a");
    }

    #[test]
    fn location() {
        let (rel, loc) = device_paths("364185d56662beb9b3ccadb34d084c94.mp3", 6051328);
        assert_eq!(rel, "6/8/722ef9a6.mp3");
        assert_eq!(loc, "/pmdb_tracks/6/8/722ef9a6.mp3");
        assert_eq!(str_path("/x/6/8/722ef9a6.mp3"), "/x/6/8/722ef9a6.str");
    }
}
