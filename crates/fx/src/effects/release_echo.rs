use backspn_dsp::{DelayLine, Smoothed};

use super::{EffectKind, pick};
use crate::{Context, Effect};

const BEATS: [f64; 11] = [
    0.0625, 0.125, 0.25, 0.5, 0.75, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0,
];
const MIN_MS: f64 = 10.0;
const MAX_MS: f64 = 4000.0;
const FEEDBACK: f32 = 0.7;
const OUT_GAIN: f32 = 0.7;
const GATE_S: f64 = 0.003_333;

fn echo_ms(beats: f64, bpm: f64) -> f64 {
    let beat_ms = if bpm > 0.0 { 60_000.0 / bpm } else { 60_000.0 };
    (beat_ms * beats).clamp(MIN_MS, MAX_MS).trunc()
}

#[derive(Debug)]
pub struct ReleaseEcho {
    rate: f64,
    line: DelayLine,
    beats: f64,
    active: bool,
    counter: f64,
    time: f64,
    gate: Smoothed,
    on: Smoothed,
}

impl ReleaseEcho {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            line: DelayLine::new((MAX_MS / 1000.0 * rate) as usize + 4),
            beats: 1.0,
            active: false,
            counter: 0.0,
            time: 0.0,
            gate: Smoothed::new(1.0, GATE_S, rate),
            on: Smoothed::new(0.0, GATE_S, rate),
        }
    }
}

impl Effect for ReleaseEcho {
    fn kind(&self) -> EffectKind {
        EffectKind::ReleaseEcho
    }

    #[allow(clippy::float_cmp)]
    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        if self.active && self.time == 0.0 {
            self.time = echo_ms(self.beats, ctx.bpm) / 1000.0 * self.rate;
        }
        let max = MAX_MS / 1000.0 * self.rate;
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames.iter_mut() {
            let on = self.on.tick();
            if on <= 0.0 && !self.active {
                continue;
            }
            let repeats = self.counter >= self.time;
            self.gate.set(if repeats { 0.0 } else { 1.0 });
            let gate = self.gate.tick();
            let back = self.line.read(self.time.max(1.0) as f32);
            self.line.push([
                frame[0] * gate + back[0] * FEEDBACK,
                frame[1] * gate + back[1] * FEEDBACK,
            ]);
            self.counter = (self.counter + 1.0).min(max);
            let effect = if repeats {
                [back[0] * OUT_GAIN, back[1] * OUT_GAIN]
            } else {
                *frame
            };
            for ch in 0..2 {
                frame[ch] = frame[ch] * (1.0 - on) + effect[ch] * on;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.beats = pick(&BEATS, amount);
    }

    fn set_pad(&mut self, pad: &crate::PadParams) {
        self.beats = super::nearest_beat(&BEATS, pad.beats);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        if index == 0 {
            self.beats = pick(&BEATS, value);
        }
    }

    fn set_button(&mut self, _index: usize, _on: bool) {}

    fn set_active(&mut self, on: bool) {
        if on && !self.active {
            self.counter = 0.0;
            self.time = 0.0;
            self.line.clear();
            self.gate = Smoothed::new(1.0, GATE_S, self.rate);
        }
        self.active = on;
        self.on.set(if on { 1.0 } else { 0.0 });
    }

    fn has_tail(&self) -> bool {
        false
    }

    fn reset(&mut self) {
        self.line.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn times_follow_the_tempo_within_limits() {
        assert!((echo_ms(1.0, 120.0) - 500.0).abs() < 1e-9);
        assert!((echo_ms(32.0, 120.0) - 4000.0).abs() < 1e-9);
        assert!((echo_ms(0.0625, 400.0) - 10.0).abs() < 1e-9);
        assert!((echo_ms(1.0, 0.0) - 4000.0).abs() < 1e-9);
    }

    #[test]
    fn the_track_goes_on_for_one_echo_then_only_repeats() {
        let mut e = ReleaseEcho::new(f64::from(RATE));
        e.set_param(0, 0.5);
        e.set_active(true);
        let input = vec![0.5f32; 2 * 4 * 24_000];
        let out = run(&mut e, &input, ctx());
        let l: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert!((l[12_000] - 0.5).abs() < 1e-3, "track first: {}", l[12_000]);
        assert!((l[30_000] - 0.35).abs() < 0.02, "{}", l[30_000]);
        assert!((l[54_000] - 0.245).abs() < 0.02, "{}", l[54_000]);
    }
}
