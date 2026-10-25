use std::f64::consts::PI;

use backspn_dsp::{OnePole, Smoothed};

use super::{EffectKind, note_hz};
use crate::{Context, Effect};

fn hold_hz(v: f32) -> f64 {
    32_100.0 * 2f64.powf(-100.0 * f64::from(v.clamp(0.0, 1.0)) / 12.0)
}

fn bits_of(v: f32) -> (f64, f32) {
    let v = v.clamp(0.0, 1.0);
    let levels = 2f64.powf(11.0 - 11.3 * f64::from(v));
    let gain = if v > 0.7 { 1.0 - 2.5 * (v - 0.7) } else { 1.0 };
    (levels, gain)
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct LoFi {
    rate: f64,
    sample_rate_knob: f32,
    smooth: f32,
    bit_knob: f32,
    spread: bool,
    phase: [f64; 2],
    held: [[f32; 2]; 2],
    dc: OnePole,
    noise: u32,
    dry_wet: f32,
    on: Smoothed,
    active: bool,
}

impl LoFi {
    pub fn new(rate: f64) -> Self {
        let mut dc = OnePole::default();
        dc.set(10.0, rate);
        Self {
            rate,
            sample_rate_knob: 0.0,
            smooth: 0.498,
            bit_knob: 0.0,
            spread: false,
            phase: [0.0; 2],
            held: [[0.0; 2]; 2],
            dc,
            noise: 0x0bad_5eed,
            dry_wet: 1.0,
            on: Smoothed::new(0.0, 0.01, rate),
            active: false,
        }
    }
}

impl Effect for LoFi {
    fn kind(&self) -> EffectKind {
        EffectKind::LoFi
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        let working = self.active && self.sample_rate_knob > 0.0;
        self.on.set(if working { 1.0 } else { 0.0 });
        let step = hold_hz(self.sample_rate_knob) / self.rate;
        let (levels, gain) = bits_of(self.bit_knob);
        let quantise = self.bit_knob > 0.0;
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let g = self.on.tick() * self.dry_wet;
            if g == 0.0 && !working {
                continue;
            }
            let mut wet = [0.0f32; 2];
            for ch in 0..2 {
                self.phase[ch] += step;
                if self.phase[ch] >= 1.0 {
                    self.phase[ch] -= 1.0;
                    self.held[ch] = [frame[ch], self.held[ch][0]];
                }
                let frac = if ch == 1 && self.spread {
                    (self.phase[ch] + 0.5) % 1.0
                } else {
                    self.phase[ch]
                };
                let [now, before] = self.held[ch];
                let stepped = now;
                let ramp = before + (now - before) * frac as f32;
                let mut x = stepped + (ramp - stepped) * self.smooth;
                if quantise {
                    self.noise = self
                        .noise
                        .wrapping_mul(1_664_525)
                        .wrapping_add(1_013_904_223);
                    let dither =
                        (f64::from(self.noise >> 8) / f64::from(1u32 << 24) - 0.5) * 0.25 / levels;
                    x = (((f64::from(x) + dither) * levels).round() / levels) as f32 * gain;
                }
                let (low, _) = self.dc.tick(ch, x);
                wet[ch] = x - low;
            }
            for ch in 0..2 {
                frame[ch] += g * (wet[ch] - frame[ch]);
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        let u = 1.9 * amount.clamp(0.0, 1.0);
        self.sample_rate_knob = u * (2.0 - u) / 2.0;
        self.bit_knob = amount.min(0.8);
        self.spread = true;
        self.smooth = 0.0;
        self.dry_wet = 1.0;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match index {
            0 => self.sample_rate_knob = value,
            1 => self.smooth = value,
            2 => self.bit_knob = value,
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        if index == 0 {
            self.spread = on;
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
        self.held = [[0.0; 2]; 2];
    }
}

fn carrier_hz(v: f32) -> f64 {
    let v = f64::from(v.clamp(0.0, 1.0));
    note_hz(43.3 + 76.7 * v * (0.34 + 0.66 * v))
}

#[derive(Debug)]
pub struct Ringmod {
    rate: f64,
    phase: f64,
    pitch: Smoothed,
    raw: f32,
    modulation: f32,
    dry_wet: f32,
    on: Smoothed,
    active: bool,
}

impl Ringmod {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            phase: 0.0,
            pitch: Smoothed::new(carrier_hz(0.498) as f32, 0.02, rate),
            raw: 0.0,
            modulation: 1.0,
            dry_wet: 1.0,
            on: Smoothed::new(0.0, 0.01, rate),
            active: false,
        }
    }
}

impl Effect for Ringmod {
    fn kind(&self) -> EffectKind {
        EffectKind::Ringmod
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        self.on.set(if self.active { 1.0 } else { 0.0 });
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            self.phase = (self.phase + f64::from(self.pitch.tick()) / self.rate) % 1.0;
            let carrier = (2.0 * PI * self.phase).sin() as f32;
            let b = self.modulation;
            let gain = (1.0 - b) * (0.5 + 0.5 * carrier) + b * carrier;
            let w = self.dry_wet * self.on.tick();
            let (dry, wet) = (1.0 - w * w, w * (2.0 - w));
            for ch in 0..2 {
                let y = frame[ch] * gain;
                let driven = y + (((2.0 * y).tanh() / 2.0) - y) * self.raw;
                frame[ch] = frame[ch] * dry + driven * (1.0 - 0.2 * self.raw) * wet;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.pitch.set(carrier_hz(amount) as f32);
        self.raw = 0.0;
        self.modulation = 1.0;
        self.dry_wet = 1.0;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match index {
            0 => self.pitch.set(carrier_hz(value) as f32),
            1 => self.raw = value,
            2 => self.modulation = value,
            _ => {}
        }
    }

    fn set_button(&mut self, _index: usize, _on: bool) {}

    fn set_active(&mut self, on: bool) {
        self.active = on;
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.dry_wet = dry_wet.clamp(0.0, 1.0);
        true
    }

    fn reset(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn knobs_follow_their_curves() {
        assert!((hold_hz(0.0) - 32_100.0).abs() < 1e-6);
        assert!((hold_hz(1.0) - 99.5).abs() < 1.0);
        let (levels, _) = bits_of(0.0);
        assert!((levels - 2048.0).abs() < 1e-6);
        assert!((bits_of(1.0).1 - 0.25).abs() < 1e-6);
        assert!((carrier_hz(0.0) - 98.0).abs() < 2.0 && (carrier_hz(1.0) - 8_400.0).abs() < 150.0);
    }

    #[test]
    fn lofi_off_at_zero_and_crushes_when_turned() {
        let input = sine(300.0, RATE as usize / 4, 0.5);
        let mut l = LoFi::new(f64::from(RATE));
        l.set_amount(0.0);
        l.set_active(true);
        assert_eq!(run(&mut l, &input, ctx()), input);
        let mut l = LoFi::new(f64::from(RATE));
        l.set_amount(0.9);
        l.set_active(true);
        let out = run(&mut l, &input, ctx());
        let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
        assert!(rms(&diff) > 0.05);
    }

    #[test]
    fn ringmod_moves_the_tone() {
        let mut r = Ringmod::new(f64::from(RATE));
        r.set_amount(0.5);
        r.set_active(true);
        let input = sine(1_000.0, RATE as usize / 4, 0.5);
        let out = run(&mut r, &input, ctx());
        let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
        assert!(rms(&diff) > 0.2);
        assert!(out.iter().all(|v| v.abs() <= 0.8));
    }
}
