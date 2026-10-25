use backspn_dsp::{OnePole, Smoothed};

use super::{EffectKind, beats_per_sample, note_hz, stretch};
use crate::{Context, Effect};

const RATE_BEATS: [Option<f64>; 5] = [None, Some(1.0), Some(0.5), Some(0.25), Some(0.125)];
const STUTTER_BEATS: f64 = 0.75;
const ATTACK_S: f64 = 0.002;

fn shape_of(v: f32) -> (f64, f64) {
    let v = f64::from(v.clamp(0.0, 1.0));
    if v <= 0.5 {
        (0.5, 0.0)
    } else if v <= 0.75 {
        (0.15 + 0.7 * v, 0.0)
    } else {
        (0.5 - 1.4 * (v - 0.75), 1.0)
    }
}

fn gate_at(phase: f64, open: f64, tail: f64, attack: f64) -> f64 {
    let attack = attack.min(open / 2.0).max(1e-6);
    if phase < attack {
        phase / attack
    } else if phase < open {
        1.0
    } else if tail > 0.0 {
        let len = tail * (1.0 - open);
        (1.0 - (phase - open) / len).max(0.0)
    } else {
        0.0
    }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Gater {
    rate: f64,
    rate_knob: f32,
    shape: f32,
    noise_knob: f32,
    stutter: bool,
    mute: bool,
    noise: u32,
    noise_tone: OnePole,
    level: Smoothed,
    signal: Smoothed,
    dry_wet: f32,
    on: Smoothed,
    active: bool,
    decay: f32,
}

impl Gater {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            rate_knob: 0.0,
            shape: 0.5,
            noise_knob: 0.5,
            stutter: false,
            mute: false,
            noise: 0x1357_9bdf,
            noise_tone: OnePole::default(),
            level: Smoothed::new(0.0, ATTACK_S / 3.0, rate),
            signal: Smoothed::new(1.0, 0.01, rate),
            dry_wet: 1.0,
            on: Smoothed::new(0.0, 0.01, rate),
            active: false,
            decay: 0.0,
        }
    }

    fn white(&mut self) -> f32 {
        self.noise = self
            .noise
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.noise >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }
}

impl Effect for Gater {
    fn kind(&self) -> EffectKind {
        EffectKind::Gater
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        self.on.set(if self.active { 1.0 } else { 0.0 });
        self.signal.set(if self.mute { 0.0 } else { 1.0 });
        let step = ((4.0 * self.rate_knob.clamp(0.0, 1.0)).round() as usize).min(4);
        let beats = if self.stutter {
            Some(STUTTER_BEATS)
        } else {
            RATE_BEATS[step]
        };
        let (open, tail) = shape_of(self.shape);
        let open = f64::from(stretch(open as f32, self.decay, 0.05, 0.95));
        let noise_level = self.noise_knob
            * self.noise_knob
            * (1.5 - 0.5 * self.rate_knob)
            * 0.25
            * if self.mute { 0.4 } else { 1.0 };
        self.noise_tone
            .set(note_hz(36.0 + 92.0 * f64::from(self.rate_knob)), self.rate);
        let per_frame = beats_per_sample(ctx, self.rate);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            let target = match beats {
                Some(b) => {
                    let beat = ctx.beat + i as f64 * per_frame;
                    let attack = ATTACK_S * ctx.bpm.max(20.0) / 60.0 / b;
                    gate_at((beat / b).rem_euclid(1.0), open, tail, attack) as f32
                }
                None => 1.0,
            };
            self.level.set(target);
            let gate = self.level.tick();
            let white = self.white();
            let (noise, _) = self.noise_tone.tick(0, white);
            let m = self.dry_wet * self.on.tick();
            let signal = self.signal.tick();
            let (dry, wet) = if self.mute {
                ((2.0 * (1.0 - m)).min(1.0), (2.0 * m).min(1.0))
            } else {
                (1.0 - m * (2.0 - m), m * (2.0 - m))
            };
            for ch in 0..2 {
                let gated = (frame[ch] * signal + noise * noise_level) * gate;
                frame[ch] = frame[ch] * dry + gated * wet;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.rate_knob = amount;
        self.shape = 0.5;
        self.noise_knob = 0.5;
        self.stutter = false;
        self.mute = false;
        self.dry_wet = 1.0;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.rate_knob = value,
            1 => self.shape = value,
            2 => self.noise_knob = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.stutter = on,
            1 => self.mute = on,
            _ => {}
        }
    }

    fn set_active(&mut self, on: bool) {
        self.active = on;
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.dry_wet = dry_wet.clamp(0.0, 1.0);
        true
    }

    fn set_decay(&mut self, decay: f32) {
        self.decay = decay;
    }

    fn reset(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn shape_and_gate_are_right() {
        assert_eq!(shape_of(0.0), (0.5, 0.0));
        let (open, tail) = shape_of(1.0);
        assert!((open - 0.15).abs() < 1e-9 && (tail - 1.0).abs() < 1e-9);
        assert!((gate_at(0.3, 0.5, 0.0, 0.01) - 1.0).abs() < 1e-9);
        assert!(gate_at(0.7, 0.5, 0.0, 0.01).abs() < 1e-9);
        assert!(
            (gate_at(0.75, 0.5, 1.0, 0.01) - 0.5).abs() < 1e-9,
            "the tail half way"
        );
    }

    #[test]
    fn off_passes_and_on_chops_on_the_grid() {
        let input = vec![0.5; 2 * RATE as usize];
        let mut g = Gater::new(f64::from(RATE));
        g.set_amount(0.0);
        g.set_active(true);
        let out = run(&mut g, &input, ctx());
        assert!((out[2 * 20_000] - 0.5).abs() < 0.05, "RATE off: open");
        let mut g = Gater::new(f64::from(RATE));
        g.set_amount(0.75);
        g.set_active(true);
        let out = run(&mut g, &input, ctx());
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert!(left[24_000 + 1_500] > 0.3, "open in the first half");
        assert!(left[24_000 + 4_500].abs() < 0.15, "closed in the second");
    }
}
