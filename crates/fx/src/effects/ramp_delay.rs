use backspn_dsp::{DelayLine, Smoothed, Svf};

use super::delay_t3::{SYNC_32NDS, filter_of};
use super::{EffectKind, delay_mix};
use crate::{Context, Effect};

const LINE_FRAMES: usize = 262_144;
const DURATION_32NDS: [f64; 7] = [8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0];
const FEEDBACK: f32 = 0.52;
const FEEDBACK_PLUS: f32 = 0.945;
const GROUP_DURATION: f32 = 0.5;
const GROUP_FILTER: f32 = 0.48;
const GROUP_DRY_WET: f32 = 0.5;

fn step7(v: f32) -> usize {
    ((6.0 * v.clamp(0.0, 1.0)).round() as usize).min(6)
}

fn input_gain(rate_knob: f32, plus: bool) -> f32 {
    if !plus {
        0.9
    } else if rate_knob <= 0.4 {
        0.7
    } else {
        0.7 - 0.2 * ((rate_knob - 0.4) / 0.6).powi(2)
    }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct RampDelay {
    rate: f64,
    line: DelayLine,
    low: Svf,
    high: Svf,
    per_32nd: Smoothed,
    timed: bool,
    rate_knob: f32,
    factor: f64,
    target: f64,
    slope: f64,
    duration: f64,
    plus: bool,
    freeze: bool,
    frozen: bool,
    waited: f64,
    dry: Smoothed,
    wet: Smoothed,
}

impl RampDelay {
    pub fn new(rate: f64) -> Self {
        let factor = SYNC_32NDS[step7(0.506)];
        let mut delay = Self {
            rate,
            line: DelayLine::new(LINE_FRAMES),
            low: Svf::default(),
            high: Svf::default(),
            per_32nd: Smoothed::new(1.0, 0.05 / std::f64::consts::LN_2, rate),
            timed: false,
            rate_knob: 0.506,
            factor,
            target: factor,
            slope: 0.0,
            duration: DURATION_32NDS[step7(GROUP_DURATION)],
            plus: false,
            freeze: false,
            frozen: false,
            waited: 0.0,
            dry: Smoothed::new(1.0, 0.025, rate),
            wet: Smoothed::new(delay_mix(GROUP_DRY_WET).1, 0.025, rate),
        };
        delay.set_filter(GROUP_FILTER);
        delay
    }

    fn set_filter(&mut self, v: f32) {
        let (lp, hp) = filter_of(v, self.rate);
        self.low.set(lp, std::f64::consts::FRAC_1_SQRT_2, self.rate);
        self.high
            .set(hp, std::f64::consts::FRAC_1_SQRT_2, self.rate);
    }

    fn set_rate(&mut self, v: f32) {
        self.rate_knob = v;
        let target = SYNC_32NDS[step7(v)];
        if (target - self.target).abs() > 1e-9 {
            self.target = target;
            self.slope = (target - self.factor) / self.duration;
        }
    }
}

impl Effect for RampDelay {
    fn kind(&self) -> EffectKind {
        EffectKind::RampDelay
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let per_32nd = (self.rate * 60.0 / (8.0 * ctx.bpm.max(20.0))).round() as f32;
        if !self.timed {
            self.per_32nd = Smoothed::new(per_32nd, 0.05 / std::f64::consts::LN_2, self.rate);
            self.timed = true;
        }
        self.per_32nd.set(per_32nd);
        let gain = input_gain(self.rate_knob, self.plus);
        let feedback = if self.plus { FEEDBACK_PLUS } else { FEEDBACK };
        let max = self.line.capacity() as f32;
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let unit = self.per_32nd.tick();
            let step = self.slope / f64::from(unit.max(1.0));
            if (self.target - self.factor).abs() <= step.abs() || step == 0.0 {
                self.factor = self.target;
            } else {
                self.factor += step;
            }
            let t = (self.factor as f32 * unit).clamp(1.0, max);
            if self.freeze && !self.frozen {
                self.waited += 1.0;
                self.frozen = self.waited >= f64::from(t) - 1.0;
            }
            let (dry, wet) = if self.frozen {
                (0.0, 1.0)
            } else {
                (self.dry.tick(), self.wet.tick())
            };
            let raw = self.line.read(t);
            let mut back = [0.0; 2];
            let mut echo = [0.0; 2];
            for ch in 0..2 {
                let (lp, _) = self.low.tick(ch, raw[ch]);
                let (_, hp) = self.high.tick(ch, lp);
                let x = hp.clamp(-512.0, 512.0);
                echo[ch] = x * (1.0 - x.abs() / 1024.0);
                let (input, fb) = if self.frozen {
                    (0.0, 1.0)
                } else {
                    (frame[ch] * gain, feedback)
                };
                back[ch] = input + echo[ch] * fb;
            }
            self.line.push(back);
            frame[0] = frame[0] * dry + echo[0] * wet;
            frame[1] = frame[1] * dry + echo[1] * wet;
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.set_dry_wet(GROUP_DRY_WET);
        self.duration = DURATION_32NDS[step7(GROUP_DURATION)];
        self.set_filter(GROUP_FILTER);
        self.plus = false;
        self.freeze = false;
        self.frozen = false;
        self.set_rate(amount);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.set_rate(value),
            1 => self.duration = DURATION_32NDS[step7(value)],
            2 => self.set_filter(value),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.plus = on,
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

    #[test]
    fn a_new_rate_takes_the_whole_duration() {
        let mut d = RampDelay::new(f64::from(RATE));
        d.set_param(1, 0.0);
        d.set_param(0, 0.0);
        let start = d.factor;
        let _ = run(&mut d, &vec![0.0; 2 * 12_000], ctx());
        let half = d.factor;
        assert!(
            ((half - start) / (32.0 - start) - 0.5).abs() < 0.02,
            "{start} → {half}"
        );
        let _ = run(&mut d, &vec![0.0; 2 * 12_000], ctx());
        assert!((d.factor - 32.0).abs() < 1e-9);
        assert!((input_gain(1.0, true) - 0.5).abs() < 1e-6);
        assert!((input_gain(0.3, false) - 0.9).abs() < 1e-6);
    }

    #[test]
    fn plus_feedback_rings_longer() {
        let tail = |plus: bool| {
            let mut d = RampDelay::new(f64::from(RATE));
            d.set_param(0, 4.0 / 6.0);
            d.set_button(0, plus);
            let out = run(&mut d, &impulse(RATE as usize * 2), ctx());
            rms(&out[2 * 60_000..])
        };
        assert!(tail(true) > 5.0 * tail(false));
    }
}
