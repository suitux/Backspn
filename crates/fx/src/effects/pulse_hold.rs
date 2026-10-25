use std::f64::consts::PI;

use backspn_dsp::{DelayLine, Smoothed};

use super::{EffectKind, note_hz};
use crate::{Context, Effect};

const FLANGER_FRAMES: usize = 24_000;
const MIN_DELAY_S: f64 = 0.000_045;
const FEEDBACK_FROM_S: f64 = 0.000_25;
const ENV_SMOOTH_S: f64 = 0.002;
const SECTIONS: usize = 4;

#[derive(Debug, Clone, Copy)]
struct Envelope {
    level: f32,
    held: f64,
    smoothed: Smoothed,
}

impl Envelope {
    fn new(rate: f64) -> Self {
        Self {
            level: 0.0,
            held: 0.0,
            smoothed: Smoothed::new(0.0, ENV_SMOOTH_S, rate),
        }
    }

    fn tick(&mut self, x: [f32; 2], shape: f32, rate: f64) -> f32 {
        let (hold, decay) = hold_decay(shape);
        let peak = x[0].abs().max(x[1].abs()).clamp(1e-6, 0.95);
        if peak > self.level {
            self.level = peak;
            self.held = 0.0;
        } else if self.held < 1.0 {
            self.held += 1.0 / (hold * rate);
        } else {
            self.level = (self.level - (1.0 / (decay * rate)) as f32).max(0.0);
        }
        self.smoothed.set(self.level);
        self.smoothed.tick()
    }
}

fn hold_decay(shape: f32) -> (f64, f64) {
    let b = 2.0 * f64::from(shape.clamp(0.0, 1.0)) - 1.0;
    if b > 0.01 {
        (0.1 * (2.0 * (1.0 - b.abs()) + 1.0), 0.05)
    } else {
        (0.01, 0.1 * (15.0 * (1.0 - b.abs()) + 2.0))
    }
}

fn compensation(fb: f64, threshold: f64, k: f64) -> f64 {
    if fb > threshold {
        1.0 - k * ((fb - threshold) / (1.0 - threshold)).powi(2)
    } else {
        1.0
    }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Flanger {
    pulse: bool,
    rate: f64,
    line: DelayLine,
    envelope: Envelope,
    knobs: [f32; 3],
    spread: bool,
    negative: bool,
    dry_wet: f32,
    on: Smoothed,
    active: bool,
}

impl Flanger {
    pub fn new(pulse: bool, rate: f64) -> Self {
        let knobs = if pulse {
            [0.502, 0.502, 0.506]
        } else {
            [0.498, 1.0, 0.0]
        };
        Self {
            pulse,
            rate,
            line: DelayLine::new(FLANGER_FRAMES),
            envelope: Envelope::new(rate),
            knobs,
            spread: false,
            negative: false,
            dry_wet: 1.0,
            on: Smoothed::new(0.0, 0.01, rate),
            active: false,
        }
    }

    fn shape(&self, env: f64) -> (f64, f64, f64, f64, f64) {
        let b = 2.0 * f64::from(self.knobs[0]) - 1.0;
        let a = b.abs();
        let factor = if self.negative { 1.0 } else { 0.5 };
        if self.pulse {
            let depth = b.signum() * 0.078 * a * (0.8 * a + 0.2);
            let mut delay = MIN_DELAY_S - depth.min(0.0) + env * depth;
            if self.negative {
                delay *= 0.5;
            }
            let fb = 0.25 + 0.745 * f64::from(self.knobs[1]);
            let (th, k) = if self.negative {
                (0.44, 0.51)
            } else {
                (0.54, 0.46)
            };
            (
                delay,
                (50.0 * a).min(1.0),
                fb,
                compensation(fb, th, k),
                factor,
            )
        } else {
            let delay = MIN_DELAY_S + 0.005_31 * a;
            let fb = 0.26 + 0.57 * f64::from(self.knobs[1]);
            let m = (33.3 * a).min(1.0);
            let c = compensation(fb, 0.5, 0.33);
            (delay, m, fb, (1.2 - 0.33 * m) * c, factor)
        }
    }
}

impl Effect for Flanger {
    fn kind(&self) -> EffectKind {
        if self.pulse {
            EffectKind::FlangerPulse
        } else {
            EffectKind::FlangerHold
        }
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        self.on.set(if self.active { 1.0 } else { 0.0 });
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let on = f64::from(self.on.tick());
            let env = if self.pulse {
                f64::from(self.envelope.tick(*frame, self.knobs[2], self.rate))
            } else {
                0.0
            };
            let (delay, fade, fb, comp, factor) = self.shape(env);
            let fb = if self.negative { -fb } else { fb } * (delay / FEEDBACK_FROM_S).min(1.0);
            let mut taps = [0.0; 2];
            for (ch, tap) in taps.iter_mut().enumerate() {
                let side = if ch == 1 && self.spread {
                    if self.pulse {
                        delay + 0.000_1 * fade
                    } else {
                        delay * 1.066
                    }
                } else {
                    delay
                };
                *tap = self.line.read((side * self.rate).max(1.0) as f32)[ch];
            }
            self.line.push([
                (frame[0] + taps[0] * fb as f32).clamp(-2.0, 2.0),
                (frame[1] + taps[1] * fb as f32).clamp(-2.0, 2.0),
            ]);
            let knob = f64::from(self.dry_wet);
            let level = on
                * if self.pulse {
                    knob * (1.45 - 0.45 * knob)
                } else {
                    knob
                };
            let share = fade * factor;
            let rest = 1.0 - share;
            let wet = level * (2.0 - level) * comp * share * (2.0 - share);
            let dry = 1.0 + level * level * (comp * rest * (2.0 - rest) - 1.0);
            for ch in 0..2 {
                frame[ch] = (f64::from(frame[ch]) * dry + f64::from(taps[ch]) * wet) as f32;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.knobs[0] = amount;
        self.spread = true;
        self.negative = false;
        if self.pulse {
            self.knobs[1] = 0.995;
            self.knobs[2] = 0.66;
            self.dry_wet = 0.78;
        } else {
            self.knobs[1] = 0.83;
            self.dry_wet = 1.0;
        }
    }

    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(k) = self.knobs.get_mut(index) {
            *k = value.clamp(0.0, 1.0);
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.spread = on,
            1 => self.negative = on,
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

    fn reset(&mut self) {
        self.line.clear();
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AllPass {
    z: [[f32; 2]; 2],
}

impl AllPass {
    pub(crate) fn tick(&mut self, ch: usize, x: f32, c1: f32, c2: f32) -> f32 {
        let z = &mut self.z[ch];
        let y = c2 * x + z[0];
        z[0] = c1 * x - c1 * y + z[1];
        z[1] = x - c2 * y;
        y
    }
}

pub(crate) fn allpass_coefs(hz: f64, rate: f64) -> (f32, f32) {
    let w = 2.0 * PI * hz.clamp(rate / 24_576.0, rate / 2.125) / rate;
    let (s, c) = w.sin_cos();
    (
        ((-2.0 * c) / (1.0 + s)) as f32,
        ((1.0 - s) / (1.0 + s)) as f32,
    )
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Phaser {
    pulse: bool,
    rate: f64,
    sections: [AllPass; SECTIONS],
    last: [f32; 2],
    envelope: Envelope,
    knobs: [f32; 3],
    spread: bool,
    eight: bool,
    dry_wet: f32,
    on: Smoothed,
    active: bool,
}

impl Phaser {
    pub fn new(pulse: bool, rate: f64) -> Self {
        let knobs = if pulse {
            [0.698, 0.502, 0.502]
        } else {
            [0.498, 0.502, 0.0]
        };
        Self {
            pulse,
            rate,
            sections: [AllPass::default(); SECTIONS],
            last: [0.0; 2],
            envelope: Envelope::new(rate),
            knobs,
            spread: false,
            eight: false,
            dry_wet: 1.0,
            on: Smoothed::new(0.0, 0.01, rate),
            active: false,
        }
    }

    fn shape(&self, env: f64) -> (f64, f64, f64) {
        let b = 2.0 * f64::from(self.knobs[0]) - 1.0;
        let a = b.abs();
        if self.pulse {
            let note = if b > 0.0 {
                111.0 - 84.0 * a + 48.0 * a * env
            } else {
                111.0 + 20.0 * a - 84.0 * a * env
            };
            let v = f64::from(self.knobs[1]);
            let fb = 0.2 + 0.78 * v * (2.0 - v);
            let side = if b <= 0.0 { 1.0 - 0.1 * a } else { 1.0 };
            (
                note,
                fb,
                1.2 * side * (1.0 - 0.1 * (1.0 - fb)) * (1.0 - 0.3 * fb * fb),
            )
        } else {
            let v = f64::from(self.knobs[1]);
            let fb = 0.1 + 0.774 * v * (2.0 - v);
            (34.4 + 88.4 * a, fb, 1.0 - 0.3 * fb * fb)
        }
    }
}

impl Effect for Phaser {
    fn kind(&self) -> EffectKind {
        if self.pulse {
            EffectKind::PhaserPulse
        } else {
            EffectKind::PhaserHold
        }
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        self.on.set(if self.active { 1.0 } else { 0.0 });
        let used = if self.eight { SECTIONS } else { SECTIONS / 2 };
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let on = f64::from(self.on.tick());
            let env = if self.pulse {
                f64::from(self.envelope.tick(*frame, self.knobs[2], self.rate))
            } else {
                0.0
            };
            let (note, fb, gain) = self.shape(env);
            let mut wet = [0.0f32; 2];
            for ch in 0..2 {
                let hz = if ch == 1 && self.spread {
                    if self.pulse {
                        note_hz(note + 0.25)
                    } else {
                        note_hz(note) * 1.12
                    }
                } else {
                    note_hz(note)
                };
                let (c1, c2) = allpass_coefs(hz, self.rate);
                let mut v = frame[ch] + self.last[ch] * fb as f32;
                v = v.clamp(-2.0, 2.0);
                for s in &mut self.sections[..used] {
                    v = s.tick(ch, v, c1, c2);
                }
                self.last[ch] = v;
                wet[ch] = v;
            }
            let v = f64::from(self.dry_wet);
            let (w, d) = if self.pulse {
                let d = on * v;
                (d * gain, 1.0 + d * ((fb - 1.0) * gain - 1.0))
            } else {
                let m = on * v;
                let m = m * (2.0 - m);
                (m * gain, 1.0 + m * ((fb - 1.0) * gain - 1.0))
            };
            for ch in 0..2 {
                frame[ch] = (f64::from(frame[ch]) * d + f64::from(wet[ch]) * w) as f32;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.knobs[0] = amount;
        self.spread = true;
        self.eight = true;
        self.dry_wet = 1.0;
        if self.pulse {
            self.knobs[1] = 0.66;
            self.knobs[2] = 0.6;
        } else {
            self.knobs[1] = 0.78;
        }
    }

    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(k) = self.knobs.get_mut(index) {
            *k = value.clamp(0.0, 1.0);
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.spread = on,
            1 => self.eight = on,
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

    fn reset(&mut self) {
        self.sections = [AllPass::default(); SECTIONS];
        self.last = [0.0; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn curves_follow_the_spec_values() {
        assert_eq!(hold_decay(1.0), (0.1, 0.05));
        let (hold, decay) = hold_decay(0.5);
        assert!((hold - 0.01).abs() < 1e-9 && (decay - 1.7).abs() < 1e-9);
        assert!((compensation(1.0, 0.54, 0.46) - 0.54).abs() < 1e-9);
        let mut f = Flanger::new(false, f64::from(RATE));
        f.set_param(0, 0.5);
        let (delay, m, _, _, _) = f.shape(0.0);
        assert!(
            (delay - MIN_DELAY_S).abs() < 1e-6 && m.abs() < 0.03,
            "centre: no comb"
        );
    }

    #[test]
    fn every_variant_changes_the_sound_and_stays_bounded() {
        let effects: [Box<dyn Effect>; 4] = [
            Box::new(Flanger::new(true, f64::from(RATE))),
            Box::new(Flanger::new(false, f64::from(RATE))),
            Box::new(Phaser::new(true, f64::from(RATE))),
            Box::new(Phaser::new(false, f64::from(RATE))),
        ];
        let input = sine(500.0, RATE as usize / 2, 0.5);
        for mut e in effects {
            e.set_amount(0.9);
            e.set_active(true);
            let out = run(e.as_mut(), &input, ctx());
            assert!(
                out.iter().all(|v| v.is_finite() && v.abs() < 3.0),
                "{:?}",
                e.kind()
            );
            let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
            assert!(rms(&diff) > 0.02, "{:?} changes it", e.kind());
        }
    }
}
