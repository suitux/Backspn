use backspn_dsp::{DelayLine, Isolator, Smoothed};

use super::echo::{ECHO_BEATS, GROUP_BEATS, HIGH_BAND_HZ, LOW_BAND_HZ, centred, mix_of};
use super::{EffectKind, beats_per_sample, pick};
use crate::{Context, Effect};

const MAX_MS: f64 = 4000.0;
const MIN_MS: f64 = 1.0;
const MIN_REVERSE_MS: f64 = 10.0;
const FADE_S: f64 = 0.003_333;
const PAN_GLIDE_S: f64 = 0.03;
const SIDES: [(f64, f64); 9] = [
    (1.0, 3.0),
    (1.0, 2.0),
    (1.0, 1.5),
    (1.0, 1.25),
    (1.0, 1.0),
    (1.25, 1.0),
    (1.5, 1.0),
    (2.0, 1.0),
    (3.0, 1.0),
];
const PAN_BEATS: [f64; 5] = [0.0625, 0.125, 0.25, 0.5, 1.0];

fn sides_of(v: f32) -> (f64, f64) {
    SIDES[((v.clamp(0.0, 1.0) * 9.0) as usize).min(8)]
}

fn pan_of(v: f32) -> Option<(f64, bool, f32)> {
    let zone = ((v.clamp(0.0, 1.0) * 11.0) as usize).min(10);
    let level = 1.0 + 0.4125 * centred(v).abs();
    match zone {
        0..=4 => Some((PAN_BEATS[zone], true, level)),
        5 => None,
        _ => Some((PAN_BEATS[10 - zone], false, level)),
    }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct BeatDelay {
    reverse: bool,
    rate: f64,
    line: DelayLine,
    bands: Isolator,
    beats: f64,
    sides: (f64, f64),
    pan: Option<(f64, bool, f32)>,
    lo_cut: bool,
    hi_cut: bool,
    dry: Smoothed,
    wet: Smoothed,
    panned: Smoothed,
    pan_gain: [Smoothed; 2],
    into: [f64; 2],
    stretch: Option<i64>,
}

impl BeatDelay {
    pub fn new(reverse: bool, rate: f64) -> Self {
        let glide = |v| Smoothed::new(v, PAN_GLIDE_S, rate);
        let mut delay = Self {
            reverse,
            rate,
            line: DelayLine::new((2.0 * MAX_MS / 1000.0 * rate) as usize + 4),
            bands: Isolator::new(LOW_BAND_HZ, HIGH_BAND_HZ, rate),
            beats: GROUP_BEATS,
            sides: (1.0, 1.0),
            pan: None,
            lo_cut: false,
            hi_cut: false,
            dry: Smoothed::new(1.0, 0.01, rate),
            wet: Smoothed::new(0.0, 0.01, rate),
            panned: Smoothed::new(0.0, FADE_S, rate),
            pan_gain: [glide(1.0), glide(1.0)],
            into: [0.0; 2],
            stretch: None,
        };
        delay.set_level(0.0);
        delay
    }

    fn set_level(&mut self, level: f32) {
        let (dry, wet) = mix_of(level);
        self.dry.set(dry);
        self.wet.set(wet);
    }

    fn cut(&mut self, x: [f32; 2]) -> [f32; 2] {
        if !self.lo_cut && !self.hi_cut {
            return x;
        }
        let mut out = x;
        for (ch, v) in out.iter_mut().enumerate() {
            let [low, mid, high] = self.bands.split(ch, *v);
            *v = mid + if self.lo_cut { 0.0 } else { low } + if self.hi_cut { 0.0 } else { high };
        }
        out
    }

    fn reversed(&mut self, ch: usize, len: f64, restart: bool) -> f32 {
        if restart || self.into[ch] >= len {
            self.into[ch] = 0.0;
        }
        let t = self.into[ch];
        self.into[ch] += 1.0;
        let fade = (FADE_S * self.rate).min(len / 2.0).max(1.0);
        let gain = (t / fade).min((len - t) / fade).clamp(0.0, 1.0) as f32;
        self.line.read((2.0 * t + 1.0) as f32)[ch] * gain
    }
}

impl Effect for BeatDelay {
    fn kind(&self) -> EffectKind {
        if self.reverse {
            EffectKind::ReverseDelay
        } else {
            EffectKind::BeatDelay
        }
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let min = if self.reverse { MIN_REVERSE_MS } else { MIN_MS };
        let beat_ms = self.beats * 60_000.0 / ctx.bpm.max(20.0);
        let times = [self.sides.0, self.sides.1]
            .map(|m| (beat_ms * m).clamp(min, MAX_MS) / 1000.0 * self.rate);
        let own = [self.sides.0 > 1.0, self.sides.1 > 1.0];
        let per_beat = beats_per_sample(ctx, self.rate);
        self.panned.set(if self.pan.is_some() { 1.0 } else { 0.0 });
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            let beat = ctx.beat + i as f64 * per_beat;
            let (dry, wet) = (self.dry.tick(), self.wet.tick());
            let mut echo = [0.0; 2];
            if self.reverse {
                let stretch = (beat / self.beats).floor() as i64;
                let new = self.stretch != Some(stretch);
                self.stretch = Some(stretch);
                for ch in 0..2 {
                    echo[ch] = self.reversed(ch, times[ch], new && !own[ch]);
                }
            } else {
                echo = [
                    self.line.read(times[0] as f32)[0],
                    self.line.read(times[1] as f32)[1],
                ];
            }
            let input = self.cut(*frame);
            self.line.push(input);
            if let Some((period, left_first, level)) = self.pan {
                let half = ((beat / period * 2.0).floor() as i64).rem_euclid(2) == 0;
                let left = half == left_first;
                self.pan_gain[0].set(if left { level } else { 0.0 });
                self.pan_gain[1].set(if left { 0.0 } else { level });
            }
            let panned = self.panned.tick();
            for ch in 0..2 {
                let g = self.pan_gain[ch].tick();
                let e = echo[ch] * (1.0 - panned + panned * g);
                frame[ch] = frame[ch] * dry + e * wet;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.beats = GROUP_BEATS;
        self.sides = (1.0, 1.0);
        self.pan = None;
        self.lo_cut = false;
        self.hi_cut = false;
        self.set_level(amount);
    }

    fn set_pad(&mut self, pad: &crate::PadParams) {
        self.set_amount(pad.level);
        self.beats = pad.beats;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.beats = pick(&ECHO_BEATS, value),
            1 => self.sides = sides_of(value),
            2 => self.pan = pan_of(value),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.lo_cut = on,
            1 => self.hi_cut = on,
            _ => {}
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.set_level(dry_wet);
        true
    }

    fn has_tail(&self) -> bool {
        true
    }

    fn reset(&mut self) {
        self.line.clear();
        self.into = [0.0; 2];
        self.stretch = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    fn left(out: &[f32]) -> Vec<f32> {
        out.iter().step_by(2).copied().collect()
    }

    fn right(out: &[f32]) -> Vec<f32> {
        out.iter().skip(1).step_by(2).copied().collect()
    }

    fn near(x: &[f32], at: usize) -> f32 {
        x[at - 3..at + 3].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }

    #[test]
    fn knobs_follow_their_zones() {
        assert_eq!(sides_of(0.0), (1.0, 3.0));
        assert_eq!(sides_of(0.5), (1.0, 1.0));
        assert_eq!(sides_of(1.0), (3.0, 1.0));
        assert_eq!(pan_of(0.5), None);
        let (period, left, level) = pan_of(0.0).unwrap();
        assert!((period - 0.0625).abs() < 1e-9 && left && (level - 1.4125).abs() < 1e-6);
        let (period, left, _) = pan_of(6.5 / 11.0).unwrap();
        assert!((period - 1.0).abs() < 1e-9 && !left);
    }

    #[test]
    fn one_repeat_per_side_and_no_more() {
        let mut d = BeatDelay::new(false, f64::from(RATE));
        d.set_dry_wet(0.75);
        d.set_param(0, 3.0 / 9.0);
        d.set_param(1, 1.0);
        let out = run(&mut d, &impulse(RATE as usize * 2), ctx());
        let (l, r) = (left(&out), right(&out));
        assert!(near(&r, 12_000) > 0.9, "right on the beat");
        assert!(near(&l, 36_000) > 0.9, "left three times later");
        assert!(near(&r, 24_000) < 1e-3, "no second repeat");
    }

    #[test]
    fn reverse_delay_plays_the_last_stretch_backwards() {
        let mut d = BeatDelay::new(true, f64::from(RATE));
        d.set_dry_wet(1.0);
        d.set_param(0, 3.0 / 9.0);
        let mut input: Vec<f32> = (0..12_000).flat_map(|i| [i as f32 / 12_000.0; 2]).collect();
        input.resize(2 * 48_000, 0.0);
        let out = left(&run(&mut d, &input, ctx()));
        let (a, b) = (out[12_000 + 2_000], out[12_000 + 8_000]);
        assert!(a > b, "falling: {a} then {b}");
    }
}
