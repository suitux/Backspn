use backspn_dsp::{DelayLine, Isolator, OnePole, Smoothed};

use super::echo::{ECHO_BEATS, GROUP_BEATS, HIGH_BAND_HZ, LOW_BAND_HZ, centred, mix_of};
use super::{EffectKind, beats_per_sample, pick};
use crate::{Context, Effect};

const COMBS: [[f64; 5]; 2] = [
    [0.021_5, 0.025_7, 0.029_9, 0.035_3, 0.041_5],
    [0.022_3, 0.026_6, 0.031_1, 0.036_4, 0.040_7],
];
const ALLPASSES: [f64; 4] = [0.007, 0.008_2, 0.022_7, 0.021_5];
const ALLPASS_GAIN: f32 = 0.7;
const DAMP_HZ: f64 = 14_000.0;
const OUTPUT_GAIN: f32 = 0.708;
const MAX_SCALE: f64 = 2.08;

fn hundred(v: f32) -> f64 {
    let c = f64::from(centred(v));
    if c < 0.0 {
        1.0 + 49.0 * (1.0 + c)
    } else {
        50.0 + 50.0 * c
    }
}

fn room_scale(r: f64) -> f64 {
    0.2 + 0.171 * (1.01 + 0.023_06 * r).powi(2)
}

fn decay_s(x: f64) -> f64 {
    2.872_8e-6 * x.powi(4) - 3.339_1e-4 * x.powi(3) + 0.016_808 * x * x - 0.225_53 * x + 1.740_86
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct BeatReverb {
    rate: f64,
    bands: Isolator,
    combs: [DelayLine; 5],
    damp: [OnePole; 5],
    allpasses: [DelayLine; 4],
    scale: Smoothed,
    decay: f64,
    lo_cut: bool,
    hi_cut: bool,
    dry: Smoothed,
    wet: Smoothed,
}

impl BeatReverb {
    pub fn new(rate: f64) -> Self {
        let comb = |k: usize| {
            DelayLine::new((COMBS[0][k].max(COMBS[1][k]) * MAX_SCALE * rate) as usize + 4)
        };
        let mut damp = OnePole::default();
        damp.set(DAMP_HZ, rate);
        let mut r = Self {
            rate,
            bands: Isolator::new(LOW_BAND_HZ, HIGH_BAND_HZ, rate),
            combs: std::array::from_fn(comb),
            damp: [damp; 5],
            allpasses: ALLPASSES.map(|d| DelayLine::new((d * rate) as usize + 4)),
            scale: Smoothed::new(1.0, 0.03, rate),
            decay: decay_s(50.0),
            lo_cut: false,
            hi_cut: false,
            dry: Smoothed::new(1.0, 0.01, rate),
            wet: Smoothed::new(0.0, 0.01, rate),
        };
        r.set_level(0.0);
        r
    }

    fn set_level(&mut self, level: f32) {
        let (dry, wet) = mix_of(level);
        self.dry.set(dry);
        self.wet.set(wet);
    }
}

impl Effect for BeatReverb {
    fn kind(&self) -> EffectKind {
        EffectKind::BeatReverb
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let scale = f64::from(self.scale.tick());
            let (dry, wet) = (self.dry.tick(), self.wet.tick());
            let mut input = [0.0f32; 2];
            for ch in 0..2 {
                let [low, mid, high] = self.bands.split(ch, frame[ch]);
                input[ch] = mid
                    + if self.lo_cut { 0.0 } else { low }
                    + if self.hi_cut { 0.0 } else { high };
            }
            let mut tail = [0.0f32; 2];
            for k in 0..5 {
                let mut push = [0.0; 2];
                for ch in 0..2 {
                    let d = COMBS[ch][k] * scale;
                    let g = 10f64.powf(-3.0 * d / self.decay) as f32;
                    let out = self.combs[k].read((d * self.rate) as f32)[ch];
                    let (damped, _) = self.damp[k].tick(ch, out);
                    push[ch] = input[ch] * 0.2 + damped * g;
                    tail[ch] += out;
                }
                self.combs[k].push(push);
            }
            for (line, d) in self.allpasses.iter_mut().zip(ALLPASSES) {
                let delayed = line.read((d * self.rate) as f32);
                let v = [
                    tail[0] + ALLPASS_GAIN * delayed[0],
                    tail[1] + ALLPASS_GAIN * delayed[1],
                ];
                line.push(v);
                tail = [
                    delayed[0] - ALLPASS_GAIN * v[0],
                    delayed[1] - ALLPASS_GAIN * v[1],
                ];
            }
            for ch in 0..2 {
                frame[ch] = frame[ch] * dry + tail[ch] * OUTPUT_GAIN * wet;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.scale.set(room_scale(50.0) as f32);
        self.decay = decay_s(50.0);
        self.lo_cut = false;
        self.hi_cut = false;
        self.set_level(amount);
    }

    fn set_pad(&mut self, pad: &crate::PadParams) {
        self.set_amount(pad.level);
        self.set_param(1, pad.room);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            1 => self.scale.set(room_scale(hundred(value)) as f32),
            2 => self.decay = decay_s(hundred(value)).max(0.1),
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
        for line in &mut self.combs {
            line.clear();
        }
        for line in &mut self.allpasses {
            line.clear();
        }
    }
}

const STRETCH_S: f64 = 16.0;
const SHIFT_WINDOW_S: f64 = 0.04;
const GATE_S: f64 = 0.003_3;

fn speed_of(d: f32) -> f64 {
    let d = f64::from(d.clamp(0.0, 1.0));
    if d < 0.5 { 1.0 - d } else { 0.75 - 0.5 * d }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Stretch {
    rate: f64,
    history: DelayLine,
    shifter: DelayLine,
    bands: Isolator,
    live_bands: Isolator,
    lag: f64,
    phase: f64,
    level: f32,
    beats: f64,
    eq: f32,
    repitch: bool,
    bpf: bool,
    group: bool,
    segment: Option<i64>,
    gate: f32,
    jumping: bool,
}

impl Stretch {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            history: DelayLine::new((STRETCH_S * rate) as usize),
            shifter: DelayLine::new((SHIFT_WINDOW_S * rate) as usize + 8),
            bands: Isolator::new(LOW_BAND_HZ, HIGH_BAND_HZ, rate),
            live_bands: Isolator::new(LOW_BAND_HZ, HIGH_BAND_HZ, rate),
            lag: 1.0,
            phase: 0.0,
            level: 0.0,
            beats: GROUP_BEATS,
            eq: 0.5,
            repitch: false,
            bpf: false,
            group: true,
            segment: None,
            gate: 1.0,
            jumping: false,
        }
    }

    fn shift(&mut self, x: [f32; 2], ratio: f64) -> [f32; 2] {
        self.shifter.push(x);
        if (ratio - 1.0).abs() < 1e-6 {
            return x;
        }
        let window = SHIFT_WINDOW_S * self.rate;
        self.phase = (self.phase + (1.0 - ratio) / window).rem_euclid(1.0);
        let mut out = [0.0; 2];
        for head in [self.phase, (self.phase + 0.5) % 1.0] {
            let gain = (std::f64::consts::PI * head).sin() as f32;
            let tap = self.shifter.read((1.0 + head * window) as f32);
            out[0] += tap[0] * gain;
            out[1] += tap[1] * gain;
        }
        out
    }
}

impl Effect for Stretch {
    fn kind(&self) -> EffectKind {
        EffectKind::Stretch
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let speed = speed_of(self.level);
        let on = self.level > 0.0;
        let per_beat = beats_per_sample(ctx, self.rate);
        let step = 1.0 / (GATE_S * self.rate) as f32;
        let c = if self.group {
            0.0
        } else {
            f64::from(centred(self.eq))
        };
        let (low_gain, high_gain) = if c < 0.0 {
            ((1.0 + c) as f32, 1.0)
        } else {
            (1.0, (1.0 - c) as f32)
        };
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            self.history.push(*frame);
            if !on {
                self.lag = 1.0;
                self.segment = None;
                continue;
            }
            let beat = ctx.beat + i as f64 * per_beat;
            let stretch = (beat / self.beats).floor() as i64;
            if self.segment.is_some_and(|s| s != stretch) {
                self.jumping = true;
            }
            self.segment = Some(stretch);
            if self.jumping {
                self.gate -= step;
                if self.gate <= 0.0 {
                    self.gate = 0.0;
                    self.lag = 1.0;
                    self.jumping = false;
                }
            } else {
                self.gate = (self.gate + step).min(1.0);
            }
            self.lag = (self.lag + 1.0 - speed).min(self.history.capacity() as f64 - 2.0);
            let slowed = self.history.read(self.lag as f32);
            let ratio = if self.repitch && !self.group {
                1.0
            } else {
                1.0 / speed
            };
            let played = self.shift(slowed, ratio.min(4.0));
            let bpf = self.bpf && !self.group;
            for ch in 0..2 {
                let [low, mid, high] = self.bands.split(ch, played[ch] * self.gate);
                let [live_low, _, live_high] = self.live_bands.split(ch, frame[ch]);
                let (low, high) = if bpf {
                    (live_low, live_high)
                } else {
                    (low, high)
                };
                frame[ch] = mid + low * low_gain + high * high_gain;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.group = true;
        self.beats = GROUP_BEATS;
        self.level = amount;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        self.group = false;
        match index {
            0 => self.beats = pick(&ECHO_BEATS, value),
            1 => self.eq = value,
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.repitch = on,
            1 => self.bpf = on,
            _ => {}
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.group = false;
        self.level = dry_wet;
        true
    }

    fn reset(&mut self) {
        self.history.clear();
        self.shifter.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn curves_follow_the_spec_values() {
        assert!((hundred(0.5) - 50.0).abs() < 1e-9 && (hundred(1.0) - 100.0).abs() < 1e-9);
        assert!((room_scale(50.0) - 1.0).abs() < 0.01 && (room_scale(100.0) - 2.08).abs() < 0.02);
        assert!((decay_s(50.0) - 8.7).abs() < 0.1 && (decay_s(1.0) - 1.53).abs() < 0.01);
        assert!((speed_of(0.5) - 0.5).abs() < 1e-9 && (speed_of(1.0) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn reverb_rings_and_stays_bounded() {
        let mut r = BeatReverb::new(f64::from(RATE));
        r.set_amount(0.75);
        let out = run(&mut r, &impulse(RATE as usize), ctx());
        assert!(rms(&out[2 * 24_000..]) > 1e-3, "a tail");
        assert!(out.iter().all(|v| v.is_finite() && v.abs() < 2.0));
    }

    #[test]
    fn stretch_plays_slower() {
        let mut s = Stretch::new(f64::from(RATE));
        s.set_dry_wet(0.5);
        s.set_button(0, true);
        let out = run(&mut s, &sine(800.0, RATE as usize / 4, 0.5), ctx());
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let crossings = left[2_000..10_000]
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        let hz = crossings as f64 / (8_000.0 / 48_000.0);
        assert!((hz - 400.0).abs() < 40.0, "{hz} Hz");
    }
}
