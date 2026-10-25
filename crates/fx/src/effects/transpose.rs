use backspn_dsp::{DelayLine, Smoothed};

use super::EffectKind;
use crate::{Context, Effect};

const BUFFER: usize = 1 << 19;
const RAMP_S: f64 = 0.1;

fn key_semitones(knob: f32) -> f64 {
    let b = 2.0 * f64::from(knob.clamp(0.0, 1.0)) - 1.0;
    let x = b.abs() * 24.0;
    let point = |k: f64| -> f64 {
        if k < 1.0 {
            0.0
        } else if b > 0.0 {
            if k < 2.0 {
                k - 1.0
            } else {
                1.0 + 0.5 * (k - 2.0)
            }
        } else if k <= 6.0 {
            1.0 - k
        } else if k <= 12.0 {
            -k
        } else {
            -12.0 - 4.0 * (k - 12.0)
        }
    };
    let step = x.floor().min(23.0);
    let semitones = point(step) + (point(step + 1.0) - point(step)) * (x - step);
    (4.0 * semitones).round() / 4.0
}

fn auto_grain_ms(v: f32) -> f64 {
    let b = 2.0 * f64::from(v.clamp(0.0, 1.0)) - 1.0;
    if b > 0.0 {
        80.0 - 35.0 * b
    } else {
        80.0 + 66.0 * b.abs()
    }
}

fn grain_ms(v: f32) -> f64 {
    5.0 + 328.0 * f64::from(1.0 - v.clamp(0.0, 1.0)).powi(3)
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Transpose {
    rate: f64,
    line: DelayLine,
    captured: usize,
    loop_pos: f64,
    looping: bool,
    phase: f64,
    key: f32,
    grain_knob: f32,
    stretch: f32,
    two_bars: bool,
    manual_grain: bool,
    ratio: Smoothed,
    grain: Smoothed,
    mix: Smoothed,
    dry_wet: f32,
    active: bool,
}

impl Transpose {
    pub fn new(rate: f64) -> Self {
        let smooth = |v| Smoothed::new(v, RAMP_S / 3.0, rate);
        Self {
            rate,
            line: DelayLine::new(BUFFER),
            captured: 0,
            loop_pos: 0.0,
            looping: false,
            phase: 0.0,
            key: 0.498,
            grain_knob: 1.0,
            stretch: 0.0,
            two_bars: false,
            manual_grain: false,
            ratio: smooth(1.0),
            grain: smooth((0.08 * rate) as f32),
            mix: smooth(0.0),
            dry_wet: 1.0,
            active: false,
        }
    }

    fn grain_back(phase: f64, grain: f64, ratio: f64, base: f64) -> f64 {
        let start = base + grain * (ratio - 1.0).max(0.0);
        start + grain * phase * (1.0 - ratio)
    }
}

impl Effect for Transpose {
    fn kind(&self) -> EffectKind {
        EffectKind::TransposeStretch
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let semitones = key_semitones(self.key);
        self.ratio.set(2f32.powf(semitones as f32 / 12.0));
        let ms = if self.manual_grain {
            grain_ms(self.grain_knob)
        } else {
            auto_grain_ms(self.key)
        };
        self.grain.set((ms / 1000.0 * self.rate) as f32);
        self.mix.set(if self.active { self.dry_wet } else { 0.0 });
        let bars = if self.two_bars { 8.0 } else { 4.0 };
        let loop_len = bars * 60.0 / ctx.bpm.max(20.0) * self.rate;
        let looping = self.active && self.stretch > 0.0;
        if looping && !self.looping {
            self.captured = 0;
            self.loop_pos = 0.0;
        }
        self.looping = looping;
        let speed = f64::from(1.0 - self.stretch.clamp(0.0, 1.0));
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            if !self.looping || (self.captured as f64) < loop_len {
                self.line.push(*frame);
                self.captured += 1;
            }
            let (ratio, grain) = (f64::from(self.ratio.tick()), f64::from(self.grain.tick()));
            let base = if self.looping {
                let written = self.captured.min(BUFFER - 1) as f64;
                self.loop_pos = (self.loop_pos + speed) % loop_len.min(written).max(1.0);
                (written - self.loop_pos).max(1.0)
            } else {
                1.0
            };
            self.phase = (self.phase + 1.0 / grain.max(1.0)) % 1.0;
            let mut wet = [0.0; 2];
            for offset in [0.0, 0.5] {
                let p = (self.phase + offset) % 1.0;
                let window = (0.5 - 0.5 * (2.0 * std::f64::consts::PI * p).cos()) as f32;
                let back = Self::grain_back(p, grain, ratio, base);
                let s = self.line.read(back.clamp(1.0, (BUFFER - 8) as f64) as f32);
                wet[0] += s[0] * window;
                wet[1] += s[1] * window;
            }
            let w = self.mix.tick();
            let (dry, gain) = (1.0 - w * w, w * (2.0 - w));
            frame[0] = frame[0] * dry + wet[0] * gain;
            frame[1] = frame[1] * dry + wet[1] * gain;
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.key = amount;
        self.stretch = 0.0;
        self.manual_grain = false;
        self.two_bars = false;
        self.dry_wet = 1.0;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.key = value,
            1 => self.grain_knob = value,
            2 => self.stretch = value,
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.two_bars = on,
            1 => self.manual_grain = on,
            _ => {}
        }
    }

    fn set_active(&mut self, on: bool) {
        self.active = on;
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.dry_wet = dry_wet.clamp(0.0, 1.0);
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

    fn crossings(x: &[f32]) -> usize {
        x.iter()
            .step_by(2)
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|w| *w[0] <= 0.0 && *w[1] > 0.0)
            .count()
    }

    #[test]
    fn key_follows_its_curve() {
        assert!(key_semitones(0.5).abs() < 1e-9);
        assert!((key_semitones(1.0) - 12.0).abs() < 1e-9);
        assert!((key_semitones(0.0) + 60.0).abs() < 1e-9);
        assert!(
            (key_semitones(0.25) + 12.0).abs() < 1e-9,
            "an octave down at a quarter"
        );
        assert!((auto_grain_ms(1.0) - 45.0).abs() < 1e-9);
        assert!((grain_ms(0.0) - 333.0).abs() < 1e-9);
    }

    #[test]
    fn an_octave_up_doubles_the_frequency() {
        let mut t = Transpose::new(f64::from(RATE));
        t.set_amount(1.0);
        t.set_active(true);
        let out = run(&mut t, &sine(300.0, RATE as usize, 0.5), ctx());
        let second = &out[RATE as usize..];
        let hz = crossings(second) as f64 * 2.0;
        assert!((hz - 600.0).abs() < 40.0, "{hz} Hz");
    }
}
