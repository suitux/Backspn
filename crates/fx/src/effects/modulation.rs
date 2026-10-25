use std::f32::consts::TAU;

use backspn_dsp::{DelayLine, Smoothed};

use super::{EffectKind, LFO_BEATS, beats_per_sample, pick};
use crate::{Context, Effect};

#[inline]
fn lfo(beat: f64, period: f64, offset: f32) -> f32 {
    let phase = ((beat / period).rem_euclid(1.0)) as f32 + offset;
    0.5 - 0.5 * (TAU * phase).cos()
}

#[derive(Debug)]
pub struct Flanger {
    rate: f64,
    line: DelayLine,
    depth: Smoothed,
    feedback: f32,
    period: f64,
    last: [f32; 2],
}

impl Flanger {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            line: DelayLine::new((0.02 * rate) as usize),
            depth: Smoothed::new(0.0, 0.02, rate),
            feedback: 0.4,
            period: 2.0,
            last: [0.0; 2],
        }
    }
}

impl Effect for Flanger {
    fn kind(&self) -> EffectKind {
        EffectKind::Flanger
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let step = beats_per_sample(ctx, self.rate);
        let ms = self.rate as f32 / 1_000.0;
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            let beat = ctx.beat + i as f64 * step;
            let depth = self.depth.tick();
            let input = [
                frame[0] + self.last[0] * self.feedback,
                frame[1] + self.last[1] * self.feedback,
            ];
            self.line.push(input);
            let wet: [f32; 2] = std::array::from_fn(|ch| {
                let delay = (0.3 + 6.0 * lfo(beat, self.period, ch as f32 * 0.25)) * ms;
                self.line.read(delay)[ch]
            });
            self.last = wet;
            for ch in 0..2 {
                frame[ch] = (frame[ch] + wet[ch] * depth) / (1.0 + 0.5 * depth);
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.depth.set(amount);
        self.feedback = 0.3 + 0.4 * amount;
        self.period = 4.0;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match index {
            0 => self.depth.set(value),
            1 => self.feedback = value * 0.9,
            2 => self.period = pick(&LFO_BEATS, value),
            _ => {}
        }
    }

    fn set_button(&mut self, _index: usize, _on: bool) {}

    fn reset(&mut self) {
        self.line.clear();
        self.last = [0.0; 2];
    }
}

const STAGES: usize = 6;

#[derive(Debug)]
pub struct Phaser {
    rate: f64,
    state: [[f32; STAGES]; 2],
    depth: Smoothed,
    feedback: f32,
    period: f64,
    last: [f32; 2],
}

impl Phaser {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            state: [[0.0; STAGES]; 2],
            depth: Smoothed::new(0.0, 0.02, rate),
            feedback: 0.4,
            period: 2.0,
            last: [0.0; 2],
        }
    }
}

impl Effect for Phaser {
    fn kind(&self) -> EffectKind {
        EffectKind::Phaser
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let step = beats_per_sample(ctx, self.rate);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            let beat = ctx.beat + i as f64 * step;
            let depth = self.depth.tick();
            for ch in 0..2 {
                let sweep = lfo(beat, self.period, ch as f32 * 0.25);
                let freq = 200.0 * (4_000.0f32 / 200.0).powf(sweep);
                let t = (std::f32::consts::PI * freq / self.rate as f32).tan();
                let a = (t - 1.0) / (t + 1.0);
                let mut y = frame[ch] + self.last[ch] * self.feedback;
                for z in &mut self.state[ch] {
                    let out = a * y + *z;
                    *z = y - a * out;
                    y = out;
                }
                self.last[ch] = y;
                frame[ch] = (frame[ch] + y * depth) / (1.0 + 0.5 * depth);
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.depth.set(amount);
        self.feedback = 0.2 + 0.4 * amount;
        self.period = 2.0;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match index {
            0 => self.depth.set(value),
            1 => self.feedback = value * 0.85,
            2 => self.period = pick(&LFO_BEATS, value),
            _ => {}
        }
    }

    fn set_button(&mut self, _index: usize, _on: bool) {}

    fn reset(&mut self) {
        self.state = [[0.0; STAGES]; 2];
        self.last = [0.0; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn lfo_locks_to_beats() {
        assert!(lfo(0.0, 2.0, 0.0) < 1e-6);
        assert!((lfo(1.0, 2.0, 0.0) - 1.0).abs() < 1e-6);
        assert!(lfo(2.0, 2.0, 0.0) < 1e-6);
    }

    #[test]
    fn modulators_change_the_signal_and_stay_bounded() {
        for effect in [
            &mut Flanger::new(f64::from(RATE)) as &mut dyn Effect,
            &mut Phaser::new(f64::from(RATE)),
        ] {
            effect.set_amount(1.0);
            let input = sine(800.0, RATE as usize * 2, 0.5);
            let out = run(effect, &input, ctx());
            let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
            assert!(
                rms(&diff[RATE as usize..]) > 0.05,
                "{:?} did nothing",
                effect.kind()
            );
            assert!(
                out.iter().all(|v| v.abs() < 1.5),
                "{:?} blew up",
                effect.kind()
            );
        }
    }
}
