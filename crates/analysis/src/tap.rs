use crate::onset::Transient;

const SEARCH: [(f32, usize); 3] = [(25.0, 501), (25.0 * 0.05, 101), (25.0 * 0.002, 101)];
const BINS: usize = 128;
const KERNEL: i32 = 5;
const TAP_WINDOW: f32 = 4.0;
const REFINE_WINDOW: f32 = 64.0;
const BEATS_PER_BAR: i32 = 4;

pub fn tapped_grid(transients: &[Transient], seed_bpm: f32, taps: &[f32]) -> (f32, f32) {
    let mut bpm = seed_bpm;
    for (range, steps) in SEARCH {
        bpm = scan_tempo(transients, bpm, range, steps);
    }
    let period = 60.0 / bpm;
    let first_guess = period * densest_phase(transients, period);
    (bpm, tapped_phase(transients, bpm, first_guess, taps))
}

fn phases(transients: &[Transient], period: f32) -> Vec<f32> {
    transients
        .iter()
        .map(|t| (t.position_s % f64::from(period)) as f32 / period)
        .collect()
}

fn scan_tempo(transients: &[Transient], center: f32, range: f32, steps: usize) -> f32 {
    let low = center - range;
    let step = (center + range - low) / if steps == 1 { 1.0 } else { (steps - 1) as f32 };
    let n = transients.len();
    let ramp = if n == 1 { 1.0 } else { 1.0 / (n as f32 - 1.0) };
    let mut best = center;
    let mut best_spread = 0.0_f32;
    for k in 0..steps {
        let bpm = step.mul_add(k as f32, low);
        let spread = if n == 0 {
            f32::NAN
        } else {
            let mut p = phases(transients, 60.0 / bpm);
            p.sort_by(f32::total_cmp);
            for (i, x) in p.iter_mut().enumerate() {
                *x -= i as f32 * ramp;
            }
            let mean = p.iter().sum::<f32>() / n as f32;
            let sum: f32 = p.iter().map(|x| (x - mean) * (x - mean)).sum();
            sum / (n as f32 - 1.0).max(f32::EPSILON)
        };
        if spread > best_spread {
            best = bpm;
        }
        best_spread = best_spread.max(spread);
    }
    best
}

fn densest_phase(transients: &[Transient], period: f32) -> f32 {
    let mut p = phases(transients, period);
    p.sort_by(f32::total_cmp);
    let n = p.len();
    let (eighth, sixteenth) = (n >> 3, n >> 4);
    p.extend_from_within(..eighth);
    let mut best = 1e7_f32;
    let mut phase = 0.0;
    for i in 0..n {
        let width = p[i + eighth] - p[i];
        if width < best {
            phase = p[i + sixteenth] % 1.0;
            best = width;
        }
    }
    phase
}

fn coherence(transients: &[Transient], period: f32) -> [f32; BINS] {
    let mut hist = [0.0; BINS];
    let bins = BINS as i32;
    for t in transients {
        let phase = (t.position_s as f32 % period) / period;
        let bin = ((phase * BINS as f32).floor() as i32).clamp(0, bins - 1);
        for offset in -KERNEL..=KERNEL {
            let gain = 1.0 - offset.abs() as f32 / KERNEL as f32;
            hist[(bin + offset).rem_euclid(bins) as usize] += t.weight * gain;
        }
    }
    hist
}

fn fold(mut d: f32) -> f32 {
    if d < -0.5 {
        d += 1.0;
    }
    if d >= 0.5 {
        d -= 1.0;
    }
    d
}

fn tapped_phase(transients: &[Transient], bpm: f32, fallback: f32, taps: &[f32]) -> f32 {
    let period = 60.0 / bpm;
    let Some(&first) = taps.first() else {
        return fallback;
    };
    let mut expected = first;
    let mut offset = 0.0;
    for &tap in &taps[1..] {
        expected += period;
        offset += tap - expected;
    }
    let reference = first + offset / taps.len() as f32;
    let mut tapped = reference % period;
    if tapped < 0.0 {
        tapped += period;
    }
    let beat_of_bar = ((first / period).floor() as i32) % BEATS_PER_BAR;
    let mut bin = (tapped / period * BINS as f32) as i32;
    let hist = coherence(transients, period);
    let mut best_score = 0.0;
    let mut best_bin = None;
    for (k, &weight) in hist.iter().enumerate() {
        let d = fold(bin as f32 / BINS as f32);
        let window = (1.0 - (d * TAP_WINDOW).abs()).max(0.0);
        let score = window * window * weight;
        if score > best_score {
            best_score = score;
            best_bin = Some(k);
        }
        bin -= 1;
    }
    let mut phase = match best_bin {
        Some(k) => period * (k as f32 / BINS as f32 + beat_of_bar as f32),
        None => fallback,
    };
    if phase >= 0.0 && bpm > 0.0 {
        phase = period * refine(transients, period, phase / period);
    }
    phase
}

fn refine(transients: &[Transient], period: f32, beats: f32) -> f32 {
    let (mut sum, mut total) = (0.0_f32, 0.0_f32);
    for t in transients {
        let d = fold((t.position_s as f32 % period) / period - beats);
        let window = 1.0 - (d.abs() * REFINE_WINDOW).min(1.0);
        let w = window * window;
        sum += d * w;
        total += w;
    }
    if total > 0.0 {
        beats + sum / total
    } else {
        beats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beats(bpm: f64, first_s: f64, seconds: f64) -> Vec<Transient> {
        let period = 60.0 / bpm;
        let mut out = Vec::new();
        let mut t = first_s;
        while t < seconds {
            out.push(Transient {
                position_s: t,
                weight: 1.0,
                bass_weight: 1.0,
            });
            out.push(Transient {
                position_s: t + period / 2.0,
                weight: 0.3,
                bass_weight: 0.0,
            });
            t += period;
        }
        out
    }

    #[test]
    fn finds_the_tempo_near_the_taps() {
        let t = beats(126.0, 0.2, 120.0);
        assert!((scan_tempo(&t, 120.0, 25.0, 501) - 126.0).abs() < 0.1);
        let (bpm, _) = tapped_grid(&t, 121.0, &[10.0, 10.5, 11.0, 11.5]);
        assert!((bpm - 126.0).abs() < 0.02, "{bpm}");
    }

    #[test]
    fn the_phase_follows_the_beats_and_the_bar_of_the_first_tap() {
        let t = beats(120.0, 0.2, 120.0);
        let taps = [10.23, 10.72, 11.24, 11.71];
        let (bpm, phase) = tapped_grid(&t, 120.0, &taps);
        assert!((bpm - 120.0).abs() < 0.02);
        assert!((phase - 0.2).abs() < 0.005, "{phase}");
        let taps = [10.73, 11.22, 11.74, 12.21];
        let (_, phase) = tapped_grid(&t, 120.0, &taps);
        assert!((phase - 0.7).abs() < 0.005, "{phase}");
    }

    #[test]
    fn without_transients_the_taps_tempo_stays() {
        let (bpm, phase) = tapped_grid(&[], 123.0, &[1.0, 1.5, 2.0, 2.5]);
        assert!((bpm - 123.0).abs() < 1e-3);
        assert_eq!(phase, 0.0);
    }

    #[test]
    fn densest_phase_and_fold() {
        assert!((fold(0.7) + 0.3).abs() < 1e-6);
        assert!((fold(-0.6) - 0.4).abs() < 1e-6);
        assert_eq!(fold(0.2), 0.2);
        let on_beats: Vec<Transient> = beats(120.0, 0.1, 60.0)
            .into_iter()
            .filter(|t| t.weight > 0.5)
            .collect();
        assert!((densest_phase(&on_beats, 0.5) - 0.2).abs() < 0.01);
    }
}
