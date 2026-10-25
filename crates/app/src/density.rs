use std::f32::consts::TAU;

use backspn_decode::Track;
use eframe::egui::{Color32, ColorImage};

use crate::waveform::{FRAMES_PER_BIN, WavePalette, spectral_color};

pub const ROWS: usize = 32;

struct Splitter {
    f: [f32; 3],
    band: [f32; 3],
    low: [f32; 3],
    env: [f32; 3],
    attack: [f32; 3],
    release: [f32; 3],
}

const CUTOFF: [f32; 3] = [150.0, 671.0, 3000.0];
const ATTACK: [f32; 3] = [0.01, 0.005, 0.0025];
const RELEASE: [f32; 3] = [0.1, 0.035, 0.025];
const SCALE: [f32; 3] = [0.6, 1.0, 1.2];
const DAMPING: f32 = 2.0;

impl Splitter {
    fn new(rate: f32) -> Self {
        let coeff = |t: f32| (-1.0 / (rate * t)).exp();
        Self {
            f: CUTOFF.map(|hz| (TAU * hz / rate).min(1.0)),
            band: [0.0; 3],
            low: [0.0; 3],
            env: [0.0; 3],
            attack: ATTACK.map(coeff),
            release: RELEASE.map(coeff),
        }
    }

    fn split(&mut self, x: f32) -> [f32; 3] {
        let mut high = [0.0; 3];
        for i in 0..3 {
            high[i] = x - DAMPING * self.band[i] - self.low[i];
            self.band[i] += self.f[i] * high[i];
            self.low[i] += self.f[i] * self.band[i];
        }
        let signals = [self.low[0], self.band[1], high[2]];
        for i in 0..3 {
            let s = signals[i].abs();
            let c = if self.env[i] < s {
                self.attack[i]
            } else {
                self.release[i]
            };
            self.env[i] = self.env[i] * c + (1.0 - c) * s;
        }
        std::array::from_fn(|i| self.env[i] * SCALE[i])
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Cell {
    hits: u16,
    bands: [u8; 3],
}

#[derive(Debug, Clone, Default)]
pub struct Density {
    bins: Vec<[Cell; ROWS]>,
    bin_s: f64,
}

fn row_of(v: f32) -> usize {
    ((f32::midpoint(v.clamp(-1.0, 1.0), 1.0) * (ROWS - 1) as f32).round() as usize).min(ROWS - 1)
}

impl Density {
    pub fn compute(track: &Track) -> Self {
        let samples = track.samples();
        let peak = samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[l, r]| l.midpoint(r).abs())
            .fold(0.0f32, f32::max);
        let gain = if peak > 1e-6 { 0.95 / peak } else { 1.0 };
        let mut splitter = Splitter::new(track.sample_rate() as f32);
        let mut previous = row_of(0.0);
        let mut bins = Vec::with_capacity(track.frames() / FRAMES_PER_BIN + 1);
        for chunk in samples.chunks(FRAMES_PER_BIN * 2) {
            let mut sums = [[0.0f32; 3]; ROWS];
            let mut hits = [0u32; ROWS];
            for &[l, r] in chunk.as_chunks::<2>().0 {
                let v = l.midpoint(r) * gain;
                let levels = splitter.split(v);
                let row = row_of(v);
                for y in row.min(previous)..=row.max(previous) {
                    hits[y] += 1;
                    for (s, l) in sums[y].iter_mut().zip(levels) {
                        *s += l;
                    }
                }
                previous = row;
            }
            bins.push(std::array::from_fn(|y| {
                let n = hits[y];
                Cell {
                    hits: n.min(u32::from(u16::MAX)) as u16,
                    bands: if n == 0 {
                        [0; 3]
                    } else {
                        sums[y].map(|s| ((s / n as f32).clamp(0.0, 1.0) * 255.0) as u8)
                    },
                }
            }));
        }
        Self {
            bins,
            bin_s: FRAMES_PER_BIN as f64 / f64::from(track.sample_rate()),
        }
    }

    pub fn image(
        &self,
        (from_s, to_s): (f64, f64),
        width: usize,
        colors: [Color32; 3],
        direct: bool,
    ) -> ColorImage {
        let mut pixels = vec![Color32::TRANSPARENT; width * ROWS];
        if self.bins.is_empty() || width == 0 || self.bin_s <= 0.0 {
            return ColorImage::new([width.max(1), ROWS], pixels);
        }
        let per_px = (to_s - from_s) / width as f64;
        for x in 0..width {
            let t0 = from_s + x as f64 * per_px;
            let t1 = t0 + per_px;
            if t1 <= 0.0 {
                continue;
            }
            let start = ((t0.max(0.0) / self.bin_s) as usize).min(self.bins.len());
            let end = ((t1 / self.bin_s).ceil() as usize).clamp(start, self.bins.len());
            if start >= end {
                continue;
            }
            let frames = ((end - start) * FRAMES_PER_BIN) as f32;
            for y in 0..ROWS {
                let mut n = 0u32;
                let mut sum = [0.0f32; 3];
                for bin in &self.bins[start..end] {
                    let cell = bin[y];
                    n += u32::from(cell.hits);
                    for (s, b) in sum.iter_mut().zip(cell.bands) {
                        *s += f32::from(b) * f32::from(cell.hits);
                    }
                }
                if n == 0 {
                    continue;
                }
                let bands = sum.map(|s| s / (n as f32 * 255.0));
                let colour = band_colour(bands, colors, direct);
                let density = (n as f32 / frames).sqrt() * DENSITY_GAIN;
                let alpha = (density.min(1.0) * 255.0) as u8;
                let [r, g, b, _] = colour.to_array();
                pixels[(ROWS - 1 - y) * width + x] =
                    Color32::from_rgba_unmultiplied(r, g, b, alpha);
            }
        }
        ColorImage::new([width, ROWS], pixels)
    }
}

impl Density {
    pub fn mean_bands(&self, from_s: f64, to_s: f64) -> [f32; 3] {
        if self.bins.is_empty() || self.bin_s <= 0.0 || to_s <= 0.0 {
            return [0.0; 3];
        }
        let start = ((from_s.max(0.0) / self.bin_s) as usize).min(self.bins.len());
        let end = ((to_s / self.bin_s).ceil() as usize).clamp(start, self.bins.len());
        let mut n = 0u32;
        let mut sum = [0.0f32; 3];
        for cell in self.bins[start..end].iter().flatten() {
            n += u32::from(cell.hits);
            for (s, b) in sum.iter_mut().zip(cell.bands) {
                *s += f32::from(b) * f32::from(cell.hits);
            }
        }
        if n == 0 {
            return [0.0; 3];
        }
        sum.map(|s| s / (n as f32 * 255.0))
    }
}

pub fn band_colour(bands: [f32; 3], colors: [Color32; 3], direct: bool) -> Color32 {
    if direct {
        let top = bands.iter().copied().fold(1e-6f32, f32::max);
        let [r, g, b] = bands.map(|v| (v / top * 255.0) as u8);
        Color32::from_rgb(r, g, b)
    } else {
        spectral_color(colors, bands)
    }
}

const DENSITY_GAIN: f32 = 1.6;

pub fn is_direct(palette: WavePalette) -> bool {
    palette == WavePalette::Spectrum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, rate: u32, seconds: f32) -> Track {
        let n = (rate as f32 * seconds) as usize;
        let samples = (0..n)
            .flat_map(|i| {
                let s = (TAU * hz * i as f32 / rate as f32).sin() * 0.8;
                [s, s]
            })
            .collect();
        Track::from_stereo(rate, samples)
    }

    #[test]
    fn splitter_puts_tones_in_their_band() {
        for (hz, band) in [(60.0, 0), (670.0, 1), (8000.0, 2)] {
            let mut s = Splitter::new(44_100.0);
            let mut levels = [0.0; 3];
            for i in 0..44_100 {
                levels = s.split((TAU * hz * i as f32 / 44_100.0).sin());
            }
            let top = (0..3)
                .max_by(|&a, &b| levels[a].total_cmp(&levels[b]))
                .unwrap();
            assert_eq!(top, band, "{hz} Hz: {levels:?}");
        }
    }

    #[test]
    fn a_sine_fills_rows_densest_at_its_peaks() {
        let d = Density::compute(&tone(60.0, 44_100, 1.0));
        let image = d.image((0.2, 0.8), 10, [Color32::RED; 3], true);
        let alpha = |y: usize| image.pixels[y * 10 + 5].a();
        assert_eq!(alpha(0), 0, "nothing above the peak");
        assert!(
            alpha(1) > alpha(ROWS / 2),
            "{} vs {}",
            alpha(1),
            alpha(ROWS / 2)
        );
        assert!(alpha(ROWS / 2) > 0, "it crosses the middle");
        let p = image.pixels[10 + 5];
        assert!(p.r() > p.g() && p.r() > p.b(), "{p:?}");
    }

    #[test]
    fn outside_the_track_is_transparent() {
        let d = Density::compute(&tone(440.0, 44_100, 0.5));
        let image = d.image((-2.0, -1.0), 4, [Color32::WHITE; 3], false);
        assert!(image.pixels.iter().all(|p| p.a() == 0));
    }
}
