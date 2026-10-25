use std::path::Path;

use backspn_formats::bstem::StemFile;

use crate::{Error, Track};

const MAX_FRAME: usize = 5760;

pub fn decode(path: &Path) -> Result<Vec<Track>, Error> {
    let bytes = std::fs::read(path).map_err(|source| Error::Open {
        path: path.to_owned(),
        source,
    })?;
    let file =
        StemFile::parse(&bytes).ok_or_else(|| Error::Unsupported("not a stem file".into()))?;
    let header = &file.header;
    let frames = usize::try_from(header.frames).map_err(|_| Error::Empty)?;
    let skip = header.pre_skip as usize;
    let opus = |e: opus::Error| Error::Unsupported(e.to_string());
    file.packets
        .iter()
        .map(|packets| {
            let mut decoder =
                opus::Decoder::new(header.rate, opus::Channels::Stereo).map_err(opus)?;
            let mut stereo = Vec::with_capacity(2 * (frames + skip));
            let mut buffer = vec![0.0; 2 * MAX_FRAME];
            for packet in packets {
                let n = decoder
                    .decode_float(packet, &mut buffer, false)
                    .map_err(opus)?;
                stereo.extend_from_slice(&buffer[..2 * n]);
            }
            let mut stereo = stereo.split_off((2 * skip).min(stereo.len()));
            stereo.resize(2 * frames, 0.0);
            Ok(Track::from_stereo(header.rate, stereo))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use backspn_formats::bstem::{Header, VERSION};
    use backspn_formats::stem::StemInfo;

    fn silent(path: &Path, stems: usize, frames: usize) {
        let mut encoder =
            opus::Encoder::new(48_000, opus::Channels::Stereo, opus::Application::Audio).unwrap();
        let skip = usize::try_from(encoder.get_lookahead().unwrap()).unwrap();
        let input = vec![0.0; 2 * 960];
        let packets: Vec<Vec<u8>> = (0..(frames + skip).div_ceil(960))
            .map(|_| encoder.encode_vec_float(&input, 4000).unwrap())
            .collect();
        let info = StemInfo {
            name: "Stem".into(),
            color: "#000000".into(),
        };
        let file = StemFile {
            header: Header {
                version: VERSION,
                rate: 48_000,
                frames: frames as u64,
                pre_skip: skip as u32,
                stems: vec![info; stems],
            },
            packets: vec![packets; stems],
        };
        std::fs::write(path, file.to_bytes().unwrap()).unwrap();
    }

    #[test]
    fn stems_decode_to_the_stored_length() {
        let dir = std::env::temp_dir().join(format!("backspn-bstem-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.bstem");
        silent(&path, 4, 10_000);
        let stems = decode(&path).unwrap();
        assert_eq!(stems.len(), 4);
        for s in &stems {
            assert_eq!(s.sample_rate(), 48_000);
            assert_eq!(s.frames(), 10_000);
        }
        std::fs::write(&path, b"BSPNSTEM").unwrap();
        assert!(matches!(decode(&path), Err(Error::Unsupported(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
