use std::f32::consts::TAU;

use backspn_dsp::Biquad;

pub const RATE: u32 = 44_100;

const PEAK: f32 = 0.9;

const FADE_SECONDS: f32 = 0.004;

const METAL: [f32; 6] = [263.0, 400.0, 421.0, 474.0, 587.0, 845.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Analog,
    Club,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice {
    Kick,
    Snare,
    Clap,
    ClosedHat,
    OpenHat,
    Ride,
    Rim,
    Tom,
}

impl Voice {
    pub const ALL: [Self; 8] = [
        Self::Kick,
        Self::Snare,
        Self::Clap,
        Self::ClosedHat,
        Self::OpenHat,
        Self::Ride,
        Self::Rim,
        Self::Tom,
    ];
}

struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

struct Band {
    high: Biquad,
    low: Biquad,
}

impl Band {
    fn new(from: f32, to: f32) -> Self {
        Self {
            high: Biquad::highpass(f64::from(from), f64::from(RATE)),
            low: Biquad::lowpass(f64::from(to), f64::from(RATE)),
        }
    }

    fn tick(&mut self, x: f32) -> f32 {
        self.low.tick(0, self.high.tick(0, x))
    }
}

struct Drum {
    start: f32,
    end: f32,
    sweep: f32,
    decay: f32,
    seconds: f32,
    drive: f32,
    click: f32,
}

struct Snare {
    tones: [f32; 2],
    tone_decay: f32,
    noise_decay: f32,
    noise: f32,
    band: (f32, f32),
    snap: f32,
    body: f32,
    thwack: f32,
    seconds: f32,
}

struct Metal {
    scale: f32,
    squares: f32,
    noise: f32,
    band: (f32, f32),
    decay: f32,
    seconds: f32,
}

fn decay(t: f32, tau: f32) -> f32 {
    (-t / tau).exp()
}

fn times(seconds: f32) -> impl Iterator<Item = f32> {
    let frames = (seconds * RATE as f32) as usize;
    (0..frames).map(|i| i as f32 / RATE as f32)
}

fn drum(p: &Drum) -> Vec<f32> {
    let mut noise = Noise(0x1234_5678);
    let mut phase = 0.0f32;
    times(p.seconds)
        .map(|t| {
            let freq = p.end + (p.start - p.end) * decay(t, p.sweep);
            phase = (phase + freq / RATE as f32).fract();
            let body = (TAU * phase).sin() * decay(t, p.decay);
            let click = noise.next() * decay(t, 0.002) * p.click;
            ((body + click) * p.drive).tanh()
        })
        .collect()
}

fn snare(p: &Snare) -> Vec<f32> {
    let mut noise = Noise(0x0bad_cafe);
    let mut band = Band::new(p.band.0, p.band.1);
    let mut crack = Band::new(4500.0, 16_000.0);
    let mut shell = Band::new(350.0, 2200.0);
    let mut phases = [0.0f32; 2];
    times(p.seconds)
        .map(|t| {
            let bend = 1.0 + 1.0 * decay(t, 0.006);
            let mut tone = 0.0;
            for (phase, freq) in phases.iter_mut().zip(p.tones) {
                *phase = (*phase + freq * bend / RATE as f32).fract();
                tone += (TAU * *phase).sin();
            }
            let white = noise.next();
            let hiss = band.tick(white) * decay(t, p.noise_decay) * p.noise;
            let snap = crack.tick(white) * decay(t, 0.025) * p.snap;
            let thwack = shell.tick(white) * decay(t, 0.05) * p.thwack;
            tone * 0.5 * p.body * decay(t, p.tone_decay) + hiss + snap + thwack
        })
        .collect()
}

fn clap(center: f32, tail: f32) -> Vec<f32> {
    const BURSTS: [f32; 3] = [0.0, 0.011, 0.022];
    let mut noise = Noise(0x00c1_a900);
    let mut band = Band::new(center * 0.7, center * 1.4);
    times(0.6)
        .map(|t| {
            let bursts = BURSTS
                .iter()
                .filter(|&&at| t >= at)
                .map(|&at| decay(t - at, 0.004))
                .fold(0.0f32, f32::max);
            let after = t - 0.03;
            let tail = if after >= 0.0 {
                decay(after, tail)
            } else {
                0.0
            };
            band.tick(noise.next()) * bursts.max(tail)
        })
        .collect()
}

fn metal(p: &Metal) -> Vec<f32> {
    let mut noise = Noise(0x5eed_0001);
    let mut band = Band::new(p.band.0, p.band.1);
    let mut phases = [0.0f32; METAL.len()];
    times(p.seconds)
        .map(|t| {
            let mut square = 0.0;
            for (phase, freq) in phases.iter_mut().zip(METAL) {
                *phase = (*phase + freq * p.scale / RATE as f32).fract();
                square += if *phase < 0.5 { 1.0 } else { -1.0 };
            }
            let source = square / METAL.len() as f32 * p.squares + noise.next() * p.noise;
            band.tick(source) * decay(t, p.decay)
        })
        .collect()
}

fn rim(tones: [f32; 2], tau: f32, noise_level: f32) -> Vec<f32> {
    let mut noise = Noise(0x7171_0042);
    let mut band = Band::new(400.0, 9000.0);
    times(0.12)
        .map(|t| {
            let tone: f32 = tones.iter().map(|f| (TAU * f * t).sin()).sum();
            band.tick(tone * 0.5 + noise.next() * noise_level) * decay(t, tau)
        })
        .collect()
}

fn normalize(samples: &mut [f32], to: f32) {
    let peak = samples.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 0.0 {
        let gain = to / peak;
        for x in samples.iter_mut() {
            *x *= gain;
        }
    }
}

fn finish(mut samples: Vec<f32>, drive: f32) -> Vec<f32> {
    normalize(&mut samples, 1.0);
    for x in &mut samples {
        *x = (*x * drive).tanh();
    }
    normalize(&mut samples, PEAK);
    let fade = ((FADE_SECONDS * RATE as f32) as usize).min(samples.len());
    let len = samples.len();
    for (i, x) in samples[len - fade..].iter_mut().enumerate() {
        *x *= 1.0 - (i + 1) as f32 / fade as f32;
    }
    samples
}

pub fn render(style: Style, voice: Voice) -> Vec<f32> {
    let analog = style == Style::Analog;
    let samples = match voice {
        Voice::Kick if analog => drum(&Drum {
            start: 160.0,
            end: 48.0,
            sweep: 0.025,
            decay: 0.45,
            seconds: 1.2,
            drive: 1.0,
            click: 0.0,
        }),
        Voice::Kick => drum(&Drum {
            start: 300.0,
            end: 52.0,
            sweep: 0.018,
            decay: 0.28,
            seconds: 0.8,
            drive: 2.5,
            click: 0.4,
        }),
        Voice::Tom if analog => drum(&Drum {
            start: 135.0,
            end: 105.0,
            sweep: 0.08,
            decay: 0.3,
            seconds: 0.8,
            drive: 1.0,
            click: 0.0,
        }),
        Voice::Tom => drum(&Drum {
            start: 210.0,
            end: 150.0,
            sweep: 0.05,
            decay: 0.25,
            seconds: 0.7,
            drive: 1.4,
            click: 0.15,
        }),
        Voice::Snare if analog => snare(&Snare {
            tones: [175.0, 320.0],
            tone_decay: 0.08,
            noise_decay: 0.2,
            noise: 1.3,
            band: (1200.0, 11_000.0),
            snap: 0.9,
            body: 1.6,
            thwack: 1.0,
            seconds: 0.5,
        }),
        Voice::Snare => snare(&Snare {
            tones: [190.0, 345.0],
            tone_decay: 0.07,
            noise_decay: 0.26,
            noise: 1.6,
            band: (1000.0, 14_000.0),
            snap: 1.1,
            body: 1.5,
            thwack: 1.2,
            seconds: 0.6,
        }),
        Voice::Clap if analog => clap(1200.0, 0.1),
        Voice::Clap => clap(1500.0, 0.16),
        Voice::ClosedHat | Voice::OpenHat | Voice::Ride => metal(&metal_params(style, voice)),
        Voice::Rim if analog => rim([480.0, 1650.0], 0.012, 0.0),
        Voice::Rim => rim([1900.0, 2900.0], 0.008, 0.3),
    };
    finish(samples, drive(style, voice))
}

fn drive(style: Style, voice: Voice) -> f32 {
    let base = match voice {
        Voice::Snare => 3.2,
        Voice::Clap | Voice::Rim => 2.2,
        Voice::Kick | Voice::Tom => 1.8,
        Voice::ClosedHat | Voice::OpenHat => 1.6,
        Voice::Ride => 1.3,
    };
    match style {
        Style::Analog => base,
        Style::Club => base * 1.25,
    }
}

fn metal_params(style: Style, voice: Voice) -> Metal {
    let (scale, metal, noise, band) = match style {
        Style::Analog => (1.0, 1.0, 0.35, (6500.0, 17_000.0)),
        Style::Club => (1.3, 0.6, 0.8, (7500.0, 19_000.0)),
    };
    let (decay, seconds, band) = match (voice, style) {
        (Voice::ClosedHat, Style::Analog) => (0.04, 0.25, band),
        (Voice::ClosedHat, Style::Club) => (0.05, 0.25, band),
        (Voice::OpenHat, Style::Analog) => (0.3, 1.0, band),
        (Voice::OpenHat, Style::Club) => (0.35, 1.0, band),
        (_, Style::Analog) => (0.9, 2.5, (4500.0, 14_000.0)),
        (_, Style::Club) => (1.1, 2.5, (6000.0, 16_000.0)),
    };
    Metal {
        scale,
        squares: metal,
        noise,
        band,
        decay,
        seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn every_sound_is_finite_audible_and_within_full_scale() {
        for style in [Style::Analog, Style::Club] {
            for voice in Voice::ALL {
                let s = render(style, voice);
                assert!(!s.is_empty(), "{style:?} {voice:?}");
                assert!(s.iter().all(|x| x.is_finite() && x.abs() <= PEAK + 1e-6));
                assert!(rms(&s) > 0.01, "{style:?} {voice:?} is silent");
                assert!(
                    s.last().unwrap().abs() < 1e-6,
                    "{style:?} {voice:?} ends in a click"
                );
            }
        }
    }

    #[test]
    fn sounds_are_the_same_every_time() {
        assert_eq!(
            render(Style::Club, Voice::Snare),
            render(Style::Club, Voice::Snare)
        );
    }

    #[test]
    fn styles_sound_different() {
        for voice in Voice::ALL {
            assert_ne!(
                render(Style::Analog, voice),
                render(Style::Club, voice),
                "{voice:?}"
            );
        }
    }
}
