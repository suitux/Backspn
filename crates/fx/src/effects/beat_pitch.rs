use backspn_dsp::{Biquad, Isolator, Smoothed};

use super::EffectKind;
use super::echo::{HIGH_BAND_HZ, LOW_BAND_HZ};
use crate::{Context, Effect};

const MIN_DELAY_S: f64 = 0.013;
const MAX_DELAY_S: f64 = 0.023;
const JUMP_S: f64 = 0.003;
const START_S: f64 = 0.03;
const MUTE_SPEED: f64 = 0.05;
const MUTE_S: f64 = 0.02;
const MAX_SLEW: f64 = 0.05;
const SMOOTH_S: f64 = 0.003_3;
const GUIDE_MIX: [f32; 2] = [0.914, 0.086];
const GUIDE_HP_HZ: f64 = 30.0;
const GUIDE_LP_HZ: f64 = 300.0;
const BUFFER_S: f64 = 0.1;

fn speed_of(p: f64) -> f64 {
    let p = p.clamp(-200.0, 200.0);
    if p >= 100.0 {
        1.0 + (0.02 * p - 1.0)
    } else if p >= 0.0 {
        1.0 + 0.01 * p
    } else {
        1.0 + 0.005 * p
    }
}

const GROUP_PITCH: f64 = -28.0;

#[allow(clippy::many_single_char_names)]
fn cubic(y: [f32; 4], t: f32) -> f32 {
    let a = -0.5 * y[0] + 1.5 * y[1] - 1.5 * y[2] + 0.5 * y[3];
    let b = y[0] - 2.5 * y[1] + 2.0 * y[2] - 0.5 * y[3];
    let c = -0.5 * y[0] + 0.5 * y[2];
    ((a * t + b) * t + c) * t + y[1]
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct BeatPitch {
    rate: f64,
    buf: Vec<[f32; 2]>,
    write: usize,
    filled: usize,
    delay: f64,
    old_delay: f64,
    jump: f64,
    speed: f64,
    pitch_knob: f32,
    group: bool,
    level: f32,
    lo_cut: bool,
    hi_cut: bool,
    bands: Isolator,
    guide_hp: Biquad,
    guide_lp: Biquad,
    guide_last: f32,
    dry: Smoothed,
    wet: Smoothed,
    start: Smoothed,
    mute: Smoothed,
}

impl BeatPitch {
    pub fn new(rate: f64) -> Self {
        let mut p = Self {
            rate,
            buf: vec![[0.0; 2]; (BUFFER_S * rate) as usize + 8],
            write: 0,
            filled: 0,
            delay: MIN_DELAY_S * rate,
            old_delay: 0.0,
            jump: 1.0,
            speed: 1.0,
            pitch_knob: 0.5,
            group: true,
            level: 0.0,
            lo_cut: false,
            hi_cut: false,
            bands: Isolator::new(LOW_BAND_HZ, HIGH_BAND_HZ, rate),
            guide_hp: Biquad::highpass(GUIDE_HP_HZ, rate),
            guide_lp: Biquad::lowpass(GUIDE_LP_HZ, rate),
            guide_last: 0.0,
            dry: Smoothed::new(1.0, SMOOTH_S, rate),
            wet: Smoothed::new(0.0, SMOOTH_S, rate),
            start: Smoothed::new(0.0, JUMP_S, rate),
            mute: Smoothed::new(1.0, MUTE_S, rate),
        };
        p.update();
        p
    }

    fn amount(&self) -> f64 {
        let level = f64::from(self.level);
        if self.group {
            GROUP_PITCH * 2.0 * level
        } else {
            (400.0 * f64::from(self.pitch_knob) - 200.0) * level
        }
    }

    fn update(&mut self) {
        let centred = !self.group && (self.pitch_knob - 0.5).abs() < f32::EPSILON;
        let off = self.level.abs() < 1.19e-7 || centred;
        self.dry.set(if off { 1.0 } else { 0.0 });
        self.wet.set(if off { 0.0 } else { 1.0 });
    }

    fn at(&self, behind: f64) -> [f32; 2] {
        let n = self.buf.len();
        let pos = self.write as f64 - behind;
        let base = pos.floor();
        let t = (pos - base) as f32;
        let i = base as i64;
        let get = |k: i64| self.buf[(i + k).rem_euclid(n as i64) as usize];
        let p = [get(-1), get(0), get(1), get(2)];
        [
            cubic([p[0][0], p[1][0], p[2][0], p[3][0]], t),
            cubic([p[0][1], p[1][1], p[2][1], p[3][1]], t),
        ]
    }

    fn shifted(&mut self, wanted: f64, guide: f32) -> [f32; 2] {
        self.speed += (wanted - self.speed).clamp(-MAX_SLEW, MAX_SLEW);
        let (lo, hi) = (MIN_DELAY_S * self.rate, MAX_DELAY_S * self.rate);
        let margin = JUMP_S * self.speed * self.rate;
        self.delay += 1.0 - self.speed;
        if self.jump >= 1.0 {
            let falling = self.guide_last > 0.0 && guide <= 0.0;
            let near = self.delay < lo + margin || self.delay > hi - margin;
            let out = self.delay < lo || self.delay > hi;
            if (near && falling) || out {
                self.old_delay = self.delay;
                self.delay = if self.speed > 1.0 { hi } else { lo };
                self.jump = 0.0;
            }
        }
        self.guide_last = guide;
        let new = self.at(self.delay);
        if self.jump >= 1.0 {
            return new;
        }
        self.old_delay += 1.0 - self.speed;
        let old = self.at(self.old_delay.clamp(1.0, (self.buf.len() - 4) as f64));
        let t = self.jump as f32;
        let g = t * t * (3.0 - 2.0 * t);
        self.jump += 1.0 / (JUMP_S * self.rate);
        [
            old[0] * (1.0 - g) + new[0] * g,
            old[1] * (1.0 - g) + new[1] * g,
        ]
    }

    fn split(&mut self, x: [f32; 2]) -> ([f32; 2], f32) {
        if !self.lo_cut && !self.hi_cut {
            return (x, 0.0);
        }
        let mut into = x;
        let mut thru = [0.0; 2];
        for (ch, v) in into.iter_mut().enumerate() {
            let [low, _, high] = self.bands.split(ch, *v);
            let kept = if self.lo_cut { low } else { 0.0 } + if self.hi_cut { high } else { 0.0 };
            thru[ch] = kept;
            *v -= kept;
        }
        (into, thru[0])
    }
}

impl Effect for BeatPitch {
    fn kind(&self) -> EffectKind {
        EffectKind::BeatPitch
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        let wanted = speed_of(self.amount());
        let start_frames = (START_S * self.rate) as usize;
        self.mute.set(if wanted < MUTE_SPEED { 0.0 } else { 1.0 });
        let n = self.buf.len();
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames.iter_mut() {
            let (into, thru) = self.split(*frame);
            self.buf[self.write] = into;
            self.write = (self.write + 1) % n;
            self.filled = (self.filled + 1).min(n);
            let guide = self.guide_lp.tick(
                0,
                self.guide_hp
                    .tick(0, into[0] * GUIDE_MIX[0] + into[1] * GUIDE_MIX[1]),
            );
            let s = self.shifted(wanted, guide);
            if self.filled >= start_frames {
                self.start.set(1.0);
            }
            let start = self.start.tick();
            let mute = self.mute.tick();
            let (dry, wet) = (self.dry.tick(), self.wet.tick());
            for ch in 0..2 {
                let shifted = (s[ch] * start + into[ch] * (1.0 - start)) * mute + thru;
                frame[ch] = frame[ch] * dry + shifted * wet;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.group = true;
        self.lo_cut = false;
        self.hi_cut = false;
        self.level = amount.clamp(0.0, 1.0);
        self.update();
    }

    fn set_param(&mut self, index: usize, value: f32) {
        self.group = false;
        if index == 0 {
            self.pitch_knob = value.clamp(0.0, 1.0);
        }
        self.update();
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
        self.level = dry_wet.clamp(0.0, 1.0);
        self.update();
        true
    }

    fn reset(&mut self) {
        self.buf.fill([0.0; 2]);
        self.filled = 0;
        self.start = Smoothed::new(0.0, JUMP_S, self.rate);
        self.delay = MIN_DELAY_S * self.rate;
        self.jump = 1.0;
        self.speed = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn the_amount_maps_to_octaves() {
        assert!((speed_of(100.0) - 2.0).abs() < 1e-9);
        assert!((speed_of(-100.0) - 0.5).abs() < 1e-9);
        assert!((speed_of(200.0) - 4.0).abs() < 1e-9);
        assert!(speed_of(-200.0).abs() < 1e-9);
        assert!((speed_of(50.0) - 1.5).abs() < 1e-9);
    }

    fn crossings(x: &[f32]) -> usize {
        x.iter()
            .step_by(2)
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|w| *w[0] <= 0.0 && *w[1] > 0.0)
            .count()
    }

    #[test]
    fn an_octave_up_doubles_the_frequency_and_centre_is_dry() {
        let input = sine(220.0, RATE as usize, 0.5);
        let mut p = BeatPitch::new(f64::from(RATE));
        p.set_param(0, 0.75);
        p.set_dry_wet(1.0);
        let out = run(&mut p, &input, ctx());
        let tail = &out[out.len() / 2..];
        let f = crossings(tail) * 2;
        assert!((400..=480).contains(&f), "{f} Hz");
        let mut c = BeatPitch::new(f64::from(RATE));
        c.set_param(0, 0.5);
        c.set_dry_wet(1.0);
        let out = run(&mut c, &input, ctx());
        assert!((out[1000] - input[1000]).abs() < 1e-3, "centre: dry");
    }
}
