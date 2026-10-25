use backspn_dsp::{DelayLine, OnePole, Smoothed};

use super::{EffectKind, delay_mix, note_hz, stretch};
use crate::{Context, Effect};

const LINE_FRAMES: usize = 262_144;
const LEFT_32NDS: [f64; 7] = [32.0, 12.0, 8.0, 6.0, 4.0, 2.0, 1.0];
const RIGHT_32NDS: [f64; 7] = [32.0, 16.0, 12.0, 8.0, 6.0, 4.0, 1.0];
const GROUP_RATE: f32 = 0.5;
const RAMP_S: f64 = 0.05;

fn step_of(v: f32) -> f64 {
    (6.0 * f64::from(v.clamp(0.0, 1.0)) + 0.5).floor().min(6.0)
}

fn feedback_of(v: f32) -> f64 {
    let v = f64::from(v.clamp(0.0, 1.0));
    ((1.666 - 0.666 * v) * v).powi(2)
}

fn filter_notes(v: f32) -> (f64, f64) {
    let v = f64::from(v.clamp(0.0, 1.0));
    (-20.0 + 85.5 * v, 136.5 - 25.7 * v)
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Delay {
    rate: f64,
    line: DelayLine,
    low_cut: [OnePole; 2],
    high_cut: [OnePole; 2],
    unit: Smoothed,
    index: Smoothed,
    timed: bool,
    rate_knob: f32,
    feedback: f32,
    filter: f32,
    spread: bool,
    freeze: bool,
    frozen: bool,
    waited: f64,
    dry_wet: f32,
    dry: Smoothed,
    wet: Smoothed,
    active: bool,
    decay: f32,
}

impl Delay {
    pub fn new(rate: f64) -> Self {
        let mut d = Self {
            rate,
            line: DelayLine::new(LINE_FRAMES),
            low_cut: [OnePole::default(); 2],
            high_cut: [OnePole::default(); 2],
            unit: Smoothed::new(1.0, RAMP_S / 3.0, rate),
            index: Smoothed::new(step_of(GROUP_RATE) as f32, RAMP_S / 3.0, rate),
            timed: false,
            rate_knob: GROUP_RATE,
            feedback: 0.0,
            filter: 0.0,
            spread: false,
            freeze: false,
            frozen: false,
            waited: 0.0,
            dry_wet: 0.0,
            dry: Smoothed::new(1.0, RAMP_S / 3.0, rate),
            wet: Smoothed::new(0.0, RAMP_S / 3.0, rate),
            active: false,
            decay: 0.0,
        };
        d.set_filter(0.0);
        d.set_mix(0.0);
        d
    }

    fn set_filter(&mut self, v: f32) {
        self.filter = v.clamp(0.0, 1.0);
        let (hp, lp) = filter_notes(v);
        for f in &mut self.low_cut {
            f.set(note_hz(hp).min(0.494 * self.rate), self.rate);
        }
        for f in &mut self.high_cut {
            f.set(note_hz(lp).min(0.494 * self.rate), self.rate);
        }
    }

    fn set_mix(&mut self, v: f32) {
        self.dry_wet = v.clamp(0.0, 1.0);
        let (dry, wet) = delay_mix(v);
        self.dry.set(dry);
        self.wet.set(wet);
    }

    fn time(table: &[f64; 7], index: f64, unit: f64) -> f64 {
        let i = (index.floor() as usize).min(5);
        let frac = (index - i as f64).clamp(0.0, 1.0);
        let at = |k: usize| table[k.min(6)] * unit;
        at(i) + (at(i + 1) - at(i)) * frac
    }
}

impl Effect for Delay {
    fn kind(&self) -> EffectKind {
        EffectKind::Delay
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let unit = (self.rate / (8.0 * (ctx.bpm / 60.0).max(0.01))).round() as f32;
        if !self.timed {
            self.unit = Smoothed::new(unit, RAMP_S / 3.0, self.rate);
            self.timed = true;
        }
        self.unit.set(unit);
        self.index.set(step_of(self.rate_knob) as f32);
        let f = feedback_of(self.feedback);
        let extra = if self.filter > 0.2 {
            0.18 * f64::from(self.filter - 0.2) / 0.8
        } else {
            0.0
        };
        let loop_gain = stretch((0.91 * f + extra) as f32, self.decay, 0.0, 0.95);
        let g = if f <= 0.5 { f } else { 2.0 * (f - 0.5).powi(2) };
        let input_gain = (1.0 - g + 0.33 * f64::from(self.filter)) as f32;
        let on = if self.active { 1.0 } else { 0.0 };
        let offset =
            (0.002 * self.rate * f64::from(self.dry_wet * (1.0 - self.dry_wet)) + 0.5).floor();
        let max = (LINE_FRAMES - 4) as f64;
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let unit = f64::from(self.unit.tick());
            let index = f64::from(self.index.tick());
            let left = Self::time(&LEFT_32NDS, index, unit);
            let right = Self::time(
                if self.spread && self.active {
                    &RIGHT_32NDS
                } else {
                    &LEFT_32NDS
                },
                index,
                unit,
            );
            if self.freeze && !self.frozen {
                self.waited += 1.0;
                self.frozen = self.waited >= left;
            }
            let shift = if self.frozen { 0.0 } else { offset };
            let times = [
                (left + shift).clamp(1.0, max),
                (right - shift).clamp(1.0, max),
            ];
            let (mut dry, mut wet) = (self.dry.tick(), self.wet.tick());
            if self.frozen {
                (dry, wet) = (0.0, 1.0);
            } else if !self.active {
                dry = 1.0;
            }
            let mut echo = [0.0f32; 2];
            let mut back = [0.0f32; 2];
            for ch in 0..2 {
                let raw = self.line.read(times[ch] as f32)[ch];
                let (_, high) = self.low_cut[ch].tick(ch, raw);
                let (filtered, _) = self.high_cut[ch].tick(ch, high);
                echo[ch] = filtered;
                let (input, fb) = if self.frozen {
                    (0.0, 1.0)
                } else {
                    (frame[ch] * input_gain * on, loop_gain)
                };
                back[ch] = (input + filtered * fb).clamp(-8.0, 8.0);
            }
            self.line.push(back);
            frame[0] = frame[0] * dry + echo[0] * wet;
            frame[1] = frame[1] * dry + echo[1] * wet;
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.rate_knob = GROUP_RATE;
        self.set_filter(0.0);
        self.spread = false;
        self.freeze = false;
        self.frozen = false;
        let v = amount.clamp(0.0, 1.0);
        self.feedback = (2.0 * v - 1.0).max(0.0);
        self.set_mix(v.min(0.5));
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.rate_knob = value,
            1 => self.feedback = value,
            2 => self.set_filter(value),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.spread = on,
            1 => {
                self.freeze = on;
                self.frozen = false;
                self.waited = 0.0;
            }
            _ => {}
        }
    }

    fn set_active(&mut self, on: bool) {
        self.active = on;
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.set_mix(dry_wet);
        true
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
    fn knobs_follow_their_curves() {
        assert!((step_of(0.5) - 3.0).abs() < 1e-9, "3/16 at the centre");
        assert!((feedback_of(1.0) - 1.0).abs() < 1e-9);
        assert!((feedback_of(0.5) - 0.444_222).abs() < 1e-5);
        let (hp, lp) = filter_notes(0.0);
        assert!(note_hz(hp) < 3.0 && note_hz(lp) > 20_000.0, "open");
    }

    #[test]
    fn group_echoes_at_three_sixteenths() {
        let mut d = Delay::new(f64::from(RATE));
        d.set_amount(0.5);
        d.set_active(true);
        let out = run(&mut d, &impulse(RATE as usize), ctx());
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let peak = left[1_000..]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap()
            .0
            + 1_000;
        assert!((peak as i64 - 18_000).abs() <= 30, "echo at {peak}");
        assert!(
            left[36_100..37_000].iter().all(|v| v.abs() < 0.01),
            "one repeat only"
        );
    }

    #[test]
    fn zero_amount_is_transparent() {
        let mut d = Delay::new(f64::from(RATE));
        d.set_amount(0.0);
        d.set_active(true);
        let input = sine(440.0, 48_000, 0.5);
        let out = run(&mut d, &input, ctx());
        let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
        assert!(rms(&diff[2 * 4_800..]) < 1e-4);
    }
}
