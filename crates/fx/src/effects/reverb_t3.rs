use backspn_dsp::{Smoothed, Svf};

use super::tank::{Shape, Tank};
use super::{EffectKind, note_hz, reverb_mix};
use crate::{Context, Effect};

const MAIN_NOTES: [f64; 2] = [7.5, 10.5];
const DIFFUSER_NOTES: [[f64; 2]; 3] = [[10.5, 13.5], [13.5, 16.5], [16.5, 19.5]];
const GROUP_LP: f32 = 0.92;
const GROUP_HP: f32 = 0.2;
const FREEZE_S: f64 = 0.125;

fn decay_s(size: f32) -> f64 {
    0.001 * 10f64.powf((24.0 + 72.0 * f64::from(size.clamp(0.0, 1.0))) / 20.0)
}

fn pass_gain(delay_s: f64, decay_s: f64) -> f32 {
    10f64.powf((-60.0 * delay_s / decay_s).max(-144.0) / 20.0) as f32
}

fn shape_of(size: f32, rate: f64) -> Shape {
    let x = 24.0 * f64::from(1.0 - size.clamp(0.0, 1.0));
    let period = |n: f64| 1.0 / note_hz(x + n);
    let decay = decay_s(size);
    Shape {
        main: MAIN_NOTES.map(|n| (period(n) * rate) as f32),
        gain: MAIN_NOTES.map(|n| pass_gain(period(n), decay)),
        diffusers: DIFFUSER_NOTES.map(|pair| pair.map(|n| (period(n) * rate) as f32)),
    }
}

fn lp_hz(v: f32) -> f64 {
    note_hz(60.0 + 70.0 * f64::from(v.clamp(0.0, 1.0)))
}

fn hp_hz(v: f32) -> f64 {
    note_hz(40.0 + 32.0 * f64::from(v.clamp(0.0, 1.0)))
}

#[derive(Debug)]
pub struct ReverbT3 {
    rate: f64,
    tank: Tank,
    low: Svf,
    high: Svf,
    size: Smoothed,
    shape: Shape,
    dry: Smoothed,
    wet: Smoothed,
    freeze: bool,
    frozen_in: f64,
}

impl ReverbT3 {
    pub fn new(rate: f64) -> Self {
        let mut reverb = Self {
            rate,
            tank: Tank::new(rate),
            low: Svf::default(),
            high: Svf::default(),
            size: Smoothed::new(0.5, 0.05, rate),
            shape: shape_of(0.5, rate),
            dry: Smoothed::new(1.0, 0.01, rate),
            wet: Smoothed::new(0.0, 0.01, rate),
            freeze: false,
            frozen_in: 0.0,
        };
        reverb.set_lp(GROUP_LP);
        reverb.set_hp(GROUP_HP);
        reverb
    }

    fn set_lp(&mut self, v: f32) {
        self.low
            .set(lp_hz(v), std::f64::consts::FRAC_1_SQRT_2, self.rate);
    }

    fn set_hp(&mut self, v: f32) {
        self.high
            .set(hp_hz(v), std::f64::consts::FRAC_1_SQRT_2, self.rate);
    }

    fn set_mix(&mut self, dry_wet: f32) {
        let (dry, wet) = reverb_mix(dry_wet);
        self.dry.set(dry);
        self.wet.set(wet);
    }
}

impl Effect for ReverbT3 {
    fn kind(&self) -> EffectKind {
        EffectKind::ReverbT3
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        let (frames, _) = buf.as_chunks_mut::<2>();
        for _ in 0..frames.len() {
            self.size.tick();
        }
        self.shape = shape_of(self.size.value(), self.rate);
        for frame in frames {
            let (mut dry, wet) = (self.dry.tick(), self.wet.tick());
            if self.freeze {
                self.frozen_in += 1.0;
                if self.frozen_in >= FREEZE_S * self.rate {
                    dry = 0.0;
                }
            }
            let tail = self.tank.tick(*frame, &self.shape);
            for ch in 0..2 {
                let (lp, _) = self.low.tick(ch, tail[ch]);
                let (_, hp) = self.high.tick(ch, lp);
                frame[ch] = frame[ch] * dry + hp * wet;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.size.set(amount);
        self.set_mix((10.0 * amount).min(0.5));
        self.set_lp(GROUP_LP);
        self.set_hp(GROUP_HP);
        self.freeze = false;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.size.set(value),
            1 => self.set_lp(value),
            2 => self.set_hp(value),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        if index == 0 {
            self.freeze = on;
            self.frozen_in = 0.0;
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.set_mix(dry_wet);
        true
    }

    fn has_tail(&self) -> bool {
        true
    }

    fn reset(&mut self) {
        self.tank.clear();
        self.low.reset();
        self.high.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn size_follows_its_curve() {
        assert!((decay_s(0.0) - 0.015_85).abs() < 1e-4);
        assert!((decay_s(0.5) - 1.0).abs() < 1e-9);
        assert!((decay_s(1.0) - 63.1).abs() < 0.1);
        let big = shape_of(1.0, f64::from(RATE));
        let small = shape_of(0.0, f64::from(RATE));
        assert!(
            (big.main[0] / small.main[0] - 4.0).abs() < 0.01,
            "two octaves"
        );
        assert!(
            (pass_gain(1.0, 1.0) - 0.001).abs() < 1e-6,
            "−60 dB per decay"
        );
        assert!((lp_hz(1.0) - 14_917.0).abs() < 5.0 && (hp_hz(0.0) - 82.4).abs() < 0.1);
    }

    #[test]
    fn larger_rooms_ring_longer() {
        let tail = |size: f32| {
            let mut r = ReverbT3::new(f64::from(RATE));
            r.set_amount(size);
            let out = run(&mut r, &impulse(RATE as usize), ctx());
            rms(&out[2 * 24_000..])
        };
        assert!(tail(0.6) > 3.0 * tail(0.2));
        let mut r = ReverbT3::new(f64::from(RATE));
        r.set_amount(1.0);
        let out = run(&mut r, &sine(200.0, 2 * RATE as usize, 0.9), ctx());
        assert!(out.iter().all(|v| v.is_finite() && v.abs() < 8.0));
    }
}
