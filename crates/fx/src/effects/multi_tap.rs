use backspn_dsp::{DelayLine, Isolator, Smoothed};

use super::echo::{ECHO_BEATS, GROUP_BEATS, HIGH_BAND_HZ, LOW_BAND_HZ};
use super::{EffectKind, pick};
use crate::{Context, Effect};

const MIN_MS: f64 = 10.0;
const MAX_MS: f64 = 4000.0;
const SMOOTH_S: f64 = 0.003_3;
const TAPS: usize = 8;

fn taps_on(n: usize) -> [bool; TAPS] {
    let on: &[usize] = match n {
        1 => &[6],
        2 => &[4, 6],
        3 => &[2, 4, 6],
        4 => &[2, 4, 6, 7],
        5 => &[2, 4, 5, 6, 7],
        6 => &[2, 3, 4, 5, 6, 7],
        _ => &[1, 2, 3, 4, 5, 6, 7],
    };
    std::array::from_fn(|i| on.contains(&(i + 1)))
}

fn taps_of(v: f32) -> usize {
    (1.0 + 6.0 * v.clamp(0.0, 1.0)) as usize
}

fn swung(t: f64, v: f32) -> [f64; 2] {
    let v = f64::from(v.clamp(0.0, 1.0));
    let long = |s: f64| (t + (2.0 * t * s).trunc()).clamp(MIN_MS, MAX_MS);
    if v < 0.5 {
        [t, long(1.0 - 2.0 * v)]
    } else {
        [long(2.0 * v - 1.0), t]
    }
}

fn mix_of(level: f32) -> (f32, f32) {
    let l = level.clamp(0.0, 1.0);
    if l < 0.5 {
        (1.0, 1.4 * l)
    } else {
        (1.3 - 0.6 * l, 0.7)
    }
}

#[derive(Debug)]
pub struct MultiTap {
    rate: f64,
    line: DelayLine,
    bands: Isolator,
    beats: f64,
    swing: f32,
    taps: usize,
    group: bool,
    level: f32,
    lo_cut: bool,
    hi_cut: bool,
    times: [f64; 2],
    gains: [Smoothed; TAPS],
    gate: Smoothed,
    dry: Smoothed,
    wet: Smoothed,
}

impl MultiTap {
    pub fn new(rate: f64) -> Self {
        let mut m = Self {
            rate,
            line: DelayLine::new((MAX_MS / 1000.0 * rate) as usize + 4),
            bands: Isolator::new(LOW_BAND_HZ, HIGH_BAND_HZ, rate),
            beats: GROUP_BEATS,
            swing: 0.5,
            taps: 4,
            group: true,
            level: 0.0,
            lo_cut: false,
            hi_cut: false,
            times: [0.0; 2],
            gains: std::array::from_fn(|_| Smoothed::new(0.0, SMOOTH_S, rate)),
            gate: Smoothed::new(1.0, SMOOTH_S, rate),
            dry: Smoothed::new(1.0, SMOOTH_S, rate),
            wet: Smoothed::new(0.0, SMOOTH_S, rate),
        };
        m.set_level(0.0);
        m
    }

    fn set_level(&mut self, level: f32) {
        self.level = level.clamp(0.0, 1.0);
        let (dry, wet) = mix_of(self.level);
        self.dry.set(dry);
        self.wet.set(wet);
        self.set_gains();
    }

    fn set_gains(&mut self) {
        let on = taps_on(self.taps);
        let odd = (2.0 * self.level - 1.0).max(0.0);
        for (i, g) in self.gains.iter_mut().enumerate() {
            let k = i + 1;
            g.set(match (self.group, k) {
                (_, 8) => 0.0,
                (true, 2 | 4 | 6) => 1.0,
                (true, _) => odd,
                (false, _) => f32::from(u8::from(on[i])),
            });
        }
    }

    fn split(&mut self, x: [f32; 2]) -> ([f32; 2], f32) {
        if !self.lo_cut && !self.hi_cut {
            return (x, 0.0);
        }
        let mut into = x;
        let mut thru = [0.0; 2];
        for (ch, v) in into.iter_mut().enumerate() {
            let [low, mid, high] = self.bands.split(ch, *v);
            let (l, h) = (
                if self.lo_cut { low } else { 0.0 },
                if self.hi_cut { high } else { 0.0 },
            );
            thru[ch] = l + h;
            *v -= l + h;
            let _ = mid;
        }
        (into, thru[0])
    }
}

impl Effect for MultiTap {
    fn kind(&self) -> EffectKind {
        EffectKind::MultiTapDelay
    }

    #[allow(clippy::float_cmp)]
    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let t = (self.beats * 60_000.0 / ctx.bpm.max(20.0))
            .clamp(MIN_MS, MAX_MS)
            .trunc();
        let wanted = swung(t, self.swing);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames.iter_mut() {
            if wanted != self.times {
                self.gate.set(0.0);
                if self.gate.value() <= 1e-2 || self.times == [0.0; 2] {
                    self.times = wanted;
                    self.gate.set(1.0);
                }
            }
            let gate = self.gate.tick();
            let (dry, wet) = (self.dry.tick(), self.wet.tick());
            let mut echo = [0.0f32; 2];
            for (i, g) in self.gains.iter_mut().enumerate() {
                let g = g.tick();
                if g == 0.0 {
                    continue;
                }
                for ch in 0..2 {
                    let ms = (self.times[ch] * (i + 1) as f64 / 8.0).trunc();
                    let at = (ms / 1000.0 * self.rate).max(1.0) as f32;
                    echo[ch] += g * self.line.read(at)[ch];
                }
            }
            let (into, thru) = self.split(*frame);
            self.line.push(into);
            for ch in 0..2 {
                frame[ch] = frame[ch] * dry + (echo[ch] * gate + thru) * wet;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.group = true;
        self.beats = GROUP_BEATS;
        self.swing = 0.5;
        self.lo_cut = false;
        self.hi_cut = false;
        self.set_level(amount);
    }

    fn set_pad(&mut self, pad: &crate::PadParams) {
        self.set_amount(pad.level);
        self.beats = pad.beats;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        self.group = false;
        match index {
            0 => self.beats = pick(&ECHO_BEATS, value),
            1 => self.swing = value.clamp(0.0, 1.0),
            2 => self.taps = taps_of(value),
            _ => {}
        }
        self.set_gains();
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.lo_cut = on,
            1 => self.hi_cut = on,
            _ => {}
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.group = false;
        self.set_level(dry_wet);
        true
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
    fn taps_swing_and_mix_are_right() {
        assert_eq!(taps_of(0.5), 4);
        assert_eq!(taps_of(1.0), 7);
        let four = taps_on(4);
        assert_eq!(four, [false, true, false, true, false, true, true, false]);
        assert_eq!(swung(500.0, 0.5), [500.0, 500.0]);
        assert_eq!(swung(500.0, 1.0), [1500.0, 500.0]);
        assert_eq!(swung(500.0, 0.0), [500.0, 1500.0]);
        assert_eq!(mix_of(0.0), (1.0, 0.0));
        let (d, w) = mix_of(0.5);
        assert!((d - 1.0).abs() < 1e-6 && (w - 0.7).abs() < 1e-6);
        let (d, w) = mix_of(1.0);
        assert!((d - 0.7).abs() < 1e-6 && (w - 0.7).abs() < 1e-6);
    }

    #[test]
    fn the_taps_sound_at_eighths_and_nothing_repeats() {
        let mut m = MultiTap::new(f64::from(RATE));
        m.set_param(0, 5.0 / 9.0);
        m.set_param(2, 0.0);
        m.set_dry_wet(1.0);
        let out = run(&mut m, &impulse(RATE as usize), ctx());
        let l: Vec<f32> = out.iter().step_by(2).copied().collect();
        let at = 18_000;
        let peak = l[at - 3..at + 3].iter().fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(peak > 0.6, "{peak}");
        let late = l[at + 100..].iter().fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(late < 1e-3, "no feedback: {late}");
    }
}
