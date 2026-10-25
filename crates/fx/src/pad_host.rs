use backspn_dsp::Smoothed;

use crate::effects::color::{self, ColorFx};
use crate::pad_fx::{LAYERS, PadFxType, PadPreset};
use crate::{CHANNELS, Context, Effect, EffectKind, MAX_BLOCK_FRAMES, PadParams, create};

const RELEASE_KINDS: [EffectKind; 3] = [
    EffectKind::ReleaseBrake,
    EffectKind::ReleaseEcho,
    EffectKind::BackSpin,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseKind {
    Brake,
    Echo,
    BackSpin,
}

pub fn effect(preset: &PadPreset, sample_rate: u32) -> Option<Box<dyn Effect>> {
    let mut fx: Box<dyn Effect> = match effect_kind(preset.kind) {
        Some(kind) => create(kind, sample_rate),
        None => Box::new(PadColour(ColorFx::new(
            colour_kind(preset.kind)?,
            f64::from(sample_rate),
        ))),
    };
    fx.set_pad(&params(preset));
    Some(fx)
}

pub fn plays(kind: PadFxType) -> bool {
    effect_kind(kind).is_some() || colour_kind(kind).is_some() || release_kind(kind).is_some()
}

fn effect_kind(kind: PadFxType) -> Option<EffectKind> {
    use PadFxType as T;
    Some(match kind {
        T::Delay => EffectKind::BeatDelay,
        T::RevDelay => EffectKind::ReverseDelay,
        T::Echo => EffectKind::Echo,
        T::CoilEcho => EffectKind::CoilEcho,
        T::Reverb => EffectKind::BeatReverb,
        T::Trans => EffectKind::Trans,
        T::FilterLfo => EffectKind::BeatFilter,
        T::Flanger => EffectKind::BeatFlanger,
        T::Phaser => EffectKind::BeatPhaser,
        T::Pan => EffectKind::Pan,
        T::Roll => EffectKind::Roll,
        T::SlipRoll => EffectKind::SlipRoll,
        T::RevRoll => EffectKind::ReverseRoll,
        T::MtDelay => EffectKind::MultiTapDelay,
        _ => return None,
    })
}

pub fn release_kind(kind: PadFxType) -> Option<ReleaseKind> {
    match kind {
        PadFxType::VBrake => Some(ReleaseKind::Brake),
        PadFxType::ReleaseEcho => Some(ReleaseKind::Echo),
        PadFxType::Backspin => Some(ReleaseKind::BackSpin),
        _ => None,
    }
}

pub fn params(preset: &PadPreset) -> PadParams {
    PadParams {
        beats: preset.beat,
        level: preset.level01(),
        room: preset.room01(),
        color: preset.color01(),
    }
}

fn colour_kind(kind: PadFxType) -> Option<color::Kind> {
    Some(match kind {
        PadFxType::Crush => color::Kind::Crush,
        PadFxType::Space => color::Kind::Space,
        PadFxType::DubEcho => color::Kind::DubEcho,
        PadFxType::Pitch => color::Kind::Pitch,
        PadFxType::Sweep => color::Kind::Sweep,
        PadFxType::Noise => color::Kind::Noise,
        PadFxType::GateComp => color::Kind::GateComp,
        _ => return None,
    })
}

#[derive(Debug)]
struct PadColour(ColorFx);

impl Effect for PadColour {
    fn kind(&self) -> EffectKind {
        EffectKind::None
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        self.0.process(buf);
    }

    fn set_amount(&mut self, amount: f32) {
        self.0.set_knob(2.0 * amount - 1.0);
    }

    fn set_param(&mut self, _index: usize, _value: f32) {}

    fn set_button(&mut self, _index: usize, _on: bool) {}

    fn reset(&mut self) {
        self.0.reset();
    }

    fn set_pad(&mut self, pad: &PadParams) {
        self.0.set_parameter(0.5);
        self.set_amount(pad.color);
    }
}

#[derive(Debug)]
pub enum PadCommand {
    Start {
        layer: usize,
        effect: Option<Box<dyn Effect>>,
        params: PadParams,
        quantized: bool,
        tail: bool,
    },
    Update {
        layer: usize,
        params: PadParams,
    },
    Stop {
        layer: usize,
    },
    StartRelease {
        kind: ReleaseKind,
        params: PadParams,
    },
    StopRelease,
}

struct Layer {
    effect: Box<dyn Effect>,
    on: bool,
    pending: bool,
    tail: bool,
    gain: Smoothed,
}

const RAMP_S: f64 = 0.003_333;

impl Layer {
    fn new(effect: Box<dyn Effect>, rate: f64) -> Self {
        let tail = effect.has_tail();
        Self {
            effect,
            on: false,
            pending: false,
            tail,
            gain: Smoothed::new(0.0, RAMP_S, rate),
        }
    }

    fn switch(&mut self, on: bool) {
        self.on = on;
        self.pending = false;
        self.gain.set(if on { 1.0 } else { 0.0 });
        self.effect.set_active(on);
    }
}

struct Channel {
    layers: [Layer; LAYERS],
    release: [Layer; 3],
    quiet: [usize; LAYERS + 3],
}

pub struct PadHost {
    channels: [Channel; CHANNELS],
    input: Box<[f32]>,
    sent: Box<[f32]>,
    idle_frames: usize,
}

impl std::fmt::Debug for PadHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PadHost").finish_non_exhaustive()
    }
}

const IDLE_S: f64 = 10.0;
const SILENCE: f32 = 1e-5;

impl PadHost {
    pub fn new(sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate);
        Self {
            channels: std::array::from_fn(|_| Channel {
                layers: std::array::from_fn(|_| {
                    Layer::new(create(EffectKind::None, sample_rate), rate)
                }),
                release: RELEASE_KINDS.map(|k| Layer::new(create(k, sample_rate), rate)),
                quiet: [usize::MAX / 2; LAYERS + 3],
            }),
            input: vec![0.0; 2 * MAX_BLOCK_FRAMES].into_boxed_slice(),
            sent: vec![0.0; 2 * MAX_BLOCK_FRAMES].into_boxed_slice(),
            idle_frames: (IDLE_S * rate) as usize,
        }
    }

    pub fn handle(&mut self, channel: usize, command: PadCommand) -> Option<Box<dyn Effect>> {
        let Some(ch) = self.channels.get_mut(channel) else {
            return match command {
                PadCommand::Start { effect, .. } => effect,
                _ => None,
            };
        };
        match command {
            PadCommand::Start {
                layer,
                effect,
                params,
                quantized,
                tail,
            } if layer < LAYERS => {
                let l = &mut ch.layers[layer];
                let old = effect.map(|e| std::mem::replace(&mut l.effect, e));
                l.effect.set_pad(&params);
                l.tail = tail && l.effect.has_tail();
                if quantized {
                    l.pending = true;
                } else {
                    l.switch(true);
                }
                ch.quiet[layer] = 0;
                old
            }
            PadCommand::Start { effect, .. } => effect,
            PadCommand::Update { layer, params } => {
                if let Some(l) = ch.layers.get_mut(layer) {
                    l.effect.set_pad(&params);
                }
                None
            }
            PadCommand::Stop { layer } => {
                if let Some(l) = ch.layers.get_mut(layer) {
                    l.switch(false);
                }
                None
            }
            PadCommand::StartRelease { kind, params } => {
                let i = kind as usize;
                for (n, l) in ch.release.iter_mut().enumerate() {
                    if n != i && l.on {
                        l.switch(false);
                    }
                }
                let l = &mut ch.release[i];
                l.effect.set_pad(&params);
                l.switch(true);
                ch.quiet[LAYERS + i] = 0;
                None
            }
            PadCommand::StopRelease => {
                for l in &mut ch.release {
                    if l.on {
                        l.switch(false);
                    }
                }
                None
            }
        }
    }

    pub fn on(&self, channel: usize, layer: usize) -> bool {
        self.channels.get(channel).is_some_and(|ch| {
            if layer < LAYERS {
                ch.layers[layer].on
            } else {
                ch.release.iter().any(|l| l.on)
            }
        })
    }

    pub fn process(&mut self, channel: usize, buf: &mut [f32], ctx: &Context) {
        let Some(ch) = self.channels.get_mut(channel) else {
            return;
        };
        let n = buf.len().min(2 * MAX_BLOCK_FRAMES);
        let buf = &mut buf[..n];
        let start = on_grid(ctx.beat);
        let idle = self.idle_frames;
        let layers = ch.layers.iter_mut().chain(ch.release.iter_mut());
        for (l, quiet) in layers.zip(ch.quiet.iter_mut()) {
            if l.pending && start {
                l.switch(true);
            }
            let listens = l.effect.listens();
            if !l.on && *quiet >= idle && !listens {
                continue;
            }
            let peak = run(l, buf, &mut self.input[..n], &mut self.sent[..n], ctx);
            *quiet = if l.on || peak >= SILENCE {
                0
            } else {
                quiet.saturating_add(n / 2)
            };
        }
    }
}

fn on_grid(beat: f64) -> bool {
    let f = beat.rem_euclid(1.0);
    f < 0.25 || (0.5..0.75).contains(&f)
}

fn run(l: &mut Layer, buf: &mut [f32], input: &mut [f32], sent: &mut [f32], ctx: &Context) -> f32 {
    input.copy_from_slice(buf);
    let mut peak = 0.0f32;
    if l.tail {
        for (o, i) in sent
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(input.as_chunks::<2>().0.iter())
        {
            let g = l.gain.tick();
            o[0] = i[0] * g;
            o[1] = i[1] * g;
        }
        buf.copy_from_slice(sent);
        l.effect.process(buf, ctx);
        for ((o, s), i) in buf
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(sent.as_chunks::<2>().0.iter())
            .zip(input.as_chunks::<2>().0.iter())
        {
            let tail = [o[0] - s[0], o[1] - s[1]];
            peak = peak.max(tail[0].abs()).max(tail[1].abs());
            o[0] = i[0] + tail[0];
            o[1] = i[1] + tail[1];
        }
    } else {
        l.effect.process(buf, ctx);
        for (o, i) in buf
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(input.as_chunks::<2>().0.iter())
        {
            let g = l.gain.tick();
            o[0] = i[0] + (o[0] - i[0]) * g;
            o[1] = i[1] + (o[1] - i[1]) * g;
        }
        if l.gain.value() > 0.0 {
            peak = 1.0;
        }
    }
    peak
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn ctx(beat: f64, frames: usize) -> Context {
        let bpm = 120.0;
        Context {
            bpm,
            beat,
            beat_end: beat + frames as f64 * bpm / 60.0 / f64::from(RATE),
        }
    }

    fn params() -> PadParams {
        PadParams {
            beats: 0.5,
            level: 1.0,
            room: 0.5,
            color: 0.5,
        }
    }

    fn echo() -> Box<dyn Effect> {
        let mut e = create(EffectKind::Echo, RATE);
        e.set_pad(&params());
        e
    }

    #[test]
    fn every_factory_pad_has_its_effect_but_slip_loop() {
        for bank in 0..2 {
            for pad in 0..16 {
                let p = crate::pad_fx::factory(bank, pad).unwrap();
                let playable = effect(&p, RATE).is_some() || release_kind(p.kind).is_some();
                assert_eq!(playable, plays(p.kind));
                assert_eq!(
                    playable,
                    p.kind != PadFxType::SlipLoop,
                    "{bank} {pad} {p:?}"
                );
            }
        }
    }

    #[test]
    fn a_quantized_start_waits_for_the_next_beat() {
        let mut host = PadHost::new(RATE);
        let old = host.handle(
            0,
            PadCommand::Start {
                layer: 1,
                effect: Some(echo()),
                params: params(),
                quantized: true,
                tail: true,
            },
        );
        assert_eq!(old.map(|e| e.kind()), Some(EffectKind::None));
        let mut buf = vec![0.1; 2 * 480];
        host.process(0, &mut buf, &ctx(0.3, 480));
        assert!(!host.on(0, 1));
        host.process(0, &mut buf, &ctx(0.8, 480));
        assert!(!host.on(0, 1));
        host.process(0, &mut buf, &ctx(1.05, 480));
        assert!(host.on(0, 1));
        assert!(on_grid(0.6) && !on_grid(0.4) && on_grid(2.1));
    }

    #[test]
    fn an_immediate_start_and_stop_and_the_release_layer() {
        let mut host = PadHost::new(RATE);
        host.handle(
            2,
            PadCommand::Start {
                layer: 0,
                effect: Some(echo()),
                params: params(),
                quantized: false,
                tail: true,
            },
        );
        assert!(host.on(2, 0));
        host.handle(2, PadCommand::Stop { layer: 0 });
        assert!(!host.on(2, 0));
        host.handle(
            2,
            PadCommand::StartRelease {
                kind: ReleaseKind::BackSpin,
                params: params(),
            },
        );
        assert!(host.on(2, LAYERS));
        host.handle(2, PadCommand::StopRelease);
        assert!(!host.on(2, LAYERS));
    }

    #[test]
    fn a_running_echo_takes_new_settings_without_stopping() {
        let mut host = PadHost::new(RATE);
        host.handle(
            0,
            PadCommand::Start {
                layer: 0,
                effect: Some(echo()),
                params: params(),
                quantized: false,
                tail: true,
            },
        );
        let mut buf = vec![0.25; 2 * 480];
        host.process(0, &mut buf, &ctx(0.0, 480));
        host.handle(
            0,
            PadCommand::Update {
                layer: 0,
                params: PadParams {
                    beats: 0.25,
                    ..params()
                },
            },
        );
        assert!(host.on(0, 0));
    }
}
