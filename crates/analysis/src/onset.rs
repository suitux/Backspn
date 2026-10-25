use crate::signal::ANALYSIS_RATE;
use crate::stft::Stft;

pub const FRAME: usize = 1024;
pub const HOP: usize = 128;
pub const ENVELOPE_RATE: f64 = ANALYSIS_RATE as f64 / HOP as f64;
const DIFF_LAG: usize = 2;
const ONSET_DELAY: usize = FRAME / 2 - HOP * DIFF_LAG / 2 + 352;
const BASS_HZ: f64 = 150.0;
const TREBLE_HZ: f64 = 5_000.0;
const GAMMA: f32 = 100.0;

#[derive(Debug, Clone, Default)]
pub struct Envelopes {
    pub full: Vec<f32>,
    pub bass: Vec<f32>,
    pub treble: Vec<f32>,
}

impl Envelopes {
    pub fn time_of(i: f64) -> f64 {
        let offset = ONSET_DELAY as f64;
        (i * HOP as f64 + offset) / f64::from(ANALYSIS_RATE)
    }

    pub fn index_of(t: f64) -> f64 {
        let offset = ONSET_DELAY as f64;
        (t * f64::from(ANALYSIS_RATE) - offset) / HOP as f64
    }
}

pub fn envelopes(signal: &[f32]) -> Envelopes {
    if signal.len() < FRAME {
        return Envelopes::default();
    }
    let frames = (signal.len() - FRAME) / HOP + 1;
    let mut stft = Stft::hann(FRAME);
    let bins = FRAME / 2 + 1;
    let bass_bins = (BASS_HZ * FRAME as f64 / f64::from(ANALYSIS_RATE)).ceil() as usize + 1;
    let treble_bin = (TREBLE_HZ * FRAME as f64 / f64::from(ANALYSIS_RATE)).floor() as usize;

    let mut history: Vec<Vec<f32>> = vec![vec![0.0; bins]; DIFF_LAG + 1];
    let mut full = Vec::with_capacity(frames);
    let mut bass = Vec::with_capacity(frames);
    let mut treble = Vec::with_capacity(frames);
    for f in 0..frames {
        let output = stft.spectrum(&signal[f * HOP..f * HOP + FRAME]);
        let current = &mut history[f % (DIFF_LAG + 1)];
        for (m, c) in current.iter_mut().zip(output) {
            *m = (1.0 + GAMMA * c.norm()).ln();
        }
        let current = &history[f % (DIFF_LAG + 1)];
        let previous = &history[(f + 1) % (DIFF_LAG + 1)];
        let mut total = 0.0;
        let mut low = 0.0;
        let mut high = 0.0;
        for (k, (c, p)) in current.iter().zip(previous).enumerate() {
            let rise = (c - p).max(0.0);
            total += rise;
            if k < bass_bins {
                low += rise;
            } else if k >= treble_bin {
                high += rise;
            }
        }
        if f < DIFF_LAG {
            total = 0.0;
            low = 0.0;
            high = 0.0;
        }
        full.push(total);
        bass.push(low);
        treble.push(high);
    }
    Envelopes { full, bass, treble }
}

pub fn detrend(x: &[f32], radius: usize) -> Vec<f32> {
    let n = x.len();
    let mut prefix = vec![0.0f64; n + 1];
    for (i, v) in x.iter().enumerate() {
        prefix[i + 1] = prefix[i] + f64::from(*v);
    }
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(radius);
            let hi = (i + radius + 1).min(n);
            let mean = (prefix[hi] - prefix[lo]) / (hi - lo) as f64;
            (x[i] - mean as f32).max(0.0)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transient {
    pub position_s: f64,
    pub weight: f32,
    pub bass_weight: f32,
}

pub fn transients(env: &Envelopes) -> Vec<Transient> {
    let radius = (0.05 * ENVELOPE_RATE).round() as usize;
    let trend = (0.5 * ENVELOPE_RATE).round() as usize;
    let context = ENVELOPE_RATE.round() as usize;
    let peaks = detrend(&env.full, trend);
    let bass = detrend(&env.bass, trend);
    let loudest = moving_max(&peaks, context);
    let floor =
        (0.5 * mean(&peaks)).max(MIN_GLOBAL_WEIGHT * peaks.iter().copied().fold(0.0, f32::max));
    let n = peaks.len();
    let mut out = Vec::new();
    for i in 1..n.saturating_sub(1) {
        let v = peaks[i];
        if v <= floor || v < MIN_RELATIVE_WEIGHT * loudest[i] {
            continue;
        }
        let lo = i.saturating_sub(radius);
        let hi = (i + radius + 1).min(n);
        if peaks[lo..i].iter().any(|&p| p >= v) || peaks[i + 1..hi].iter().any(|&p| p > v) {
            continue;
        }
        let offset = parabolic_offset(peaks[i - 1], v, peaks[i + 1]);
        let position_s = Envelopes::time_of(i as f64 + offset);
        out.push(Transient {
            position_s: position_s.max(0.0),
            weight: v,
            bass_weight: bass[i],
        });
    }
    out
}

const MIN_RELATIVE_WEIGHT: f32 = 0.25;
const MIN_GLOBAL_WEIGHT: f32 = 0.1;

fn moving_max(x: &[f32], radius: usize) -> Vec<f32> {
    let n = x.len();
    let mut out = Vec::with_capacity(n);
    let mut deque = std::collections::VecDeque::new();
    let mut next = 0;
    for i in 0..n {
        let hi = (i + radius).min(n - 1);
        while next <= hi {
            while deque.back().is_some_and(|&b| x[b] <= x[next]) {
                deque.pop_back();
            }
            deque.push_back(next);
            next += 1;
        }
        while deque.front().is_some_and(|&f| f + radius < i) {
            deque.pop_front();
        }
        out.push(x[deque[0]]);
    }
    out
}

pub fn parabolic_offset(left: f32, centre: f32, right: f32) -> f64 {
    let denom = left - 2.0 * centre + right;
    if denom.abs() < f32::EPSILON {
        0.0
    } else {
        f64::from((0.5 * (left - right) / denom).clamp(-0.5, 0.5))
    }
}

pub(crate) fn mean(x: &[f32]) -> f32 {
    if x.is_empty() {
        0.0
    } else {
        let n = x.len() as f32;
        x.iter().sum::<f32>() / n
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn kicks(onsets: &[f64], seconds: f64) -> Vec<f32> {
        let rate = f64::from(ANALYSIS_RATE);
        let mut x = vec![0.0f32; (seconds * rate) as usize];
        let mut seed = 1u32;
        for v in &mut x {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *v = (f64::from(seed >> 8) / f64::from(1u32 << 24) - 0.5) as f32 * 0.01;
        }
        for &t in onsets {
            let start = (t * rate).round() as usize;
            for i in 0..(0.2 * rate) as usize {
                if let Some(v) = x.get_mut(start + i) {
                    let s = i as f64 / rate;
                    *v +=
                        ((2.0 * std::f64::consts::PI * 60.0 * s).sin() * (-s * 25.0).exp()) as f32;
                }
            }
        }
        x
    }

    #[test]
    fn finds_kicks_with_millisecond_precision() {
        let onsets: Vec<f64> = (0..20).map(|i| 0.3 + f64::from(i) * 0.5).collect();
        let found = transients(&envelopes(&kicks(&onsets, 11.0)));
        assert_eq!(found.len(), onsets.len(), "{found:?}");
        let bias: f64 = onsets
            .iter()
            .zip(&found)
            .map(|(t, f)| f.position_s - t)
            .sum::<f64>()
            / 20.0;
        eprintln!("mean timing bias {:.2} ms", bias * 1e3);
        for (t, f) in onsets.iter().zip(&found) {
            assert!(
                (f.position_s - t).abs() < 0.002,
                "expected {t}, got {}",
                f.position_s
            );
            assert!(f.bass_weight > 0.0);
        }
    }

    #[test]
    fn moving_max_matches_naive() {
        let x: Vec<f32> = (0..50u16).map(|i| f32::from((i * 37) % 23)).collect();
        for radius in [0, 1, 3, 7] {
            let fast = moving_max(&x, radius);
            for (i, &got) in fast.iter().enumerate() {
                let lo = i.saturating_sub(radius);
                let hi = (i + radius + 1).min(x.len());
                let naive = x[lo..hi].iter().copied().fold(f32::MIN, f32::max);
                assert_eq!(got, naive, "radius {radius} at {i}");
            }
        }
    }

    #[test]
    fn index_and_time_are_inverse() {
        let t = Envelopes::time_of(123.4);
        assert!((Envelopes::index_of(t) - 123.4).abs() < 1e-9);
    }

    #[test]
    fn silence_has_no_transients() {
        assert_eq!(transients(&envelopes(&vec![0.0; 22_050])), Vec::new());
        assert_eq!(envelopes(&[0.0; 10]).full, Vec::<f32>::new());
    }
}
