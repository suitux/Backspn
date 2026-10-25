use crate::signal::ANALYSIS_RATE;
use crate::stft::Stft;

const FRAME: usize = 8192;
const HOP: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub tonic: u8,
    pub minor: bool,
}

impl Key {
    pub fn nml_value(self) -> u8 {
        self.tonic + if self.minor { 12 } else { 0 }
    }

    pub fn from_nml_value(value: u8) -> Option<Self> {
        (value < 24).then_some(Self {
            tonic: value % 12,
            minor: value >= 12,
        })
    }

    #[must_use]
    pub fn relative(self) -> Self {
        if self.minor {
            Self {
                tonic: (self.tonic + 3) % 12,
                minor: false,
            }
        } else {
            Self {
                tonic: (self.tonic + 9) % 12,
                minor: true,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Profile {
    Krumhansl,
    Temperley,
    #[default]
    Learned,
}

impl Profile {
    pub fn weights(self) -> ([f32; 12], [f32; 12]) {
        match self {
            Self::Krumhansl => (
                [
                    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
                ],
                [
                    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
                ],
            ),
            Self::Temperley | Self::Learned => (
                [
                    0.748, 0.060, 0.488, 0.082, 0.670, 0.460, 0.096, 0.715, 0.104, 0.366, 0.057,
                    0.400,
                ],
                [
                    0.712, 0.084, 0.474, 0.618, 0.049, 0.460, 0.105, 0.747, 0.404, 0.067, 0.133,
                    0.330,
                ],
            ),
        }
    }
}

pub const LOWEST_MIDI: u8 = 33;
pub const SEMITONES: usize = 72;

pub fn semitone_spectrogram(signal: &[f32]) -> Vec<[f32; SEMITONES]> {
    if signal.len() < FRAME {
        return Vec::new();
    }
    let mut stft = Stft::hann(FRAME);
    let mut mapping = Vec::new();
    for bin in 1..FRAME / 2 {
        let hz = bin as f64 * f64::from(ANALYSIS_RATE) / FRAME as f64;
        let pos = 69.0 + 12.0 * (hz / 440.0).log2() - f64::from(LOWEST_MIDI);
        let lower = pos.floor();
        let frac = (pos - lower) as f32;
        for (index, weight) in [(lower, 1.0 - frac), (lower + 1.0, frac)] {
            if index >= 0.0 && index < SEMITONES as f64 {
                mapping.push((bin, index as usize, weight));
            }
        }
    }

    let mut rows = Vec::with_capacity(signal.len() / HOP);
    let mut start = 0;
    while start + FRAME <= signal.len() {
        let output = stft.spectrum(&signal[start..start + FRAME]);
        let mut row = [0.0f32; SEMITONES];
        for &(bin, semitone, w) in &mapping {
            row[semitone] += w * output[bin].norm();
        }
        rows.push(row);
        start += HOP;
    }
    rows
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromaOptions {
    pub from: usize,
    pub to: usize,
    pub log_gain: f32,
    pub frame_norm: bool,
    pub whiten: bool,
    pub median_radius: usize,
}

impl Default for ChromaOptions {
    fn default() -> Self {
        Self {
            from: 0,
            to: 48,
            log_gain: 0.0,
            frame_norm: true,
            whiten: false,
            median_radius: 0,
        }
    }
}

pub fn chroma_from(spectrogram: &[[f32; SEMITONES]], options: &ChromaOptions) -> [f32; 12] {
    let mut chroma = [0.0f32; 12];
    let to = options.to.min(SEMITONES);
    let filtered;
    let spectrogram = if options.median_radius > 0 {
        filtered = median_filter(spectrogram, options.median_radius);
        &filtered[..]
    } else {
        spectrogram
    };
    for row in spectrogram {
        let whitened;
        let row = if options.whiten {
            whitened = whiten(row);
            &whitened
        } else {
            row
        };
        let mut frame = [0.0f32; 12];
        for (i, &m) in row.iter().enumerate().take(to).skip(options.from) {
            let v = if options.log_gain > 0.0 {
                (1.0 + options.log_gain * m).ln()
            } else {
                m
            };
            frame[(usize::from(LOWEST_MIDI) + i) % 12] += v;
        }
        let total: f32 = frame.iter().sum();
        if total <= f32::EPSILON {
            continue;
        }
        let scale = if options.frame_norm { 1.0 / total } else { 1.0 };
        for (c, f) in chroma.iter_mut().zip(frame) {
            *c += f * scale;
        }
    }
    chroma
}

fn median_filter(rows: &[[f32; SEMITONES]], radius: usize) -> Vec<[f32; SEMITONES]> {
    let mut window = Vec::with_capacity(2 * radius + 1);
    (0..rows.len())
        .map(|t| {
            let lo = t.saturating_sub(radius);
            let hi = (t + radius + 1).min(rows.len());
            std::array::from_fn(|i| {
                window.clear();
                window.extend(rows[lo..hi].iter().map(|r| r[i]));
                window.sort_by(f32::total_cmp);
                window[window.len() / 2]
            })
        })
        .collect()
}

fn whiten(row: &[f32; SEMITONES]) -> [f32; SEMITONES] {
    std::array::from_fn(|i| {
        let lo = i.saturating_sub(6);
        let hi = (i + 7).min(SEMITONES);
        let mean = row[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
        (row[i] - mean).max(0.0)
    })
}

pub fn chroma(signal: &[f32]) -> [f32; 12] {
    chroma_from(&semitone_spectrogram(signal), &ChromaOptions::default())
}

pub fn estimate(chroma: &[f32; 12], profile: Profile) -> Option<(Key, f32)> {
    estimate_with(chroma, &profile.weights())
}

pub fn estimate_with(
    chroma: &[f32; 12],
    (major, minor): &([f32; 12], [f32; 12]),
) -> Option<(Key, f32)> {
    if chroma.iter().all(|&c| c <= 0.0) {
        return None;
    }
    let mut best: Option<(Key, f32)> = None;
    for tonic in 0..12u8 {
        for (minor_mode, weights) in [(false, major), (true, minor)] {
            let rotated: [f32; 12] =
                std::array::from_fn(|pc| weights[(pc + 12 - usize::from(tonic)) % 12]);
            let r = pearson(chroma, &rotated);
            if best.is_none_or(|(_, b)| r > b) {
                best = Some((
                    Key {
                        tonic,
                        minor: minor_mode,
                    },
                    r,
                ));
            }
        }
    }
    best
}

pub fn key_of(signal: &[f32], profile: Profile) -> Option<Key> {
    match profile {
        Profile::Learned => detect(signal).map(|(k, _)| k),
        p => estimate(&chroma(signal), p).map(|(k, _)| k),
    }
}

const FINE_FRAME: usize = 16384;
const FINE_HOP: usize = 8192;
const SUB: usize = 3;
pub const FINE: usize = SEMITONES * SUB;
const BANDS: usize = SEMITONES / 12;
pub const FEATURES: usize = 2 * BANDS * 12 + 12;

#[derive(Debug, Clone, PartialEq)]
pub struct FineSpectra {
    pub lin: [f32; FINE],
    pub peaks: [f32; FINE],
}

fn fine_pos(hz: f64) -> f64 {
    (69.0 + 12.0 * (hz / 440.0).log2() - f64::from(LOWEST_MIDI)) * SUB as f64 + 1.0
}

fn add_fine(row: &mut [f32; FINE], pos: f64, v: f32) {
    let lower = pos.floor();
    let frac = (pos - lower) as f32;
    for (i, w) in [(lower, 1.0 - frac), (lower + 1.0, frac)] {
        if i >= 0.0 && (i as usize) < FINE {
            row[i as usize] += w * v;
        }
    }
}

pub fn fine_spectra(signal: &[f32]) -> Option<FineSpectra> {
    let mut stft = Stft::hann(FINE_FRAME);
    let bin_hz = f64::from(ANALYSIS_RATE) / FINE_FRAME as f64;
    let top_midi = f64::from(LOWEST_MIDI) + SEMITONES as f64;
    let max_bin = (440.0 * 2f64.powf((top_midi - 69.0) / 12.0) / bin_hz) as usize + 2;
    let min_bin =
        ((440.0 * 2f64.powf((f64::from(LOWEST_MIDI) - 1.0 - 69.0) / 12.0)) / bin_hz) as usize;
    let mut lin_sum = [0.0f32; FINE];
    let mut peaks_sum = [0.0f32; FINE];
    let mut mags = vec![0.0f32; FINE_FRAME / 2 + 1];
    let mut start = 0;
    while start + FINE_FRAME <= signal.len() {
        let output = stft.spectrum(&signal[start..start + FINE_FRAME]);
        for (m, c) in mags.iter_mut().zip(output) {
            *m = c.norm();
        }
        start += FINE_HOP;
        let frame_max = mags[min_bin..max_bin].iter().copied().fold(0.0, f32::max);
        if frame_max <= 1e-6 {
            continue;
        }
        let mut lin = [0.0f32; FINE];
        let mut peaks = [0.0f32; FINE];
        for bin in min_bin.max(1)..max_bin {
            let m = mags[bin];
            add_fine(&mut lin, fine_pos(bin as f64 * bin_hz), m);
            let (a, c) = (mags[bin - 1], mags[bin + 1]);
            if m > a && m >= c && m > 0.05 * frame_max {
                let (la, lb, lc) = (a.max(1e-9).ln(), m.ln(), c.max(1e-9).ln());
                let d = 0.5 * (la - lc) / (la - 2.0 * lb + lc);
                add_fine(
                    &mut peaks,
                    fine_pos((bin as f64 + f64::from(d)) * bin_hz),
                    m,
                );
            }
        }
        for (sum, row) in [(&mut lin_sum, lin), (&mut peaks_sum, peaks)] {
            let total: f32 = row.iter().sum();
            if total > 0.0 {
                for (s, v) in sum.iter_mut().zip(row) {
                    *s += v / total;
                }
            }
        }
    }
    let total: f32 = lin_sum.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let unit = |sum: [f32; FINE]| {
        let t = sum.iter().sum::<f32>().max(1e-12);
        sum.map(|v| v / t)
    };
    Some(FineSpectra {
        lin: unit(lin_sum),
        peaks: unit(peaks_sum),
    })
}

pub fn features(spectra: &FineSpectra, semitones: &[f32; masked::SEMITONES]) -> [f32; FEATURES] {
    let lin: [f32; SEMITONES] = std::array::from_fn(|s| spectra.lin[SUB * s + 1]);
    let peaks: [f32; SEMITONES] =
        std::array::from_fn(|s| spectra.peaks[SUB * s..SUB * s + SUB].iter().sum());
    let mut out = [0.0f32; FEATURES];
    for (block, semis) in out
        .as_chunks_mut::<SEMITONES>()
        .0
        .iter_mut()
        .zip([lin, peaks])
    {
        for (band, chroma) in block.as_chunks_mut::<12>().0.iter_mut().enumerate() {
            for s in 12 * band..12 * band + 12 {
                chroma[(usize::from(LOWEST_MIDI) + s) % 12] += semis[s];
            }
            let total = chroma.iter().sum::<f32>().max(1e-12);
            for c in chroma.iter_mut() {
                *c /= total;
            }
        }
    }
    let chroma = masked::chroma(semitones);
    let total = chroma.iter().sum::<f32>().max(1e-9);
    for (o, c) in out[2 * SEMITONES..].iter_mut().zip(chroma) {
        *o = c / total;
    }
    out
}

pub mod masked {
    use crate::signal::ANALYSIS_RATE;
    use crate::stft::{Stft, hann_symmetric};

    pub const WINDOW: usize = 5513;
    pub const HOP: usize = 2756;
    const FFT: usize = 8192;
    pub const LOWEST_MIDI: usize = 27;
    pub const SEMITONES: usize = 96;
    pub const CHROMA_FROM: usize = 33 - LOWEST_MIDI;
    pub const CHROMA_TO: usize = 117 - LOWEST_MIDI;
    const MASK_REACH: usize = 5;

    struct Analyzer {
        filters: Vec<(usize, Vec<f32>)>,
        threshold: [f32; SEMITONES],
    }

    impl Default for Analyzer {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Analyzer {
        fn new() -> Self {
            let bin_hz = f64::from(ANALYSIS_RATE) / FFT as f64;
            let filters = (0..SEMITONES)
                .map(|k| {
                    let midi = (LOWEST_MIDI + k) as f64;
                    let hz = 440.0 * 2f64.powf((midi - 69.0) / 12.0);
                    let sigma = 0.25 * hz * (1.0 - 2f64.powf(-1.0 / 12.0));
                    let first = ((hz - 3.0 * sigma) / bin_hz).ceil().max(0.0) as usize;
                    let last = ((hz + 3.0 * sigma) / bin_hz).floor() as usize;
                    let weights = (first..=last.min(FFT / 2))
                        .map(|b| {
                            let d = (b as f64 * bin_hz - hz) / sigma;
                            (-0.5 * d * d).exp() as f32
                        })
                        .collect();
                    (first, weights)
                })
                .collect();
            let threshold = std::array::from_fn(|k| {
                (63.706 - 30.5345 * ((LOWEST_MIDI + k) as f64).log10()) as f32
            });
            Self { filters, threshold }
        }

        fn frame(&self, samples: &[f32], stft: &mut Stft) -> [f32; SEMITONES] {
            let spectrum = stft.spectrum(samples);
            let scale = 2.0 / FFT as f32;
            let mut db = [0.0f32; SEMITONES];
            for (k, (first, weights)) in self.filters.iter().enumerate() {
                let energy: f32 = weights
                    .iter()
                    .zip(&spectrum[*first..])
                    .map(|(w, c)| w * c.norm() * scale)
                    .sum();
                let level = 20.0 * energy.max(1e-4).log10() + 80.0;
                db[k] = (level - self.threshold[k]).max(0.0);
            }
            let mut out = [0.0f32; SEMITONES];
            for (j, o) in out.iter_mut().enumerate() {
                let lo = j.saturating_sub(MASK_REACH);
                let hi = (j + MASK_REACH).min(SEMITONES - 1);
                let mut mask = 0.0f32;
                for (i, &d) in db.iter().enumerate().take(hi + 1).skip(lo) {
                    if i != j {
                        let gain = 1.0 - i.abs_diff(j) as f32 / (MASK_REACH + 1) as f32;
                        mask += 10f32.powf(gain * d / 20.0) - 1.0;
                    }
                }
                *o = (db[j] - 20.0 * (1.0 + mask).log10()).max(0.0);
            }
            out
        }
    }

    pub fn mean_semitones(signal: &[f32]) -> Option<[f32; SEMITONES]> {
        if signal.len() < WINDOW {
            return None;
        }
        let analyzer = Analyzer::new();
        let mut stft = Stft::new(FFT, hann_symmetric(WINDOW));
        let mut sum = [0.0f32; SEMITONES];
        let mut frames = 0;
        let mut start = 0;
        while start + WINDOW <= signal.len() {
            let f = analyzer.frame(&signal[start..start + WINDOW], &mut stft);
            for (s, v) in sum.iter_mut().zip(f) {
                *s += v;
            }
            frames += 1;
            start += HOP;
        }
        Some(sum.map(|v| v / frames as f32))
    }

    pub fn chroma(semitones: &[f32; SEMITONES]) -> [f32; 12] {
        let mut c = [0.0f32; 12];
        for (k, v) in semitones
            .iter()
            .enumerate()
            .take(CHROMA_TO)
            .skip(CHROMA_FROM)
        {
            c[(LOWEST_MIDI + k) % 12] += v;
        }
        c
    }
}

pub fn detect(signal: &[f32]) -> Option<(Key, f32)> {
    let spectra = fine_spectra(signal)?;
    let semitones = masked::mean_semitones(signal)?;
    classify(&features(&spectra, &semitones))
}

pub fn classify(x: &[f32; FEATURES]) -> Option<(Key, f32)> {
    use crate::key_model::{BIAS, MEAN, SCALE, WEIGHTS};
    let mut scores = [0.0f32; 24];
    for (minor, (w, b)) in WEIGHTS.iter().zip(BIAS).enumerate() {
        for tonic in 0..12 {
            let mut s = b;
            for (chroma, wc) in x.as_chunks::<12>().0.iter().zip(w.as_chunks::<12>().0) {
                for (i, wi) in wc.iter().enumerate() {
                    s += wi * (chroma[(i + tonic) % 12] - MEAN) / SCALE;
                }
            }
            scores[12 * minor + tonic] = s;
        }
    }
    let (best, top) = scores
        .iter()
        .copied()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    let total: f32 = scores.iter().map(|s| (s - top).exp()).sum();
    Key::from_nml_value(best as u8).map(|k| (k, 1.0 / total))
}

fn pearson(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let ma = a.iter().sum::<f32>() / 12.0;
    let mb = b.iter().sum::<f32>() / 12.0;
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        num += (x - ma) * (y - mb);
        da += (x - ma) * (x - ma);
        db += (y - mb) * (y - mb);
    }
    num / (da * db).sqrt().max(f32::EPSILON)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(midi_notes: &[u8], seconds: f64) -> Vec<f32> {
        let rate = f64::from(ANALYSIS_RATE);
        (0..(seconds * rate) as usize)
            .map(|i| {
                let t = i as f64 / rate;
                midi_notes
                    .iter()
                    .map(|&m| {
                        let hz = 440.0 * 2f64.powf((f64::from(m) - 69.0) / 12.0);
                        (2.0 * std::f64::consts::PI * hz * t).sin()
                    })
                    .sum::<f64>() as f32
                    * 0.2
            })
            .collect()
    }

    fn rich_chord(midi_notes: &[u8], seconds: f64) -> Vec<f32> {
        let rate = f64::from(ANALYSIS_RATE);
        (0..(seconds * rate) as usize)
            .map(|i| {
                let t = i as f64 / rate;
                let mut v = 0.0;
                for &m in midi_notes {
                    let hz = 440.0 * 2f64.powf((f64::from(m) - 69.0) / 12.0);
                    for h in 1..=6 {
                        let h = f64::from(h);
                        v += (2.0 * std::f64::consts::PI * hz * h * t).sin() / h;
                    }
                }
                (v * 0.1) as f32
            })
            .collect()
    }

    #[test]
    fn chroma_peaks_on_played_notes() {
        let c = chroma(&chord(&[57], 2.0));
        let (max_pc, _) = c
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap();
        assert_eq!(max_pc, 9);
    }

    #[test]
    fn detects_major_and_minor_triads_with_scale() {
        let c_major = chord(&[48, 52, 55, 60, 62, 64, 65, 67, 69, 71], 3.0);
        let (key, _) = estimate(&chroma(&c_major), Profile::Temperley).unwrap();
        assert_eq!(
            key,
            Key {
                tonic: 0,
                minor: false
            }
        );
        let a_minor = chord(&[45, 57, 60, 64, 57, 59, 62, 65, 67, 45], 3.0);
        let (key, _) = estimate(&chroma(&a_minor), Profile::Temperley).unwrap();
        assert_eq!(
            key,
            Key {
                tonic: 9,
                minor: true
            }
        );
    }

    #[test]
    fn nml_encoding_round_trips() {
        for v in 0..24 {
            assert_eq!(Key::from_nml_value(v).unwrap().nml_value(), v);
        }
        assert_eq!(Key::from_nml_value(24), None);
        let am = Key {
            tonic: 9,
            minor: true,
        };
        assert_eq!(
            am.relative(),
            Key {
                tonic: 0,
                minor: false
            }
        );
        assert_eq!(am.relative().relative(), am);
    }

    #[test]
    fn learned_model_finds_scales_and_chords() {
        let c_major = rich_chord(&[36, 48, 52, 55, 60, 62, 64, 65, 67, 69, 71], 6.0);
        assert_eq!(key_of(&c_major, Profile::Learned), Key::from_nml_value(0));
        let a_minor = rich_chord(&[33, 45, 57, 60, 64, 59, 62, 65, 67], 6.0);
        assert_eq!(key_of(&a_minor, Profile::Learned), Key::from_nml_value(21));
    }

    #[test]
    fn learned_features_are_unit_chroma_per_octave() {
        let spectra = fine_spectra(&chord(&[45, 57, 69], 3.0)).unwrap();
        let x = features(&spectra, &[0.0; masked::SEMITONES]);
        for band in x
            .as_chunks::<12>()
            .0
            .iter()
            .filter(|b| b.iter().sum::<f32>() > 0.0)
        {
            assert!((band.iter().sum::<f32>() - 1.0).abs() < 1e-4);
        }
        assert!(x[12 + 9] > 0.5, "A2 octave peaks on A: {:?}", &x[12..24]);
        assert_eq!(fine_spectra(&vec![0.0; 40_000]), None);
    }

    #[test]
    fn masked_semitones_find_played_notes_and_mask_neighbours() {
        let m = masked::mean_semitones(&rich_chord(&[45, 52], 3.0)).unwrap();
        let at = |midi: usize| m[midi - masked::LOWEST_MIDI];
        assert!(at(45) > 10.0 && at(52) > 10.0, "{} {}", at(45), at(52));
        assert!(at(46) < at(45) / 4.0, "next semitone masked: {}", at(46));
        let chroma = masked::chroma(&m);
        let top = (0..12)
            .max_by(|&a, &b| chroma[a].total_cmp(&chroma[b]))
            .unwrap();
        assert!(top == 9 || top == 4, "A or E: {chroma:?}");
        assert_eq!(masked::mean_semitones(&[0.0; 100]), None);
    }

    #[test]
    fn silence_has_no_key() {
        assert_eq!(
            estimate(&chroma(&vec![0.0; 20_000]), Profile::Temperley),
            None
        );
    }
}
