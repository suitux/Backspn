use anyhow::{Context, bail, ensure};

const MAGIC: &[u8; 8] = b"BSPNKIT1";
const MAX_BYTES: u64 = 1 << 30;

pub type Files = Vec<(String, Vec<u8>)>;

pub fn pack(json: &str, files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    put_bytes(&mut out, json.as_bytes());
    out.extend_from_slice(&(files.len() as u32).to_le_bytes());
    for (name, data) in files {
        put_bytes(&mut out, name.as_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

pub fn unpack(bytes: &[u8]) -> anyhow::Result<(String, Files)> {
    let mut r = Reader { bytes, at: 0 };
    ensure!(r.take(MAGIC.len())? == MAGIC, "not a kit file");
    let json = r.string()?;
    let count = r.u32()?;
    let mut files = Vec::new();
    for _ in 0..count {
        let name = r.string()?;
        ensure!(is_plain_name(&name), "bad file name in kit: {name:?}");
        let len = r.u64()?;
        ensure!(len <= MAX_BYTES, "kit file too large");
        files.push((name, r.take(len as usize)?.to_vec()));
    }
    Ok((json, files))
}

pub fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && !name.starts_with('.')
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> anyhow::Result<&'a [u8]> {
        let end = self.at.checked_add(n).context("kit file damaged")?;
        if end > self.bytes.len() {
            bail!("kit file is cut short");
        }
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u32(&mut self) -> anyhow::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }

    fn u64(&mut self) -> anyhow::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into()?))
    }

    fn string(&mut self) -> anyhow::Result<String> {
        let len = self.u32()? as usize;
        Ok(String::from_utf8(self.take(len)?.to_vec())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let files = vec![
            ("kick.wav".to_owned(), vec![1, 2, 3]),
            ("snare.wav".to_owned(), vec![]),
        ];
        let bytes = pack("{\"name\":\"K\"}", &files);
        let (json, back) = unpack(&bytes).unwrap();
        assert_eq!(json, "{\"name\":\"K\"}");
        assert_eq!(back, files);
    }

    #[test]
    fn damaged_or_foreign_files_are_refused() {
        let bytes = pack("{}", &[("a.wav".to_owned(), vec![9; 10])]);
        assert!(unpack(&bytes[..bytes.len() - 1]).is_err(), "cut short");
        assert!(unpack(b"RIFF....").is_err(), "not a kit");
        let evil = pack("{}", &[("../evil.wav".to_owned(), vec![])]);
        assert!(unpack(&evil).is_err(), "names stay inside the kit");
    }

    #[test]
    fn plain_names() {
        assert!(is_plain_name("Kick 1.wav"));
        for bad in ["", "..", "a/b.wav", "a\\b", ".hidden"] {
            assert!(!is_plain_name(bad), "{bad}");
        }
    }
}
