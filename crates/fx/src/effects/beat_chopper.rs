use backspn_dsp::{DelayLine, Smoothed};

use super::{EffectKind, beats_per_sample};
use crate::{Context, Effect};

const LENGTH_BEATS: [f64; 9] = [4.0, 3.0, 2.0, 1.5, 1.0, 0.75, 0.5, 0.25, 0.125];
const HISTORY_S: f64 = 8.5;
const FADE_S: f64 = 0.003;

fn length_of(v: f32) -> f64 {
    LENGTH_BEATS[((8.0 * v.clamp(0.0, 1.0)).round() as usize).min(8)]
}

fn gate_of(v: f32) -> (f64, bool) {
    let g = f64::from(v.clamp(0.0, 1.0)) - 0.5;
    let a = 0.5 - g.abs();
    let c = if a > 0.45 { 0.499_99 } else { 1.111 * a };
    let value = if g > 0.0 { 1.0 - c } else { c };
    (1.0 - (2.0 * value - 1.0).abs(), g > 0.0)
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct BeatChopper {
    rate: f64,
    history: DelayLine,
    written: u64,
    active: bool,
    beats: f64,
    rotate: f32,
    gate: f32,
    reverse: bool,
    wrap: bool,
    start: Option<u64>,
    len: f64,
    into: f64,
    next_beats: f64,
    point: Option<f64>,
    dry_wet: f32,
    mix: Smoothed,
}

impl BeatChopper {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            history: DelayLine::new((HISTORY_S * rate) as usize),
            written: 0,
            active: false,
            beats: 1.0,
            rotate: 0.0,
            gate: 0.5,
            reverse: false,
            wrap: true,
            start: None,
            len: 1.0,
            into: 0.0,
            next_beats: 1.0,
            point: None,
            dry_wet: 1.0,
            mix: Smoothed::new(0.0, FADE_S, rate),
        }
    }

    fn frame(&mut self, x: [f32; 2], beat: f64, bpm: f64) -> [f32; 2] {
        self.history.push(x);
        let now = self.written;
        self.written += 1;
        self.mix.set(if self.active { self.dry_wet } else { 0.0 });
        let mix = self.mix.tick();
        if !self.active {
            self.start = None;
            self.point = None;
            return x;
        }
        let point = *self
            .point
            .get_or_insert_with(|| (beat / self.beats).ceil() * self.beats);
        if self.start.is_none() {
            if beat < point {
                return x;
            }
            self.start = Some(now);
            self.len = self.beats * 60.0 / bpm.max(20.0) * self.rate;
            self.into = 0.0;
        }
        let start = self.start.unwrap_or(now) as f64;
        if self.into >= self.len {
            self.into = 0.0;
            if self.wrap {
                self.len = self.next_beats * 60.0 / bpm.max(20.0) * self.rate;
            }
        }
        if !self.wrap {
            self.len = self.beats * 60.0 / bpm.max(20.0) * self.rate;
        }
        let rotated = (self.into + f64::from(self.rotate) * self.len).rem_euclid(self.len);
        let pos = if self.reverse {
            self.len - 1.0 - rotated
        } else {
            rotated
        };
        let at = (start + pos) as u64;
        let played = if at >= now || now - at >= self.history.capacity() as u64 {
            x
        } else {
            self.history.at(at as usize)
        };
        let (open, at_end) = gate_of(self.gate);
        let share = self.into / self.len;
        let heard = if at_end {
            share < open
        } else {
            share >= 1.0 - open
        };
        let edge = ((self.into.min(self.len - self.into)) / (FADE_S * self.rate)).min(1.0) as f32;
        let gain = if heard { edge } else { 0.0 };
        self.into += 1.0;
        [
            x[0] + (played[0] * gain - x[0]) * mix,
            x[1] + (played[1] * gain - x[1]) * mix,
        ]
    }
}

impl Effect for BeatChopper {
    fn kind(&self) -> EffectKind {
        EffectKind::BeatChopper
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let per_frame = beats_per_sample(ctx, self.rate);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            *frame = self.frame(*frame, ctx.beat + i as f64 * per_frame, ctx.bpm);
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.set_param(0, amount);
        self.wrap = true;
        self.rotate = 0.0;
        self.gate = 0.5;
        self.reverse = false;
        self.dry_wet = 1.0;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => {
                self.next_beats = length_of(value);
                if self.start.is_none() {
                    self.beats = self.next_beats;
                }
            }
            1 => self.rotate = ((8.0 * value.clamp(0.0, 1.0)).round() * 2.0) / 16.0,
            2 => self.gate = value,
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.reverse = on,
            1 => self.wrap = on,
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

    fn listens(&self) -> bool {
        true
    }

    fn reset(&mut self) {
        self.history.clear();
        self.start = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn knobs_follow_their_steps() {
        assert!((length_of(0.0) - 4.0).abs() < 1e-9 && (length_of(1.0) - 0.125).abs() < 1e-9);
        assert!((length_of(0.5) - 1.0).abs() < 1e-9, "1/4 at the centre");
        let (open, _) = gate_of(0.5);
        assert!((open - 1.0).abs() < 1e-3, "neutral");
        let (open, at_end) = gate_of(1.0);
        assert!(open < 0.01 && at_end);
    }

    #[test]
    fn repeats_from_the_grid_point() {
        let mut b = BeatChopper::new(f64::from(RATE));
        b.set_amount(6.0 / 8.0);
        b.set_active(true);
        let ramp: Vec<f32> = (0..RATE as usize)
            .flat_map(|i| [i as f32 / 48_000.0; 2])
            .collect();
        let out = run(&mut b, &ramp, ctx());
        assert!((out[2 * (12_000 + 3_000)] - ramp[2 * 3_000]).abs() < 0.01);
    }
}
