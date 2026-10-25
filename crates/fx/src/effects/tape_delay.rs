use backspn_dsp::{DelayLine, OnePole, Smoothed};

use super::{EffectKind, delay_mix};
use crate::{Context, Effect};

const BASE_S: f64 = 0.125;
const MIN_SPEED: f64 = 0.25;
const LOW_HZ: f64 = 147.0;
const HIGH_HZ: f64 = 2_790.0;
const GLIDE_FAST_S: f64 = 0.1;
const GLIDE_SLOW_S: f64 = 0.33;
const GROUP_FEEDBACK: f32 = 0.5;
const GROUP_FILTER: f32 = 0.65;
const GROUP_DRY_WET: f32 = 0.5;
const FREEZE_WET: f32 = 0.8;

fn speed_of(v: f32) -> f64 {
    MIN_SPEED * 2f64.powf(2.0 * f64::from(v.clamp(0.0, 1.0)))
}

fn feedback_of(v: f32) -> f32 {
    1.4 * v.clamp(0.0, 1.0)
}

fn tilt_db(v: f32) -> (f64, f64) {
    let v = f64::from(v.clamp(0.0, 1.0));
    ((1.0 - v) * 40.0 - 20.0, 0.7 * v * 40.0 - 20.0)
}

fn db(x: f64) -> f32 {
    10f64.powf(x / 20.0) as f32
}

#[derive(Debug)]
pub struct TapeDelay {
    rate: f64,
    line: DelayLine,
    low: OnePole,
    high: OnePole,
    time: Smoothed,
    speed: f64,
    feedback: f32,
    low_gain: f32,
    high_gain: f32,
    accel: bool,
    freeze: bool,
    timed: bool,
    dry: Smoothed,
    wet: Smoothed,
}

impl TapeDelay {
    pub fn new(rate: f64) -> Self {
        let mut low = OnePole::default();
        low.set(LOW_HZ, rate);
        let mut high = OnePole::default();
        high.set(HIGH_HZ, rate);
        let speed = speed_of(0.5);
        let mut tape = Self {
            rate,
            line: DelayLine::new((BASE_S / MIN_SPEED * rate) as usize + 8),
            low,
            high,
            time: Smoothed::new((BASE_S / speed * rate) as f32, GLIDE_SLOW_S, rate),
            speed,
            feedback: feedback_of(GROUP_FEEDBACK),
            low_gain: 1.0,
            high_gain: 1.0,
            accel: false,
            freeze: false,
            timed: false,
            dry: Smoothed::new(1.0, 0.01, rate),
            wet: Smoothed::new(delay_mix(GROUP_DRY_WET).1, 0.01, rate),
        };
        tape.set_tilt(GROUP_FILTER);
        tape
    }

    fn set_tilt(&mut self, v: f32) {
        let (low, high) = tilt_db(v);
        self.low_gain = db(low);
        self.high_gain = db(high);
    }

    fn set_speed(&mut self, v: f32) {
        self.speed = speed_of(v);
    }

    fn set_accel(&mut self, on: bool) {
        self.accel = on;
        let glide = if on { GLIDE_FAST_S } else { GLIDE_SLOW_S };
        let now = self.time.value();
        self.time = Smoothed::new(now, glide, self.rate);
    }
}

impl Effect for TapeDelay {
    fn kind(&self) -> EffectKind {
        EffectKind::TapeDelay
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        let target = (BASE_S / self.speed * self.rate) as f32;
        if !self.timed {
            let glide = if self.accel {
                GLIDE_FAST_S
            } else {
                GLIDE_SLOW_S
            };
            self.time = Smoothed::new(target, glide, self.rate);
            self.timed = true;
        }
        self.time.set(target);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let (dry, wet) = (self.dry.tick(), self.wet.tick());
            let raw = self.line.read(self.time.tick());
            let mut echo = [0.0; 2];
            let mut back = [0.0; 2];
            for ch in 0..2 {
                let (lp, _) = self.low.tick(ch, raw[ch]);
                let (_, hp) = self.high.tick(ch, raw[ch]);
                let toned = raw[ch] + (self.low_gain - 1.0) * lp + (self.high_gain - 1.0) * hp;
                echo[ch] = raw[ch];
                let (input, fb) = if self.freeze {
                    (0.0, 1.0)
                } else {
                    (frame[ch], self.feedback)
                };
                back[ch] = (input + toned * fb).tanh();
            }
            self.line.push(back);
            let (dry, wet) = if self.freeze {
                (0.0, FREEZE_WET)
            } else {
                (dry, wet)
            };
            frame[0] = frame[0] * dry + echo[0] * wet;
            frame[1] = frame[1] * dry + echo[1] * wet;
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.set_dry_wet(GROUP_DRY_WET);
        self.freeze = false;
        self.set_speed(amount);
        self.feedback = feedback_of(GROUP_FEEDBACK);
        self.set_tilt(GROUP_FILTER);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.set_speed(value),
            1 => self.feedback = feedback_of(value),
            2 => self.set_tilt(value),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.set_accel(on),
            1 => self.freeze = on,
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
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    fn first_peak(out: &[f32]) -> usize {
        out.iter()
            .step_by(2)
            .skip(100)
            .position(|v| v.abs() > 0.2)
            .unwrap()
            + 100
    }

    #[test]
    fn knobs_follow_their_curves() {
        assert!((speed_of(0.0) - 0.25).abs() < 1e-9);
        assert!((speed_of(1.0) - 1.0).abs() < 1e-9);
        assert!((feedback_of(1.0) - 1.4).abs() < 1e-6);
        assert_eq!(tilt_db(0.0), (20.0, -20.0));
        let (low, high) = tilt_db(1.0);
        assert!((low + 20.0).abs() < 1e-9 && (high - 8.0).abs() < 1e-9);
    }

    #[test]
    fn faster_tape_shortens_the_echo_and_ignores_the_tempo() {
        let echo_at = |speed: f32, bpm: f64| {
            let mut d = TapeDelay::new(f64::from(RATE));
            d.set_amount(speed);
            let mut c = ctx();
            c.bpm = bpm;
            first_peak(&run(&mut d, &impulse(RATE as usize), c))
        };
        let (slow, fast) = (echo_at(0.0, 120.0), echo_at(1.0, 120.0));
        assert!(
            (slow as f64 / fast as f64 - 4.0).abs() < 0.1,
            "{slow} {fast}"
        );
        assert_eq!(echo_at(1.0, 90.0), fast);
    }

    #[test]
    fn full_feedback_stays_bounded() {
        let mut d = TapeDelay::new(f64::from(RATE));
        d.set_param(1, 1.0);
        let out = run(&mut d, &sine(300.0, 2 * RATE as usize, 0.9), ctx());
        assert!(out.iter().all(|v| v.is_finite() && v.abs() < 2.0));
    }
}
