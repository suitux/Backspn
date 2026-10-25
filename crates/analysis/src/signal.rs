use std::f64::consts::PI;

use backspn_decode::Track;

pub const ANALYSIS_RATE: u32 = 22_050;

const ZERO_CROSSINGS: usize = 12;
const PHASES: usize = 256;
const CUTOFF: f64 = 0.92;

pub fn mono_at_analysis_rate(track: &Track) -> Vec<f32> {
    let (frames, _) = track.samples().as_chunks::<2>();
    let mono: Vec<f32> = frames.iter().map(|&[l, r]| l.midpoint(r)).collect();
    resample(&mono, track.sample_rate(), ANALYSIS_RATE)
}

pub fn resample_stereo(stereo: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to {
        return stereo.to_vec();
    }
    let channel = |c: usize| -> Vec<f32> { stereo.iter().skip(c).step_by(2).copied().collect() };
    let (l, r) = (
        resample(&channel(0), from, to),
        resample(&channel(1), from, to),
    );
    l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect()
}

pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = f64::from(to) / f64::from(from);
    let cutoff = CUTOFF * ratio.min(1.0);
    let half_width = (ZERO_CROSSINGS as f64 / cutoff).ceil() as usize;
    let kernel = Kernel::new(half_width, cutoff);

    let out_len = (input.len() as f64 * ratio).floor() as usize;
    let half = isize::try_from(half_width).expect("kernel size fits isize");
    let len = isize::try_from(input.len()).expect("input size fits isize");
    (0..out_len)
        .map(|n| {
            let t = n as f64 / ratio;
            let base = t.floor();
            let phase = (((t - base) * PHASES as f64).round() as usize).min(PHASES);
            let taps = kernel.taps(phase);
            let start = base as isize - half + 1;
            let mut acc = 0.0f32;
            for (k, &w) in taps.iter().enumerate() {
                let i = start + k as isize;
                if (0..len).contains(&i) {
                    let x = input[i as usize];
                    acc += x * w;
                }
            }
            acc
        })
        .collect()
}

struct Kernel {
    width: usize,
    table: Vec<f32>,
}

impl Kernel {
    fn new(half_width: usize, cutoff: f64) -> Self {
        let width = 2 * half_width;
        let mut table = Vec::with_capacity((PHASES + 1) * width);
        for p in 0..=PHASES {
            let frac = p as f64 / PHASES as f64;
            for k in 0..width {
                let x = (k as f64 - half_width as f64 + 1.0) - frac;
                let window_pos = (x / half_width as f64).clamp(-1.0, 1.0);
                let window =
                    0.42 + 0.5 * (PI * window_pos).cos() + 0.08 * (2.0 * PI * window_pos).cos();
                table.push((cutoff * sinc(cutoff * x) * window) as f32);
            }
        }
        Self { width, table }
    }

    fn taps(&self, phase: usize) -> &[f32] {
        &self.table[phase * self.width..(phase + 1) * self.width]
    }
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: u32, seconds: f64) -> Vec<f32> {
        let n = (seconds * f64::from(rate)) as usize;
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f64 / f64::from(rate)).sin() as f32)
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn keeps_passband_and_length() {
        for from in [44_100, 48_000, 96_000] {
            let out = resample(&sine(1_000.0, from, 1.0), from, ANALYSIS_RATE);
            assert_eq!(out.len(), 22_050);
            let body = &out[1_000..21_000];
            let expected = sine(1_000.0, ANALYSIS_RATE, 1.0);
            let err: Vec<f32> = body
                .iter()
                .zip(&expected[1_000..21_000])
                .map(|(a, b)| a - b)
                .collect();
            assert!(rms(&err) < 0.01, "{from} Hz: rms error {}", rms(&err));
        }
    }

    #[test]
    fn rejects_aliases() {
        let out = resample(&sine(15_000.0, 44_100, 1.0), 44_100, ANALYSIS_RATE);
        assert!(rms(&out[1_000..21_000]) < 0.01);
    }

    #[test]
    fn identity_when_rates_match() {
        let x = vec![0.1, 0.2, 0.3];
        assert_eq!(resample(&x, 22_050, 22_050), x);
    }

    #[test]
    fn stereo_keeps_channels_apart_and_scales_length() {
        let stereo: Vec<f32> = (0..48_000).flat_map(|_| [0.5, -0.25]).collect();
        let out = resample_stereo(&stereo, 48_000, 44_100);
        let frames = out.len() / 2;
        assert!((frames as i64 - 44_100).abs() <= 2, "{frames}");
        let mid = frames / 2;
        assert!((out[mid * 2] - 0.5).abs() < 1e-3);
        assert!((out[mid * 2 + 1] + 0.25).abs() < 1e-3);
        assert_eq!(resample_stereo(&stereo, 44_100, 44_100), stereo);
    }
}
