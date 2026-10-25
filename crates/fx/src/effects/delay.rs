use backspn_dsp::{DelayLine, OnePole, Smoothed};

use super::{EffectKind, NOTE_BEATS, pick, stretch};
use crate::{Context, Effect};

const MAX_SECONDS: f64 = 4.0;

#[derive(Debug)]
pub struct Delay {
    rate: f64,
    line: DelayLine,
    tone: OnePole,
    time: Smoothed,
    feedback: Smoothed,
    mix: Smoothed,
    filter: f32,
    beats: f64,
    freeze: bool,
    decay: f32,
}

impl Delay {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            line: DelayLine::new((MAX_SECONDS * rate) as usize),
            tone: OnePole::default(),
            time: Smoothed::new(rate as f32 / 4.0, 0.05, rate),
            feedback: Smoothed::new(0.5, 0.02, rate),
            mix: Smoothed::new(0.0, 0.02, rate),
            filter: 0.0,
            beats: 0.5,
            freeze: false,
            decay: 0.0,
        }
    }
}

impl Effect for Delay {
    fn kind(&self) -> EffectKind {
        EffectKind::Delay
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let samples =
            (self.beats * 60.0 / ctx.bpm.max(20.0) * self.rate).min(self.line.capacity() as f64);
        self.time.set(samples as f32);
        let cutoff = if self.filter < 0.0 {
            20_000.0 * (300.0f64 / 20_000.0).powf(f64::from(-self.filter))
        } else {
            20.0 * (3_000.0f64 / 20.0).powf(f64::from(self.filter))
        };
        self.tone.set(cutoff, self.rate);
        let high = self.filter > 0.0;
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let t = self.time.tick();
            let fb = if self.freeze {
                1.0
            } else {
                stretch(self.feedback.tick(), self.decay, 0.0, 0.95)
            };
            let mix = self.mix.tick();
            let echo = self.line.read(t);
            let mut back = [0.0; 2];
            for ch in 0..2 {
                let (lp, hp) = self.tone.tick(ch, echo[ch]);
                let toned = if self.filter.abs() < 0.02 {
                    echo[ch]
                } else if high {
                    hp
                } else {
                    lp
                };
                let input = if self.freeze { 0.0 } else { frame[ch] };
                back[ch] = (input + toned * fb).clamp(-4.0, 4.0);
            }
            self.line.push(back);
            frame[0] += echo[0] * mix;
            frame[1] += echo[1] * mix;
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.beats = 0.5;
        self.mix.set(amount);
        self.feedback.set(0.3 + 0.4 * amount);
        self.filter = 0.0;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.filter = (value * 2.0 - 1.0).clamp(-1.0, 1.0),
            1 => {
                self.feedback.set(value.clamp(0.0, 1.0) * 0.95);
                self.mix.set(0.8);
            }
            2 => self.beats = pick(&NOTE_BEATS, value),
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
        self.line.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn echoes_land_on_the_beat_fraction() {
        let mut d = Delay::new(f64::from(RATE));
        d.set_amount(1.0);
        let out = run(&mut d, &impulse(RATE as usize), ctx());
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let (peak, _) = left[1_000..]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap();
        assert!(
            (peak as i64 + 1_000 - 12_000).abs() <= 2,
            "echo at {}",
            peak + 1_000
        );
    }

    #[test]
    fn zero_amount_is_transparent() {
        let mut d = Delay::new(f64::from(RATE));
        d.set_amount(0.0);
        let input = sine(440.0, 48_000, 0.5);
        let out = run(&mut d, &input, ctx());
        let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
        assert!(rms(&diff[2 * 4_800..]) < 1e-4);
    }
}
