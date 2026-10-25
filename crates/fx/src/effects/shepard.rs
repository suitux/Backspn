use backspn_dsp::DelayLine;

use super::echo::ECHO_BEATS;
use super::{EffectKind, beats_per_sample, pick};
use crate::{Context, Effect};

const BASE_HZ: f64 = 55.0;
const VOICES: usize = 4;
const SPAN: f64 = 8.0;
const LEVEL_STEP: f32 = 0.000_32;
const ECHO_MS: [f64; 2] = [80.5, 12.3];
const ECHO_TAP: f32 = 0.3;
const JET_MIN: f64 = 1.0 / 10_000.0;
const JET_RANGE: f64 = 1.0 / 500.0;
const JET_TAPS: [f64; 4] = [0.0, 0.5, 0.25, 0.75];

fn tone_level(level: f32) -> f32 {
    let w = f64::from(level.clamp(0.0, 1.0)) * 1023.9;
    let g = if w < 10.0 {
        0.0
    } else if w <= 511.0 {
        (w - 10.0) * 0.001_992
    } else if w <= 1012.0 {
        1.0 + (w - 512.0).powi(2) * 1.337e-5
    } else {
        4.37
    };
    (g * 0.211) as f32
}

pub(crate) fn vibrato_of(v: f32) -> Option<(f64, f64)> {
    let m = (f64::from(v.clamp(0.0, 1.0)) * 255.9).floor();
    let rate = |k: f64| f64::midpoint(4.0, 0.006_047 * k * k);
    if m <= 125.0 {
        Some((rate(m), 0.25 * (4.0 - 0.031_75 * m).min(1.0)))
    } else if m <= 130.0 {
        None
    } else {
        let k = m - 130.0;
        Some((rate(k), 0.25 * (0.015_87 * k).min(1.0)))
    }
}

fn echo_of(e: f32) -> (f32, f32) {
    let e = e.clamp(0.0, 1.0);
    let g = if 2.0 * e < 0.9 {
        0.9
    } else if 2.0 * e <= 1.0 {
        2.0 * e
    } else {
        0.8 + 0.4 * e
    };
    (0.8 * g, (2.0 * e).powi(2))
}

fn voice_level(x: f64) -> f64 {
    if x < 1.0 {
        x
    } else if x < 5.0 {
        1.0
    } else {
        (6.0 - x).max(0.0)
    }
}

fn jet_level(level: f32) -> (f32, f32, f32, f32) {
    let w = f64::from(level.clamp(0.0, 1.0)) * 1023.9;
    let (wet, a, b) = if w < 10.0 {
        (0.0, 0.0, 0.0)
    } else if w < 512.0 {
        let k = w - 10.0;
        (k * 0.000_996, k * 0.001_394_4, k * 0.001_792_8)
    } else {
        let k = w - 512.0;
        (0.5, 0.7 + k * 0.000_39, 0.9 + k * 0.000_176)
    };
    (wet as f32, (1.0 - wet) as f32, a as f32, b as f32)
}

fn triangle(phase: f64) -> f64 {
    1.0 - (2.0 * phase.rem_euclid(1.0) - 1.0).abs()
}

#[derive(Debug, Clone, Copy)]
struct Sweep {
    pos: f64,
    beats: f64,
    reverse: bool,
    freeze: bool,
}

impl Sweep {
    fn step(&mut self, per_frame: f64, span: f64) {
        if self.freeze {
            return;
        }
        let d = per_frame / self.beats;
        self.pos = (self.pos + if self.reverse { -d } else { d }).rem_euclid(span);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wave {
    Saw,
    Triangle,
}

#[derive(Debug)]
pub struct Shepard {
    wave: Wave,
    rate: f64,
    sweep: Sweep,
    phases: [f64; VOICES],
    vibrato: Option<(f64, f64)>,
    vibrato_phase: f64,
    level: f32,
    gain: f32,
    echo: DelayLine,
    echo_knob: f32,
    send: f32,
    active: bool,
}

impl Shepard {
    pub fn new(wave: Wave, rate: f64) -> Self {
        Self {
            wave,
            rate,
            sweep: Sweep {
                pos: 0.0,
                beats: 1.0,
                reverse: false,
                freeze: false,
            },
            phases: [0.0; VOICES],
            vibrato: None,
            vibrato_phase: 0.0,
            level: 0.0,
            gain: 0.0,
            echo: DelayLine::new((ECHO_MS[0] / 1000.0 * rate) as usize + 4),
            echo_knob: 0.5,
            send: 0.0,
            active: false,
        }
    }

    fn tone(&mut self) -> f32 {
        let vib = self.vibrato.map_or(1.0, |(hz, depth)| {
            self.vibrato_phase = (self.vibrato_phase + hz / self.rate).rem_euclid(1.0);
            1.0 + depth * (triangle(self.vibrato_phase) - 0.5)
        });
        let mut sum = 0.0;
        for (k, phase) in self.phases.iter_mut().enumerate() {
            let x = (self.sweep.pos + 2.0 * k as f64).rem_euclid(SPAN);
            let a = voice_level(x);
            let hz = BASE_HZ * 2f64.powf(x) * vib;
            *phase = (*phase + hz / self.rate).rem_euclid(1.0);
            sum += a
                * a
                * match self.wave {
                    Wave::Saw => 1.4 * (*phase - 0.5),
                    Wave::Triangle => 2.0 * (triangle(*phase) - 0.5),
                };
        }
        sum as f32
    }
}

impl Effect for Shepard {
    fn kind(&self) -> EffectKind {
        match self.wave {
            Wave::Saw => EffectKind::ShepardSaw,
            Wave::Triangle => EffectKind::ShepardTri,
        }
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let per_frame = beats_per_sample(ctx, self.rate);
        let (feedback, send_cap) = echo_of(self.echo_knob);
        let target = if self.active {
            tone_level(self.level)
        } else {
            0.0
        };
        let lens = ECHO_MS.map(|ms| (ms / 1000.0 * self.rate) as f32);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            self.gain += (target - self.gain).clamp(-LEVEL_STEP, LEVEL_STEP);
            self.send += (send_cap - self.send).clamp(-LEVEL_STEP, LEVEL_STEP);
            self.sweep.step(per_frame, SPAN);
            let tone = self.tone() * self.gain;
            let (long, short) = (self.echo.read(lens[0])[0], self.echo.read(lens[1])[1]);
            self.echo
                .push([tone * self.send + short * feedback, long * feedback]);
            let echo = (long + short) * ECHO_TAP;
            frame[0] += tone + echo;
            frame[1] += tone + echo;
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.level = amount;
        self.sweep.beats = 1.0;
        self.sweep.reverse = false;
        self.sweep.freeze = false;
        self.vibrato = None;
        self.echo_knob = 0.5;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.sweep.beats = pick(&ECHO_BEATS, value),
            1 => self.echo_knob = value,
            2 => self.vibrato = vibrato_of(value),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.sweep.reverse = on,
            1 => self.sweep.freeze = on,
            _ => {}
        }
    }

    fn set_active(&mut self, on: bool) {
        self.active = on;
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.level = dry_wet;
        true
    }

    fn has_tail(&self) -> bool {
        true
    }

    fn reset(&mut self) {
        self.echo.clear();
    }
}

#[derive(Debug)]
pub struct ShepardJet {
    rate: f64,
    sweep: Sweep,
    lines: [DelayLine; 4],
    vibrato: Option<(f64, f64)>,
    vibrato_phase: f64,
    level: f32,
    feedback_knob: f32,
}

fn tap_level(p: f64) -> f64 {
    if p < 0.1 {
        (10.0 * p).powi(2)
    } else if p > 0.9 {
        (10.0 - 10.0 * p).powi(2)
    } else {
        1.0
    }
}

impl ShepardJet {
    pub fn new(rate: f64) -> Self {
        let len = ((JET_MIN + JET_RANGE) * 1.5 * rate) as usize + 4;
        Self {
            rate,
            sweep: Sweep {
                pos: 0.0,
                beats: 1.0,
                reverse: false,
                freeze: false,
            },
            lines: std::array::from_fn(|_| DelayLine::new(len)),
            vibrato: None,
            vibrato_phase: 0.0,
            level: 0.0,
            feedback_knob: 0.5,
        }
    }

    fn feedback(&self, low: f32, high: f32) -> f32 {
        let f = 2.0 * self.feedback_knob.clamp(0.0, 1.0);
        if f < 1.0 {
            f * low
        } else {
            low + (high - low) * (f - 1.0)
        }
    }
}

impl Effect for ShepardJet {
    fn kind(&self) -> EffectKind {
        EffectKind::ShepardJet
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let per_frame = beats_per_sample(ctx, self.rate);
        let (wet, dry, low, high) = jet_level(self.level);
        let fb = self.feedback(low, high);
        let (min, range) = (JET_MIN * self.rate, JET_RANGE * self.rate);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            self.sweep.step(per_frame, 1.0);
            let vib = self.vibrato.map_or(1.0, |(hz, depth)| {
                self.vibrato_phase = (self.vibrato_phase + hz / self.rate).rem_euclid(1.0);
                1.0 + depth * (triangle(self.vibrato_phase) - 0.5)
            });
            let mut sum = [0.0f32; 2];
            let mut weight = 0.0;
            for (line, offset) in self.lines.iter_mut().zip(JET_TAPS) {
                let p = (self.sweep.pos + offset).rem_euclid(1.0);
                let delay = ((min + p * range) * vib).clamp(min, min + range);
                let tap = line.read(delay.max(1.0) as f32);
                line.push([frame[0] + tap[0] * fb, frame[1] + tap[1] * fb]);
                let level = tap_level(p);
                weight += level;
                sum[0] += tap[0] * level as f32;
                sum[1] += tap[1] * level as f32;
            }
            let norm = (1.0 / weight.max(1.0)) as f32;
            frame[0] = frame[0] * dry + sum[0] * norm * wet;
            frame[1] = frame[1] * dry + sum[1] * norm * wet;
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.level = amount;
        self.sweep.beats = 1.0;
        self.sweep.reverse = false;
        self.sweep.freeze = false;
        self.vibrato = None;
        self.feedback_knob = 0.5;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.sweep.beats = pick(&ECHO_BEATS, value),
            1 => self.feedback_knob = value,
            2 => self.vibrato = vibrato_of(value),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.sweep.reverse = on,
            1 => self.sweep.freeze = on,
            _ => {}
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.level = dry_wet;
        true
    }

    fn reset(&mut self) {
        for line in &mut self.lines {
            line.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn curves_follow_the_spec_values() {
        assert!(tone_level(0.0).abs() < 1e-9);
        assert!((tone_level(0.5) - 0.211).abs() < 0.001);
        assert!((tone_level(1.0) - 0.922).abs() < 0.001);
        assert_eq!(vibrato_of(0.5), None);
        let (hz, depth) = vibrato_of(0.0).unwrap();
        assert!((hz - 2.0).abs() < 1e-9 && (depth - 0.25).abs() < 1e-9);
        assert_eq!(echo_of(0.5), (0.8, 1.0));
        let (wet, dry, low, high) = jet_level(1.0);
        assert!((wet - 0.5).abs() < 1e-6 && (dry - 0.5).abs() < 1e-6);
        assert!((low - 0.9).abs() < 0.01 && (high - 0.99).abs() < 0.01);
        assert!((tap_level(0.05) - 0.25).abs() < 1e-9 && (tap_level(0.5) - 1.0).abs() < 1e-9);
        assert!((voice_level(0.5) - 0.5).abs() < 1e-9 && voice_level(7.0).abs() < 1e-9);
    }

    #[test]
    fn shepard_adds_a_tone_only_while_on() {
        let mut m = Shepard::new(Wave::Saw, f64::from(RATE));
        m.set_amount(1.0);
        let silence = vec![0.0; 2 * RATE as usize / 2];
        assert!(rms(&run(&mut m, &silence, ctx())) < 1e-6, "off: nothing");
        m.set_active(true);
        let out = run(&mut m, &silence, ctx());
        assert!(rms(&out[2 * 12_000..]) > 0.05, "on: the tone");
    }

    #[test]
    fn shepard_jet_combs_the_sound() {
        let mut j = ShepardJet::new(f64::from(RATE));
        j.set_amount(1.0);
        let input = sine(3_000.0, RATE as usize, 0.5);
        let out = run(&mut j, &input, ctx());
        assert!(out.iter().all(|v| v.is_finite() && v.abs() < 2.0));
        let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
        assert!(rms(&diff) > 0.05, "changes the sound");
        let mut j = ShepardJet::new(f64::from(RATE));
        j.set_amount(0.0);
        assert_eq!(run(&mut j, &input, ctx()), input);
    }
}
