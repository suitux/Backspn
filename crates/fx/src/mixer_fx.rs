use backspn_dsp::{Smoothed, Svf};
use serde::{Deserialize, Serialize};

use crate::effects::color::{self, ColorFx};
use crate::effects::{EffectKind, mixer_engine};
use crate::{Context, Effect, MAX_BLOCK_FRAMES};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum MixerFx {
    #[default]
    Reverb,
    DualDelay,
    Noise,
    TimeGater,
    Flanger,
    BarberPole,
    DottedDelay,
    Crush,
    Jet,
    BitCrush,
    ColorNoise,
    Pitch,
    Space,
    DubEcho,
    ColorEcho,
    Sweep,
    GateComp,
    KeyTune,
    ColorReverb,
}

impl MixerFx {
    pub const ALL: [Self; 19] = [
        Self::Reverb,
        Self::DualDelay,
        Self::Noise,
        Self::TimeGater,
        Self::Flanger,
        Self::BarberPole,
        Self::DottedDelay,
        Self::Crush,
        Self::Jet,
        Self::BitCrush,
        Self::ColorNoise,
        Self::Pitch,
        Self::Space,
        Self::DubEcho,
        Self::ColorEcho,
        Self::Sweep,
        Self::GateComp,
        Self::KeyTune,
        Self::ColorReverb,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Reverb => "Reverb",
            Self::DualDelay => "Dual Delay",
            Self::Noise => "Noise",
            Self::TimeGater => "Time Gater",
            Self::Flanger => "Flanger",
            Self::BarberPole => "Barber Pole",
            Self::DottedDelay => "Dotted Delay",
            Self::Crush => "Crush",
            Self::Jet => "Jet",
            Self::BitCrush => "Bit Crush",
            Self::ColorNoise => "Color Noise",
            Self::Pitch => "Pitch",
            Self::Space => "Space",
            Self::DubEcho => "Dub Echo",
            Self::ColorEcho => "Color Echo",
            Self::Sweep => "Sweep",
            Self::GateComp => "Gate/Comp",
            Self::KeyTune => "Key Tune",
            Self::ColorReverb => "Color Reverb",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::Reverb => "RVRB",
            Self::DualDelay => "DLDL",
            Self::Noise => "NOISE",
            Self::TimeGater => "TIMG",
            Self::Flanger => "FLNG",
            Self::BarberPole => "BRPL",
            Self::DottedDelay => "DTDL",
            Self::Crush => "CRSH",
            Self::Jet => "JET",
            Self::BitCrush => "BCRS",
            Self::ColorNoise => "CNOS",
            Self::Pitch => "PTCH",
            Self::Space => "SPCE",
            Self::DubEcho => "DUB",
            Self::ColorEcho => "CECH",
            Self::Sweep => "SWEP",
            Self::GateComp => "GTCP",
            Self::KeyTune => "KTUN",
            Self::ColorReverb => "CRVB",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|&k| k == self).unwrap_or(0)
    }

    pub fn is_color(self) -> bool {
        self.color().is_some()
    }

    fn color(self) -> Option<color::Kind> {
        Some(match self {
            Self::Jet => color::Kind::Jet,
            Self::BitCrush => color::Kind::Crush,
            Self::ColorNoise => color::Kind::Noise,
            Self::Pitch => color::Kind::Pitch,
            Self::Space => color::Kind::Space,
            Self::DubEcho => color::Kind::DubEcho,
            Self::ColorEcho => color::Kind::Echo,
            Self::Sweep => color::Kind::Sweep,
            Self::GateComp => color::Kind::GateComp,
            Self::KeyTune => color::Kind::KeyTune,
            Self::ColorReverb => color::Kind::Reverb,
            _ => return None,
        })
    }
}

pub const DEFAULT_SLOTS: [MixerFx; 4] = [
    MixerFx::Reverb,
    MixerFx::DualDelay,
    MixerFx::Noise,
    MixerFx::TimeGater,
];

const DEAD_ZONE: f32 = 0.01;
const COLOUR_HZ: f64 = 1200.0;

pub struct ChannelFx {
    instances: Vec<Option<Box<dyn Effect>>>,
    colors: Vec<Option<ColorFx>>,
    selected: Option<MixerFx>,
    on: bool,
    knob: f32,
    gain: Smoothed,
    colour: Svf,
    noise: u32,
    dry: Box<[f32]>,
    rate: f64,
    /// The effects hold no sound from before: set once a faded-out effect
    /// has been cleared, so switching it back on starts from silence.
    clear: bool,
}

impl std::fmt::Debug for ChannelFx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelFx")
            .field("selected", &self.selected)
            .field("on", &self.on)
            .field("knob", &self.knob)
            .finish_non_exhaustive()
    }
}

impl ChannelFx {
    pub fn new(sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate);
        let base = |kind| match kind {
            MixerFx::Reverb => Some(EffectKind::Reverb),
            MixerFx::DualDelay | MixerFx::DottedDelay => Some(EffectKind::Delay),
            MixerFx::TimeGater => Some(EffectKind::Gater),
            MixerFx::Flanger => Some(EffectKind::Flanger),
            MixerFx::BarberPole => Some(EffectKind::Phaser),
            MixerFx::Crush => Some(EffectKind::LoFi),
            _ => None,
        };
        Self {
            instances: MixerFx::ALL
                .iter()
                .map(|&kind| base(kind).map(|k| mixer_engine(k, sample_rate)))
                .collect(),
            colors: MixerFx::ALL
                .iter()
                .map(|&kind| kind.color().map(|k| ColorFx::new(k, rate)))
                .collect(),
            selected: None,
            on: false,
            knob: 0.0,
            gain: Smoothed::new(0.0, 0.01, rate),
            colour: Svf::default(),
            noise: 0x1234_5678,
            dry: vec![0.0; 2 * MAX_BLOCK_FRAMES].into_boxed_slice(),
            rate,
            clear: true,
        }
    }

    pub fn select(&mut self, effect: Option<MixerFx>) {
        if effect != self.selected {
            self.selected = effect;
            self.clear = false;
            self.reset();
        }
    }

    fn reset(&mut self) {
        if self.clear {
            return;
        }
        for e in self.instances.iter_mut().flatten() {
            e.reset();
        }
        for c in self.colors.iter_mut().flatten() {
            c.reset();
        }
        self.colour.reset();
        self.clear = true;
    }

    pub fn set_on(&mut self, on: bool) {
        self.on = on;
    }

    pub fn set_parameter(&mut self, value: f32) {
        for c in self.colors.iter_mut().flatten() {
            c.set_parameter(value);
        }
    }

    pub fn set_knob(&mut self, knob: f32) {
        self.knob = knob.clamp(-1.0, 1.0);
    }

    pub fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        if let Some(kind) = self.selected.filter(|k| k.is_color()) {
            self.gain.set(if self.on { 1.0 } else { 0.0 });
            if !self.on && self.gain.value() == 0.0 {
                self.reset();
                return;
            }
            let resuming = std::mem::take(&mut self.clear);
            let Some(color) = self.colors[kind.index()].as_mut() else {
                return;
            };
            color.set_knob(self.knob);
            if resuming {
                color.settle();
            }
            let n = buf.len().min(self.dry.len());
            self.dry[..n].copy_from_slice(&buf[..n]);
            color.process(&mut buf[..n]);
            for (frame, dry) in buf[..n]
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .zip(self.dry[..n].as_chunks::<2>().0)
            {
                let g = self.gain.tick();
                for ch in 0..2 {
                    frame[ch] = dry[ch] + g * (frame[ch] - dry[ch]);
                }
            }
            return;
        }
        let active = self.on && self.selected.is_some() && self.knob.abs() > DEAD_ZONE;
        self.gain.set(if active { 1.0 } else { 0.0 });
        let Some(kind) = self.selected else {
            return;
        };
        if !active && self.gain.value() == 0.0 {
            self.reset();
            return;
        }
        self.clear = false;
        let amount = self.knob.abs();
        let high = self.knob > 0.0;
        self.colour.set(COLOUR_HZ, 0.707, self.rate);
        let n = buf.len().min(self.dry.len());
        self.dry[..n].copy_from_slice(&buf[..n]);
        match (kind, &mut self.instances[kind.index()]) {
            (MixerFx::Noise, _) => {
                for frame in buf[..n].as_chunks_mut::<2>().0 {
                    self.noise ^= self.noise << 13;
                    self.noise ^= self.noise >> 17;
                    self.noise ^= self.noise << 5;
                    let white = (self.noise as f32 / u32::MAX as f32) * 2.0 - 1.0;
                    let level = 0.3 * amount * amount;
                    frame[0] += white * level;
                    frame[1] += white * level;
                }
            }
            (_, Some(effect)) => {
                effect.set_amount(amount);
                if matches!(kind, MixerFx::DottedDelay) {
                    effect.set_param(2, 0.75);
                }
                effect.process(&mut buf[..n], ctx);
            }
            (_, None) => {}
        }
        for (frame, dry) in buf[..n]
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(self.dry[..n].as_chunks::<2>().0)
        {
            let g = self.gain.tick();
            for ch in 0..2 {
                let wet = frame[ch] - dry[ch];
                let (lp, hp) = self.colour.tick(ch, wet);
                let coloured = if high { hp } else { lp };
                frame[ch] = dry[ch] + g * coloured;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::test_util::{ctx, rms, sine};

    #[test]
    fn centre_or_off_leaves_the_sound() {
        let input = sine(440.0, 4096, 0.5);
        let mut fx = ChannelFx::new(48_000);
        fx.select(Some(MixerFx::Noise));
        fx.set_on(true);
        fx.set_knob(0.0);
        let mut buf = input.clone();
        fx.process(&mut buf, &ctx());
        assert_eq!(buf, input, "centre: off");
        fx.set_knob(0.8);
        fx.set_on(false);
        let mut buf = input.clone();
        fx.process(&mut buf, &ctx());
        assert_eq!(buf, input, "button off");
        fx.select(None);
        fx.set_on(true);
        let mut buf = input.clone();
        fx.process(&mut buf, &ctx());
        assert_eq!(buf, input, "the filter is the mixer's");
    }

    #[test]
    fn turning_away_from_the_centre_brings_the_effect_in() {
        let silence = vec![0.0f32; 2 * 4096];
        let mut fx = ChannelFx::new(48_000);
        fx.select(Some(MixerFx::Noise));
        fx.set_on(true);
        fx.set_knob(-0.9);
        let mut buf = silence.clone();
        fx.process(&mut buf, &ctx());
        assert!(rms(&buf) > 0.01, "noise added: {}", rms(&buf));
        let mut more = ChannelFx::new(48_000);
        more.select(Some(MixerFx::Noise));
        more.set_on(true);
        more.set_knob(0.3);
        let mut quiet = silence;
        more.process(&mut quiet, &ctx());
        assert!(rms(&quiet) < rms(&buf), "less amount, less noise");
    }

    #[test]
    fn switching_back_on_starts_without_the_old_tail() {
        let input = sine(220.0, 4096, 0.5);
        let silence = vec![0.0f32; 2 * 4096];
        for kind in [MixerFx::DubEcho, MixerFx::DualDelay, MixerFx::Reverb] {
            let mut fx = ChannelFx::new(48_000);
            fx.select(Some(kind));
            fx.set_on(true);
            fx.set_knob(0.8);
            let mut buf = input.clone();
            fx.process(&mut buf, &ctx());
            fx.set_on(false);
            fx.set_knob(0.0);
            for _ in 0..4 {
                let mut buf = silence.clone();
                fx.process(&mut buf, &ctx());
            }
            fx.set_on(true);
            fx.set_knob(0.8);
            let mut buf = silence.clone();
            fx.process(&mut buf, &ctx());
            assert!(rms(&buf) < 1e-6, "{kind:?} replays its tail: {}", rms(&buf));
        }
    }

    #[test]
    fn switching_back_on_at_the_centre_leaves_the_sound() {
        let input = sine(220.0, 4096, 0.5);
        let silence = vec![0.0f32; 2 * 4096];
        let mut fx = ChannelFx::new(48_000);
        fx.select(Some(MixerFx::DubEcho));
        fx.set_on(true);
        fx.set_knob(0.9);
        for _ in 0..8 {
            let mut buf = input.clone();
            fx.process(&mut buf, &ctx());
        }
        fx.set_on(false);
        for _ in 0..4 {
            let mut buf = silence.clone();
            fx.process(&mut buf, &ctx());
        }
        fx.set_knob(0.0);
        fx.set_on(true);
        for _ in 0..4 {
            let mut buf = input.clone();
            fx.process(&mut buf, &ctx());
            let diff = buf
                .iter()
                .zip(&input)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            assert!(diff < 1e-6, "the echo opens again: {diff}");
        }
    }

    #[test]
    fn every_effect_runs() {
        let input = sine(220.0, 4096, 0.5);
        for kind in MixerFx::ALL {
            let mut fx = ChannelFx::new(48_000);
            fx.select(Some(kind));
            fx.set_on(true);
            fx.set_knob(0.7);
            let mut buf = input.clone();
            fx.process(&mut buf, &ctx());
            assert!(buf.iter().all(|s| s.is_finite()), "{kind:?}");
        }
    }
}
