use std::path::Path;

use backspn_analysis::signal::resample_stereo;
use backspn_formats::bstem::{Header, StemFile, VERSION};
use backspn_formats::stem::StemInfo;

pub const STEMS: usize = 4;

pub const RATE: u32 = 48_000;

pub const BIT_RATE: i32 = 160_000;

const FRAME: usize = 960;

const MAX_PACKET: usize = 4000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("stem tracks of different lengths")]
    Lengths,
    #[error("cannot encode the stems: {0}")]
    Encode(String),
    #[error("could not write the stems: {0}")]
    Write(#[from] std::io::Error),
}

pub fn generated_stems() -> Vec<StemInfo> {
    [
        ("Drums", "#FD6C38"),
        ("Bass", "#D232F4"),
        ("Other", "#00FFAC"),
        ("Vocals", "#45DAFD"),
    ]
    .map(|(name, color)| StemInfo {
        name: name.into(),
        color: color.into(),
    })
    .to_vec()
}

pub fn encode(
    rate: u32,
    stems: [&[f32]; STEMS],
    progress: &mut dyn FnMut(f32),
) -> Result<StemFile, Error> {
    let len = stems[0].len();
    if stems.iter().any(|s| s.len() != len) || !len.is_multiple_of(2) {
        return Err(Error::Lengths);
    }
    let fail = |e: opus::Error| Error::Encode(e.to_string());
    let mut pre_skip = 0;
    let mut frames = 0;
    let mut packets = Vec::with_capacity(STEMS);
    for (i, stem) in stems.into_iter().enumerate() {
        let mut encoder =
            opus::Encoder::new(RATE, opus::Channels::Stereo, opus::Application::Audio)
                .map_err(fail)?;
        encoder
            .set_bitrate(opus::Bitrate::Bits(BIT_RATE))
            .map_err(fail)?;
        let skip = usize::try_from(encoder.get_lookahead().map_err(fail)?).unwrap_or(0);
        let mut stereo = resample_stereo(stem, rate, RATE);
        frames = stereo.len() / 2;
        pre_skip = skip;
        stereo.resize(2 * (frames + skip).div_ceil(FRAME) * FRAME, 0.0);
        let stem_packets = stereo
            .as_chunks::<{ 2 * FRAME }>()
            .0
            .iter()
            .map(|frame| encoder.encode_vec_float(frame, MAX_PACKET).map_err(fail))
            .collect::<Result<Vec<_>, _>>()?;
        packets.push(stem_packets);
        progress((i + 1) as f32 / STEMS as f32);
    }
    Ok(StemFile {
        header: Header {
            version: VERSION,
            rate: RATE,
            frames: frames as u64,
            pre_skip: u32::try_from(pre_skip).unwrap_or(0),
            stems: generated_stems(),
        },
        packets,
    })
}

pub fn write(
    path: &Path,
    rate: u32,
    stems: [&[f32]; STEMS],
    progress: &mut dyn FnMut(f32),
) -> Result<(), Error> {
    let bytes = encode(rate, stems, progress)?
        .to_bytes()
        .ok_or_else(|| Error::Encode("a packet is too long".into()))?;
    std::fs::write(path, bytes)?;
    Ok(())
}
