use backspn_dsp::{Smoothed, Svf};

use super::{EffectKind, beats_per_sample, note_hz};
use crate::{Context, Effect};

const AUTO_CAPTURE: usize = 131_072;
const STEP_STUTTER_CAPTURE_S: f64 = 10.0;
const FADE_S: f64 = 0.007;
const MAX_HZ: f64 = 142.0;
const LOOK_AHEAD_S: f64 = 0.007;
const CYCLE: usize = 32;
const AUT_BEATS: f64 = 4.0;

fn bipolar(v: f32, dead: f64) -> f64 {
    let x = 2.0 * f64::from(v.clamp(0.0, 1.0)) - 1.0;
    if x.abs() <= dead {
        0.0
    } else {
        x.signum() * (x.abs() - dead) / (1.0 - dead)
    }
}

fn step_stutter_hz(v: f32, bps: f64, double: bool) -> f64 {
    let s = bipolar(v, 0.05);
    let base = if double { 4.0 } else { 2.0 } * bps;
    if s >= 0.0 {
        base + s * s * (MAX_HZ - base)
    } else {
        base + s.abs() * (bps / 2.0 - base)
    }
}

fn step_stutter_semitones(v: f32) -> f64 {
    let s = bipolar(v, 0.01);
    if s >= 0.0 { 36.0 * s * s } else { 36.0 * s }
}

fn ratio_of(semitones: f64) -> f64 {
    2f64.powf(semitones / 12.0).clamp(0.125, 12.0)
}

fn segments(pattern: usize, alt: bool) -> &'static [usize] {
    match (alt, pattern) {
        (false, 0) => &[32],
        (false, 1) => &[16, 16],
        (false, 2) => &[8; 4],
        (false, 3) => &[4; 8],
        (false, _) => &[2; 16],
        (true, 0) => &[24, 8],
        (true, 1) => &[12, 12, 8],
        (true, 2) => &[6, 6, 4, 6, 6, 4],
        (true, 3) => &[3, 3, 2, 3, 3, 2, 3, 3, 2, 3, 3, 2],
        (true, _) => &[1; 32],
    }
}

fn segment_at(pattern: usize, alt: bool, step: usize) -> Option<usize> {
    let mut at = 0;
    for &len in segments(pattern, alt) {
        if at == step % CYCLE {
            return Some(len);
        }
        at += len;
    }
    None
}

fn bend_of(v: f32) -> (f64, f64, f64) {
    let b = bipolar(v, 0.05);
    let lerp = |a: f64, c: f64| a + (c - a) * b.abs();
    if b < 0.0 {
        (lerp(16.0, 128.0), lerp(16.0, 2.0), lerp(0.66, 1.0))
    } else {
        (lerp(16.0, 2.0), lerp(16.0, 128.0), 0.66)
    }
}

fn curve(p: f64, k: f64) -> f64 {
    let a = 2.0 * p.clamp(0.0, 1.0) - 1.0;
    let u = a * (1.0 + k * (1.0 - a.abs()));
    0.5 + u * (0.335 + 0.165 * u.abs())
}

fn auto_semitones(v: f32) -> f64 {
    let y = bipolar(v, 0.05);
    24.0 * y.signum() * y * y
}

#[derive(Debug, Clone, Copy, Default)]
struct Voice {
    buffer: usize,
    pos: f64,
    ratio: f64,
}

#[derive(Debug)]
struct Repeater {
    buffers: [Box<[[f32; 2]]>; 2],
    written: [usize; 2],
    current: usize,
    voices: [Voice; 2],
    newest: usize,
    fade: f32,
    fade_step: f32,
}

impl Repeater {
    fn new(frames: usize, rate: f64) -> Self {
        Self {
            buffers: std::array::from_fn(|_| vec![[0.0; 2]; frames].into_boxed_slice()),
            written: [0; 2],
            current: 0,
            voices: [Voice::default(); 2],
            newest: 0,
            fade: 1.0,
            fade_step: 1.0 / (FADE_S * rate) as f32,
        }
    }

    fn capture(&mut self) {
        self.current = 1 - self.current;
        self.written[self.current] = 0;
    }

    fn retrigger(&mut self, ratio: f64) {
        self.newest = 1 - self.newest;
        self.voices[self.newest] = Voice {
            buffer: self.current,
            pos: 0.0,
            ratio,
        };
        self.fade = 0.0;
    }

    fn record(&mut self, x: [f32; 2]) {
        let b = self.current;
        if self.written[b] < self.buffers[b].len() {
            self.buffers[b][self.written[b]] = x;
            self.written[b] += 1;
        }
    }

    fn read(&self, v: Voice) -> [f32; 2] {
        let last = self.written[v.buffer];
        if last == 0 {
            return [0.0; 2];
        }
        self.buffers[v.buffer][(v.pos as usize).min(last - 1)]
    }

    fn tick(&mut self) -> [f32; 2] {
        self.fade = (self.fade + self.fade_step).min(1.0);
        let (new, old) = (self.voices[self.newest], self.voices[1 - self.newest]);
        let (a, b) = (self.read(new), self.read(old));
        for v in &mut self.voices {
            v.pos += v.ratio;
        }
        let f = self.fade;
        [a[0] * f + b[0] * (1.0 - f), a[1] * f + b[1] * (1.0 - f)]
    }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct StepStutter {
    rate: f64,
    repeater: Repeater,
    lowpass: Svf,
    active: bool,
    playing: bool,
    phase: f64,
    since_capture: f64,
    speed: f32,
    transpose: f32,
    double: bool,
    hold: bool,
    dry_wet: f32,
    mix: Smoothed,
}

impl StepStutter {
    pub fn new(rate: f64) -> Self {
        let mut b = Self {
            rate,
            repeater: Repeater::new((STEP_STUTTER_CAPTURE_S * rate) as usize, rate),
            lowpass: Svf::default(),
            active: false,
            playing: false,
            phase: 0.0,
            since_capture: 0.0,
            speed: 0.498,
            transpose: 0.5,
            double: false,
            hold: true,
            dry_wet: 1.0,
            mix: Smoothed::new(0.0, FADE_S, rate),
        };
        b.set_filter(1.0);
        b
    }

    fn set_filter(&mut self, v: f32) {
        let hz = note_hz(40.0 + 92.0 * f64::from(v.clamp(0.0, 1.0)));
        self.lowpass
            .set(hz, std::f64::consts::FRAC_1_SQRT_2, self.rate);
    }
}

impl Effect for StepStutter {
    fn kind(&self) -> EffectKind {
        EffectKind::StepStutter
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let bps = ctx.bpm.max(20.0) / 60.0;
        let hz = step_stutter_hz(self.speed, bps, self.double).min(self.rate / 2.0);
        let ratio = ratio_of(step_stutter_semitones(self.transpose));
        let per_beat = beats_per_sample(ctx, self.rate);
        if self.active && !self.playing {
            self.playing = true;
            self.repeater.capture();
            self.repeater.retrigger(ratio);
            self.phase = 0.0;
            self.since_capture = 0.0;
        } else if !self.active {
            self.playing = false;
        }
        self.mix.set(if self.playing { 1.0 } else { 0.0 });
        let v = self.dry_wet;
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let a = self.mix.tick();
            if a == 0.0 && !self.playing {
                continue;
            }
            self.repeater.record(*frame);
            self.phase += hz / self.rate;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                self.repeater.retrigger(ratio);
            }
            self.since_capture += per_beat;
            if !self.hold && self.since_capture >= AUT_BEATS {
                self.since_capture = 0.0;
                self.repeater.capture();
                self.repeater.retrigger(ratio);
                self.phase = 0.0;
            }
            let wet = self.repeater.tick();
            let (w, d) = (a * v * (2.0 - v), 1.0 - a * v * v);
            for ch in 0..2 {
                let (lp, _) = self.lowpass.tick(ch, wet[ch]);
                frame[ch] = frame[ch] * d + lp * w;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.speed = amount;
        self.transpose = 0.5;
        self.double = false;
        self.hold = true;
        self.dry_wet = 1.0;
        self.set_filter(1.0);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.speed = value,
            1 => self.set_filter(value),
            2 => self.transpose = value,
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.double = on,
            1 => self.hold = on,
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
        self.playing = false;
    }
}

#[derive(Debug)]
pub struct AutoStepStutter {
    rate: f64,
    repeater: Repeater,
    active: bool,
    start: Option<f64>,
    last_step: Option<usize>,
    progress: f64,
    length: usize,
    phase: f64,
    pattern: usize,
    alt: bool,
    double: bool,
    bend: f32,
    transpose: f32,
    dry_wet: f32,
    mix: Smoothed,
}

impl AutoStepStutter {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            repeater: Repeater::new(AUTO_CAPTURE, rate),
            active: false,
            start: None,
            last_step: None,
            progress: 0.0,
            length: CYCLE,
            phase: 0.0,
            pattern: 0,
            alt: false,
            double: false,
            bend: 0.502,
            transpose: 0.502,
            dry_wet: 1.0,
            mix: Smoothed::new(0.0, FADE_S, rate),
        }
    }

    fn frame(&mut self, input: [f32; 2], beat: f64, bps: f64) -> [f32; 2] {
        let Some(start) = self.start else {
            return input;
        };
        let ahead = LOOK_AHEAD_S * bps;
        let pos = (beat + ahead - start) * 4.0;
        if pos < 0.0 {
            return input;
        }
        let step = pos.floor() as usize % CYCLE;
        if self.last_step != Some(step) {
            self.last_step = Some(step);
            if let Some(len) = segment_at(self.pattern, self.alt, step) {
                self.length = len;
                self.progress = 0.0;
                self.repeater.capture();
                self.phase = 1.0;
            }
        }
        self.repeater.record(input);
        let (from, to, shape) = bend_of(self.bend);
        let along = curve(self.progress, shape);
        let scale = if self.double { 0.75 } else { 0.375 };
        let hz = (scale * (from + along * (to - from))).clamp(0.1, MAX_HZ);
        self.progress += bps * 4.0 / (self.length as f64 * self.rate);
        self.phase += hz / self.rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.repeater
                .retrigger(ratio_of(auto_semitones(self.transpose) * along));
        }
        let wet = self.repeater.tick();
        let on = self.mix.tick();
        let w = self.dry_wet * (2.0 - self.dry_wet);
        let (wet_gain, dry_gain) = (on * 0.92 * w * (2.0 - w), 1.0 - on * self.dry_wet.powi(4));
        [
            input[0] * dry_gain + wet[0] * wet_gain,
            input[1] * dry_gain + wet[1] * wet_gain,
        ]
    }
}

impl Effect for AutoStepStutter {
    fn kind(&self) -> EffectKind {
        EffectKind::AutoStepStutter
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        if self.active && self.start.is_none() {
            self.start = Some((ctx.beat * 4.0).ceil() / 4.0);
            self.last_step = None;
        }
        self.mix.set(if self.active { 1.0 } else { 0.0 });
        let bps = ctx.bpm.max(20.0) / 60.0;
        let per_beat = beats_per_sample(ctx, self.rate);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            *frame = self.frame(*frame, ctx.beat + i as f64 * per_beat, bps);
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.bend = amount;
        self.pattern = 0;
        self.alt = false;
        self.double = false;
        self.transpose = 0.5;
        self.dry_wet = 1.0;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.pattern = ((value.clamp(0.0, 1.0) * 4.0).round() as usize).min(4),
            1 => self.bend = value,
            2 => self.transpose = value,
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.alt = on,
            1 => self.double = on,
            _ => {}
        }
    }

    fn set_active(&mut self, on: bool) {
        if on != self.active {
            self.active = on;
            if !on {
                self.start = None;
            }
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.dry_wet = dry_wet.clamp(0.0, 1.0);
        true
    }

    fn reset(&mut self) {
        self.start = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn curves_follow_the_spec_values() {
        assert!(
            (step_stutter_hz(0.5, 2.0, false) - 4.0).abs() < 1e-9,
            "centre: 1/8"
        );
        assert!((step_stutter_hz(1.0, 2.0, false) - MAX_HZ).abs() < 1e-9);
        assert!((step_stutter_hz(0.0, 2.0, false) - 1.0).abs() < 1e-9);
        assert!((step_stutter_semitones(1.0) - 36.0).abs() < 1e-9);
        assert!((step_stutter_semitones(0.0) + 36.0).abs() < 1e-9);
        assert_eq!(segment_at(0, false, 0), Some(32));
        assert_eq!(segment_at(3, true, 3), Some(3));
        assert_eq!(segment_at(3, true, 6), Some(2));
        assert_eq!(segment_at(1, true, 30), None);
        for p in 0..5 {
            for alt in [false, true] {
                assert_eq!(segments(p, alt).iter().sum::<usize>(), CYCLE);
            }
        }
        assert!(curve(0.0, 1.0).abs() < 1e-9 && (curve(1.0, 0.66) - 1.0).abs() < 1e-9);
        assert_eq!(bend_of(0.5), (16.0, 16.0, 0.66));
        assert_eq!(bend_of(1.0), (2.0, 128.0, 0.66));
    }

    #[test]
    fn step_stutter_repeats_the_moment_it_caught() {
        let mut b = StepStutter::new(f64::from(RATE));
        b.set_amount(0.5);
        b.set_active(true);
        let ramp: Vec<f32> = (0..RATE as usize)
            .flat_map(|i| [i as f32 / 48_000.0; 2])
            .collect();
        let out = run(&mut b, &ramp, ctx());
        assert!((out[2 * (12_000 + 1_000)] - ramp[2 * 1_000]).abs() < 0.01);
        assert!((out[2 * (36_000 + 1_000)] - ramp[2 * 1_000]).abs() < 0.01);
    }

    #[test]
    fn auto_step_stutter_starts_a_new_capture_on_each_segment() {
        let mut a = AutoStepStutter::new(f64::from(RATE));
        a.set_param(0, 1.0);
        a.set_active(true);
        let ramp: Vec<f32> = (0..2 * RATE as usize)
            .flat_map(|i| [i as f32 / 96_000.0; 2])
            .collect();
        let out = run(&mut a, &ramp, ctx());
        let start = 4.0 * 12_000.0 - LOOK_AHEAD_S * 48_000.0;
        let at = (start + 8_000.0 + 2_000.0) as usize;
        let expected = 0.92 * (start + 2_000.0) / 96_000.0;
        assert!(
            (f64::from(out[2 * at]) - expected).abs() < 0.01,
            "{} not {expected}",
            out[2 * at]
        );
    }
}
