use std::f64::consts::PI;

use backspn_dsp::{DelayLine, Isolator, Smoothed, Svf};

use super::echo::{ECHO_BEATS, HIGH_BAND_HZ, LOW_BAND_HZ, centred};
use super::{EffectKind, beats_per_sample, pick};
use crate::{Context, Effect};

const EDGE_S: f64 = 0.003_333;
const MIN_OPEN_S: f64 = 0.005;
const PAN_PEAK: f32 = 1.4125;
const GROUP_BEATS: f64 = 1.0;
const GROUP_TRANS_BEATS: f64 = 0.5;
const LPF_HZ: (f64, f64) = (23_700.0, 485.0);
const HPF_HZ: (f64, f64) = (52.0, 8_900.0);
const FLANGER_MS: f64 = 2.667;
const FLANGER_EXTRA_MS: f64 = 25.0;
const PHASER_HZ: (f64, f64) = (299.96, 8_058.92);
const MAX_STAGES: usize = 60;
const WOBBLE_BPM: f64 = 120.0;

fn triangle(phase: f64) -> f64 {
    1.0 - (2.0 * phase.rem_euclid(1.0) - 1.0).abs()
}

fn half(v: f32) -> f64 {
    let c = f64::from(centred(v));
    if c < 0.0 { 1.0 + c } else { c }
}

fn chop_mix(v: f32) -> (f32, f32) {
    let v = v.clamp(0.0, 1.0);
    if v < 0.5 {
        (1.0 - 2.0 * v, 2.0 * v)
    } else {
        (0.0, 1.0)
    }
}

fn filter_mix(v: f32) -> (f32, f32) {
    let v = f64::from(v.clamp(0.0, 1.0));
    if v <= 0.156_25 {
        let dry = 0.5 + 0.5 * (PI * v / 0.156_25).cos();
        (dry as f32, (1.0 - dry) as f32)
    } else {
        (0.0, 1.0)
    }
}

fn comb_mix(v: f32) -> (f32, f32) {
    let v = v.clamp(0.0, 1.0);
    if v < 0.25 {
        (1.0 - v, 3.0 * v)
    } else {
        (0.75, 0.75)
    }
}

fn feedback_of(v: f32, reverse: bool) -> f64 {
    let h = half(v);
    let left = centred(v) < 0.0;
    if !reverse {
        if left {
            0.266_7 * h
        } else {
            0.266_7 + 0.663_3 * h
        }
    } else if left {
        -(if h < 0.1 {
            7.7 * h
        } else {
            0.769_7 + 0.003_33 * h
        })
    } else {
        -(0.8 + 0.15 * h)
    }
}

fn compensation(v: f32, fb: f64, reverse: bool, phaser: bool) -> f64 {
    if !reverse {
        return if fb.abs() > 0.8 {
            3.660_3 - 3.325_4 * fb.abs()
        } else {
            1.0
        };
    }
    let x = f64::from(v.clamp(0.0, 1.0));
    if x < 0.1 {
        1.0 - 1.087_5 * x
    } else if x < 0.5 {
        0.891_3
    } else if phaser {
        1.151_5 - 0.520_6 * x
    } else {
        1.220_2 - 0.657_8 * x
    }
}

fn pan_fractions(v: f32) -> (f64, f64) {
    const STEPS: [f64; 5] = [1.0 / 8.0, 1.0 / 4.0, 1.0 / 3.0, 1.0 / 2.0, 2.0 / 3.0];
    let zone = ((f64::from(v.clamp(0.0, 1.0)) + 0.05) * 10.0).floor() as usize;
    match zone {
        0..=4 => (STEPS[zone], 1.0),
        5 => (1.0, 1.0),
        _ => (1.0, STEPS[(10 - zone.min(10)).min(4)]),
    }
}

fn wobble_of(c: f64) -> (f64, f64) {
    let fraction = match c {
        c if c < 0.2 => 1.0,
        c if c < 0.4 => 0.5,
        c if c < 0.6 => 0.25,
        c if c < 0.8 => 0.125,
        _ => 0.0625,
    };
    let depth = if c < 0.6 { 0.3 } else { 0.375 * c + 0.075 };
    (fraction, depth)
}

fn stages_of(v: f32) -> (usize, usize) {
    let c = f64::from(centred(v));
    if c == 0.0 {
        return (6, 6);
    }
    let n = 6 + (c.abs() * 54.0).floor() as usize;
    if c > 0.0 {
        (6, (n + n % 2).min(MAX_STAGES))
    } else {
        ((n + 1).min(MAX_STAGES), 6)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Trans,
    Pan,
    Filter,
    Flanger,
    Phaser,
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct BeatLfo {
    kind: Kind,
    rate: f64,
    bands: Isolator,
    beats: f64,
    knobs: [f32; 2],
    buttons: [bool; 2],
    group: bool,
    level: f32,
    dry: Smoothed,
    wet: Smoothed,
    gains: [Smoothed; 2],
    start: Option<f64>,
    filters: [Svf; 2],
    line: DelayLine,
    stages: [[f32; MAX_STAGES]; 2],
    stage_out: [[f32; MAX_STAGES]; 2],
    feedback: [f32; 2],
    active: bool,
}

impl BeatLfo {
    pub fn new(kind: Kind, rate: f64) -> Self {
        let edge = |v| Smoothed::new(v, EDGE_S / 3.0, rate);
        Self {
            kind,
            rate,
            bands: Isolator::new(LOW_BAND_HZ, HIGH_BAND_HZ, rate),
            beats: if kind == Kind::Trans {
                GROUP_TRANS_BEATS
            } else {
                GROUP_BEATS
            },
            knobs: [0.5; 2],
            buttons: [false; 2],
            group: true,
            level: 0.0,
            dry: edge(1.0),
            wet: edge(0.0),
            gains: [edge(1.0), edge(1.0)],
            start: None,
            filters: [Svf::default(); 2],
            line: DelayLine::new(((FLANGER_MS + FLANGER_EXTRA_MS) / 1000.0 * rate) as usize + 8),
            stages: [[0.0; MAX_STAGES]; 2],
            stage_out: [[0.0; MAX_STAGES]; 2],
            feedback: [0.0; 2],
            active: false,
        }
    }

    fn mix(&self) -> (f32, f32) {
        match self.kind {
            Kind::Trans | Kind::Pan => chop_mix(self.level),
            Kind::Filter => filter_mix(self.level),
            Kind::Flanger | Kind::Phaser => comb_mix(self.level),
        }
    }

    fn trans_gate(&self, phase: f64, ch: usize, bpm: f64) -> f32 {
        let duty = if self.group {
            if self.level < 0.5 {
                0.5
            } else {
                0.937_5 - 0.875 * f64::from(self.level)
            }
        } else if centred(self.knobs[0]) < 0.0 {
            1.0 - 0.5 * half(self.knobs[0])
        } else {
            0.5 - 0.437_5 * half(self.knobs[0])
        };
        let period_s = self.beats * 60.0 / bpm.max(20.0);
        let duty = duty.max(MIN_OPEN_S / period_s.max(1e-6));
        let c = if self.group {
            0.0
        } else {
            f64::from(centred(self.knobs[1]))
        };
        let head = match (c < 0.0, ch) {
            (true, 1) | (false, 0) if c != 0.0 => 1.0 - 0.37 * c.abs(),
            _ => 0.0,
        };
        if (phase + head).rem_euclid(1.0) < duty {
            1.0
        } else {
            0.0
        }
    }

    fn effect(&mut self, x: [f32; 2], beat: f64, bpm: f64) -> [f32; 2] {
        let phase = beat / self.beats;
        match self.kind {
            Kind::Trans => {
                let mut out = [0.0; 2];
                for ch in 0..2 {
                    self.gains[ch].set(self.trans_gate(phase, ch, bpm));
                    let g = self.gains[ch].tick();
                    let [low, mid, high] = self.bands.split(ch, x[ch]);
                    let (lo_thru, hi_thru) = if self.group {
                        (false, false)
                    } else {
                        (self.buttons[0], self.buttons[1])
                    };
                    out[ch] = mid * g
                        + if lo_thru { low } else { low * g }
                        + if hi_thru { high } else { high * g };
                }
                out
            }
            Kind::Pan => {
                let start = *self.start.get_or_insert(beat);
                let elapsed = (beat - start) / (2.0 * self.beats);
                let (fl, fr) = if self.group {
                    (1.0, 1.0)
                } else {
                    pan_fractions(self.knobs[0])
                };
                let swing = if self.group {
                    0.0
                } else {
                    0.185 * f64::from(centred(self.knobs[1]))
                };
                let gains = [
                    triangle(elapsed / fl + 0.5 - swing),
                    triangle(elapsed / fr + swing),
                ];
                let mut out = [0.0; 2];
                for ch in 0..2 {
                    let g = gains[ch] as f32 * PAN_PEAK;
                    let [low, mid, high] = self.bands.split(ch, x[ch]);
                    let (mid_thru, sides_thru) = if self.group {
                        (false, false)
                    } else {
                        (self.buttons[0], self.buttons[1])
                    };
                    out[ch] = if mid_thru { mid } else { mid * g }
                        + if sides_thru {
                            low + high
                        } else {
                            (low + high) * g
                        };
                }
                out
            }
            Kind::Filter => self.filter(x, beat, bpm),
            Kind::Flanger => self.flanger(x, phase),
            Kind::Phaser => self.phaser(x, phase),
        }
    }

    fn filter(&mut self, x: [f32; 2], beat: f64, bpm: f64) -> [f32; 2] {
        let phase = beat / self.beats;
        let reverse = !self.group && self.buttons[1];
        let high = !self.group && self.buttons[0];
        let main = if reverse {
            1.0 - triangle(phase)
        } else {
            triangle(phase)
        };
        let resonance = if self.group {
            let s = if self.level < 0.398 {
                1.0
            } else {
                1.0 - f64::from(self.level)
            };
            0.96 * (1.0 - s * s)
        } else if centred(self.knobs[0]) < 0.0 {
            0.72 * half(self.knobs[0])
        } else {
            0.72 + 0.24 * half(self.knobs[0])
        };
        let c = if self.group {
            0.0
        } else {
            f64::from(centred(self.knobs[1]))
        };
        let (fraction, depth) = wobble_of(c.abs());
        let wobble_beats = (60.0 / WOBBLE_BPM * fraction).max(0.02) * bpm.max(20.0) / 60.0;
        let depth = if self.beats < 2.0 * wobble_beats {
            0.0
        } else {
            depth
        };
        let wobble = depth * (2.0 * triangle(beat / wobble_beats) - 1.0);
        let mut out = [0.0; 2];
        for ch in 0..2 {
            let wobbling = c != 0.0 && ((c < 0.0) == (ch == 0));
            let at = (main + if wobbling { wobble } else { 0.0 }).clamp(0.0, 1.0);
            let (lo, hi) = if high { HPF_HZ } else { LPF_HZ };
            let hz = lo * (hi / lo).powf(at);
            self.filters[ch].set(hz, 0.707 / (1.0 - 0.9 * resonance), self.rate);
            let (lp, hp) = self.filters[ch].tick(ch, x[ch]);
            out[ch] = if high { hp } else { lp };
        }
        out
    }

    fn flanger(&mut self, x: [f32; 2], phase: f64) -> [f32; 2] {
        let reverse_fb = !self.group && self.buttons[0];
        let reverse_lfo = !self.group && self.buttons[1];
        let fb = if self.group {
            if self.level < 0.25 {
                0.0
            } else {
                1.066_7 * f64::from(self.level) - 0.266_7
            }
        } else {
            feedback_of(self.knobs[0], reverse_fb)
        };
        let c = if self.group {
            0.0
        } else {
            f64::from(centred(self.knobs[1]))
        };
        let lfo = if reverse_lfo {
            1.0 - triangle(phase)
        } else {
            triangle(phase)
        };
        let mut taps = [0.0; 2];
        for (ch, tap) in taps.iter_mut().enumerate() {
            let longer = c != 0.0 && ((c < 0.0) == (ch == 0));
            let top = FLANGER_MS
                + if longer {
                    FLANGER_EXTRA_MS * c.abs()
                } else {
                    0.0
                };
            let ms = (lfo * top).max(1000.0 / self.rate);
            let gain = if ms < 0.5 {
                1.262_8 * ms + 0.368_6
            } else {
                1.0
            };
            *tap = self.line.read((ms / 1000.0 * self.rate) as f32)[ch] * gain as f32;
        }
        self.line
            .push([x[0] + taps[0] * fb as f32, x[1] + taps[1] * fb as f32]);
        taps
    }

    fn phaser(&mut self, input: [f32; 2], phase: f64) -> [f32; 2] {
        let reverse_fb = !self.group && self.buttons[0];
        let reverse_lfo = !self.group && self.buttons[1];
        let fb = if self.group {
            if self.level < 0.25 {
                0.0
            } else {
                1.066_7 * f64::from(self.level) - 0.266_7
            }
        } else {
            feedback_of(self.knobs[0], reverse_fb)
        };
        let (left, right) = if self.group {
            (6, 6)
        } else {
            stages_of(self.knobs[1])
        };
        let tri = triangle(phase + 0.5);
        let at = if reverse_lfo { 1.0 - tri } else { tri };
        let hz = PHASER_HZ.0 + (PHASER_HZ.1 - PHASER_HZ.0) * at.powi(3);
        let omega = 2.0 * PI * hz / self.rate;
        let coef = ((2.0 - omega) / (2.0 + omega)) as f32;
        let fb = (fb * (1.0 - 0.1 * (1.0 - at).powi(8))) as f32;
        let mut out = [0.0; 2];
        for (ch, n) in [left, right].into_iter().enumerate() {
            let mut v = input[ch] + self.feedback[ch] * fb;
            for k in 0..n {
                let out = -coef * v + self.stages[ch][k] + coef * self.stage_out[ch][k];
                self.stages[ch][k] = v;
                self.stage_out[ch][k] = out;
                v = out;
            }
            self.feedback[ch] = v.clamp(-4.0, 4.0);
            out[ch] = v;
        }
        out
    }
}

impl Effect for BeatLfo {
    fn kind(&self) -> EffectKind {
        match self.kind {
            Kind::Trans => EffectKind::Trans,
            Kind::Pan => EffectKind::Pan,
            Kind::Filter => EffectKind::BeatFilter,
            Kind::Flanger => EffectKind::BeatFlanger,
            Kind::Phaser => EffectKind::BeatPhaser,
        }
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let (dry, wet) = self.mix();
        let comp = match self.kind {
            Kind::Flanger | Kind::Phaser if !self.group => {
                let reverse = self.buttons[0];
                compensation(
                    self.knobs[0],
                    feedback_of(self.knobs[0], reverse),
                    reverse,
                    self.kind == Kind::Phaser,
                )
            }
            Kind::Flanger | Kind::Phaser => {
                let fb = if self.level < 0.25 {
                    0.0
                } else {
                    1.066_7 * f64::from(self.level) - 0.266_7
                };
                compensation(self.level, fb, false, false)
            }
            _ => 1.0,
        } as f32;
        self.dry.set(dry * comp);
        self.wet.set(wet * comp);
        let per_beat = beats_per_sample(ctx, self.rate);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            let beat = ctx.beat + i as f64 * per_beat;
            let fx = self.effect(*frame, beat, ctx.bpm);
            let (d, w) = (self.dry.tick(), self.wet.tick());
            frame[0] = frame[0] * d + fx[0] * w;
            frame[1] = frame[1] * d + fx[1] * w;
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.group = true;
        self.beats = if self.kind == Kind::Trans {
            GROUP_TRANS_BEATS
        } else {
            GROUP_BEATS
        };
        self.level = amount;
    }

    fn set_pad(&mut self, pad: &crate::PadParams) {
        self.set_amount(pad.level);
        self.beats = pad.beats;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        self.group = false;
        match index {
            0 => self.beats = pick(&ECHO_BEATS, value),
            1 | 2 => self.knobs[index - 1] = value,
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        if let Some(b) = self.buttons.get_mut(index) {
            *b = on;
        }
    }

    fn set_active(&mut self, on: bool) {
        if on != self.active {
            self.active = on;
            self.start = None;
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.group = false;
        self.level = dry_wet;
        true
    }

    fn reset(&mut self) {
        self.line.clear();
        self.stages = [[0.0; MAX_STAGES]; 2];
        self.stage_out = [[0.0; MAX_STAGES]; 2];
        self.feedback = [0.0; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn curves_follow_the_spec_values() {
        assert_eq!(chop_mix(0.25), (0.5, 0.5));
        assert_eq!(chop_mix(0.75), (0.0, 1.0));
        assert_eq!(comb_mix(0.5), (0.75, 0.75));
        assert!((feedback_of(0.5, false) - 0.266_7).abs() < 1e-6);
        assert!((feedback_of(1.0, false) - 0.93).abs() < 1e-6);
        assert!((feedback_of(0.5, true) + 0.8).abs() < 1e-6);
        assert_eq!(pan_fractions(0.0), (0.125, 1.0));
        assert_eq!(pan_fractions(0.5), (1.0, 1.0));
        assert_eq!(pan_fractions(1.0), (1.0, 0.125));
        assert_eq!(stages_of(0.5), (6, 6));
        assert_eq!(stages_of(1.0), (6, 60));
        let (_, depth) = wobble_of(1.0);
        assert!((depth - 0.45).abs() < 1e-9);
    }

    #[test]
    fn trans_chops_on_the_beat() {
        let mut t = BeatLfo::new(Kind::Trans, f64::from(RATE));
        t.set_amount(0.6);
        let out = run(&mut t, &vec![0.5; 2 * RATE as usize], ctx());
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert!(left[1_000] > 0.4, "open at the beat");
        assert!(left[10_000] < 0.05, "closed before the next");
    }

    #[test]
    fn every_kind_stays_bounded() {
        for kind in [
            Kind::Trans,
            Kind::Pan,
            Kind::Filter,
            Kind::Flanger,
            Kind::Phaser,
        ] {
            let mut e = BeatLfo::new(kind, f64::from(RATE));
            e.set_param(1, 1.0);
            e.set_param(2, 0.0);
            e.set_dry_wet(1.0);
            let out = run(&mut e, &sine(700.0, RATE as usize, 0.5), ctx());
            assert!(
                out.iter().all(|v| v.is_finite() && v.abs() < 4.0),
                "{kind:?}"
            );
        }
    }
}
