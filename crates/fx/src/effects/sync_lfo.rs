use backspn_dsp::{DelayLine, Smoothed};

use super::pulse_hold::{AllPass, allpass_coefs};
use super::{EffectKind, LfoStart, beats_per_sample, note_hz};
use crate::{Context, Effect};

const RATE_BARS: [f64; 11] = [
    16.0, 8.0, 4.0, 2.0, 1.0, 0.5, 0.375, 0.25, 0.1875, 0.125, 0.0625,
];
const FLANGER_FRAMES: usize = 4_096;
const SECTIONS: usize = 4;

fn period_beats(v: f32, free: bool) -> f64 {
    let x = f64::from(v.clamp(0.0, 1.0)) * 10.0;
    let bars = if free {
        let i = (x.floor() as usize).min(9);
        RATE_BARS[i] + (RATE_BARS[i + 1] - RATE_BARS[i]) * (x - i as f64)
    } else {
        RATE_BARS[(x.round() as usize).min(10)]
    };
    4.0 * bars
}

fn triangle(phase: f64, up: bool) -> f64 {
    let t = 2.0 * (phase.rem_euclid(1.0) - 0.5).abs();
    if up { t } else { 1.0 - t }
}

#[derive(Debug, Clone, Copy)]
struct SyncLfo {
    rate: f32,
    free: bool,
    up: bool,
    spread: f32,
    phase: f64,
    start: LfoStart,
}

impl SyncLfo {
    fn begin(&mut self, ctx: &Context) {
        let period = period_beats(self.rate, self.free);
        if self.start.begin(ctx, period, f64::INFINITY) && self.free {
            self.phase = 0.0;
        }
    }

    fn set_rate(&mut self, value: f32) {
        if !self.free
            && (period_beats(value, false) - period_beats(self.rate, false)).abs() > f64::EPSILON
        {
            self.start.reset();
        }
        self.rate = value;
    }

    fn tick(&mut self, beat: f64, per_frame: f64) -> [f64; 2] {
        let period = period_beats(self.rate, self.free);
        self.phase = if self.free {
            (self.phase + per_frame / period).rem_euclid(1.0)
        } else {
            (self.start.beat(beat) / period).rem_euclid(1.0)
        };
        let right = self.phase + 0.5 * f64::from(self.spread * self.spread);
        [triangle(self.phase, self.up), triangle(right, self.up)]
    }
}

#[derive(Debug)]
pub struct Flanger {
    rate: f64,
    line: DelayLine,
    lfo: SyncLfo,
    feedback: f32,
    dry_wet: Smoothed,
    on: Smoothed,
    active: bool,
}

impl Flanger {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            line: DelayLine::new(FLANGER_FRAMES),
            lfo: SyncLfo {
                rate: 0.0,
                free: true,
                up: true,
                spread: 0.0,
                phase: 0.0,
                start: LfoStart::default(),
            },
            feedback: 0.9,
            dry_wet: Smoothed::new(0.8, 0.05, rate),
            on: Smoothed::new(0.0, 0.01, rate),
            active: false,
        }
    }
}

impl Effect for Flanger {
    fn kind(&self) -> EffectKind {
        EffectKind::Flanger
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        self.on.set(if self.active { 1.0 } else { 0.0 });
        let per_frame = beats_per_sample(ctx, self.rate);
        self.lfo.begin(ctx);
        let fb_knob = f64::from(self.feedback.clamp(0.0, 1.0));
        let f = fb_knob * (2.0 - fb_knob);
        let comp = if f > 0.4 {
            1.0 - 0.32 * ((f - 0.4) / 0.6).powi(2)
        } else {
            1.0
        };
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            let beat = ctx.beat + i as f64 * per_frame;
            let lfo = self.lfo.tick(beat, per_frame);
            let depth = f64::from(self.dry_wet.tick());
            let on = f64::from(self.on.tick());
            let ceiling = (1.25 - 0.584 * depth * depth).max(0.01) as f32;
            let gain = f * (0.9 + 0.1 * depth * depth);
            let mut taps = [0.0f32; 2];
            let mut fed = [0.0f32; 2];
            for ch in 0..2 {
                let delay = self.rate * (0.000_025 + lfo[ch] * (0.002_2 + 0.003_3 * depth * depth));
                let fb = (gain * (delay / (self.rate * 0.000_125)).min(1.0)) as f32;
                let tap = self
                    .line
                    .read(delay.clamp(1.0, (FLANGER_FRAMES - 2) as f64) as f32)[ch];
                taps[ch] = tap;
                let x = (frame[ch] + tap * fb).clamp(-2.0 * ceiling, 2.0 * ceiling);
                fed[ch] = x * (1.0 - x.abs() / (4.0 * ceiling));
            }
            self.line.push(fed);
            let m = (2.0 * depth).min(1.0) * on;
            let wet = 0.75 * comp * m * (2.0 - m);
            let dry = 1.0 - m * m * (1.0 - 0.75 * comp);
            for ch in 0..2 {
                frame[ch] = (f64::from(frame[ch]) * dry + f64::from(taps[ch]) * wet) as f32;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.lfo = SyncLfo {
            rate: amount,
            free: true,
            up: true,
            spread: 0.0,
            phase: self.lfo.phase,
            start: self.lfo.start,
        };
        self.feedback = 0.9;
        self.dry_wet.set(0.8);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.lfo.set_rate(value),
            1 => self.feedback = value,
            2 => self.lfo.spread = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.lfo.free = on,
            1 => self.lfo.up = on,
            _ => {}
        }
    }

    fn set_active(&mut self, on: bool) {
        if on && !self.active {
            self.lfo.start.start();
        }
        self.active = on;
    }

    fn lfo_reset(&mut self) {
        self.lfo.start.reset();
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.dry_wet.set(dry_wet.clamp(0.0, 1.0));
        true
    }

    fn reset(&mut self) {
        self.line.clear();
    }
}

#[derive(Debug)]
pub struct Phaser {
    rate: f64,
    sections: [AllPass; SECTIONS],
    last: [f32; 2],
    lfo: SyncLfo,
    feedback: f32,
    dry_wet: Smoothed,
    on: Smoothed,
    active: bool,
}

impl Phaser {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            sections: [AllPass::default(); SECTIONS],
            last: [0.0; 2],
            lfo: SyncLfo {
                rate: 0.498,
                free: true,
                up: true,
                spread: 0.0,
                phase: 0.0,
                start: LfoStart::default(),
            },
            feedback: 0.78,
            dry_wet: Smoothed::new(1.0, 0.05, rate),
            on: Smoothed::new(0.0, 0.01, rate),
            active: false,
        }
    }
}

impl Effect for Phaser {
    fn kind(&self) -> EffectKind {
        EffectKind::Phaser
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        self.on.set(if self.active { 1.0 } else { 0.0 });
        let per_frame = beats_per_sample(ctx, self.rate);
        self.lfo.begin(ctx);
        let bps = ctx.bpm.max(20.0) / 60.0;
        let base = 65.0 + 14.0 * (1.0 - 1.0 / ((bps - 2.3).max(0.0) + 1.0)).powi(2);
        let range = 113.3 - base;
        let v = f64::from(self.feedback.clamp(0.0, 1.0));
        let fb = 0.1 + 0.774 * v * (2.0 - v);
        let comp = (1.5 - 0.7 * fb) * (1.0 - 0.3 * fb * fb);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            let beat = ctx.beat + i as f64 * per_frame;
            let lfo = self.lfo.tick(beat, per_frame);
            let m = f64::from(self.dry_wet.tick()) * f64::from(self.on.tick());
            let shaped = m * (2.0 - m);
            for ch in 0..2 {
                let t = lfo[ch];
                let note = base + range * t * (0.5 * t + 0.5);
                let (c1, c2) = allpass_coefs(note_hz(note), self.rate);
                let mut x = frame[ch] + self.last[ch] * fb as f32;
                x *= 1.0 - 0.1 * x.abs().min(2.0);
                for s in &mut self.sections {
                    x = s.tick(ch, x, c1, c2);
                }
                self.last[ch] = x.clamp(-4.0, 4.0);
                let wet = shaped * comp;
                let dry = 1.0 + shaped * ((fb - 1.0) * comp - 1.0);
                frame[ch] = (f64::from(frame[ch]) * dry + f64::from(x) * wet) as f32;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.lfo = SyncLfo {
            rate: amount,
            free: true,
            up: true,
            spread: 0.0,
            phase: self.lfo.phase,
            start: self.lfo.start,
        };
        self.feedback = 0.78;
        self.dry_wet.set(1.0);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.lfo.set_rate(value),
            1 => self.feedback = value,
            2 => self.lfo.spread = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.lfo.free = on,
            1 => self.lfo.up = on,
            _ => {}
        }
    }

    fn set_active(&mut self, on: bool) {
        if on && !self.active {
            self.lfo.start.start();
        }
        self.active = on;
    }

    fn lfo_reset(&mut self) {
        self.lfo.start.reset();
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.dry_wet.set(dry_wet.clamp(0.0, 1.0));
        true
    }

    fn reset(&mut self) {
        self.sections = [AllPass::default(); SECTIONS];
        self.last = [0.0; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn rate_follows_its_steps() {
        assert!((period_beats(0.0, false) - 64.0).abs() < 1e-9, "16 bars");
        assert!((period_beats(1.0, false) - 0.25).abs() < 1e-9, "1/16");
        assert!((period_beats(0.4, false) - 4.0).abs() < 1e-9, "one bar");
        assert!(
            (period_beats(0.45, true) - 3.0).abs() < 1e-5,
            "half way to 2/4"
        );
        assert!((triangle(0.0, true) - 1.0).abs() < 1e-9 && triangle(0.5, true).abs() < 1e-9);
    }

    #[test]
    fn both_sweep_and_stay_bounded() {
        let effects: [Box<dyn Effect>; 2] = [
            Box::new(Flanger::new(f64::from(RATE))),
            Box::new(Phaser::new(f64::from(RATE))),
        ];
        for mut e in effects {
            e.set_param(0, 0.8);
            e.set_param(1, 1.0);
            e.set_dry_wet(1.0);
            e.set_active(true);
            let out = run(e.as_mut(), &sine(2_000.0, RATE as usize, 0.5), ctx());
            assert!(
                out.iter().all(|v| v.is_finite() && v.abs() < 4.0),
                "{:?}",
                e.kind()
            );
            let chunks: Vec<f32> = out.chunks(2 * 1_200).map(rms).collect();
            let (lo, hi) = chunks
                .iter()
                .fold((f32::MAX, 0.0f32), |(l, h), &v| (l.min(v), h.max(v)));
            assert!(hi > 1.3 * lo, "{:?} sweeps: {lo} … {hi}", e.kind());
        }
    }
}
