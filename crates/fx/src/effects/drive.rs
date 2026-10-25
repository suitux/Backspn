use backspn_dsp::{Biquad, DelayLine, OnePole, Smoothed};

use super::{EffectKind, reverb_mix};
use crate::{Context, Effect};

const EMPHASIS_HZ: f64 = 1_200.0;
const COMB_FRAMES: usize = 20_000;
const ENVELOPE_HALF_LIFE_S: f64 = 0.178;
const GROUP_TONE: f32 = 0.66;
const GROUP_DRY_WET: f32 = 0.5;

fn drive_of(v: f32) -> (f32, f64, f64, f64) {
    let v = f64::from(v.clamp(0.0, 1.0));
    let t2 = v.min(0.5);
    let t1 = if v < 0.5 {
        1.32 * v
    } else {
        0.66 + 0.68 * (v - 0.5)
    };
    let d0 = 2.0 * t1 - 1.0;
    let s = 0.5 + d0 * (0.2 + 0.3 * d0.abs());
    let level = -12.0 * t2 + 9.0 * (1.5 * s) * (2.0 - 1.5 * s) - 16.0;
    let m = (2.0 * s).min(1.0);
    (
        (s * (2.0 - s)) as f32,
        level,
        (9_000.0 - 5_244.0 * m) * 1.333,
        -12.0 - 28.0 * m,
    )
}

fn feedback_of(v: f32) -> f32 {
    0.8 * v.clamp(0.0, 1.0).powi(3)
}

fn tone_s(v: f32) -> f64 {
    let v = f64::from(v.clamp(0.0, 1.0));
    0.028_99 * v * v + 0.000_01
}

fn clip(x: f32) -> f32 {
    x / (1.0 + x.abs())
}

#[derive(Debug)]
pub struct Drive {
    rate: f64,
    split: OnePole,
    post: OnePole,
    lows: [Biquad; 2],
    comb: DelayLine,
    gain_in: f32,
    out_db: f64,
    emphasis: f32,
    feedback: f32,
    invert: bool,
    tone: f64,
    envelope: f32,
    release: f32,
    dry: Smoothed,
    wet: Smoothed,
}

impl Drive {
    pub fn new(rate: f64) -> Self {
        let mut split = OnePole::default();
        split.set(EMPHASIS_HZ, rate);
        let mut d = Self {
            rate,
            split,
            post: split,
            lows: [Biquad::default(); 2],
            comb: DelayLine::new(COMB_FRAMES),
            gain_in: 0.0,
            out_db: 0.0,
            emphasis: 1.0,
            feedback: 0.0,
            invert: false,
            tone: tone_s(GROUP_TONE),
            envelope: 0.0,
            release: (0.5f64.powf(1.0 / (ENVELOPE_HALF_LIFE_S * rate))) as f32,
            dry: Smoothed::new(1.0, 0.01, rate),
            wet: Smoothed::new(1.0, 0.01, rate),
        };
        d.set_drive(0.502);
        d
    }

    fn set_drive(&mut self, v: f32) {
        let (gain, level, lowpass, emphasis) = drive_of(v);
        self.gain_in = gain;
        self.out_db = level;
        self.emphasis = 10f64.powf(-emphasis / 40.0) as f32;
        let lp = Biquad::lowpass(lowpass, self.rate);
        self.lows = [lp, lp];
    }

    fn out_gain(&self) -> f32 {
        let fb = f64::from(self.feedback);
        10f64.powf((self.out_db - 6.0 * fb - 6.0) / 20.0) as f32
    }
}

impl Effect for Drive {
    fn kind(&self) -> EffectKind {
        EffectKind::Drive
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        let gain = self.out_gain();
        let fb = 0.3
            * if self.invert {
                -self.feedback
            } else {
                self.feedback
            };
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let peak = frame[0].abs().max(frame[1].abs());
            self.envelope = peak.max(self.envelope * self.release);
            let t = ((self.tone + f64::from(self.envelope) * 0.000_390_625) * self.rate)
                .clamp(1.0, (COMB_FRAMES - 1) as f64);
            let back = self.comb.read(t as f32);
            let (dry, wet) = (self.dry.tick(), self.wet.tick());
            let mut out = [0.0; 2];
            for ch in 0..2 {
                let x = (frame[ch] + back[ch] * fb) * self.gain_in;
                let (low, high) = self.split.tick(ch, x);
                let (low, high) = self.post.tick(ch, clip(low + high * self.emphasis));
                let shaped = low + high / self.emphasis;
                let first = self.lows[0].tick(ch, shaped);
                let y = self.lows[1].tick(ch, first);
                out[ch] = y;
            }
            self.comb.push(out);
            for ch in 0..2 {
                frame[ch] = frame[ch] * dry + out[ch] * gain * wet;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.set_drive(amount);
        self.feedback = 0.0;
        self.invert = false;
        self.tone = tone_s(GROUP_TONE);
        self.set_dry_wet(GROUP_DRY_WET);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.set_drive(value),
            1 => self.feedback = feedback_of(value),
            2 => self.tone = tone_s(value),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        if index == 0 {
            self.invert = on;
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        let (dry, wet) = reverb_mix(dry_wet);
        self.dry.set(dry);
        self.wet.set(wet);
        true
    }

    fn reset(&mut self) {
        self.comb.clear();
        self.envelope = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn curves_follow_the_spec_values() {
        let (gain, _, _, _) = drive_of(0.0);
        assert!(gain.abs() < 1e-6, "no drive at the left");
        let (gain, _, lowpass, emphasis) = drive_of(1.0);
        assert!((gain - 1.0).abs() < 1e-6);
        assert!((lowpass - 3_756.0 * 1.333).abs() < 1.0 && (emphasis + 40.0).abs() < 1e-9);
        assert!((feedback_of(1.0) - 0.8).abs() < 1e-6);
        assert!((tone_s(1.0) - 0.029).abs() < 1e-5);
    }

    #[test]
    fn more_drive_more_harmonics() {
        let harsh = |v: f32| {
            let mut d = Drive::new(f64::from(RATE));
            d.set_amount(v);
            d.set_dry_wet(1.0);
            let input = sine(200.0, RATE as usize / 4, 0.8);
            let out = run(&mut d, &input, ctx());
            assert!(out.iter().all(|s| s.is_finite()));
            let hp: Vec<f32> = out.windows(4).step_by(2).map(|w| w[2] - w[0]).collect();
            rms(&hp) / rms(&out).max(1e-9)
        };
        assert!(harsh(1.0) > harsh(0.3));
    }
}
