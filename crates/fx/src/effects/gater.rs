use super::{EffectKind, LfoStart, beats_per_sample, pick, stretch};
use crate::{Context, Effect};

const RATES: [f64; 5] = [0.03125, 0.0625, 0.125, 0.25, 0.5];

#[derive(Debug)]
pub struct Gater {
    rate: f64,
    depth: f32,
    period: f64,
    duty: f32,
    gain: f32,
    smoothing: f32,
    decay: f32,
    start: LfoStart,
    active: bool,
}

impl Gater {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            depth: 0.0,
            period: 0.0625,
            duty: 0.5,
            gain: 1.0,
            smoothing: (1.0 - (-1.0 / (0.002 * rate)).exp()) as f32,
            decay: 0.0,
            start: LfoStart::default(),
            active: false,
        }
    }
}

impl Effect for Gater {
    fn kind(&self) -> EffectKind {
        EffectKind::Gater
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let step = beats_per_sample(ctx, self.rate);
        self.start.begin(ctx, self.period, 0.5);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            let beat = self.start.beat(ctx.beat + i as f64 * step);
            let phase = (beat / self.period).rem_euclid(1.0) as f32;
            let open = if phase < stretch(self.duty, self.decay, 0.05, 0.95) {
                1.0
            } else {
                1.0 - self.depth
            };
            self.gain += (open - self.gain) * self.smoothing;
            frame[0] *= self.gain;
            frame[1] *= self.gain;
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.depth = amount.clamp(0.0, 1.0);
        self.period = 0.0625;
        self.duty = 0.5;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match index {
            0 => self.depth = value,
            1 => self.period = pick(&RATES, 1.0 - value),
            2 => self.duty = 0.1 + 0.8 * value,
            _ => {}
        }
    }

    fn set_button(&mut self, _index: usize, _on: bool) {}

    fn set_decay(&mut self, decay: f32) {
        self.decay = decay;
    }

    fn set_active(&mut self, on: bool) {
        if on && !self.active {
            self.start.start();
        }
        self.active = on;
    }

    fn lfo_reset(&mut self) {
        self.start.reset();
    }

    fn reset(&mut self) {
        self.gain = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn gates_in_time() {
        let mut g = Gater::new(f64::from(RATE));
        g.set_amount(1.0);
        let input = vec![1.0f32; 2 * RATE as usize];
        let out = run(&mut g, &input, ctx());
        let at = |frame: usize| out[2 * frame];
        assert!(at(30_000 + 375) > 0.95, "open half");
        assert!(at(30_000 + 1_125) < 0.05, "closed half");
    }
}
