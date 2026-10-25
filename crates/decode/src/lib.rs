mod bstem;

use std::fs::File;
use std::path::{Path, PathBuf};

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot open {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("unsupported or unrecognised audio: {0}")]
    Unsupported(String),
    #[error("decoding failed: {0}")]
    Decode(#[from] SymphoniaError),
    #[error("file contains no decodable audio")]
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    sample_rate: u32,
    samples: Vec<f32>,
}

impl Track {
    pub fn from_stereo(sample_rate: u32, samples: Vec<f32>) -> Self {
        assert!(sample_rate > 0, "sample rate must be positive");
        assert!(
            samples.len().is_multiple_of(2),
            "stereo data must have an even length"
        );
        Self {
            sample_rate,
            samples,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    pub fn duration_s(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate)
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    #[inline]
    pub fn frame(&self, index: isize) -> [f32; 2] {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.samples.get(2 * i..2 * i + 2))
            .map_or([0.0; 2], |s| [s[0], s[1]])
    }
}

pub fn decode_file(path: impl AsRef<Path>) -> Result<Track, Error> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| Error::Open {
        path: path.to_owned(),
        source,
    })?;
    let extension = path.extension().and_then(|e| e.to_str());
    decode_source(Box::new(file), extension)
}

pub fn cover_art(path: impl AsRef<Path>) -> Option<Vec<u8>> {
    use symphonia::core::meta::StandardVisualKey;
    let path = path.as_ref();
    let file = File::open(path).ok()?;
    let extension = path.extension().and_then(|e| e.to_str());
    let mut format = probe(Box::new(file), extension).ok()?;
    let mut metadata = format.metadata();
    let revision = metadata.skip_to_latest()?;
    let visuals = &revision.media.visuals;
    visuals
        .iter()
        .find(|v| v.usage == Some(StandardVisualKey::FrontCover))
        .or_else(|| visuals.first())
        .map(|v| v.data.to_vec())
}

fn probe(
    source: Box<dyn MediaSource>,
    extension: Option<&str>,
) -> symphonia::core::errors::Result<Box<dyn symphonia::core::formats::FormatReader>> {
    let mut hint = Hint::new();
    if let Some(ext) = extension {
        hint.with_extension(ext);
    }
    let stream = MediaSourceStream::new(source, MediaSourceStreamOptions::default());
    symphonia::default::get_probe().probe(
        &hint,
        stream,
        FormatOptions::default(),
        MetadataOptions::default(),
    )
}

pub fn decode_source(
    source: Box<dyn MediaSource>,
    extension: Option<&str>,
) -> Result<Track, Error> {
    let mut format = probe(source, extension).map_err(|e| Error::Unsupported(e.to_string()))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| Error::Unsupported("no audio track".into()))?
        .id;
    let mut tracks = decode_tracks(format.as_mut(), &[track])?;
    tracks.pop().ok_or(Error::Empty)
}

pub fn decode_stems(path: impl AsRef<Path>) -> Result<Vec<Track>, Error> {
    let path = path.as_ref();
    if backspn_formats::bstem::is_own_stem_file(path) {
        return bstem::decode(path);
    }
    let file = File::open(path).map_err(|source| Error::Open {
        path: path.to_owned(),
        source,
    })?;
    let extension = path.extension().and_then(|e| e.to_str());
    let mut format =
        probe(Box::new(file), extension).map_err(|e| Error::Unsupported(e.to_string()))?;
    let audio: Vec<u32> = format
        .tracks()
        .iter()
        .filter(|t| t.codec_params.as_ref().and_then(|p| p.audio()).is_some())
        .map(|t| t.id)
        .collect();
    if audio.len() < 2 {
        return Err(Error::Unsupported("not a stem file".into()));
    }
    decode_tracks(format.as_mut(), &audio[1..])
}

fn decode_tracks(
    format: &mut dyn symphonia::core::formats::FormatReader,
    ids: &[u32],
) -> Result<Vec<Track>, Error> {
    struct Stream {
        id: u32,
        params: symphonia::core::codecs::audio::AudioCodecParameters,
        decoder: Box<dyn symphonia::core::codecs::audio::AudioDecoder>,
        sample_rate: Option<u32>,
        stereo: Vec<f32>,
    }
    let codecs = symphonia::default::get_codecs();
    let options = AudioDecoderOptions::default();
    let mut streams = Vec::with_capacity(ids.len());
    for &id in ids {
        let params = format
            .tracks()
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| t.codec_params.as_ref())
            .and_then(|p| p.audio())
            .ok_or_else(|| Error::Unsupported("track has no audio codec parameters".into()))?
            .clone();
        let decoder = codecs
            .make_audio_decoder(&params, &options)
            .map_err(|e| Error::Unsupported(e.to_string()))?;
        streams.push(Stream {
            id,
            sample_rate: params.sample_rate,
            params,
            decoder,
            stereo: Vec::new(),
        });
    }
    let mut packet_samples: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::IoError(_)) if streams.iter().any(|s| !s.stereo.is_empty()) => {
                break;
            }
            Err(e) => return Err(e.into()),
        };
        let Some(stream) = streams.iter_mut().find(|s| s.id == packet.track_id) else {
            continue;
        };
        let buffer = match stream.decoder.decode(&packet) {
            Ok(buffer) => buffer,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::ResetRequired) => {
                stream.decoder = codecs
                    .make_audio_decoder(&stream.params, &options)
                    .map_err(|e| Error::Unsupported(e.to_string()))?;
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        let spec = buffer.spec();
        stream.sample_rate.get_or_insert(spec.rate());
        let channels = spec.channels().count();
        if channels == 0 {
            continue;
        }
        buffer.copy_to_vec_interleaved(&mut packet_samples);
        append_as_stereo(&mut stream.stereo, &packet_samples, channels);
    }
    streams
        .into_iter()
        .map(|s| {
            let sample_rate = s.sample_rate.ok_or(Error::Empty)?;
            if s.stereo.is_empty() || sample_rate == 0 {
                return Err(Error::Empty);
            }
            Ok(Track::from_stereo(sample_rate, s.stereo))
        })
        .collect()
}

fn append_as_stereo(out: &mut Vec<f32>, interleaved: &[f32], channels: usize) {
    out.reserve(interleaved.len() / channels * 2);
    for frame in interleaved.chunks_exact(channels) {
        let (l, r) = if channels == 1 {
            (frame[0], frame[0])
        } else {
            (frame[0], frame[1])
        };
        out.push(l);
        out.push(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn wav(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let data_len = u32::try_from(samples.len() * 2).unwrap();
        let block_align = channels * 2;
        let mut out = Vec::new();
        out.extend(b"RIFF");
        out.extend((36 + data_len).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(sample_rate.to_le_bytes());
        out.extend((sample_rate * u32::from(block_align)).to_le_bytes());
        out.extend(block_align.to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend(data_len.to_le_bytes());
        for s in samples {
            out.extend(s.to_le_bytes());
        }
        out
    }

    fn decode_bytes(bytes: Vec<u8>) -> Result<Track, Error> {
        decode_source(Box::new(Cursor::new(bytes)), Some("wav"))
    }

    #[test]
    fn decodes_stereo_wav() {
        let track = decode_bytes(wav(44_100, 2, &[16_384, -16_384, 0, 8_192])).unwrap();
        assert_eq!(track.sample_rate(), 44_100);
        assert_eq!(track.frames(), 2);
        assert_eq!(track.frame(0), [0.5, -0.5]);
        assert_eq!(track.frame(1), [0.0, 0.25]);
        assert_eq!(track.frame(2), [0.0, 0.0]);
        assert_eq!(track.frame(-1), [0.0, 0.0]);
    }

    #[test]
    fn duplicates_mono() {
        let track = decode_bytes(wav(48_000, 1, &[16_384, -8_192])).unwrap();
        assert_eq!(track.sample_rate(), 48_000);
        assert_eq!(track.samples(), [0.5, 0.5, -0.25, -0.25]);
    }

    #[test]
    fn keeps_first_two_of_many_channels() {
        let track = decode_bytes(wav(44_100, 3, &[16_384, 8_192, 4_096])).unwrap();
        assert_eq!(track.samples(), [0.5, 0.25]);
    }

    #[test]
    fn duration() {
        let track = Track::from_stereo(4, vec![0.0; 16]);
        assert!((track.duration_s() - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode_bytes(b"definitely not audio".to_vec()).is_err());
    }

    #[test]
    fn decodes_the_four_stems_of_a_stem_file() {
        let Some(dir) = std::env::var_os("BACKSPN_STEM_FILES") else {
            return;
        };
        let dir = std::path::Path::new(&dir);
        let Some(file) = std::fs::read_dir(dir).ok().and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.to_string_lossy().ends_with(".stem.mp4"))
        }) else {
            return;
        };
        let stems = decode_stems(&file).unwrap();
        let mix = decode_file(&file).unwrap();
        assert_eq!(stems.len(), 4);
        for stem in &stems {
            assert_eq!(stem.sample_rate(), mix.sample_rate());
            assert_eq!(stem.frames(), mix.frames(), "{}", file.display());
        }
    }

    #[test]
    fn a_plain_file_is_not_a_stem_file() {
        let dir = std::env::temp_dir().join(format!("backspn-decode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.wav");
        std::fs::write(&path, wav(44_100, 2, &[0, 0, 1, 1])).unwrap();
        assert!(matches!(decode_stems(&path), Err(Error::Unsupported(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
