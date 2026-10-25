use backspn_dsp::Smoothed;

use super::{EffectKind, stretch};
use crate::{Context, Effect};

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;
const INPUT_GAIN: f32 = 0.015;

#[derive(Debug, Clone)]
struct Comb {
    buffer: Box<[f32]>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self {
            buffer: vec![0.0; len.max(1)].into_boxed_slice(),
            pos: 0,
            store: 0.0,
        }
    }

    #[inline]
    fn tick(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let y = self.buffer[self.pos];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buffer[self.pos] = x + self.store * feedback;
        self.pos = (self.pos + 1) % self.buffer.len();
        y
    }
}

#[derive(Debug, Clone)]
struct AllPass {
    buffer: Box<[f32]>,
    pos: usize,
}

impl AllPass {
    fn new(len: usize) -> Self {
        Self {
            buffer: vec![0.0; len.max(1)].into_boxed_slice(),
            pos: 0,
        }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let b = self.buffer[self.pos];
        self.buffer[self.pos] = x + b * 0.5;
        self.pos = (self.pos + 1) % self.buffer.len();
        b - x
    }
}

#[derive(Debug)]
pub struct Reverb {
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<AllPass>; 2],
    size: Smoothed,
    damp: Smoothed,
    mix: Smoothed,
    freeze: bool,
    decay: f32,
}

impl Reverb {
    pub fn new(rate: f64) -> Self {
        let scale = |n: usize| ((n as f64) * rate / 44_100.0).round() as usize;
        let side = |offset: usize| -> (Vec<Comb>, Vec<AllPass>) {
            (
                COMBS
                    .iter()
                    .map(|&n| Comb::new(scale(n + offset)))
                    .collect(),
                ALLPASSES
                    .iter()
                    .map(|&n| AllPass::new(scale(n + offset)))
                    .collect(),
            )
        };
        let (cl, al) = side(0);
        let (cr, ar) = side(SPREAD);
        Self {
            combs: [cl, cr],
            allpasses: [al, ar],
            size: Smoothed::new(0.7, 0.05, rate),
            damp: Smoothed::new(0.4, 0.05, rate),
            mix: Smoothed::new(0.0, 0.02, rate),
            freeze: false,
            decay: 0.0,
        }
    }
}

impl Effect for Reverb {
    fn kind(&self) -> EffectKind {
        EffectKind::Reverb
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let size = stretch(self.size.tick(), self.decay, 0.05, 0.98);
            let damp = self.damp.tick();
            let mix = self.mix.tick();
            let (feedback, damp, gain) = if self.freeze {
                (1.0, 0.0, 0.0)
            } else {
                (0.7 + 0.28 * size, damp * 0.4, INPUT_GAIN)
            };
            let input = (frame[0] + frame[1]) * gain;
            for ch in 0..2 {
                let mut acc = 0.0;
                for comb in &mut self.combs[ch] {
                    acc += comb.tick(input, feedback, damp);
                }
                for ap in &mut self.allpasses[ch] {
                    acc = ap.tick(acc);
                }
                frame[ch] += acc * mix * 3.0;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.size.set(0.8);
        self.damp.set(0.4);
        self.mix.set(amount);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match index {
            0 => self.size.set(value),
            1 => self.damp.set(value),
            2 => self.mix.set(value),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        if index == 0 {
            self.freeze = on;
        }
    }

    fn has_tail(&self) -> bool {
        true
    }

    fn set_decay(&mut self, decay: f32) {
        self.decay = decay;
    }

    fn reset(&mut self) {
        for side in &mut self.combs {
            for c in side {
                c.buffer.fill(0.0);
                c.store = 0.0;
            }
        }
        for side in &mut self.allpasses {
            for a in side {
                a.buffer.fill(0.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn tail_follows_an_impulse_and_decays() {
        let mut r = Reverb::new(f64::from(RATE));
        r.set_amount(1.0);
        let out = run(&mut r, &impulse(4 * RATE as usize), ctx());
        let early = rms(&out[2 * 2_400..2 * 24_000]);
        let late = rms(&out[2 * 168_000..]);
        assert!(early > 1e-3, "no tail: {early}");
        assert!(late < early / 10.0, "no decay: {late} vs {early}");
    }

    #[test]
    fn freeze_sustains() {
        let mut r = Reverb::new(f64::from(RATE));
        r.set_amount(1.0);
        let _ = run(&mut r, &sine(300.0, RATE as usize / 2, 0.5), ctx());
        r.set_button(0, true);
        let held = run(&mut r, &vec![0.0; 2 * 4 * RATE as usize], ctx());
        let early = rms(&held[..2 * 24_000]);
        let late = rms(&held[held.len() - 2 * 24_000..]);
        assert!(
            early > 1e-3 && late > early * 0.5,
            "freeze lost energy: {early} → {late}"
        );
    }
}
