use crate::onset::{ENVELOPE_RATE, Envelopes, detrend, mean};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BpmRange {
    pub min: f64,
    pub max: f64,
}

impl BpmRange {
    pub const AUTOMATIC: Self = Self {
        min: 110.0,
        max: 220.0,
    };

    pub fn from_mode(mode: u8) -> Self {
        let (min, max) = match mode {
            0 => (48.0, 95.0),
            1 => (58.0, 115.0),
            2 => (68.0, 135.0),
            3 => (78.0, 155.0),
            4 => (88.0, 175.0),
            5 => (98.0, 195.0),
            6 => (108.0, 215.0),
            7 => (118.0, 235.0),
            8 => (128.0, 255.0),
            9 => (110.0, 220.0),
            _ => (40.0, 300.0),
        };
        Self { min, max }
    }

    pub fn fold(self, mut bpm: f64) -> f64 {
        if bpm <= 0.0 || !bpm.is_finite() {
            return bpm;
        }
        while bpm < self.min {
            bpm *= 2.0;
        }
        while bpm >= self.max {
            bpm /= 2.0;
        }
        bpm
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Beatgrid {
    pub bpm: f64,
    pub anchor_s: f64,
}

impl Beatgrid {
    pub fn period_s(&self) -> f64 {
        60.0 / self.bpm
    }

    pub fn beats(&self, from_s: f64, to_s: f64) -> impl Iterator<Item = f64> + '_ {
        let period = self.period_s();
        let first = ((from_s - self.anchor_s) / period).ceil();
        (0..)
            .map(move |k| self.anchor_s + (first + f64::from(k)) * period)
            .take_while(move |&t| t < to_s)
    }
}

const WINDOW_S: f64 = 8.0;
const WINDOW_OVERLAP: f64 = 0.75;
const CLUSTER_TOLERANCE: f64 = 0.02;
const COARSE_STEP_BPM: f64 = 0.1;
const FINE_STEP_BPM: f64 = 0.01;
pub const PHASE_BINS: usize = 128;
const LAG_MULTIPLES: [(f64, f32); 3] = [(1.0, 1.0), (2.0, 0.5), (4.0, 0.25)];

pub fn estimate(env: &Envelopes, range: BpmRange) -> Option<Beatgrid> {
    let novelty = novelty(env);
    let duration_s = novelty.len() as f64 / ENVELOPE_RATE;
    if duration_s < 2.0 * 60.0 / range.min {
        return None;
    }
    let coarse = consensus(&window_votes(&novelty, range))?;
    let bpm = quantize(refine_tempo(&novelty, coarse));
    let anchor_s =
        (resolve_half_beat(env, phase(&novelty, bpm), bpm) - GRID_LEAD_S).rem_euclid(60.0 / bpm);
    Some(Beatgrid { bpm, anchor_s })
}

pub fn novelty(env: &Envelopes) -> Vec<f32> {
    let full_scale = 1.0 / mean(&env.full).max(f32::EPSILON);
    let bass_scale = 1.0 / mean(&env.bass).max(f32::EPSILON);
    let mixed: Vec<f32> = env
        .full
        .iter()
        .zip(&env.bass)
        .map(|(f, b)| f * full_scale + b * bass_scale)
        .collect();
    detrend(&mixed, (0.25 * ENVELOPE_RATE) as usize)
}

const GRID_LEAD_S: f64 = 0.0025;

const SNAP_TOLERANCE_BPM: f64 = 0.02;

fn quantize(bpm: f64) -> f64 {
    if (bpm - bpm.round()).abs() <= SNAP_TOLERANCE_BPM {
        bpm.round()
    } else {
        bpm
    }
}

const HALF_BEAT_WEIGHTS: [f32; 3] = [0.5, 1.0, -1.0];

fn resolve_half_beat(env: &Envelopes, anchor_s: f64, bpm: f64) -> f64 {
    let radius = (0.25 * ENVELOPE_RATE) as usize;
    let mid: Vec<f32> = env
        .full
        .iter()
        .zip(&env.bass)
        .zip(&env.treble)
        .map(|((f, b), t)| (f - b - t).max(0.0))
        .collect();
    let period_s = 60.0 / bpm;
    let period_frames = period_s * ENVELOPE_RATE;
    let mut on_beat = 0.0;
    let mut off_beat = 0.0;
    for (band, weight) in [&env.bass, &mid, &env.treble]
        .into_iter()
        .zip(HALF_BEAT_WEIGHTS)
    {
        let hist = phase_histogram(&detrend(band, radius), bpm);
        let total = hist.iter().sum::<f32>().max(f32::EPSILON);
        let at = |t: f64| {
            let n = hist.len();
            let bin = (Envelopes::index_of(t).rem_euclid(period_frames) / period_frames * n as f64)
                as usize
                % n;
            hist[bin] / total
        };
        on_beat += weight * at(anchor_s);
        off_beat += weight * at(anchor_s + period_s / 2.0);
    }
    if off_beat > on_beat {
        (anchor_s + period_s / 2.0).rem_euclid(period_s)
    } else {
        anchor_s
    }
}

fn window_votes(novelty: &[f32], range: BpmRange) -> Vec<(f64, f32)> {
    let window = (WINDOW_S * ENVELOPE_RATE) as usize;
    let hop = (WINDOW_S * (1.0 - WINDOW_OVERLAP) * ENVELOPE_RATE) as usize;
    let max_multiple = LAG_MULTIPLES.iter().map(|m| m.0).fold(1.0, f64::max);
    let max_lag = (max_multiple * 60.0 * ENVELOPE_RATE / range.min).ceil() as usize + 2;
    if novelty.len() < window.min(max_lag * 2) {
        return Vec::new();
    }
    let window = window.min(novelty.len());
    let candidates: Vec<f64> = candidate_grid(range.min, range.max, COARSE_STEP_BPM);

    let mut votes = Vec::new();
    let mut start = 0;
    while start + window <= novelty.len() {
        let acf = autocorrelation(&novelty[start..start + window], max_lag.min(window - 1));
        if acf[0] > 0.0 {
            let best = candidates
                .iter()
                .map(|&bpm| (bpm, lag_score(&acf, bpm)))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((bpm, score)) = best.filter(|b| b.1 > 0.0) {
                votes.push((bpm, score / acf[0]));
            }
        }
        start += hop;
    }
    votes
}

fn candidate_grid(from: f64, to: f64, step: f64) -> Vec<f64> {
    let n = ((to - from) / step).floor() as usize;
    (0..n).map(|i| from + i as f64 * step).collect()
}

fn autocorrelation(x: &[f32], max_lag: usize) -> Vec<f32> {
    let m = mean(x);
    let centred: Vec<f32> = x.iter().map(|v| v - m).collect();
    (0..=max_lag)
        .map(|lag| {
            centred[..centred.len() - lag]
                .iter()
                .zip(&centred[lag..])
                .map(|(a, b)| a * b)
                .sum()
        })
        .collect()
}

fn lag_score(acf: &[f32], bpm: f64) -> f32 {
    let beat = 60.0 * ENVELOPE_RATE / bpm;
    LAG_MULTIPLES
        .iter()
        .map(|&(multiple, weight)| weight * interpolate(acf, beat * multiple))
        .sum()
}

fn interpolate(x: &[f32], pos: f64) -> f32 {
    let i = pos.floor() as usize;
    if i + 1 >= x.len() {
        return 0.0;
    }
    let frac = (pos - pos.floor()) as f32;
    x[i] * (1.0 - frac) + x[i + 1] * frac
}

fn consensus(votes: &[(f64, f32)]) -> Option<f64> {
    let cluster = |centre: f64| {
        votes
            .iter()
            .filter(move |(bpm, _)| (bpm / centre - 1.0).abs() <= CLUSTER_TOLERANCE)
    };
    let (centre, _) = votes
        .iter()
        .map(|&(bpm, _)| (bpm, cluster(bpm).map(|v| v.1).sum::<f32>()))
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    let (sum, weight) = cluster(centre).fold((0.0, 0.0), |(s, w), &(bpm, wt)| {
        (s + bpm * f64::from(wt), w + f64::from(wt))
    });
    (weight > 0.0).then(|| sum / weight)
}

fn refine_tempo(novelty: &[f32], coarse: f64) -> f64 {
    candidate_grid(
        coarse * (1.0 - CLUSTER_TOLERANCE),
        coarse * (1.0 + CLUSTER_TOLERANCE),
        FINE_STEP_BPM,
    )
    .into_iter()
    .map(|bpm| (bpm, sharpness(&phase_histogram(novelty, bpm))))
    .max_by(|a, b| a.1.total_cmp(&b.1))
    .map_or(coarse, |(bpm, _)| bpm)
}

pub fn phase_histogram(novelty: &[f32], bpm: f64) -> Vec<f32> {
    let period = 60.0 * ENVELOPE_RATE / bpm;
    let scale = PHASE_BINS as f64 / period;
    let mut hist = vec![0.0f32; PHASE_BINS];
    for (i, &v) in novelty.iter().enumerate() {
        if v > 0.0 {
            let phase = (i as f64).rem_euclid(period) * scale;
            let bin = (phase as usize).min(PHASE_BINS - 1);
            hist[bin] += v;
        }
    }
    smooth_circular(&hist)
}

fn smooth_circular(hist: &[f32]) -> Vec<f32> {
    let n = hist.len();
    (0..n)
        .map(|i| 0.25 * hist[(i + n - 1) % n] + 0.5 * hist[i] + 0.25 * hist[(i + 1) % n])
        .collect()
}

fn sharpness(hist: &[f32]) -> f32 {
    let peak = hist.iter().copied().fold(0.0, f32::max);
    peak / mean(hist).max(f32::EPSILON)
}

pub fn phase(novelty: &[f32], bpm: f64) -> f64 {
    let hist = phase_histogram(novelty, bpm);
    let n = hist.len();
    let (peak, _) = hist
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .expect("histogram is never empty");
    let offset =
        crate::onset::parabolic_offset(hist[(peak + n - 1) % n], hist[peak], hist[(peak + 1) % n]);
    let period_frames = 60.0 * ENVELOPE_RATE / bpm;
    let phase_frames = (peak as f64 + 0.5 + offset) * period_frames / n as f64;
    Envelopes::time_of(phase_frames).rem_euclid(60.0 / bpm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onset::{envelopes, tests::kicks};

    fn kick_track(bpm: f64, first: f64, seconds: f64) -> Vec<f32> {
        let period = 60.0 / bpm;
        let count = ((seconds - 0.3 - first) / period).ceil() as u32;
        let onsets: Vec<f64> = (0..count).map(|k| first + f64::from(k) * period).collect();
        kicks(&onsets, seconds)
    }

    #[test]
    fn folds_into_range() {
        let r = BpmRange::AUTOMATIC;
        assert!((r.fold(65.0) - 130.0).abs() < 1e-9);
        assert!((r.fold(260.0) - 130.0).abs() < 1e-9);
        assert!((r.fold(220.0) - 110.0).abs() < 1e-9);
        assert!((quantize(127.987) - 128.0).abs() < 1e-12);
        assert!((quantize(127.97) - 127.97).abs() < 1e-12);
        assert_eq!(
            BpmRange::from_mode(42),
            BpmRange {
                min: 40.0,
                max: 300.0
            }
        );
        assert_eq!(BpmRange::from_mode(9), BpmRange::AUTOMATIC);
    }

    #[test]
    fn finds_tempo_and_phase_of_four_on_the_floor() {
        for (bpm, first) in [(128.0, 0.25), (136.0, 0.1), (174.0, 0.33), (122.5, 0.0)] {
            let grid = estimate(
                &envelopes(&kick_track(bpm, first, 60.0)),
                BpmRange::AUTOMATIC,
            )
            .expect("grid");
            assert!((grid.bpm - bpm).abs() < 0.05, "{bpm}: got {}", grid.bpm);
            let period = 60.0 / bpm;
            let expected = (first - GRID_LEAD_S).rem_euclid(period);
            let mut err = (grid.anchor_s - expected).rem_euclid(period);
            if err > period / 2.0 {
                err -= period;
            }
            assert!(
                err.abs() < 0.005,
                "{bpm}: anchor {} vs {expected}",
                grid.anchor_s
            );
        }
    }

    #[test]
    fn half_time_input_is_folded_up() {
        let grid = estimate(
            &envelopes(&kick_track(70.0, 0.2, 60.0)),
            BpmRange::AUTOMATIC,
        )
        .unwrap();
        assert!((grid.bpm - 140.0).abs() < 0.1, "got {}", grid.bpm);
    }

    #[test]
    fn silence_has_no_grid() {
        assert_eq!(
            estimate(&envelopes(&vec![0.0; 22_050 * 20]), BpmRange::AUTOMATIC),
            None
        );
    }

    #[test]
    fn beats_iterates_grid() {
        let grid = Beatgrid {
            bpm: 120.0,
            anchor_s: 0.1,
        };
        let beats: Vec<f64> = grid.beats(0.0, 2.0).collect();
        assert_eq!(beats.len(), 4);
        assert!((beats[3] - 1.6).abs() < 1e-9);
    }
}
