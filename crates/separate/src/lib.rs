mod chunks;
mod model;

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use backspn_analysis::signal::resample_stereo;
use backspn_decode::Track;
use ort::environment::Environment;
use ort::session::Session;
use ort::session::builder::AutoDevicePolicy;
use ort::value::Tensor;

pub use chunks::{OVERLAP, OverlapAdd, SEGMENT, segments, weight};
pub use model::{MODEL_FILE, MODEL_SHA256, MODEL_SIZE, MODEL_URL, download, verify};

pub const STEMS: usize = 4;
pub const MODEL_RATE: u32 = 44_100;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("separation model: {0}")]
    Model(#[from] ort::Error),
    #[error("model download failed: {0}")]
    Download(String),
    #[error("the downloaded model does not match its checksum")]
    Checksum,
    #[error("cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn environment() -> Result<Environment, Error> {
    static ENV: Mutex<Option<Environment>> = Mutex::new(None);
    let mut env = ENV
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if env.is_none() {
        *env = Some(ort::init().with_name("backspn").build()?);
    }
    Ok(env.clone().expect("set above"))
}

#[derive(Debug)]
pub struct Separator {
    session: Session,
}

impl Separator {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let env = environment()?;
        let session = Session::builder(&env)?
            .with_auto_device(AutoDevicePolicy::PreferCPU)
            .map_err(ort::Error::from)?
            .commit_from_file(path)?;
        Ok(Self { session })
    }

    pub fn separate(
        &mut self,
        mix: &[f32],
        progress: &mut dyn FnMut(f32),
        cancel: &AtomicBool,
    ) -> Result<[Vec<f32>; STEMS], Error> {
        let frames = mix.len() / 2;
        let parts = segments(frames);
        let mut sum = OverlapAdd::new(STEMS, 2, frames);
        let mut planar = vec![0.0f32; 2 * SEGMENT];
        for (n, seg) in parts.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            planar.fill(0.0);
            for i in 0..seg.len() {
                planar[i] = mix[(seg.start + i) * 2];
                planar[SEGMENT + i] = mix[(seg.start + i) * 2 + 1];
            }
            let input = Tensor::from_array(([1usize, 2, SEGMENT], planar.clone()))?;
            let outputs = self.session.run(ort::inputs!["mix" => input])?;
            let (_, stems) = outputs["stems"].try_extract_tensor::<f32>()?;
            sum.add(seg, |t, c, i| stems[(t * 2 + c) * SEGMENT + i]);
            progress((n + 1) as f32 / parts.len() as f32);
        }
        let tracks = sum.finish();
        Ok(tracks.try_into().expect("STEMS tracks"))
    }

    pub fn separate_track(
        &mut self,
        track: &Track,
        progress: &mut dyn FnMut(f32),
        cancel: &AtomicBool,
    ) -> Result<[Vec<f32>; STEMS], Error> {
        let rate = track.sample_rate();
        let mix = resample_stereo(track.samples(), rate, MODEL_RATE);
        let stems = self.separate(&mix, progress, cancel)?;
        Ok(stems.map(|s| {
            let mut back = resample_stereo(&s, MODEL_RATE, rate);
            back.resize(track.samples().len(), 0.0);
            back
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_with_the_real_model() {
        let Some(path) = std::env::var_os("BACKSPN_STEM_MODEL") else {
            return;
        };
        let mut sep = Separator::load(Path::new(&path)).unwrap();
        let frames = 3 * MODEL_RATE as usize;
        let mix: Vec<f32> = (0..frames)
            .flat_map(|i| {
                let x = 0.4 * (std::f32::consts::TAU * 55.0 * i as f32 / 44_100.0).sin();
                [x, x]
            })
            .collect();
        let stems = sep
            .separate(&mix, &mut |_| {}, &AtomicBool::new(false))
            .unwrap();
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
        assert!(stems.iter().all(|s| s.len() == mix.len()));
        assert!(
            rms(&stems[1]) > rms(&stems[3]),
            "bass {} vocals {}",
            rms(&stems[1]),
            rms(&stems[3])
        );
    }

    #[test]
    fn separated_stems_match_a_factory_stem_file() {
        let Some(model) = std::env::var_os("BACKSPN_STEM_MODEL") else {
            return;
        };
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
        let mix = backspn_decode::decode_file(&file).unwrap();
        let theirs = backspn_decode::decode_stems(&file).unwrap();
        let len = (20 * mix.sample_rate() as usize * 2).min(mix.samples().len());
        let part = Track::from_stereo(mix.sample_rate(), mix.samples()[..len].to_vec());
        let ours = Separator::load(Path::new(&model))
            .unwrap()
            .separate_track(&part, &mut |_| {}, &AtomicBool::new(false))
            .unwrap();
        let corr = |a: &[f32], b: &[f32]| {
            let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let n = |x: &[f32]| x.iter().map(|v| v * v).sum::<f32>().sqrt();
            dot / (n(a) * n(&b[..a.len()]) + 1e-9)
        };
        let drums = corr(&ours[0], &theirs[0].samples()[..len]);
        assert!(drums > 0.7, "drums {drums}");
        let sum: Vec<f32> = (0..len).map(|i| ours.iter().map(|s| s[i]).sum()).collect();
        let whole = corr(&sum, &mix.samples()[..len]);
        assert!(whole > 0.95, "sum {whole}");
    }
}
