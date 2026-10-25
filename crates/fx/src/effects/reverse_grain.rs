use backspn_dsp::{DelayLine, Smoothed};

use super::EffectKind;
use crate::{Context, Effect};

const CAPTURE_FRAMES: usize = 262_144;
const REF_RATE: f64 = 44_100.0;
const GRAIN_GLIDE: f64 = 5_000.0;
const ON_FADE_S: f64 = 500.0 / REF_RATE;
const GROUP_GRAIN: f32 = 0.5;

fn speed_of(v: f32) -> f64 {
    0.01 + 0.99 * f64::from(v.clamp(0.0, 1.0))
}

fn grain_of(v: f32) -> f64 {
    100.0 + (20_000.0 * f64::from(1.0 - v.clamp(0.0, 1.0)).powi(2)).round()
}

fn pitch_of(v: f32) -> f64 {
    0.1 + 0.9 * f64::from(v.clamp(0.0, 1.0))
}

fn mix_of(dry_wet: f32) -> (f32, f32) {
    let g = |y: f32| y * (2.0 - y.abs());
    let x = dry_wet.clamp(0.0, 1.0);
    (g((1.75 * (1.0 - x)).min(1.0)), g((1.75 * x).min(1.0)))
}

fn loop_frames(frames: usize, sixteenth: usize) -> usize {
    if frames < sixteenth.max(1) {
        return frames.max(1);
    }
    let mut n = sixteenth.max(1);
    while n * 2 <= frames {
        n *= 2;
    }
    n
}

#[derive(Debug, Clone, Copy, Default)]
struct Voice {
    back: f64,
    age: f64,
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct ReverseGrain {
    rate: f64,
    capture: DelayLine,
    recorded: usize,
    active: bool,
    frozen: bool,
    loop_len: f64,
    position: f64,
    voices: [Voice; 2],
    newest: usize,
    grain: f64,
    grain_target: f64,
    speed: f64,
    pitch: f64,
    forward: bool,
    invert: bool,
    on: Smoothed,
    dry: f32,
    wet: f32,
}

impl ReverseGrain {
    pub fn new(rate: f64) -> Self {
        let grain = grain_of(1.0) * rate / REF_RATE;
        let (dry, wet) = mix_of(1.0);
        Self {
            rate,
            capture: DelayLine::new(CAPTURE_FRAMES),
            recorded: 0,
            active: false,
            frozen: false,
            loop_len: 1.0,
            position: 0.0,
            voices: [Voice::default(); 2],
            newest: 0,
            grain,
            grain_target: grain,
            speed: speed_of(0.498),
            pitch: pitch_of(0.502),
            forward: true,
            invert: false,
            on: Smoothed::new(0.0, ON_FADE_S / 3.0, rate),
            dry,
            wet,
        }
    }

    fn set_grain(&mut self, v: f32) {
        self.grain_target = grain_of(v) * self.rate / REF_RATE;
    }

    fn freeze(&mut self, bpm: f64) {
        let sixteenth = (15.0 * self.rate / bpm.max(20.0)).round() as usize;
        let frames = self.recorded.min(self.capture.capacity());
        self.loop_len = loop_frames(frames, sixteenth) as f64;
        self.position = 0.0;
        self.voices = [Voice::default(); 2];
    }

    fn grains(&mut self) -> [f32; 2] {
        let len = self.grain.min(self.loop_len / 2.0).max(1.0);
        let read_forward = self.invert != self.forward;
        let step = if read_forward {
            -self.pitch
        } else {
            self.pitch
        };
        let mut out = [0.0; 2];
        for (i, v) in self.voices.iter_mut().enumerate() {
            v.age += 1.0;
            let gain = if i == self.newest {
                (v.age / len).min(1.0)
            } else {
                (1.0 - v.age / len).max(0.0)
            } as f32;
            v.back += step;
            let back = v.back.rem_euclid(self.loop_len);
            let frame = self.capture.read((back + 1.0) as f32);
            out[0] += frame[0] * gain;
            out[1] += frame[1] * gain;
        }
        if self.voices[self.newest].age >= len {
            self.newest = 1 - self.newest;
            self.voices[self.newest] = Voice {
                back: self.position,
                age: 0.0,
            };
        }
        let dir = if self.forward { -1.0 } else { 1.0 };
        self.position = (self.position + dir * self.speed).rem_euclid(self.loop_len);
        out
    }
}

impl Effect for ReverseGrain {
    fn kind(&self) -> EffectKind {
        EffectKind::ReverseGrain
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        if self.active && !self.frozen {
            self.freeze(ctx.bpm);
            self.frozen = true;
        }
        self.on.set(if self.active { 1.0 } else { 0.0 });
        let glide = (self.grain_target - self.grain) / GRAIN_GLIDE;
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let on = self.on.tick();
            if !self.active && on == 0.0 {
                self.capture.push(*frame);
                self.recorded += 1;
                continue;
            }
            if (self.grain_target - self.grain).abs() > glide.abs() {
                self.grain += glide;
            } else {
                self.grain = self.grain_target;
            }
            let grains = self.grains();
            let (keep, add) = (
                (std::f32::consts::FRAC_PI_2 * on).cos(),
                (std::f32::consts::FRAC_PI_2 * on).sin(),
            );
            for ch in 0..2 {
                let wet = frame[ch] * keep + grains[ch] * add;
                frame[ch] = frame[ch] * self.dry + wet * self.wet;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.speed = speed_of(amount);
        self.set_grain(GROUP_GRAIN);
        self.pitch = 1.0;
        self.forward = false;
        self.invert = false;
        (self.dry, self.wet) = mix_of(1.0);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.speed = speed_of(value),
            1 => self.set_grain(value),
            2 => self.pitch = pitch_of(value),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.forward = !on,
            1 => self.invert = on,
            _ => {}
        }
    }

    fn set_active(&mut self, on: bool) {
        if on != self.active {
            self.active = on;
            if !on {
                self.recorded = 0;
                self.frozen = false;
            }
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        (self.dry, self.wet) = mix_of(dry_wet);
        true
    }

    fn listens(&self) -> bool {
        true
    }

    fn reset(&mut self) {
        self.capture.clear();
        self.recorded = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn knobs_follow_their_curves() {
        assert!((speed_of(1.0) - 1.0).abs() < 1e-9 && (speed_of(0.0) - 0.01).abs() < 1e-9);
        assert!((grain_of(1.0) - 100.0).abs() < 1e-9 && (grain_of(0.0) - 20_100.0).abs() < 1e-9);
        assert!((pitch_of(0.0) - 0.1).abs() < 1e-9);
        let (dry, wet) = mix_of(0.5);
        assert!((dry - 0.98).abs() < 0.01 && (wet - 0.98).abs() < 0.01);
        assert_eq!(mix_of(1.0), (0.0, 1.0));
        assert_eq!(loop_frames(100_000, 6_000), 96_000);
        assert_eq!(loop_frames(5_000, 6_000), 5_000);
    }

    #[test]
    fn group_plays_the_past_backwards() {
        let mut g = ReverseGrain::new(f64::from(RATE));
        g.set_amount(1.0);
        let ramp: Vec<f32> = (0..RATE as usize)
            .flat_map(|i| [i as f32 / 48_000.0; 2])
            .collect();
        let out = run(&mut g, &ramp, ctx());
        assert_eq!(out, ramp, "off: the track");
        g.set_active(true);
        let out = run(&mut g, &ramp[..2 * 24_000], ctx());
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert!(
            left[20_000] < left[4_000],
            "{} then {}",
            left[4_000],
            left[20_000]
        );
    }
}
