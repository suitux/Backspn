use std::f64::consts::PI;

use backspn_dsp::{Smoothed, Svf};

use super::{EffectKind, LfoStart, note_hz};
use crate::{Context, Effect};

const TOP_NOTE: f64 = 129.0;
const FADE_FROM_HZ: f64 = 12_000.0;
const LFO_BARS: [f64; 11] = [
    16.0,
    8.0,
    4.0,
    2.0,
    1.0,
    0.5,
    0.25,
    3.0 / 16.0,
    0.125,
    0.0625,
    1.0 / 32.0,
];
const PULSE_BASE: [f64; 9] = [140.0, 130.0, 120.0, 90.0, -20.0, -20.0, -20.0, -20.0, -20.0];
const PULSE_MOD: [f64; 9] = [-130.0, -120.0, -80.0, -50.0, 0.0, 80.0, 120.0, 160.0, 170.0];
const PULSE_RELEASE_MS: [f64; 9] = [80.0, 70.0, 65.0, 60.0, 55.0, 50.0, 45.0, 40.0, 36.0];
const RETUNE_FRAMES: usize = 8;
const PULSE_ATTACK_S: f64 = 0.001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Ladder,
    Svf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drive {
    Hand,
    Lfo,
    Pulse,
}

fn through(points: &[f64], v: f64) -> f64 {
    let x = v.clamp(0.0, 1.0) * (points.len() - 1) as f64;
    let i = (x.floor() as usize).min(points.len() - 2);
    points[i] + (points[i + 1] - points[i]) * (x - i as f64)
}

fn group_curve(v: f32) -> f32 {
    let u = 2.0 * v.clamp(0.0, 1.0) - 1.0;
    f32::midpoint(1.0, 0.55 * u + 0.45 * u * u * u)
}

fn dj_mode(family: Family, knob: f32) -> (f64, f64, [f32; 3]) {
    let x = knob.clamp(0.0, 1.0) - 0.5;
    let m = (0.5 - x.abs()).min(0.475) / 0.95;
    let f = if x > 0.0 { 1.0 - m } else { m };
    let b = f64::from(2.0 * f - 1.0);
    let (lp, hp, window) = match family {
        Family::Ladder => (
            30.0 + 102.0 * (1.0 + b.min(0.0)),
            25.0 + 102.0 * b.max(0.0),
            0.01,
        ),
        Family::Svf => (
            30.0 + 104.0 * (1.0 + b.min(0.0)),
            30.0 + 96.0 * b.max(0.0),
            0.03,
        ),
    };
    let hp_w = (f.clamp(0.5, 0.5 + window) - 0.5) / window;
    let lp_w = (0.5 - f.clamp(0.5 - window, 0.5)) / window;
    (lp, hp, [1.0 - hp_w - lp_w, lp_w, hp_w])
}

#[derive(Debug, Clone, Copy, Default)]
struct Ladder {
    s: [[f32; 4]; 2],
}

impl Ladder {
    fn tick(&mut self, ch: usize, input: f32, big_g: f32, feedback: f32, high: bool) -> f32 {
        let states = &mut self.s[ch];
        let stage_gain = if high { 1.0 - big_g } else { big_g };
        let mut offset = 0.0;
        for state in states.iter() {
            let beta = (1.0 - big_g) * state;
            offset = stage_gain * offset + if high { -beta } else { beta };
        }
        let total = stage_gain.powi(4);
        let last = (total * input + offset) / (1.0 + feedback * total);
        let mut v = input - feedback * last;
        for state in states.iter_mut() {
            let w = (v - *state) * big_g;
            let lp = w + *state;
            *state = lp + w;
            v = if high { v - lp } else { lp };
        }
        v
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Pair {
    lp: Ladder,
    hp: Ladder,
    svf_lp: Svf,
    svf_hp: Svf,
    lp_g: f32,
    hp_g: f32,
    k: f32,
    gain: f32,
}

impl Pair {
    fn set(&mut self, family: Family, lp_note: f64, hp_note: f64, res: f32, rate: f64) {
        self.set_scaled(family, lp_note, hp_note, res, 0.82, rate);
    }

    fn set_scaled(
        &mut self,
        family: Family,
        lp_note: f64,
        hp_note: f64,
        res: f32,
        scale: f64,
        rate: f64,
    ) {
        let (lp_hz, hp_hz) = (note_hz(lp_note), note_hz(hp_note));
        let r = res.clamp(0.0, 1.0) * (2.0 - res.clamp(0.0, 1.0));
        match family {
            Family::Ladder => {
                let g = |hz: f64| {
                    let w = PI * hz.clamp(rate / 24_576.0, 0.47 * rate) / rate;
                    let g = w + w.powi(3) / 3.0 + 2.0 * w.powi(5) / 15.0;
                    (g / (1.0 + g)) as f32
                };
                self.lp_g = g(lp_hz);
                self.hp_g = g(hp_hz);
                let r = scale * f64::from(r);
                let k = 4.0 * r * (1.0 - 4.7e-5 * (lp_hz.clamp(5_000.0, 12_000.0) - 5_000.0));
                self.k = k as f32;
                self.gain = (1.0 + r * (lp_note.clamp(46.0, 80.0) - 46.0) / 68.0) as f32;
            }
            Family::Svf => {
                let q = 1.75 * f64::from(r) - 0.99;
                let damping = (2.0 * (1.0 - q)).max(0.05);
                self.svf_lp.set(lp_hz, 1.0 / damping, rate);
                self.svf_hp.set(hp_hz, 1.0 / damping, rate);
                self.gain = 1.0;
            }
        }
    }

    fn low(&mut self, family: Family, ch: usize, x: f32) -> f32 {
        match family {
            Family::Ladder => self.lp.tick(ch, x, self.lp_g, self.k, false) * self.gain,
            Family::Svf => self.svf_lp.tick(ch, x).0,
        }
    }

    fn high(&mut self, family: Family, ch: usize, x: f32) -> f32 {
        match family {
            Family::Ladder => self.hp.tick(ch, x, self.hp_g, self.k, true) * self.gain,
            Family::Svf => self.svf_hp.tick(ch, x).1,
        }
    }
}

fn swept_wet(hz: f64) -> f32 {
    let top = note_hz(TOP_NOTE);
    (1.0 - (hz.clamp(FADE_FROM_HZ, top) - FADE_FROM_HZ) / (top - FADE_FROM_HZ)) as f32
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct FilterFx {
    family: Family,
    drive: Drive,
    rate: f64,
    pair: Pair,
    knobs: [f32; 3],
    buttons: [bool; 2],
    group: bool,
    depth: Smoothed,
    active: bool,
    phase: f64,
    start: LfoStart,
    envelope: f32,
    smoothed_env: f32,
    pulse_left: f64,
    retune: usize,
    wet: f32,
}

impl FilterFx {
    pub fn new(family: Family, drive: Drive, rate: f64) -> Self {
        let knobs = match (family, drive) {
            (_, Drive::Hand) => [1.0, 0.498, 0.0],
            (Family::Ladder, Drive::Lfo) => [0.635, 0.502, 0.502],
            (Family::Svf, Drive::Lfo) => [0.62, 0.745, 0.498],
            (Family::Ladder, Drive::Pulse) => [1.0, 0.6, 0.51],
            (Family::Svf, Drive::Pulse) => [1.0, 0.502, 0.502],
        };
        Self {
            family,
            drive,
            rate,
            pair: Pair::default(),
            knobs,
            buttons: [false; 2],
            group: true,
            depth: Smoothed::new(1.0, 0.01, rate),
            active: false,
            phase: 0.0,
            start: LfoStart::default(),
            envelope: 0.0,
            smoothed_env: 0.0,
            pulse_left: 0.0,
            retune: 0,
            wet: 1.0,
        }
    }

    fn hand(&mut self, x: [f32; 2], d: f32) -> [f32; 2] {
        let res = if self.group {
            if self.family == Family::Ladder {
                0.5
            } else {
                0.0
            }
        } else {
            self.knobs[1]
        };
        let dj = self.group || self.buttons[0];
        let retune = self.retune == 0;
        self.retune = (self.retune + 1) % RETUNE_FRAMES;
        let mut out = [0.0; 2];
        if dj {
            let knob = if self.group {
                group_curve(self.knobs[0])
            } else {
                self.knobs[0]
            };
            let (lp, hp, [centre, lw, hw]) = dj_mode(self.family, knob);
            if retune {
                self.pair.set(self.family, lp, hp, res, self.rate);
            }
            for ch in 0..2 {
                let low = self.pair.low(self.family, ch, x[ch]);
                let high = self.pair.high(self.family, ch, x[ch]);
                out[ch] = x[ch] * (1.0 - d + d * centre) + d * (lw * low + hw * high);
            }
            return out;
        }
        let note = |k: f32| match self.family {
            Family::Ladder => 5.0 + 125.0 * f64::from(k),
            Family::Svf => 20.0 + 109.0 * f64::from(k),
        };
        let (a, b) = (note(self.knobs[0]), note(self.knobs[2]));
        let band_reject = self.buttons[1];
        if band_reject {
            if retune {
                self.pair.set(self.family, b.min(a), a, res, self.rate);
            }
            for ch in 0..2 {
                let low = self.pair.low(self.family, ch, x[ch]);
                let high = self.pair.high(self.family, ch, x[ch]);
                out[ch] = x[ch] * (1.0 - d) + d * (low + high);
            }
        } else {
            if retune {
                self.pair.set(self.family, a, b.min(a), res, self.rate);
            }
            for ch in 0..2 {
                let low = self.pair.low(self.family, ch, x[ch]);
                let band = self.pair.high(self.family, ch, low);
                out[ch] = x[ch] * (1.0 - d) + d * band;
            }
        }
        out
    }

    fn swept(&mut self, x: [f32; 2], note: f64, res: f32, scale: f64) -> [f32; 2] {
        let note = note.min(140.0);
        if self.retune == 0 {
            self.pair
                .set_scaled(self.family, note.min(TOP_NOTE), 0.0, res, scale, self.rate);
            self.wet = swept_wet(note_hz(note));
        }
        self.retune = (self.retune + 1) % RETUNE_FRAMES;
        let mut out = [0.0; 2];
        for ch in 0..2 {
            let low = self.pair.low(self.family, ch, x[ch]);
            out[ch] = x[ch] + (low - x[ch]) * self.wet;
        }
        out
    }

    fn lfo_period(&self, knob: f32) -> (bool, f64) {
        let free = self.group || self.buttons[0];
        let x = f64::from(knob) * 10.0;
        let bars = if free {
            let i = (x.floor() as usize).min(9);
            LFO_BARS[i] + (LFO_BARS[i + 1] - LFO_BARS[i]) * (x - i as f64)
        } else {
            LFO_BARS[(x.round() as usize).min(10)]
        };
        (free, 4.0 * bars)
    }

    fn lfo(&mut self, beat: f64, per_frame: f64) -> f64 {
        let (free, beats) = self.lfo_period(self.knobs[0]);
        self.phase = if free {
            (self.phase + per_frame / beats).rem_euclid(1.0)
        } else {
            (self.start.beat(beat) / beats).rem_euclid(1.0)
        };
        let shape = if self.group {
            0.5
        } else {
            0.5 + 0.5 * f64::from(self.knobs[2])
        };
        let shape = shape.min(0.999);
        if self.phase < shape {
            self.phase / shape
        } else {
            (1.0 - self.phase) / (1.0 - shape)
        }
    }

    fn pulse(&mut self, x: [f32; 2], depth: f32) -> f32 {
        let peak = x[0].abs().max(x[1].abs());
        let (mode, sensitive) = if self.group {
            (false, false)
        } else {
            (self.buttons[0], self.buttons[1])
        };
        let threshold = if sensitive { 0.8 } else { 0.5 };
        let input = if mode {
            if peak > threshold {
                self.pulse_left = self.rate / 1000.0;
            }
            self.pulse_left -= 1.0;
            if self.pulse_left > 0.0 { 1.0 } else { 0.0 }
        } else {
            peak.min(1.0)
        };
        let release_ms = through(&PULSE_RELEASE_MS, f64::from(depth));
        let half_life = |s: f64| (0.5f64.powf(1.0 / (s * self.rate))) as f32;
        let coef = if input > self.envelope {
            half_life(PULSE_ATTACK_S)
        } else {
            half_life(release_ms / 1000.0)
        };
        self.envelope = input + (self.envelope - input) * coef;
        let e = self.envelope * (2.0 - self.envelope);
        let soft = if self.group { 0.05 } else { self.knobs[2] };
        let t = f64::from(soft * soft) * 0.0595 + 0.0005;
        let c = half_life(t);
        self.smoothed_env = e + (self.smoothed_env - e) * c;
        self.smoothed_env
    }
}

impl Effect for FilterFx {
    fn kind(&self) -> EffectKind {
        match (self.family, self.drive) {
            (Family::Ladder, Drive::Hand) => EffectKind::Filter,
            (Family::Ladder, Drive::Lfo) => EffectKind::LadderLfo,
            (Family::Ladder, Drive::Pulse) => EffectKind::LadderPulse,
            (Family::Svf, Drive::Hand) => EffectKind::SvfFilter,
            (Family::Svf, Drive::Lfo) => EffectKind::SvfLfo,
            (Family::Svf, Drive::Pulse) => EffectKind::SvfPulse,
        }
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let per_frame = ctx.bpm.max(20.0) / 60.0 / self.rate;
        if self.drive == Drive::Lfo {
            let (free, beats) = self.lfo_period(self.knobs[0]);
            if self.start.begin(ctx, beats, f64::INFINITY) && free {
                self.phase = 0.0;
            }
        }
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            let on = if self.active { 1.0 } else { 0.0 };
            let depth = self.depth.tick() * on;
            *frame = match self.drive {
                Drive::Hand => {
                    let d = depth * (2.0 - depth);
                    self.hand(*frame, d)
                }
                Drive::Lfo => {
                    let beat = ctx.beat + i as f64 * per_frame;
                    let lfo = self.lfo(beat, per_frame);
                    let up = !self.group && self.buttons[1];
                    let range = 92.0 * f64::from(depth);
                    let at = if up { 1.0 - lfo } else { lfo };
                    let res = if self.group { 0.9 } else { self.knobs[1] };
                    self.swept(*frame, TOP_NOTE - range * at, res, 0.84)
                }
                Drive::Pulse => {
                    let e = f64::from(self.pulse(*frame, depth));
                    let amount = if self.group {
                        0.9
                    } else {
                        f64::from(self.knobs[0])
                    };
                    let base = through(&PULSE_BASE, amount);
                    let swing = through(&PULSE_MOD, amount);
                    let note = TOP_NOTE + (base + swing * e - TOP_NOTE) * f64::from(depth);
                    let res = match (self.group, self.family) {
                        (true, Family::Ladder) => 0.66,
                        (true, Family::Svf) => 0.75,
                        _ => self.knobs[1],
                    };
                    self.swept(*frame, note, res, 0.82)
                }
            };
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.group = true;
        match self.drive {
            Drive::Hand | Drive::Lfo => {
                self.knobs[0] = amount;
                self.depth
                    .set(if self.drive == Drive::Lfo { 0.78 } else { 1.0 });
            }
            Drive::Pulse => self.depth.set(amount),
        }
    }

    fn set_param(&mut self, index: usize, value: f32) {
        self.group = false;
        if index == 0 && self.drive == Drive::Lfo {
            let (free, old) = self.lfo_period(self.knobs[0]);
            let (_, new) = self.lfo_period(value.clamp(0.0, 1.0));
            if !free && (new - old).abs() > f64::EPSILON {
                self.start.reset();
            }
        }
        if let Some(k) = self.knobs.get_mut(index) {
            *k = value.clamp(0.0, 1.0);
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        if let Some(b) = self.buttons.get_mut(index) {
            *b = on;
        }
    }

    fn set_active(&mut self, on: bool) {
        if on && !self.active {
            self.start.start();
        }
        self.active = on;
    }

    fn lfo_reset(&mut self) {
        if self.drive == Drive::Lfo {
            self.start.reset();
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.group = false;
        self.depth.set(dry_wet.clamp(0.0, 1.0));
        true
    }

    fn reset(&mut self) {
        self.pair = Pair::default();
        self.envelope = 0.0;
        self.smoothed_env = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    fn filter(family: Family, drive: Drive) -> FilterFx {
        let mut f = FilterFx::new(family, drive, f64::from(RATE));
        f.set_active(true);
        f
    }

    #[test]
    fn curves_follow_the_spec_values() {
        let (lp, hp, w) = dj_mode(Family::Ladder, 0.5);
        assert!((lp - 132.0).abs() < 1e-9 && (hp - 25.0).abs() < 1e-9);
        assert_eq!(w, [1.0, 0.0, 0.0], "centre: dry");
        let (lp, _, w) = dj_mode(Family::Ladder, 0.0);
        assert!((lp - 30.0).abs() < 1e-9 && (w[1] - 1.0).abs() < 1e-6);
        let (_, hp, w) = dj_mode(Family::Svf, 1.0);
        assert!((hp - 126.0).abs() < 1e-9 && (w[2] - 1.0).abs() < 1e-6);
        assert!((group_curve(0.5) - 0.5).abs() < 1e-6 && (group_curve(1.0) - 1.0).abs() < 1e-6);
        assert!(
            (through(&PULSE_MOD, 0.9) - 162.0).abs() < 1e-9,
            "Group: +162"
        );
        assert!(swept_wet(note_hz(TOP_NOTE)).abs() < 1e-6);
    }

    #[test]
    fn hand_filters_cut_on_their_side_and_pass_at_the_centre() {
        for family in [Family::Ladder, Family::Svf] {
            let level = |knob: f32, hz: f64| {
                let mut f = filter(family, Drive::Hand);
                f.set_amount(knob);
                rms(&run(&mut f, &sine(hz, RATE as usize / 4, 0.3), ctx())[2 * 4_000..])
            };
            assert!(
                (level(0.5, 3_000.0) - 0.3 / 2f32.sqrt()).abs() < 0.01,
                "{family:?}"
            );
            assert!(
                level(0.1, 5_000.0) < 0.3 * level(0.5, 5_000.0),
                "{family:?} low-pass"
            );
            assert!(
                level(0.9, 100.0) < 0.3 * level(0.5, 100.0),
                "{family:?} high-pass"
            );
        }
    }

    #[test]
    fn lfo_sweeps_on_the_beat_and_pulse_follows_the_level() {
        let mut f = filter(Family::Ladder, Drive::Lfo);
        f.set_param(0, 0.4);
        f.set_dry_wet(1.0);
        let input = sine(6_000.0, 2 * RATE as usize, 0.3);
        let out = run(&mut f, &input, ctx());
        let chunks: Vec<f32> = out.chunks(2 * 4_800).map(rms).collect();
        let (lo, hi) = chunks
            .iter()
            .fold((f32::MAX, 0.0f32), |(l, h), &v| (l.min(v), h.max(v)));
        assert!(hi > 3.0 * lo, "sweeps: {lo} … {hi}");

        let mut p = filter(Family::Svf, Drive::Pulse);
        p.set_amount(1.0);
        let quiet = run(&mut p, &sine(3_000.0, RATE as usize / 4, 0.02), ctx());
        let mut p = filter(Family::Svf, Drive::Pulse);
        p.set_amount(1.0);
        let loud = run(&mut p, &sine(3_000.0, RATE as usize / 4, 0.8), ctx());
        assert!(
            rms(&loud[2 * 4_000..]) / 0.8 > 2.0 * rms(&quiet[2 * 4_000..]) / 0.02,
            "opens with the level"
        );
    }
}
