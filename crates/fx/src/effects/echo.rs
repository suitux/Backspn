use std::f64::consts::PI;

use backspn_dsp::{DelayLine, Isolator, OnePole, Smoothed};

use super::{EffectKind, pick, stretch};
use crate::{Context, Effect};

const MAX_MS: f64 = 4000.0;
const MIN_MS: f64 = 1.0;
const MIN_SIDE_MS: f64 = 10.0;
pub(crate) const ECHO_BEATS: [f64; 10] = [0.0625, 0.125, 0.25, 0.5, 0.75, 1.0, 2.0, 4.0, 8.0, 16.0];
pub(crate) const GROUP_BEATS: f64 = 0.5;
const CENTRE: (f32, f32) = (0.4863, 0.5137);
const FULL_ECHO: f32 = 0.75;
const REVERSE_FADE_MS: f64 = 5.0;
pub(crate) const LOW_BAND_HZ: f64 = 284.0;
pub(crate) const HIGH_BAND_HZ: f64 = 4_649.0;
const SHIFT_WINDOW_S: f64 = 0.04;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Echo,
    PingPong,
    PitchEcho,
    CoilEcho,
    LowCutEcho,
    LowCutPitchEcho,
}

impl Variant {
    fn feedback_curve(self) -> Option<(f32, f32)> {
        match self {
            Self::Echo | Self::PingPong | Self::LowCutEcho => Some((0.7, 0.3)),
            Self::PitchEcho | Self::LowCutPitchEcho => Some((0.85, 0.15)),
            Self::CoilEcho => None,
        }
    }

    fn kind(self) -> EffectKind {
        match self {
            Self::Echo => EffectKind::Echo,
            Self::PingPong => EffectKind::PingPong,
            Self::PitchEcho => EffectKind::PitchEcho,
            Self::CoilEcho => EffectKind::CoilEcho,
            Self::LowCutEcho => EffectKind::LowCutEcho,
            Self::LowCutPitchEcho => EffectKind::LowCutPitchEcho,
        }
    }
}

pub(crate) fn centred(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v < CENTRE.0 {
        (v - CENTRE.0) / CENTRE.0
    } else if v > CENTRE.1 {
        (v - CENTRE.1) / (1.0 - CENTRE.1)
    } else {
        0.0
    }
}

fn feedback_of(variant: Variant, v: f32) -> f32 {
    let Some((centre, right)) = variant.feedback_curve() else {
        return v.clamp(0.0, 1.0);
    };
    let c = centred(v);
    let fb = if c < 0.0 {
        centre * (1.0 + c)
    } else {
        centre + right * c
    };
    fb.clamp(0.0, 1.0)
}

fn centre_feedback(variant: Variant) -> f32 {
    feedback_of(variant, 0.5)
}

pub(crate) fn mix_of(level: f32) -> (f32, f32) {
    let level = level.clamp(0.0, 1.0);
    let echo = (level / FULL_ECHO).min(1.0);
    let dry = if level <= FULL_ECHO {
        1.0
    } else {
        (1.0 - level) / (1.0 - FULL_ECHO)
    };
    (dry, echo)
}

fn side_ratio(v: f32) -> f64 {
    let v = v.clamp(0.0, 1.0);
    if (CENTRE.0..=CENTRE.1).contains(&v) {
        return 0.75;
    }
    match v {
        v if v < 0.25 => 0.25,
        v if v < 0.5 => 0.5,
        v if v < 0.6 => 1.0,
        v if v < 0.7 => 1.25,
        v if v < 0.8 => 1.5,
        v if v < 0.9 => 1.75,
        _ => 2.0,
    }
}

fn pitch_ratio(p: f32) -> f64 {
    let p = f64::from(p.clamp(-200.0, 200.0));
    1.0 + if p >= 100.0 {
        p * 0.02 - 1.0
    } else if p >= 0.0 {
        p * 0.01
    } else {
        p * 0.005
    }
}

#[derive(Debug)]
pub(crate) struct Shifter {
    line: DelayLine,
    window: f64,
    phase: f64,
}

impl Shifter {
    pub(crate) fn new(rate: f64) -> Self {
        let window = SHIFT_WINDOW_S * rate;
        Self {
            line: DelayLine::new(window as usize + 8),
            window,
            phase: 0.0,
        }
    }

    pub(crate) fn tick(&mut self, x: [f32; 2], ratio: f64) -> [f32; 2] {
        self.line.push(x);
        if (ratio - 1.0).abs() < 1e-6 {
            return x;
        }
        self.phase = (self.phase + (1.0 - ratio) / self.window).rem_euclid(1.0);
        let mut out = [0.0; 2];
        for head in [self.phase, (self.phase + 0.5) % 1.0] {
            let gain = (PI * head).sin() as f32;
            let tap = self.line.read((1.0 + head * self.window) as f32);
            out[0] += tap[0] * gain;
            out[1] += tap[1] * gain;
        }
        out
    }

    pub(crate) fn clear(&mut self) {
        self.line.clear();
    }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Echo {
    variant: Variant,
    rate: f64,
    line: DelayLine,
    tone: OnePole,
    bands: Isolator,
    shifter: Shifter,
    time: Smoothed,
    feedback: Smoothed,
    dry: Smoothed,
    wet: Smoothed,
    beats: f64,
    filter: f32,
    side_ratio: f64,
    swing: f32,
    pitch: f64,
    lo_cut: bool,
    hi_cut: bool,
    freeze: bool,
    reverse: bool,
    reverse_at: f64,
    timed: bool,
    decay: f32,
}

impl Echo {
    pub fn new(variant: Variant, rate: f64) -> Self {
        let mut echo = Self {
            variant,
            rate,
            line: DelayLine::new((2.0 * MAX_MS / 1000.0 * rate) as usize + 4),
            tone: OnePole::default(),
            bands: Isolator::new(LOW_BAND_HZ, HIGH_BAND_HZ, rate),
            shifter: Shifter::new(rate),
            time: Smoothed::new(rate as f32 / 2.0, 0.05, rate),
            feedback: Smoothed::new(centre_feedback(variant), 0.02, rate),
            dry: Smoothed::new(1.0, 0.01, rate),
            wet: Smoothed::new(0.0, 0.01, rate),
            beats: GROUP_BEATS,
            filter: 0.0,
            side_ratio: side_ratio(0.5),
            swing: 0.0,
            pitch: 1.0,
            lo_cut: false,
            hi_cut: false,
            freeze: false,
            reverse: false,
            reverse_at: 0.0,
            timed: false,
            decay: 0.0,
        };
        echo.set_level(0.0);
        echo
    }

    fn set_level(&mut self, level: f32) {
        let (dry, wet) = mix_of(level);
        self.dry.set(dry);
        self.wet.set(wet);
    }

    fn side_times(&self, t: f64) -> [f64; 2] {
        let (min, max) = (
            MIN_SIDE_MS / 1000.0 * self.rate,
            MAX_MS / 1000.0 * self.rate,
        );
        match self.variant {
            Variant::PingPong => [t, (t * self.side_ratio).clamp(min, max)],
            Variant::LowCutEcho => {
                let stretch = 1.0 - 0.25 * f64::from(self.swing);
                if self.swing < 0.0 {
                    [t * stretch, t]
                } else {
                    [t, t * stretch]
                }
            }
            _ => [t, t],
        }
    }

    fn reversed(&self, len: f64) -> [f32; 2] {
        let t = self.reverse_at;
        let fade = (REVERSE_FADE_MS / 1000.0 * self.rate)
            .min(len / 2.0)
            .max(1.0);
        let gain = (t / fade).min((len - t) / fade).clamp(0.0, 1.0) as f32;
        let e = self.line.read((2.0 * t + 1.0) as f32);
        [e[0] * gain, e[1] * gain]
    }

    fn read(&mut self, t: f64) -> [f32; 2] {
        if self.reverse {
            let e = self.reversed(t);
            self.reverse_at += 1.0;
            if self.reverse_at >= t {
                self.reverse_at = 0.0;
            }
            return e;
        }
        let [l, r] = self.side_times(t);
        [self.line.read(l as f32)[0], self.line.read(r as f32)[1]]
    }

    fn shape(&mut self, raw: [f32; 2]) -> [f32; 2] {
        let mut e = match self.variant {
            Variant::PitchEcho | Variant::CoilEcho | Variant::LowCutPitchEcho => {
                self.shifter.tick(raw, self.pitch)
            }
            _ => raw,
        };
        let (lo_cut, hi_cut) = match self.variant {
            Variant::LowCutEcho | Variant::LowCutPitchEcho => (true, false),
            Variant::PitchEcho | Variant::CoilEcho => (self.lo_cut, self.hi_cut),
            Variant::Echo | Variant::PingPong => (false, false),
        };
        for (ch, v) in e.iter_mut().enumerate() {
            if matches!(self.variant, Variant::Echo | Variant::PingPong) && self.filter.abs() > 1e-3
            {
                let (lp, hp) = self.tone.tick(ch, *v);
                *v = if self.filter < 0.0 { lp } else { hp };
            }
            if lo_cut || hi_cut {
                let [low, mid, high] = self.bands.split(ch, *v);
                *v = mid + if lo_cut { 0.0 } else { low } + if hi_cut { 0.0 } else { high };
            }
        }
        e
    }
}

impl Effect for Echo {
    fn kind(&self) -> EffectKind {
        self.variant.kind()
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let ms = (self.beats * 60_000.0 / ctx.bpm.max(20.0)).clamp(MIN_MS, MAX_MS);
        let frames = (ms / 1000.0 * self.rate) as f32;
        if !self.timed {
            self.time = Smoothed::new(frames, 0.05, self.rate);
            self.timed = true;
        }
        self.time.set(frames);
        let amount = f64::from(self.filter.abs());
        let cutoff = if self.filter < 0.0 {
            20_000.0 * (300.0f64 / 20_000.0).powf(amount)
        } else {
            20.0 * (3_000.0f64 / 20.0).powf(amount)
        };
        self.tone.set(cutoff, self.rate);
        let cross = self.variant == Variant::PingPong;
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let t = f64::from(self.time.tick());
            let fb = if self.freeze {
                1.0
            } else {
                stretch(self.feedback.tick(), self.decay, 0.0, 0.97)
            };
            let (dry, wet) = (self.dry.tick(), self.wet.tick());
            let raw = self.read(t);
            let echo = self.shape(raw);
            let mut back = [0.0; 2];
            for ch in 0..2 {
                let input = if self.freeze { 0.0 } else { frame[ch] };
                let returned = echo[if cross { 1 - ch } else { ch }] * fb;
                back[ch] = if self.variant == Variant::CoilEcho {
                    (input + returned).tanh()
                } else {
                    (input + returned).clamp(-4.0, 4.0)
                };
            }
            self.line.push(back);
            frame[0] = frame[0] * dry + echo[0] * wet;
            frame[1] = frame[1] * dry + echo[1] * wet;
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.beats = GROUP_BEATS;
        self.feedback.set(centre_feedback(self.variant));
        self.filter = 0.0;
        self.side_ratio = side_ratio(0.5);
        self.swing = 0.0;
        self.pitch = 1.0;
        self.set_level(amount);
    }

    fn set_pad(&mut self, pad: &crate::PadParams) {
        self.set_amount(pad.level);
        self.beats = pad.beats;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        self.set_level(FULL_ECHO);
        match (index, self.variant) {
            (0, _) => self.beats = pick(&ECHO_BEATS, value),
            (1, variant) => self.feedback.set(feedback_of(variant, value)),
            (2, Variant::Echo) => self.filter = centred(value),
            (2, Variant::PingPong) => self.side_ratio = side_ratio(value),
            (2, Variant::LowCutEcho) => self.swing = centred(value),
            (2, Variant::PitchEcho | Variant::LowCutPitchEcho) => {
                self.pitch = pitch_ratio(50.0 * centred(value));
            }
            (2, Variant::CoilEcho) => {
                self.pitch = pitch_ratio(200.0 * value.clamp(0.0, 1.0) - 100.0);
            }
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match (index, self.variant) {
            (
                0,
                Variant::Echo | Variant::PingPong | Variant::LowCutEcho | Variant::LowCutPitchEcho,
            ) => {
                self.reverse = on;
                self.reverse_at = 0.0;
            }
            (1, Variant::Echo | Variant::PingPong | Variant::LowCutPitchEcho) => self.freeze = on,
            (0, Variant::PitchEcho | Variant::CoilEcho) => self.lo_cut = on,
            (1, Variant::PitchEcho | Variant::CoilEcho) => self.hi_cut = on,
            _ => {}
        }
    }

    fn has_tail(&self) -> bool {
        true
    }

    fn set_decay(&mut self, decay: f32) {
        self.decay = decay;
    }

    fn reset(&mut self) {
        self.line.clear();
        self.shifter.clear();
        self.reverse_at = 0.0;
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

    fn echo(variant: Variant) -> Echo {
        Echo::new(variant, f64::from(RATE))
    }

    fn near(x: &[f32], at: usize) -> f32 {
        x[at - 3..at + 3].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }

    #[test]
    fn knobs_follow_their_curves() {
        assert!((feedback_of(Variant::Echo, 0.5) - 0.7).abs() < 1e-6);
        assert!(
            (feedback_of(Variant::Echo, 0.49) - 0.7).abs() < 1e-6,
            "dead zone"
        );
        assert!(feedback_of(Variant::Echo, 0.0).abs() < 1e-6);
        assert!((feedback_of(Variant::Echo, 1.0) - 1.0).abs() < 1e-6);
        assert!((feedback_of(Variant::PitchEcho, 0.5) - 0.85).abs() < 1e-6);
        assert!((feedback_of(Variant::CoilEcho, 0.3) - 0.3).abs() < 1e-6);
        assert_eq!(mix_of(0.0), (1.0, 0.0));
        assert_eq!(mix_of(0.75), (1.0, 1.0));
        let (dry, wet) = mix_of(1.0);
        assert!(dry.abs() < 1e-6 && (wet - 1.0).abs() < 1e-6);
        assert!((centred(0.0) + 1.0).abs() < 1e-6 && centred(0.5) == 0.0);
        assert!((side_ratio(0.5) - 0.75).abs() < 1e-9);
        assert!((side_ratio(0.1) - 0.25).abs() < 1e-9);
        assert!((side_ratio(0.65) - 1.25).abs() < 1e-9);
        assert!((side_ratio(1.0) - 2.0).abs() < 1e-9);
        assert!((pitch_ratio(100.0) - 2.0).abs() < 1e-9);
        assert!((pitch_ratio(-100.0) - 0.5).abs() < 1e-9);
        assert!((pitch_ratio(50.0) - 1.5).abs() < 1e-9);
        assert!((pitch_ratio(-50.0) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn repeats_land_on_the_beat_and_fade_by_the_feedback() {
        let mut e = echo(Variant::Echo);
        e.set_amount(FULL_ECHO);
        let out = left(&run(&mut e, &impulse(RATE as usize), ctx()));
        let (first, second) = (near(&out, 12_000), near(&out, 24_000));
        assert!(first > 0.9, "first repeat {first}");
        assert!(
            (second / first - 0.7).abs() < 0.05,
            "feedback {}",
            second / first
        );
    }

    #[test]
    fn no_level_is_transparent_and_full_level_leaves_only_the_echo() {
        let input = sine(440.0, RATE as usize / 2, 0.5);
        let mut e = echo(Variant::Echo);
        e.set_amount(0.0);
        let out = run(&mut e, &input, ctx());
        let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
        assert!(rms(&diff[2 * 4_800..]) < 1e-4);
        let mut e = echo(Variant::Echo);
        e.set_amount(1.0);
        let out = run(&mut e, &input, ctx());
        assert!(rms(&out[2 * 2_400..2 * 11_000]) < 1e-3);
    }

    #[test]
    fn freeze_holds_the_echo_and_takes_nothing_new() {
        let mut input = sine(440.0, RATE as usize, 0.5);
        input[2 * 12_000..].fill(0.0);
        let mut e = echo(Variant::Echo);
        e.set_param(0, 3.0 / 9.0);
        e.set_param(1, 0.0);
        e.set_button(1, true);
        let out = run(&mut e, &input, ctx());
        assert!(rms(&out[2 * 30_000..]) < 1e-4, "frozen from the start");
        let mut e = echo(Variant::Echo);
        e.set_param(1, 0.0);
        let _ = run(&mut e, &input[..2 * 12_000], ctx());
        e.set_button(1, true);
        let out = run(&mut e, &vec![0.0; 2 * RATE as usize], ctx());
        assert!(
            rms(&out[2 * 36_000..]) > 0.2,
            "the last half beat goes round"
        );
    }

    #[test]
    fn filter_left_darkens_and_right_thins_the_echo() {
        let echo_of = |filter: f32, freq: f64| {
            let mut e = echo(Variant::Echo);
            e.set_param(1, 0.0);
            e.set_param(2, filter);
            let mut input = sine(freq, RATE as usize / 2, 0.5);
            input[2 * 6_000..].fill(0.0);
            let out = run(&mut e, &input, ctx());
            rms(&out[2 * 13_000..2 * 17_000])
        };
        assert!(echo_of(0.0, 8_000.0) < 0.3 * echo_of(0.5, 8_000.0));
        assert!(echo_of(1.0, 100.0) < 0.3 * echo_of(0.5, 100.0));
    }

    #[test]
    fn reverse_plays_the_echo_backwards() {
        let mut e = echo(Variant::Echo);
        e.set_param(0, 3.0 / 9.0);
        e.set_param(1, 0.0);
        e.set_button(0, true);
        let mut input: Vec<f32> = (0..12_000).flat_map(|i| [i as f32 / 12_000.0; 2]).collect();
        input.resize(2 * 48_000, 0.0);
        let out = left(&run(&mut e, &input, ctx()));
        let (a, b) = (out[12_000 + 2_000], out[12_000 + 8_000]);
        assert!(a > b, "falling echo: {a} then {b}");
    }

    #[test]
    fn ping_pong_answers_on_the_right_at_its_ratio_and_crosses_over() {
        let mut e = echo(Variant::PingPong);
        e.set_param(0, 3.0 / 9.0);
        e.set_param(1, 0.5);
        let mut input = vec![0.0; 2 * RATE as usize];
        input[0] = 1.0;
        let out = run(&mut e, &input, ctx());
        let (l, r) = (left(&out), right(&out));
        assert!(near(&l, 12_000) > 0.9, "left repeat on the beat");
        assert!(near(&r, 12_000 + 9_000) > 0.5, "crossed to the right");
        assert!(near(&r, 9_000) < 1e-3, "nothing was fed to the right");
    }

    #[test]
    fn pitch_echo_raises_each_repeat() {
        let mut e = echo(Variant::PitchEcho);
        e.set_param(0, 3.0 / 9.0);
        e.set_param(1, 0.5);
        e.set_param(2, 1.0);
        let mut input = sine(400.0, 6_000, 0.5);
        input.resize(2 * RATE as usize, 0.0);
        let out = left(&run(&mut e, &input, ctx()));
        let repeat = &out[12_000 + 1_000..12_000 + 5_000];
        let crossings = repeat
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        let freq = crossings as f64 / (4_000.0 / f64::from(RATE));
        assert!((freq - 600.0).abs() < 40.0, "repeat at {freq} Hz");
    }

    #[test]
    fn coil_echo_feedback_stays_bounded_and_low_cut_echo_loses_its_lows() {
        let mut e = echo(Variant::CoilEcho);
        e.set_param(0, 3.0 / 9.0);
        e.set_param(1, 1.0);
        e.set_param(2, 1.0);
        let input = sine(200.0, RATE as usize * 4, 0.9);
        let out = run(&mut e, &input, ctx());
        assert!(out.iter().all(|v| v.abs() < 3.0), "held by the limit");

        let lows = |variant| {
            let mut e = echo(variant);
            e.set_param(1, 0.0);
            let mut input = sine(60.0, 6_000, 0.5);
            input.resize(2 * RATE as usize / 2, 0.0);
            let out = run(&mut e, &input, ctx());
            rms(&out[2 * 13_000..2 * 17_000])
        };
        assert!(lows(Variant::LowCutEcho) < 0.5 * lows(Variant::Echo));
        assert!(lows(Variant::LowCutPitchEcho) < 0.5 * lows(Variant::Echo));
    }

    #[test]
    fn low_cut_echo_swing_moves_one_side() {
        let mut e = echo(Variant::LowCutEcho);
        e.set_param(0, 3.0 / 9.0);
        e.set_param(1, 0.0);
        e.set_param(2, 1.0);
        let input = impulse(RATE as usize);
        let out = run(&mut e, &input, ctx());
        let (l, r) = (left(&out), right(&out));
        assert!(near(&l, 12_000) > 0.3, "left on the beat");
        assert!(near(&r, 9_000) > 0.3, "right a quarter sooner");
    }
}
