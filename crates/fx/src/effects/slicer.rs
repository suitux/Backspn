use backspn_dsp::{DelayLine, Smoothed};

use super::{EffectKind, beats_per_sample};
use crate::{Context, Effect};

const STEPS: usize = 32;
const MAX_S: f64 = 8.3;
const FADE_S: f64 = 0.003;
const GROUP_STYLE: usize = 1;
const GO_LEVEL: f32 = 0.1;
const GO_AFTER_BEATS: f64 = 1.0 / 6.0;

fn slice_of(style: usize, pattern: usize, step: usize) -> usize {
    let bar = step / 16 * 16;
    let i = step % 16;
    if pattern == 0 {
        return step;
    }
    match style {
        0 => bar + i % (16 >> pattern),
        1 => {
            bar + match pattern {
                1 => i % 6,
                2 => i % 10,
                3 => i % 3,
                _ => {
                    if i < 6 {
                        i % 3
                    } else {
                        i - 6
                    }
                }
            }
        }
        2 => match pattern {
            1 => bar + (i / 8) * 8 + i % 4,
            2 => bar + (i / 4) * 4 + i % 2,
            3 => step / 2,
            _ => bar + if i < 8 { i % 4 } else { i % 2 },
        },
        3 => {
            let back = match pattern {
                1 => i % 4 == 3,
                2 => i % 3 == 2,
                3 => matches!(i, 4 | 8 | 9 | 12 | 14),
                _ => matches!(i, 1 | 4 | 6 | 9 | 10 | 13 | 15),
            };
            bar + if back { 0 } else { i }
        }
        _ => match pattern {
            1 => step % 6,
            2 => bar + if i % 8 >= 4 { (i / 8) * 8 + 4 } else { i },
            3 => step % 3 + (step / 12) * 4,
            _ => {
                if step < 4 || step == STEPS - 1 {
                    step
                } else {
                    (step - 4) % 6 + 4
                }
            }
        },
    }
    .min(STEPS - 1)
}

fn buzz_hz(v: f32, bpm: f64, rate: f64) -> Option<f64> {
    if v <= 0.001 {
        return None;
    }
    let t = ((f64::from(v) - 0.1454) / 0.8546).max(0.0);
    let hz = (4.0 + 56.0 * t * t * t) * 0.99 * bpm / 60.0;
    Some(hz.clamp(100.0, 0.5 * rate))
}

fn buzzes(step: usize) -> bool {
    !step.is_multiple_of(4)
}

#[derive(Debug)]
pub struct Slicer {
    rate: f64,
    capture: DelayLine,
    active: bool,
    start: Option<f64>,
    slice_len: f64,
    recorded: usize,
    pattern: usize,
    style: usize,
    two_bars: bool,
    buzz: f32,
    go: bool,
    base: usize,
    last: Option<usize>,
    edge: f32,
    go_level: Smoothed,
}

impl Slicer {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            capture: DelayLine::new((MAX_S * rate) as usize),
            active: false,
            start: None,
            slice_len: 1.0,
            recorded: 0,
            pattern: 0,
            style: GROUP_STYLE,
            two_bars: true,
            buzz: 0.0,
            go: false,
            base: 0,
            last: None,
            edge: 1.0,
            go_level: Smoothed::new(1.0, FADE_S, rate),
        }
    }

    fn frame(&mut self, x: [f32; 2], beat: f64, bpm: f64) -> [f32; 2] {
        let Some(start) = self.start else {
            return x;
        };
        let pos = (beat - start) * 4.0;
        if pos < 0.0 {
            return x;
        }
        let whole = pos.floor() as usize;
        let into = pos - pos.floor();
        let now = (pos * self.slice_len) as usize;
        if whole < STEPS {
            while self.recorded <= now && self.recorded < self.capture.capacity() {
                self.capture.push(x);
                self.recorded += 1;
            }
        }
        let cycle = if self.two_bars { STEPS } else { 16 };
        let step = whole % cycle;
        let slice = slice_of(self.style, self.pattern, step);
        let mut offset = into * self.slice_len;
        if !self.go
            && buzzes(step)
            && let Some(hz) = buzz_hz(self.buzz, bpm, self.rate)
        {
            offset %= self.rate / hz;
        }
        let read = (slice as f64 * self.slice_len + offset) as usize;
        let heard = (whole >= STEPS || slice < whole) && read < self.recorded;
        let (out, from) = if heard {
            (self.capture.at(self.base + read), read)
        } else {
            (x, now)
        };
        if self.last.is_none_or(|last| from.abs_diff(last + 1) > 1) {
            self.edge = 0.0;
        }
        self.last = Some(from);
        self.edge = (self.edge + 1.0 / (FADE_S * self.rate) as f32).min(1.0);
        let gated = self.go && !step.is_multiple_of(4) && into >= GO_AFTER_BEATS * 4.0;
        self.go_level.set(if gated { GO_LEVEL } else { 1.0 });
        let level = self.edge * self.go_level.tick();
        [out[0] * level, out[1] * level]
    }
}

impl Effect for Slicer {
    fn kind(&self) -> EffectKind {
        EffectKind::Slicer
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let step = beats_per_sample(ctx, self.rate);
        if self.active && self.start.is_none() {
            self.start = Some((ctx.beat * 4.0).ceil() / 4.0);
            self.slice_len = 15.0 / ctx.bpm.max(20.0) * self.rate;
            self.recorded = 0;
            self.base = self.capture.write_index();
            self.last = None;
        }
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            *frame = self.frame(*frame, ctx.beat + i as f64 * step, ctx.bpm);
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.pattern = (amount.clamp(0.0, 1.0) * 4.0).round() as usize;
        self.style = GROUP_STYLE;
        self.two_bars = true;
        self.buzz = 0.0;
        self.go = false;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let five = (value.clamp(0.0, 1.0) * 4.0).round() as usize;
        match index {
            0 => self.pattern = five,
            1 => self.style = five,
            2 => self.buzz = value,
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.two_bars = on,
            1 => self.go = on,
            _ => {}
        }
    }

    fn set_active(&mut self, on: bool) {
        if on != self.active {
            self.active = on;
            self.start = None;
        }
    }

    fn reset(&mut self) {
        self.capture.clear();
        self.start = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn every_pattern_starts_in_order_and_stays_in_the_capture() {
        for style in 0..5 {
            for pattern in 0..5 {
                for step in 0..STEPS {
                    let slice = slice_of(style, pattern, step);
                    assert!(slice < STEPS);
                    assert!(
                        slice <= step,
                        "only what has been heard: {style} {pattern} {step}"
                    );
                }
            }
            assert!((0..STEPS).all(|s| slice_of(style, 0, s) == s));
        }
        assert_eq!(buzz_hz(0.0, 120.0, 48_000.0), None);
        assert_eq!(buzz_hz(0.5, 120.0, 48_000.0), Some(100.0), "floor");
        let top = buzz_hz(1.0, 120.0, 48_000.0).unwrap();
        assert!((top - 118.8).abs() < 1e-6);
    }

    #[test]
    fn repeats_the_first_slices_once_heard() {
        let mut s = Slicer::new(f64::from(RATE));
        s.set_param(1, 0.0);
        s.set_param(0, 1.0);
        s.set_active(true);
        let ramp: Vec<f32> = (0..RATE as usize)
            .flat_map(|i| [i as f32 / 48_000.0; 2])
            .collect();
        let out = run(&mut s, &ramp, ctx());
        let at = 2 * 6_000 + 3_000;
        assert!(
            (out[2 * at] - ramp[2 * 3_000]).abs() < 0.01,
            "{}",
            out[2 * at]
        );
    }
}
