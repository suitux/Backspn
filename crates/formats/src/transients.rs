use std::path::Path;

use crate::Error;

pub const SUPPORTED_VERSION: u8 = 5;

const HEADER_LEN: usize = 5;
const RECORD_LEN: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transient {
    pub position_s: f64,
    pub weight: f32,
    pub bass_weight: f32,
}

pub fn parse(bytes: &[u8]) -> Result<Vec<Transient>, Error> {
    let header: &[u8; HEADER_LEN] = bytes.first_chunk().ok_or_else(|| {
        bad(format!(
            "file is {} bytes, shorter than the header",
            bytes.len()
        ))
    })?;
    let version = header[0];
    if version != SUPPORTED_VERSION {
        return Err(bad(format!("unsupported version {version}")));
    }
    let count = u32::from_le_bytes([header[1], header[2], header[3], header[4]]) as usize;
    let body = &bytes[HEADER_LEN..];
    if Some(body.len()) != count.checked_mul(RECORD_LEN) {
        return Err(bad(format!(
            "header says {count} records but body is {} bytes",
            body.len()
        )));
    }
    let (records, _) = body.as_chunks::<RECORD_LEN>();
    Ok(records
        .iter()
        .map(|r| Transient {
            position_s: f64::from_le_bytes(std::array::from_fn(|i| r[i])),
            weight: f32::from_le_bytes(std::array::from_fn(|i| r[8 + i])),
            bass_weight: f32::from_le_bytes(std::array::from_fn(|i| r[12 + i])),
        })
        .collect())
}

pub fn encode(transients: &[Transient]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + RECORD_LEN * transients.len());
    out.push(SUPPORTED_VERSION);
    out.extend_from_slice(
        &u32::try_from(transients.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    for t in transients.iter().take(u32::MAX as usize) {
        out.extend_from_slice(&t.position_s.to_le_bytes());
        out.extend_from_slice(&t.weight.to_le_bytes());
        out.extend_from_slice(&t.bass_weight.to_le_bytes());
    }
    out
}

pub fn from_path(path: impl AsRef<Path>) -> Result<Vec<Transient>, Error> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    parse(&bytes)
}

fn bad(message: String) -> Error {
    Error::Invalid {
        element: "transients".into(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_round_trips() {
        let transients = [
            Transient {
                position_s: 0.5,
                weight: 1.5,
                bass_weight: 200.0,
            },
            Transient {
                position_s: 12.25,
                weight: 0.1,
                bass_weight: 0.0,
            },
        ];
        let bytes = encode(&transients);
        assert_eq!(bytes.len(), 5 + 16 * 2);
        assert_eq!(parse(&bytes).unwrap(), transients);
        assert_eq!(parse(&encode(&[])).unwrap(), []);
    }

    fn raw(items: &[(f64, f32, f32)]) -> Vec<u8> {
        let transients: Vec<Transient> = items
            .iter()
            .map(|&(position_s, weight, bass_weight)| Transient {
                position_s,
                weight,
                bass_weight,
            })
            .collect();
        encode(&transients)
    }

    #[test]
    fn parses_records() {
        let t = parse(&raw(&[(0.0058, 0.96, 230.4), (0.338, 0.25, 0.0)])).unwrap();
        assert_eq!(t.len(), 2);
        assert!((t[0].position_s - 0.0058).abs() < f64::EPSILON);
        assert!((t[1].weight - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn empty_file_with_zero_records() {
        assert_eq!(parse(&raw(&[])).unwrap(), Vec::new());
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse(&[]).is_err());
        let mut wrong_version = raw(&[]);
        wrong_version[0] = 4;
        assert!(parse(&wrong_version).is_err());
        let mut truncated = raw(&[(1.0, 1.0, 1.0)]);
        truncated.pop();
        assert!(parse(&truncated).is_err());
    }
}
