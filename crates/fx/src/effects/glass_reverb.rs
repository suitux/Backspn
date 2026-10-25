use backspn_dsp::{OnePole, Smoothed};

use super::tank::{Shape, Tank};
use super::{EffectKind, note_hz, reverb_mix};
use crate::{Context, Effect};

const MAIN: [f64; 2] = [1.0, 0.86];
const DIFFUSERS: [[f64; 2]; 3] = [[0.13, 0.17], [0.21, 0.27], [0.33, 0.41]];
const INPUT_GAIN: f32 = 0.51;
const FREEZE_FEEDBACK: f32 = 0.99;
const FREEZE_S: f64 = 0.125;
const GROUP_SIZE: f32 = 0.6;
const GROUP_ICING: f32 = 0.96;

fn size_ms(v: f32) -> f64 {
    let v = f64::from(v.clamp(0.0, 1.0));
    12.0 + 98.0 * v * (2.0 - v)
}

fn color_hz(v: f32) -> f64 {
    note_hz(39.0 + 77.0 * f64::from(v.clamp(0.0, 1.0)))
}

fn icing_of(v: f32) -> (f32, f32) {
    let v = v.clamp(0.0, 1.0);
    let feedback = 0.2 + 0.66 * v * (2.0 - v);
    let sq = v * v;
    let level = if sq <= 0.8 {
        1.333
    } else {
        1.333 + 0.66 * (5.0 * (sq - 0.8)).powi(2)
    };
    (feedback, level)
}

#[derive(Debug)]
pub struct GlassReverb {
    rate: f64,
    tank: Tank,
    color: OnePole,
    size: Smoothed,
    shape: Shape,
    feedback: f32,
    level: f32,
    dry: Smoothed,
    wet: Smoothed,
    single: bool,
    freeze: bool,
    frozen_in: f64,
}

impl GlassReverb {
    pub fn new(rate: f64) -> Self {
        let (feedback, level) = icing_of(GROUP_ICING);
        let mut ice = Self {
            rate,
            tank: Tank::new(rate),
            color: OnePole::default(),
            size: Smoothed::new(GROUP_SIZE, 0.05, rate),
            shape: Shape::default(),
            feedback,
            level,
            dry: Smoothed::new(1.0, 0.01, rate),
            wet: Smoothed::new(0.0, 0.01, rate),
            single: false,
            freeze: false,
            frozen_in: 0.0,
        };
        ice.set_color(0.753);
        ice.reshape();
        ice
    }

    fn set_color(&mut self, v: f32) {
        self.color.set(color_hz(v), self.rate);
    }

    fn set_icing(&mut self, v: f32) {
        (self.feedback, self.level) = icing_of(v);
    }

    fn set_mix(&mut self, dry_wet: f32) {
        let (dry, wet) = reverb_mix(dry_wet);
        self.dry.set(dry);
        self.wet.set(wet);
    }

    fn reshape(&mut self) {
        let len = size_ms(self.size.value()) / 1000.0 * self.rate;
        let gain = if self.freeze {
            FREEZE_FEEDBACK
        } else {
            self.feedback
        };
        self.shape = Shape {
            main: MAIN.map(|r| (r * len) as f32),
            gain: [gain; 2],
            diffusers: DIFFUSERS.map(|pair| pair.map(|r| (r * len) as f32)),
        };
    }
}

impl Effect for GlassReverb {
    fn kind(&self) -> EffectKind {
        EffectKind::GlassReverb
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        let (frames, _) = buf.as_chunks_mut::<2>();
        for _ in 0..frames.len() {
            self.size.tick();
        }
        self.reshape();
        for frame in frames {
            let (mut dry, wet) = (self.dry.tick(), self.wet.tick());
            let mut input = INPUT_GAIN;
            if self.freeze {
                self.frozen_in += 1.0;
                if self.single && self.frozen_in >= FREEZE_S * self.rate {
                    dry = 0.0;
                    input = 0.0;
                }
            }
            let tail = self
                .tank
                .tick([frame[0] * input, frame[1] * input], &self.shape);
            for ch in 0..2 {
                let (colored, _) = self.color.tick(ch, tail[ch]);
                frame[ch] = frame[ch] * dry + colored * wet * self.level;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.single = false;
        self.freeze = false;
        self.set_color(amount);
        self.set_mix((10.0 * amount).min(0.5));
        self.size.set(GROUP_SIZE);
        self.set_icing(GROUP_ICING);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.size.set(value),
            1 => self.set_color(value),
            2 => self.set_icing(value),
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
        self.single = true;
        self.set_mix(dry_wet);
        true
    }

    fn has_tail(&self) -> bool {
        true
    }

    fn reset(&mut self) {
        self.tank.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn knobs_follow_their_curves() {
        assert!((size_ms(0.0) - 12.0).abs() < 1e-9 && (size_ms(1.0) - 110.0).abs() < 1e-9);
        assert!((color_hz(0.0) - 77.8).abs() < 0.1 && (color_hz(1.0) - 6_644.9).abs() < 1.0);
        let (fb, level) = icing_of(0.0);
        assert!((fb - 0.2).abs() < 1e-6 && (level - 1.333).abs() < 1e-6);
        let (fb, level) = icing_of(1.0);
        assert!((fb - 0.86).abs() < 1e-6 && (level - 1.993).abs() < 1e-3);
    }

    #[test]
    fn freeze_keeps_the_reverberation_going() {
        let tail = |freeze: bool| {
            let mut r = GlassReverb::new(f64::from(RATE));
            r.set_dry_wet(0.5);
            r.set_param(2, 0.0);
            let _ = run(&mut r, &sine(300.0, RATE as usize / 4, 0.5), ctx());
            r.set_button(0, freeze);
            let out = run(&mut r, &vec![0.0; 2 * RATE as usize], ctx());
            rms(&out[2 * 36_000..])
        };
        assert!(tail(false) < 1e-3, "short without it");
        assert!(tail(true) > 0.01, "rings on frozen");
    }
}
