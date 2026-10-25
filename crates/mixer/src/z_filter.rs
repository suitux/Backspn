use std::f32::consts::PI;

use crate::block_ramp::BlockRamp;

const RESONANCE: f32 = 0.252;
const MAKEUP: f32 = 1.252;
const RAMP_BLOCK: u32 = 128;
const RAMP_S: f64 = 0.14;
const FADE: f32 = 0.04;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    LowPass,
    HighPass,
}

fn note(k: f32) -> f32 {
    let x = 2.0 * k - 1.0;
    if k > 0.5 {
        30.0 + 82.0 * x.max(0.0)
    } else {
        43.0 + 88.0 * (x.min(0.0) + 1.0)
    }
}

fn note_hz(n: f32) -> f32 {
    2f32.powf((5.0 * n + 1_381.882_4) / 60.0 - 20.0)
}

fn wet(k: f32) -> f32 {
    if k > 0.5 {
        ((k - 0.5) / FADE).clamp(0.0, 1.0)
    } else {
        1.0 - ((k - (0.5 - FADE)) / FADE).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Ladder {
    s: [f32; 4],
}

impl Ladder {
    #[inline]
    fn tick(&mut self, x: f32, big_g: f32, side: Side) -> f32 {
        let (input_gain, state_gain) = match side {
            Side::LowPass => (big_g, 1.0 - big_g),
            Side::HighPass => (1.0 - big_g, -(1.0 - big_g)),
        };
        let mut gain = 1.0;
        let mut offset = 0.0;
        for state in self.s {
            offset = input_gain * offset + state_gain * state;
            gain *= input_gain;
        }
        let y4 = (gain * x + offset) / (1.0 + RESONANCE * gain);
        let mut u = x - RESONANCE * y4;
        for s in &mut self.s {
            let v = big_g * (u - *s);
            let lp = v + *s;
            let out = match side {
                Side::LowPass => lp,
                Side::HighPass => u - lp,
            };
            *s = lp + v;
            u = out;
        }
        MAKEUP * y4
    }
}

#[derive(Debug, Clone)]
pub struct ZFilter {
    pi_over_rate: f32,
    knob: BlockRamp<1>,
    side: Side,
    ladders: [Ladder; 2],
}

impl ZFilter {
    pub fn new(rate: f64) -> Self {
        Self {
            pi_over_rate: PI / rate as f32,
            knob: BlockRamp::new(rate, RAMP_BLOCK, RAMP_S, [0.5]),
            side: Side::LowPass,
            ladders: [Ladder::default(); 2],
        }
    }

    pub fn set(&mut self, knob: f32) {
        self.knob.set([f32::midpoint(knob.clamp(-1.0, 1.0), 1.0)]);
    }

    #[inline]
    pub fn tick(&mut self, frame: &mut [f32; 2]) {
        let [k] = self.knob.tick();
        let side = if k > 0.5 {
            Side::HighPass
        } else {
            Side::LowPass
        };
        if side != self.side {
            self.side = side;
            self.ladders = [Ladder::default(); 2];
        }
        let wet = wet(k);
        if wet <= 0.0 {
            return;
        }
        let w = note_hz(note(k)).max(0.0) * self.pi_over_rate;
        let (w2, w3) = (w * w, w * w * w);
        let g = w + w3 / 3.0 + 2.0 * w2 * w3 / 15.0;
        let big_g = g / (1.0 + g);
        for (x, ladder) in frame.iter_mut().zip(&mut self.ladders) {
            let y = ladder.tick(*x, big_g, side);
            *x = y * wet + *x * (1.0 - wet);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knob_laws() {
        assert!((note_hz(69.0) - 440.0).abs() < 0.5);
        assert_eq!(note(1.0), 112.0);
        assert_eq!(note(0.75), 71.0);
        assert_eq!(note(0.0), 43.0);
        assert_eq!(note(0.5), 131.0);
        for (k, w) in [
            (0.5, 0.0),
            (0.52, 0.5),
            (0.6, 1.0),
            (0.48, 0.5),
            (0.46, 1.0),
            (0.1, 1.0),
        ] {
            assert!((wet(k) - w).abs() < 1e-5, "wet({k})");
        }
    }

    fn level(knob: f32, hz: f32) -> f32 {
        let rate = 48_000.0;
        let mut f = ZFilter::new(f64::from(rate));
        f.set(knob);
        let mut sum = 0.0;
        for n in 0..48_000 {
            let x = (2.0 * PI * hz * n as f32 / rate).sin();
            let mut frame = [x, x];
            f.tick(&mut frame);
            if n >= 24_000 {
                sum += frame[0] * frame[0];
            }
        }
        (sum / 24_000.0).sqrt() * 2f32.sqrt()
    }

    #[test]
    fn centre_is_dry_and_the_sides_filter() {
        assert!((level(0.0, 1000.0) - 1.0).abs() < 0.01, "off");
        assert!(level(-1.0, 2000.0) < 0.01, "low-pass at 98 Hz cuts 2 kHz");
        assert!(level(1.0, 200.0) < 0.01, "high-pass at 5.3 kHz cuts 200 Hz");
        assert!(level(1.0, 15_000.0) > 0.9, "and passes 15 kHz");
    }
}
