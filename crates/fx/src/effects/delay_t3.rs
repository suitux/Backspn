use backspn_dsp::{DelayLine, Smoothed, Svf};

use super::{EffectKind, delay_mix, note_hz};
use crate::{Context, Effect};

const MAX_S: f64 = 8.0;
pub(crate) const SYNC_32NDS: [f64; 7] = [32.0, 12.0, 8.0, 6.0, 4.0, 2.0, 1.0];
const LP_NOTES: [f64; 5] = [12.0, 83.0, 130.0, 130.0, 130.0];
const HP_NOTES: [f64; 5] = [12.0, 12.0, 12.0, 60.0, 130.0];
const GROUP_FEEDBACK: f32 = 0.7;
const GROUP_FILTER: f32 = 0.48;
const GROUP_DRY_WET: f32 = 0.5;

fn sync_beats(v: f32) -> f64 {
    let i = ((6.0 * v.clamp(0.0, 1.0)) + 0.5).floor() as usize;
    SYNC_32NDS[i.min(6)] / 32.0 * 4.0
}

fn free_seconds(v: f32) -> f64 {
    0.005 + 2.0 * f64::from(1.0 - v.clamp(0.0, 1.0)).powi(4)
}

fn feedback_of(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    0.999_96 * v * (1.5 - 0.5 * v)
}

pub(crate) fn filter_of(v: f32, rate: f64) -> (f64, f64) {
    let pos = f64::from(v.clamp(0.0, 1.0)) * 4.0;
    let i = (pos.floor() as usize).min(3);
    let frac = pos - i as f64;
    let at = |notes: &[f64; 5]| {
        let note = notes[i] + (notes[i + 1] - notes[i]) * frac;
        note_hz(note).clamp(rate / 24_576.0, 0.47 * rate)
    };
    (at(&LP_NOTES), at(&HP_NOTES))
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct DelayT3 {
    rate: f64,
    line: DelayLine,
    low: Svf,
    high: Svf,
    time: Smoothed,
    feedback: f32,
    filter: f32,
    rate_knob: f32,
    free: bool,
    freeze: bool,
    frozen: bool,
    waited: f64,
    dry: Smoothed,
    wet: Smoothed,
    timed: bool,
}

impl DelayT3 {
    pub fn new(rate: f64) -> Self {
        let mut delay = Self {
            rate,
            line: DelayLine::new((MAX_S * rate) as usize),
            low: Svf::default(),
            high: Svf::default(),
            time: Smoothed::new(1.0, 0.1, rate),
            feedback: feedback_of(GROUP_FEEDBACK),
            filter: GROUP_FILTER,
            rate_knob: 0.698,
            free: false,
            freeze: false,
            frozen: false,
            waited: 0.0,
            dry: Smoothed::new(1.0, 0.01, rate),
            wet: Smoothed::new(delay_mix(GROUP_DRY_WET).1, 0.01, rate),
            timed: false,
        };
        delay.set_filter(GROUP_FILTER);
        delay
    }

    fn set_filter(&mut self, v: f32) {
        self.filter = v;
        let (lp, hp) = filter_of(v, self.rate);
        self.low.set(lp, std::f64::consts::FRAC_1_SQRT_2, self.rate);
        self.high
            .set(hp, std::f64::consts::FRAC_1_SQRT_2, self.rate);
    }

    fn frames(&self, ctx: &Context) -> f64 {
        let s = if self.free {
            free_seconds(self.rate_knob)
        } else {
            sync_beats(self.rate_knob) * 60.0 / ctx.bpm.max(20.0)
        };
        (s * self.rate).clamp(1.0, self.line.capacity() as f64)
    }

    fn input_gain(&self) -> f32 {
        let over = ((self.feedback / 0.999_96 - 0.5) * 2.0).max(0.0);
        1.0 - over * over * 0.1 + 0.33 * self.filter
    }
}

impl Effect for DelayT3 {
    fn kind(&self) -> EffectKind {
        EffectKind::DelayT3
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let target = self.frames(ctx) as f32;
        if !self.timed {
            self.time = Smoothed::new(target, 0.1, self.rate);
            self.timed = true;
        }
        self.time.set(target);
        let gain = self.input_gain();
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let (dry, wet) = (self.dry.tick(), self.wet.tick());
            let t = self.time.tick();
            if self.freeze && !self.frozen {
                self.waited += 1.0;
                self.frozen = self.waited >= f64::from(t);
            }
            let raw = self.line.read(t);
            let mut echo = [0.0; 2];
            let mut back = [0.0; 2];
            for ch in 0..2 {
                let (lp, _) = self.low.tick(ch, raw[ch]);
                let (_, hp) = self.high.tick(ch, lp);
                echo[ch] = hp;
                let (input, fb) = if self.frozen {
                    (0.0, 1.0)
                } else {
                    (frame[ch] * gain, self.feedback)
                };
                let x = (input + hp * fb).clamp(-512.0, 512.0);
                back[ch] = x * (1.0 - x.abs() / 1024.0);
            }
            self.line.push(back);
            let (dry, wet) = if self.frozen { (0.0, 1.0) } else { (dry, wet) };
            frame[0] = frame[0] * dry + echo[0] * wet;
            frame[1] = frame[1] * dry + echo[1] * wet;
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.set_dry_wet(GROUP_DRY_WET);
        self.free = false;
        self.rate_knob = amount;
        self.feedback = feedback_of(GROUP_FEEDBACK);
        self.set_filter(GROUP_FILTER);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.rate_knob = value,
            1 => self.feedback = feedback_of(value),
            2 => self.set_filter(value),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.free = on,
            1 => {
                self.freeze = on;
                self.frozen = false;
                self.waited = 0.0;
            }
            _ => {}
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        let (dry, wet) = delay_mix(dry_wet);
        self.dry.set(dry);
        self.wet.set(wet);
        true
    }

    fn has_tail(&self) -> bool {
        true
    }

    fn reset(&mut self) {
        self.line.clear();
        self.low.reset();
        self.high.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    fn left(out: &[f32]) -> Vec<f32> {
        out.iter().step_by(2).copied().collect()
    }

    #[test]
    fn knobs_follow_their_curves() {
        assert!((sync_beats(0.0) - 4.0).abs() < 1e-9, "a whole bar");
        assert!((sync_beats(1.0 / 6.0) - 1.5).abs() < 1e-9, "3/8");
        assert!((sync_beats(0.5) - 0.75).abs() < 1e-9, "3/16");
        assert!((sync_beats(1.0) - 0.125).abs() < 1e-9, "1/32");
        assert!((free_seconds(0.0) - 2.005).abs() < 1e-9);
        assert!((free_seconds(1.0) - 0.005).abs() < 1e-9);
        assert!((feedback_of(0.5) / 0.999_96 - 0.625).abs() < 1e-6);
        let (lp, hp) = filter_of(0.5, f64::from(RATE));
        assert!(lp > 14_000.0 && hp < 20.0, "centre is open: {lp} {hp}");
        let (lp, _) = filter_of(0.25, f64::from(RATE));
        assert!((lp - 987.8).abs() < 1.0, "a quarter: {lp}");
    }

    #[test]
    fn group_knob_sets_the_bar_fraction() {
        let mut d = DelayT3::new(f64::from(RATE));
        d.set_amount(4.0 / 6.0);
        let out = left(&run(&mut d, &impulse(RATE as usize), ctx()));
        let peak = out[1_000..]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap()
            .0
            + 1_000;
        assert!((peak as i64 - 12_000).abs() <= 2, "echo at {peak}");
    }

    #[test]
    fn freeze_waits_a_whole_echo_then_loops_it() {
        let mut d = DelayT3::new(f64::from(RATE));
        d.set_param(0, 4.0 / 6.0);
        d.set_param(1, 0.0);
        let input = sine(440.0, 12_000, 0.5);
        let _ = run(&mut d, &input, ctx());
        d.set_button(1, true);
        let _ = run(&mut d, &input, ctx());
        let out = run(&mut d, &vec![0.0; 2 * RATE as usize], ctx());
        assert!(rms(&out[2 * 30_000..]) > 0.2, "the echo goes round");
    }
}
