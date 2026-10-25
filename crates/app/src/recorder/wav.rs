use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

const CHANNELS: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Format {
    Pcm16,
    #[default]
    Pcm24,
    Float32,
}

impl Format {
    pub const ALL: [Self; 3] = [Self::Pcm16, Self::Pcm24, Self::Float32];

    pub fn name(self) -> &'static str {
        match self {
            Self::Pcm16 => "16-bit",
            Self::Pcm24 => "24-bit",
            Self::Float32 => "32-bit float",
        }
    }

    fn bytes(self) -> u16 {
        match self {
            Self::Pcm16 => 2,
            Self::Pcm24 => 3,
            Self::Float32 => 4,
        }
    }

    fn float(self) -> bool {
        self == Self::Float32
    }

    fn header_len(self) -> u64 {
        if self.float() { 58 } else { 44 }
    }
}

pub fn header(format: Format, rate: u32, data: u32) -> Vec<u8> {
    let bytes = format.bytes();
    let block = CHANNELS * bytes;
    let mut h = Vec::with_capacity(58);
    let riff = data.saturating_add(u32::try_from(format.header_len()).unwrap_or(44) - 8);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&riff.to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&(if format.float() { 18u32 } else { 16 }).to_le_bytes());
    h.extend_from_slice(&(if format.float() { 3u16 } else { 1 }).to_le_bytes());
    h.extend_from_slice(&CHANNELS.to_le_bytes());
    h.extend_from_slice(&rate.to_le_bytes());
    h.extend_from_slice(&(rate * u32::from(block)).to_le_bytes());
    h.extend_from_slice(&block.to_le_bytes());
    h.extend_from_slice(&(bytes * 8).to_le_bytes());
    if format.float() {
        h.extend_from_slice(&0u16.to_le_bytes());
        h.extend_from_slice(b"fact");
        h.extend_from_slice(&4u32.to_le_bytes());
        h.extend_from_slice(&(data / u32::from(block)).to_le_bytes());
    }
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data.to_le_bytes());
    h
}

pub fn to_pcm(x: f32, bits: u32, dither: f32) -> i32 {
    let full = ((1i64 << (bits - 1)) - 1) as f32;
    (x.clamp(-1.0, 1.0) * full + dither)
        .round()
        .clamp(-full, full) as i32
}

struct Dither(u32);

impl Dither {
    fn uniform(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }

    fn next(&mut self) -> f32 {
        self.uniform() - self.uniform()
    }
}

pub(crate) struct WavWriter {
    file: BufWriter<File>,
    format: Format,
    rate: u32,
    data: u64,
    dither: Dither,
}

impl WavWriter {
    pub fn create(path: &Path, format: Format, rate: u32) -> std::io::Result<Self> {
        let mut file = BufWriter::new(File::create(path)?);
        file.write_all(&header(format, rate, 0))?;
        Ok(Self {
            file,
            format,
            rate,
            data: 0,
            dither: Dither(0x9E37_79B9),
        })
    }

    pub fn write(&mut self, samples: &[f32], gain: f32) -> std::io::Result<()> {
        for &x in samples {
            let x = x * gain;
            match self.format {
                Format::Pcm16 => {
                    let d = self.dither.next();
                    self.file
                        .write_all(&(to_pcm(x, 16, d) as i16).to_le_bytes())?;
                }
                Format::Pcm24 => self
                    .file
                    .write_all(&to_pcm(x, 24, 0.0).to_le_bytes()[..3])?,
                Format::Float32 => self.file.write_all(&x.to_le_bytes())?,
            }
        }
        self.data += u64::from(self.format.bytes()) * samples.len() as u64;
        Ok(())
    }

    pub fn data_bytes(&self) -> u64 {
        self.data
    }

    pub fn finish(mut self) -> std::io::Result<()> {
        let data = u32::try_from(self.data).unwrap_or(u32::MAX);
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header(self.format, self.rate, data))?;
        self.file
            .seek(SeekFrom::Start(self.format.header_len() + self.data))?;
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_decodes_back() {
        let dir = std::env::temp_dir().join(format!("backspn-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let samples: Vec<f32> = (0..2000).map(|i| ((i as f32) * 0.01).sin() * 0.5).collect();
        for (format, tolerance) in [
            (Format::Pcm16, 2e-4),
            (Format::Pcm24, 1e-6),
            (Format::Float32, 0.0),
        ] {
            let path = dir.join(format!("{format:?}.wav"));
            let mut w = WavWriter::create(&path, format, 44_100).unwrap();
            w.write(&samples, 1.5).unwrap();
            assert_eq!(w.data_bytes(), 2000 * u64::from(format.bytes()));
            w.finish().unwrap();
            let track = backspn_decode::decode_file(&path).unwrap();
            assert_eq!(track.sample_rate(), 44_100, "{format:?}");
            assert_eq!(track.frames(), 1000, "{format:?}");
            for (got, x) in track.samples().iter().zip(&samples) {
                assert!(
                    (got - x * 1.5).abs() <= tolerance,
                    "{format:?}: {got} vs {}",
                    x * 1.5
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pcm_clips_and_dither_stays_within_one_step() {
        assert_eq!(to_pcm(2.0, 16, 0.0), 32_767);
        assert_eq!(to_pcm(-2.0, 24, 0.0), -8_388_607);
        let mut d = Dither(1);
        let values: Vec<f32> = (0..10_000).map(|_| d.next()).collect();
        assert!(values.iter().all(|v| v.abs() < 1.0));
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        assert!(mean.abs() < 0.02, "centred: {mean}");
    }
}
