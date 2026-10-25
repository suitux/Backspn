use serde::{Deserialize, Serialize};

use crate::stem::StemInfo;

pub const EXTENSION: &str = "bstem";

pub const VERSION: u32 = 1;

const MAGIC: &[u8; 8] = b"BSPNSTEM";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub version: u32,
    pub rate: u32,
    pub frames: u64,
    pub pre_skip: u32,
    pub stems: Vec<StemInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StemFile {
    pub header: Header,
    pub packets: Vec<Vec<Vec<u8>>>,
}

impl StemFile {
    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        let json = serde_json::to_vec(&self.header).ok()?;
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&u32::try_from(json.len()).ok()?.to_le_bytes());
        out.extend_from_slice(&json);
        for stem in &self.packets {
            out.extend_from_slice(&u32::try_from(stem.len()).ok()?.to_le_bytes());
            for packet in stem {
                out.extend_from_slice(&u16::try_from(packet.len()).ok()?.to_le_bytes());
                out.extend_from_slice(packet);
            }
        }
        Some(out)
    }

    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let mut r = Reader { bytes, at: 0 };
        let header = r.header()?;
        let packets = (0..header.stems.len())
            .map(|_| {
                let count = r.u32()? as usize;
                (0..count)
                    .map(|_| {
                        let len = usize::from(r.u16()?);
                        r.take(len).map(<[u8]>::to_vec)
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self { header, packets })
    }
}

pub fn header(bytes: &[u8]) -> Option<Header> {
    Reader { bytes, at: 0 }.header()
}

pub fn is_own_stem_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let out = self.bytes.get(self.at..self.at.checked_add(len)?)?;
        self.at += len;
        Some(out)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn header(&mut self) -> Option<Header> {
        if self.take(MAGIC.len())? != MAGIC {
            return None;
        }
        let len = self.u32()? as usize;
        serde_json::from_slice(self.take(len)?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> StemFile {
        let info = |name: &str| StemInfo {
            name: name.into(),
            color: "#102030".into(),
        };
        StemFile {
            header: Header {
                version: VERSION,
                rate: 48_000,
                frames: 96_000,
                pre_skip: 312,
                stems: vec![info("Drums"), info("Bass"), info("Other"), info("Vocals")],
            },
            packets: vec![
                vec![vec![1, 2, 3], vec![]],
                vec![vec![4; 300]],
                vec![],
                vec![vec![5], vec![6], vec![7]],
            ],
        }
    }

    #[test]
    fn a_stem_file_reads_back_as_written() {
        let f = file();
        let bytes = f.to_bytes().unwrap();
        assert!(bytes.starts_with(MAGIC));
        assert_eq!(StemFile::parse(&bytes), Some(f.clone()));
        assert_eq!(header(&bytes), Some(f.header));
    }

    #[test]
    fn other_or_cut_files_are_not_read() {
        let bytes = file().to_bytes().unwrap();
        assert_eq!(StemFile::parse(&bytes[..bytes.len() - 1]), None);
        assert_eq!(header(&bytes[..10]), None);
        let mut other = bytes.clone();
        other[0] = b'X';
        assert_eq!(StemFile::parse(&other), None);
        assert_eq!(StemFile::parse(&[]), None);
    }

    #[test]
    fn a_packet_too_long_is_refused() {
        let mut f = file();
        f.packets[0].push(vec![0; 70_000]);
        assert_eq!(f.to_bytes(), None);
    }

    #[test]
    fn own_stem_files_go_by_their_extension() {
        assert!(is_own_stem_file(std::path::Path::new("/s/001/ABC.bstem")));
        assert!(is_own_stem_file(std::path::Path::new("ABC.BSTEM")));
        assert!(!is_own_stem_file(std::path::Path::new("ABC.stem.mp4")));
    }
}
