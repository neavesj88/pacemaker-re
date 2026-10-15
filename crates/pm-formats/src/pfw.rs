//! `.pfw` firmware container: a 50,000 KiB ext3 image plus a 512-byte trailer
//! (`ltibCreateDmgFile` in Tonium's GPL `pmib` tree). Each trailer field is a `createBlock`:
//! two uppercase hex digits of length, the value, then NUL padding. The padding region is
//! itself an empty block, and the final field is the MD5 of everything before it.
//! [`build`] reproduces all seven known firmware images byte-for-byte.

pub const TRAILER: usize = 512;
const FIELDS: [(&str, usize); 6] =
    [("ver", 4), ("tag", 128), ("swver", 32), ("fwver", 32), ("bootloaderver", 32), ("uid", 38)];
const MD5_FIELD: usize = 34;
const EMPTY: usize = TRAILER - 4 - 128 - 32 - 32 - 32 - 38 - MD5_FIELD;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trailer {
    pub ver: String,
    pub tag: String,
    pub sw_version: String,
    pub fw_version: String,
    pub bootloader_version: String,
    pub uid: String,
    pub md5: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    TooShort,
    BadField(&'static str),
}

fn block(value: &str, size: usize) -> Vec<u8> {
    let mut raw = format!("{:02X}{value}", value.len()).into_bytes();
    raw.resize(size.max(raw.len()), 0);
    raw
}

fn field(b: &[u8], name: &'static str) -> Result<String, Error> {
    let len = std::str::from_utf8(b.get(..2).ok_or(Error::BadField(name))?)
        .ok()
        .and_then(|h| usize::from_str_radix(h, 16).ok())
        .ok_or(Error::BadField(name))?;
    let v = b.get(2..2 + len).ok_or(Error::BadField(name))?;
    Ok(String::from_utf8_lossy(v).into_owned())
}

pub fn parse(file: &[u8]) -> Result<Trailer, Error> {
    if file.len() < TRAILER {
        return Err(Error::TooShort);
    }
    let t = &file[file.len() - TRAILER..];
    let mut vals = Vec::new();
    let mut o = 0;
    for (name, n) in FIELDS {
        vals.push(field(&t[o..o + n], name)?);
        o += n;
    }
    o += EMPTY;
    let md5 = field(&t[o..o + MD5_FIELD], "md5sum")?;
    let mut v = vals.into_iter();
    Ok(Trailer {
        ver: v.next().unwrap(),
        tag: v.next().unwrap(),
        sw_version: v.next().unwrap(),
        fw_version: v.next().unwrap(),
        bootloader_version: v.next().unwrap(),
        uid: v.next().unwrap(),
        md5,
    })
}

/// True if the stored MD5 matches the file contents.
pub fn verify(file: &[u8]) -> Result<bool, Error> {
    let t = parse(file)?;
    Ok(format!("{:x}", md5::compute(&file[..file.len() - MD5_FIELD])) == t.md5)
}

/// Builds a `.pfw` from an ext image and version fields, as Tonium's build script did.
pub fn build(image: &[u8], sw: &str, fw: &str, bootloader: &str, uid: &str) -> Vec<u8> {
    let vals = ["01", &format!("dfw_{sw}"), sw, fw, bootloader, uid];
    let mut body = image.to_vec();
    for ((_, size), v) in FIELDS.iter().zip(vals) {
        body.extend(block(v, *size));
    }
    body.extend(block("", EMPTY));
    let sum = format!("{:x}", md5::compute(&body));
    body.extend(block(&sum, MD5_FIELD));
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_parse_verify() {
        let image = vec![0u8; 4096];
        let f = build(&image, "180096069", "180096069", "12838", "904933c1-2816-468b-a1cf-07192083027a");
        assert_eq!(f.len(), image.len() + TRAILER);
        let t = parse(&f).unwrap();
        assert_eq!(t.tag, "dfw_180096069");
        assert_eq!(t.bootloader_version, "12838");
        assert!(verify(&f).unwrap());
        // 13-char tag must use an uppercase length, as dc printed it
        assert_eq!(&f[image.len() + 4..image.len() + 6], b"0D");
    }
}
