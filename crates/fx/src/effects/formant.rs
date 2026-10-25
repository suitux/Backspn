use std::f64::consts::PI;

use backspn_dsp::Smoothed;

use super::{EffectKind, note_hz};
use crate::{Context, Effect};

const VOWELS: [[(f64, f64); 3]; 5] = [
    [(300.0, 0.0), (870.0, -10.0), (2_240.0, -22.0)],
    [(570.0, 0.0), (840.0, -6.0), (2_410.0, -20.0)],
    [(730.0, 0.0), (1_090.0, -4.0), (2_440.0, -18.0)],
    [(530.0, 0.0), (1_840.0, -10.0), (2_480.0, -16.0)],
    [(270.0, 0.0), (2_290.0, -14.0), (3_010.0, -18.0)],
];
const DEEP: [f64; 3] = [0.8, 0.88, 0.92];
const RADIUS: f64 = 0.97;
const RELEASE_S: f64 = 0.11;

#[derive(Debug, Clone, Copy, Default)]
struct Resonator {
    b0: f32,
    a1: f32,
    a2: f32,
    y: [[f32; 2]; 2],
}

impl Resonator {
    fn set(&mut self, hz: f64, radius: f64, rate: f64) {
        let w = 2.0 * PI * hz.clamp(rate / 24_576.0, rate / 2.125) / rate;
        self.a1 = (2.0 * radius * w.cos()) as f32;
        self.a2 = -(radius * radius) as f32;
        let peak = (1.0 - radius) * (1.0 - 2.0 * radius * (2.0 * w).cos() + radius * radius).sqrt();
        self.b0 = peak as f32;
    }

    fn tick(&mut self, ch: usize, x: f32) -> f32 {
        let [y1, y2] = self.y[ch];
        let y = self.b0 * x + self.a1 * y1 + self.a2 * y2;
        self.y[ch] = [y, y1];
        y
    }
}

fn size_of(v: f32) -> f64 {
    let v = f64::from(v.clamp(0.0, 1.0));
    if v < 0.5 {
        0.666 + (1.0 - 0.666) * 2.0 * v
    } else {
        1.0 + 0.5 * (2.0 * v - 1.0)
    }
}

fn formants(talk: f32, deep: bool) -> [(f64, f32); 3] {
    let p = 4.0 * f64::from(talk.clamp(0.0, 1.0));
    let i = (p.floor() as usize).min(3);
    let t = p - i as f64;
    std::array::from_fn(|k| {
        let (f0, g0) = VOWELS[i][k];
        let (f1, g1) = VOWELS[i + 1][k];
        let hz = (f0 + (f1 - f0) * t) * if deep { DEEP[k] } else { 1.0 };
        let db = g0 + (g1 - g0) * t;
        (hz, 10f64.powf(db / 20.0) as f32)
    })
}

#[derive(Debug)]
pub struct Formant {
    rate: f64,
    resonators: [Resonator; 3],
    gains: [f32; 3],
    talk: f32,
    size: f64,
    sharpness: f32,
    deep: bool,
    envelope: f32,
    release: f32,
    mix: Smoothed,
    dry_wet: f32,
    active: bool,
}

impl Formant {
    pub fn new(rate: f64) -> Self {
        let mut f = Self {
            rate,
            resonators: [Resonator::default(); 3],
            gains: [1.0; 3],
            talk: 0.502,
            size: 1.0,
            sharpness: 1.0,
            deep: false,
            envelope: 0.0,
            release: (0.5f64.powf(1.0 / (RELEASE_S * rate))) as f32,
            mix: Smoothed::new(0.0, 0.01, rate),
            dry_wet: 1.0,
            active: false,
        };
        f.update();
        f
    }

    fn update(&mut self) {
        let s = f64::from(self.sharpness * (2.0 - self.sharpness));
        let radius = RADIUS * (0.9 + 0.1 * s);
        for (k, (hz, gain)) in formants(self.talk, self.deep).into_iter().enumerate() {
            self.resonators[k].set(hz * self.size, radius, self.rate);
            self.gains[k] = gain;
        }
    }
}

impl Effect for Formant {
    fn kind(&self) -> EffectKind {
        EffectKind::FormantFilter
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        self.mix.set(if self.active { self.dry_wet } else { 0.0 });
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let mut wet = [0.0; 2];
            for ch in 0..2 {
                for k in 0..3 {
                    wet[ch] += self.resonators[k].tick(ch, frame[ch]) * self.gains[k];
                }
            }
            let peak = wet[0].abs().max(wet[1].abs());
            self.envelope = peak.max(self.envelope * self.release);
            let limit = 1.0 / self.envelope.max(1.0);
            let m = self.mix.tick();
            for ch in 0..2 {
                frame[ch] += (wet[ch] * limit - frame[ch]) * m;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.talk = amount;
        self.size = 1.0;
        self.sharpness = 1.0;
        self.deep = false;
        self.dry_wet = 1.0;
        self.update();
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.talk = value,
            1 => self.size = size_of(value),
            2 => self.sharpness = value.clamp(0.0, 1.0),
            _ => {}
        }
        self.update();
    }

    fn set_button(&mut self, index: usize, on: bool) {
        if index == 0 {
            self.deep = on;
            self.update();
        }
    }

    fn set_active(&mut self, on: bool) {
        self.active = on;
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.dry_wet = dry_wet.clamp(0.0, 1.0);
        true
    }

    fn reset(&mut self) {
        for r in &mut self.resonators {
            r.y = [[0.0; 2]; 2];
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Peaking {
    b: [f32; 3],
    a: [f32; 2],
    z: [[f32; 2]; 2],
}

impl Peaking {
    fn set(&mut self, hz: f64, db: f64, octaves: f64, rate: f64) {
        let w = 2.0 * PI * hz.min(0.48 * rate) / rate;
        let amp = 10f64.powf(db / 40.0);
        let alpha =
            w.sin() * (std::f64::consts::LN_2 / 2.0 * octaves.max(0.001) * w / w.sin()).sinh();
        let a0 = 1.0 + alpha / amp;
        self.b = [
            ((1.0 + alpha * amp) / a0) as f32,
            (-2.0 * w.cos() / a0) as f32,
            ((1.0 - alpha * amp) / a0) as f32,
        ];
        self.a = [
            (-2.0 * w.cos() / a0) as f32,
            ((1.0 - alpha / amp) / a0) as f32,
        ];
    }

    fn tick(&mut self, ch: usize, x: f32) -> f32 {
        let z = &mut self.z[ch];
        let y = self.b[0] * x + z[0];
        z[0] = self.b[1] * x - self.a[0] * y + z[1];
        z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

fn freq_scales(v: f32) -> (f64, f64, f64) {
    let v = f64::from(v.clamp(0.0, 1.0));
    let peak = if v < 0.2 {
        0.88 + 0.12 * v / 0.2
    } else if v < 0.85 {
        1.0
    } else {
        (1.0 - v) / 0.15
    };
    let width = 3.96 * (0.57f64 / 3.96).powf(v);
    (peak, 0.93 * (1.0 - v), width)
}

fn peak_mix(d: f32) -> (f32, f32) {
    let d = d.clamp(0.0, 1.0);
    let m = if d < 0.1 {
        1.0 - (1.0 - 10.0 * d).powi(2)
    } else {
        1.0
    };
    (m, d * (1.25 - 0.25 * d))
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct PeakFilter {
    rate: f64,
    filter: Peaking,
    background: f32,
    freq: f32,
    edge: f32,
    threshold: f32,
    kill: bool,
    dry_wet: f32,
    envelope: f32,
    release: f32,
    mix: Smoothed,
    active: bool,
    single: bool,
}

impl PeakFilter {
    pub fn new(rate: f64) -> Self {
        let mut p = Self {
            rate,
            filter: Peaking::default(),
            background: 1.0,
            freq: 0.498,
            edge: 0.2,
            threshold: 1.0 - 0.7 * 0.825,
            kill: false,
            dry_wet: 1.0,
            envelope: 0.0,
            release: (0.5f64.powf(1.0 / (RELEASE_S * rate))) as f32,
            mix: Smoothed::new(0.0, 0.025 / std::f64::consts::LN_2, rate),
            active: false,
            single: false,
        };
        p.update();
        p
    }

    fn update(&mut self) {
        let hz = note_hz(37.0 + 98.0 * f64::from(self.freq.clamp(0.0, 1.0)));
        let (peak, background, width) = freq_scales(self.freq);
        let (_, w) = peak_mix(self.dry_wet);
        let (w, e) = (f64::from(w), f64::from(self.edge));
        if self.kill {
            self.filter
                .set(hz, -45.0, 0.5 * (1.5 - 1.26 * e) * width, self.rate);
            self.background = 1.0;
        } else {
            self.filter.set(
                hz,
                (34.0 - 5.0 * e) * peak * w,
                (1.5 - 1.26 * e) * width,
                self.rate,
            );
            let cut = 10f64.powf(-(34.0 - 17.18 * e) / 20.0);
            self.background = (1.0 + w * background * (cut - 1.0)) as f32;
        }
    }
}

impl Effect for PeakFilter {
    fn kind(&self) -> EffectKind {
        EffectKind::PeakFilter
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        let (m, _) = peak_mix(self.dry_wet);
        self.mix.set(if self.active { m } else { 0.0 });
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let mut wet = [0.0; 2];
            for ch in 0..2 {
                wet[ch] = self.filter.tick(ch, frame[ch]) * self.background;
            }
            let peak = wet[0].abs().max(wet[1].abs());
            self.envelope = peak.max(self.envelope * self.release);
            let pump = 1.0 / self.envelope.max(self.threshold);
            let m = self.mix.tick();
            for ch in 0..2 {
                frame[ch] += (wet[ch] * pump - frame[ch]) * m;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.single = false;
        self.freq = amount;
        self.edge = 0.2;
        self.threshold = 1.0 - 0.7 * 0.825;
        self.kill = false;
        self.dry_wet = 1.0;
        self.update();
    }

    fn set_param(&mut self, index: usize, value: f32) {
        self.single = true;
        match index {
            0 => self.freq = value,
            1 => self.edge = value.clamp(0.0, 1.0),
            2 => self.threshold = 1.0 - 0.7 * value.clamp(0.0, 1.0),
            _ => {}
        }
        self.update();
    }

    fn set_button(&mut self, index: usize, on: bool) {
        if index == 0 {
            self.kill = on;
            self.update();
        }
    }

    fn set_active(&mut self, on: bool) {
        self.active = on;
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.dry_wet = dry_wet.clamp(0.0, 1.0);
        self.update();
        true
    }

    fn reset(&mut self) {
        self.filter.z = [[0.0; 2]; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn knobs_follow_their_curves() {
        assert!((size_of(0.0) - 0.666).abs() < 1e-9 && (size_of(1.0) - 1.5).abs() < 1e-9);
        assert!((size_of(0.5) - 1.0).abs() < 1e-9);
        let [(f1, _), _, _] = formants(0.5, false);
        assert!((f1 - 730.0).abs() < 1e-9, "A in the middle");
        assert_eq!(peak_mix(0.1), (1.0, 0.1 * 1.225));
        let (peak, _, _) = freq_scales(1.0);
        assert!(peak.abs() < 1e-9, "no peak at the top");
    }

    #[test]
    fn formant_resonates_at_the_vowel() {
        let level = |hz: f64| {
            let mut f = Formant::new(f64::from(RATE));
            f.set_amount(0.5);
            f.set_active(true);
            rms(&run(&mut f, &sine(hz, RATE as usize / 4, 0.3), ctx())[2 * 4_000..])
        };
        assert!(level(730.0) > 4.0 * level(5_000.0));
    }

    #[test]
    fn peak_filter_kill_cuts_its_frequency() {
        let mut p = PeakFilter::new(f64::from(RATE));
        p.set_param(0, 0.5);
        p.set_button(0, true);
        p.set_active(true);
        let hz = note_hz(86.0);
        let out = run(&mut p, &sine(hz, RATE as usize / 4, 0.3), ctx());
        assert!(rms(&out[2 * 4_000..]) < 0.1 * 0.3, "notch");
    }
}
