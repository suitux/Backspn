use backspn_dsp::{OnePole, Smoothed};

use super::tank::{Shape, Tank};
use super::{EffectKind, note_hz, stretch};
use crate::{Context, Effect};

const FREEZE_S: f64 = 0.125;
const GROUP_LP: f32 = 0.92;
const GROUP_HP: f32 = 0.2;
const DIFFUSERS: [[f64; 2]; 3] = [[0.13, 0.17], [0.21, 0.27], [0.33, 0.41]];

fn size_of(v: f32) -> ([f64; 2], f64) {
    let v = f64::from(v.clamp(0.0, 1.0));
    let c = v * (0.31 + 0.69 * v);
    let rest = 1.0 - c;
    let top = if rest > 0.8 {
        let t = 5.0 * (rest - 0.8);
        30.0 + 12.0 * t * t
    } else {
        30.0
    };
    let n1 = top - 16.0 * c;
    let n2 = n1 - 1.0 - 2.0 * v * (2.0 - v);
    ([1.0 / note_hz(n1), 1.0 / note_hz(n2)], 0.5 + 4.5 * c)
}

fn treble_shelf(v: f32) -> (f64, f32) {
    let v = f64::from(v.clamp(0.0, 1.0));
    let u = 1.0 - v;
    (
        note_hz(86.0 + 34.0 * v),
        10f64.powf(-12.0 * u * (2.0 - u) / 20.0) as f32,
    )
}

fn bass_shelf(v: f32) -> (f64, f32) {
    let v = f64::from(v.clamp(0.0, 1.0));
    (
        note_hz(40.0 + 24.0 * v),
        10f64.powf(-18.0 * v / 20.0) as f32,
    )
}

fn mix_of(v: f32) -> (f32, f32) {
    let v = v.clamp(0.0, 1.0);
    (
        if v < 0.5 { 1.0 } else { 2.0 * (1.0 - v) },
        0.9 * (2.0 * v).min(1.0),
    )
}

#[derive(Debug)]
pub struct Metaverb {
    rate: f64,
    tank: Tank,
    treble: OnePole,
    bass: OnePole,
    treble_gain: f32,
    bass_gain: f32,
    size: f32,
    decay: f32,
    dry: Smoothed,
    wet: Smoothed,
    single: bool,
    freeze: bool,
    frozen_in: f64,
}

impl Metaverb {
    pub fn new(rate: f64) -> Self {
        let mut r = Self {
            rate,
            tank: Tank::new(rate),
            treble: OnePole::default(),
            bass: OnePole::default(),
            treble_gain: 1.0,
            bass_gain: 1.0,
            size: 0.502,
            decay: 0.0,
            dry: Smoothed::new(1.0, 0.14 / 3.0, rate),
            wet: Smoothed::new(0.0, 0.14 / 3.0, rate),
            single: false,
            freeze: false,
            frozen_in: 0.0,
        };
        r.set_shelves(GROUP_LP, GROUP_HP);
        r
    }

    fn set_shelves(&mut self, lp: f32, hp: f32) {
        let (hz, gain) = treble_shelf(lp);
        self.treble.set(hz, self.rate);
        self.treble_gain = gain;
        let (hz, gain) = bass_shelf(hp);
        self.bass.set(hz, self.rate);
        self.bass_gain = gain;
    }

    fn set_mix(&mut self, dry_wet: f32) {
        let (dry, wet) = mix_of(dry_wet);
        self.dry.set(dry);
        self.wet.set(wet);
    }

    fn shape(&self) -> Shape {
        let ([a, b], decay_s) = size_of(self.size);
        let decay_s = f64::from(stretch(decay_s as f32, self.decay, 0.2, 10.0));
        let gain = |d: f64| 10f64.powf(-3.0 * d / decay_s) as f32;
        Shape {
            main: [(a * self.rate) as f32, (b * self.rate) as f32],
            gain: [gain(a), gain(b)],
            diffusers: DIFFUSERS.map(|pair| pair.map(|r| (r * a * self.rate) as f32)),
        }
    }
}

impl Effect for Metaverb {
    fn kind(&self) -> EffectKind {
        EffectKind::Reverb
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        let shape = self.shape();
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            let (mut dry, wet) = (self.dry.tick(), self.wet.tick());
            let mut input = 1.0;
            if self.freeze {
                self.frozen_in += 1.0;
                if self.frozen_in >= FREEZE_S * self.rate {
                    input = 0.0;
                    if self.single {
                        dry = 0.0;
                    }
                }
            }
            let tail = self.tank.tick([frame[0] * input, frame[1] * input], &shape);
            for ch in 0..2 {
                let (_, high) = self.treble.tick(ch, tail[ch]);
                let shelved = tail[ch] + (self.treble_gain - 1.0) * high;
                let (low, _) = self.bass.tick(ch, shelved);
                let shelved = shelved + (self.bass_gain - 1.0) * low;
                frame[ch] = frame[ch] * dry + shelved * wet;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.single = false;
        self.freeze = false;
        self.size = amount;
        self.set_mix((10.0 * amount).min(0.5));
        self.set_shelves(GROUP_LP, GROUP_HP);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        self.single = true;
        match index {
            0 => self.size = value,
            1 => {
                let (hz, gain) = treble_shelf(value);
                self.treble.set(hz, self.rate);
                self.treble_gain = gain;
            }
            2 => {
                let (hz, gain) = bass_shelf(value);
                self.bass.set(hz, self.rate);
                self.bass_gain = gain;
            }
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        if index == 0 {
            self.freeze = on;
            self.frozen_in = 0.0;
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.single = true;
        self.set_mix(dry_wet);
        true
    }

    fn has_tail(&self) -> bool {
        true
    }

    fn set_decay(&mut self, decay: f32) {
        self.decay = decay;
    }

    fn reset(&mut self) {
        self.tank.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn knobs_follow_their_curves() {
        let ([a, b], _) = size_of(0.0);
        assert!(
            (a - 0.010_8).abs() < 0.000_5 && (b - 0.011_5).abs() < 0.000_5,
            "{a} {b}"
        );
        let ([a, b], _) = size_of(1.0);
        assert!(
            (a - 0.054).abs() < 0.002 && (b - 0.065).abs() < 0.002,
            "{a} {b}"
        );
        assert_eq!(mix_of(0.25), (1.0, 0.45));
        assert_eq!(mix_of(1.0), (0.0, 0.9));
        let (_, gain) = treble_shelf(0.0);
        assert!((gain - 10f32.powf(-12.0 / 20.0)).abs() < 1e-4);
    }

    #[test]
    fn rings_longer_with_size() {
        let tail = |size: f32| {
            let mut r = Metaverb::new(f64::from(RATE));
            r.set_amount(size);
            let out = run(&mut r, &impulse(RATE as usize), ctx());
            rms(&out[2 * 12_000..])
        };
        assert!(tail(1.0) > tail(0.1));
    }
}
