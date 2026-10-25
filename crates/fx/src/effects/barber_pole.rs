use backspn_dsp::Smoothed;

use super::EffectKind;
use crate::{Context, Effect};

const BANDS: usize = 4;
const LOW_HZ: f64 = 20.0;
const HIGH_HZ: f64 = 20_000.0;
const SMOOTH_S: f64 = 0.02 / 3.0;

fn mod_hz(v: f32, fast: bool) -> f64 {
    let x = 2.0 * f64::from(v.clamp(0.0, 1.0)) - 1.0;
    let base: f64 = if fast { 20_001.0 } else { 3.0 };
    x.signum() * (base.powf(x.abs()) - 1.0)
}

fn range_of(v: f32) -> (f64, f64) {
    let v = f64::from(v.clamp(0.0, 1.0));
    if v < 0.5 {
        (LOW_HZ, 19.0 + 19_981f64.powf(2.0 * v))
    } else {
        (19.0 + 19_981f64.powf(2.0 * v - 1.0), HIGH_HZ)
    }
}

fn bandwidth_of(v: f32) -> (f32, f32) {
    let bw = 1.0 - 0.66 * f64::from(v.clamp(0.0, 1.0));
    let k = 2f64.powf(bw / 2.0) - 2f64.powf(-bw / 2.0);
    (k as f32, (1.4 + 0.6 * (1.0 - bw)) as f32)
}

fn mix_of(v: f32) -> f32 {
    0.5 - 0.5 * (std::f32::consts::PI * v.clamp(0.0, 1.0)).cos()
}

fn shape(points: &[(f32, f32)], v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    for w in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if v <= x1 {
            return y0 + (y1 - y0) * (v - x0) / (x1 - x0);
        }
    }
    points[points.len() - 1].1
}

fn group_of(v: f32) -> (f32, f32, f32) {
    let dw = shape(
        &[(0.0, 1.0), (0.25, 1.0), (0.5, 0.0), (0.75, 1.0), (1.0, 1.0)],
        v,
    );
    let x = 2.0 * v.clamp(0.0, 1.0) - 1.0;
    let reach = if x < 0.0 { 0.37 } else { 0.5 };
    let modulation = 0.5 + x.signum() * reach * x.abs().powf(1.5);
    let bawd = shape(
        &[(0.0, 0.8), (0.31, 0.0), (0.5, 1.0), (0.69, 0.0), (1.0, 1.0)],
        v,
    );
    (dw, modulation, bawd)
}

#[derive(Debug, Clone, Copy, Default)]
struct Band {
    ic1: [f32; 2],
    ic2: [f32; 2],
}

impl Band {
    fn tick(&mut self, ch: usize, x: f32, g: f32, k: f32) -> f32 {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let v3 = x - self.ic2[ch];
        let v1 = a1 * self.ic1[ch] + g * a1 * v3;
        let v2 = self.ic2[ch] + g * v1;
        self.ic1[ch] = 2.0 * v1 - self.ic1[ch];
        self.ic2[ch] = 2.0 * v2 - self.ic2[ch];
        v1
    }
}

#[derive(Debug)]
pub struct BarberPole {
    rate: f64,
    bands: [Band; BANDS],
    rotation: usize,
    phase: f64,
    hz: f64,
    range: (f64, f64),
    damping: Smoothed,
    level: Smoothed,
    mix: Smoothed,
    dry_wet: f32,
    modulation: f32,
    fast: bool,
    active: bool,
}

impl BarberPole {
    pub fn new(rate: f64) -> Self {
        let (k, level) = bandwidth_of(0.5);
        Self {
            rate,
            bands: [Band::default(); BANDS],
            rotation: 0,
            phase: 0.0,
            hz: mod_hz(0.75, false),
            range: range_of(0.5),
            damping: Smoothed::new(k, SMOOTH_S, rate),
            level: Smoothed::new(level, SMOOTH_S, rate),
            mix: Smoothed::new(0.0, SMOOTH_S, rate),
            dry_wet: 1.0,
            modulation: 0.75,
            fast: false,
            active: false,
        }
    }

    fn set_bandwidth(&mut self, v: f32) {
        let (k, level) = bandwidth_of(v);
        self.damping.set(k);
        self.level.set(level);
    }
}

impl Effect for BarberPole {
    fn kind(&self) -> EffectKind {
        EffectKind::BarberPole
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        self.mix.set(if self.active {
            mix_of(self.dry_wet)
        } else {
            0.0
        });
        if !self.active && self.mix.value() == 0.0 {
            return;
        }
        let (lo, hi) = self.range;
        let ratio = (hi / lo).max(1.0).powf(1.0 / BANDS as f64);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            self.phase += self.hz / self.rate;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                self.rotation = (self.rotation + BANDS - 1) % BANDS;
            } else if self.phase < 0.0 {
                self.phase += 1.0;
                self.rotation = (self.rotation + 1) % BANDS;
            }
            let (damping, level, mix) = (self.damping.tick(), self.level.tick(), self.mix.tick());
            let mut wet = [0.0f32; 2];
            for j in 0..BANDS {
                let at = self.phase + j as f64;
                let hz = (lo * ratio.powf(at)).min(0.49 * self.rate);
                let angle = std::f64::consts::PI * hz / self.rate;
                let g = (angle + angle.powi(3) / 3.0 + 2.0 * angle.powi(5) / 15.0) as f32;
                let w = (1.0 - (2.0 * at / BANDS as f64 - 1.0).powi(16)) as f32;
                let band = &mut self.bands[(j + self.rotation) % BANDS];
                for (ch, out) in wet.iter_mut().enumerate() {
                    *out += w * damping * band.tick(ch, frame[ch], g, damping);
                }
            }
            for ch in 0..2 {
                frame[ch] = (1.0 - mix) * frame[ch] + mix * level * wet[ch];
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        let (dw, modulation, bawd) = group_of(amount);
        self.dry_wet = dw;
        self.hz = mod_hz(modulation, self.fast);
        self.range = range_of(0.5);
        self.set_bandwidth(bawd);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => {
                self.modulation = value;
                self.hz = mod_hz(value, self.fast);
            }
            1 => self.range = range_of(value),
            2 => self.set_bandwidth(value),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        if index == 0 {
            self.fast = on;
            self.hz = mod_hz(self.modulation, on);
        }
    }

    fn set_active(&mut self, on: bool) {
        self.active = on;
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.dry_wet = dry_wet;
        true
    }

    fn reset(&mut self) {
        self.bands = [Band::default(); BANDS];
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn knobs_follow_their_curves() {
        assert!((mod_hz(1.0, false) - 2.0).abs() < 1e-9 && (mod_hz(0.0, false) + 2.0).abs() < 1e-9);
        assert!(mod_hz(0.5, true).abs() < 1e-9 && (mod_hz(1.0, true) - 20_000.0).abs() < 1e-6);
        assert_eq!(range_of(0.5), (20.0, 20_000.0));
        let (lo, hi) = range_of(0.25);
        assert!((lo - 20.0).abs() < 1e-9 && (hi - 160.4).abs() < 1.0);
        let (_, level) = bandwidth_of(1.0);
        assert!((level - 1.796).abs() < 1e-3);
        assert!(mix_of(0.0).abs() < 1e-6 && (mix_of(1.0) - 1.0).abs() < 1e-6);
        let (dw, modulation, _) = group_of(0.5);
        assert!(
            dw.abs() < 1e-6 && (modulation - 0.5).abs() < 1e-6,
            "centre: dry"
        );
    }

    #[test]
    fn sweeps_without_end_and_stays_bounded() {
        let mut b = BarberPole::new(f64::from(RATE));
        b.set_amount(1.0);
        b.set_active(true);
        let input = sine(1_000.0, 4 * RATE as usize, 0.5);
        let out = run(&mut b, &input, ctx());
        assert!(out.iter().all(|v| v.is_finite() && v.abs() < 3.0));
        let chunks: Vec<f32> = out.chunks(2 * 4_800).map(rms).collect();
        let (lo, hi) = chunks
            .iter()
            .fold((f32::MAX, 0.0f32), |(l, h), &v| (l.min(v), h.max(v)));
        assert!(hi > 1.5 * lo, "{lo} … {hi}");
    }
}
