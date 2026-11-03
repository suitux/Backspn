mod effects;
pub mod mixer_fx;
pub mod pad_fx;
pub mod pad_host;
pub mod pattern;

use backspn_dsp::{Smoothed, Svf};
use serde::{Deserialize, Serialize};

pub use effects::{EffectInfo, EffectKind, create};
pub use pattern::DrumMachine;

pub const UNITS: usize = 4;
pub const SLOTS: usize = 3;
pub const CHANNELS: usize = 4;
pub const MAX_BLOCK_FRAMES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Context {
    pub bpm: f64,
    pub beat: f64,
    pub beat_end: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadParams {
    pub beats: f64,
    pub level: f32,
    pub room: f32,
    pub color: f32,
}

pub trait Effect: Send + std::fmt::Debug {
    fn kind(&self) -> EffectKind;
    fn process(&mut self, buf: &mut [f32], ctx: &Context);
    fn set_amount(&mut self, amount: f32);
    fn set_param(&mut self, index: usize, value: f32);
    fn set_button(&mut self, index: usize, on: bool);
    fn reset(&mut self);
    fn has_tail(&self) -> bool {
        false
    }
    fn set_decay(&mut self, _decay: f32) {}
    fn set_active(&mut self, _on: bool) {}
    fn lfo_reset(&mut self) {}
    fn listens(&self) -> bool {
        false
    }
    fn set_dry_wet(&mut self, _dry_wet: f32) -> bool {
        false
    }
    fn set_pad(&mut self, pad: &PadParams) {
        self.set_amount(pad.level);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Group,
    Single,
    DrumMachine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Routing {
    #[default]
    Insert,
    PostFader,
    Send,
}

#[derive(Debug)]
pub enum Command {
    SetMode(usize, Mode),
    SetRouting(usize, Routing),
    SetOn(usize, bool),
    SetDryWet(usize, f32),
    SetEffect {
        unit: usize,
        slot: usize,
        effects: [Box<dyn Effect>; CHANNELS],
    },
    SetAmount(usize, usize, f32),
    SetSlotOn(usize, usize, bool),
    SetParam(usize, usize, f32),
    SetButton(usize, usize, bool),
    Assign {
        channel: usize,
        unit: usize,
        on: bool,
    },
    MixerFxSelect(usize, Option<mixer_fx::MixerFx>),
    MixerFxOn(usize, bool),
    MixerFxKnob(usize, f32),
    MixerFxParameter(f32),
    SetKit {
        unit: usize,
        players: [Box<DrumMachine>; CHANNELS],
    },
    PatternKnob(usize, usize, f32),
    PatternDuck(usize, bool),
    PatternSound(usize, usize),
    PatternStep(usize, usize, bool),
    PatternReset(usize),
    PatternClear(usize),
    PatternRoll(usize, bool),
    LfoReset,
    Pad(usize, pad_host::PadCommand),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SlotState {
    pub kind: EffectKind,
    pub amount: f32,
    pub on: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PatternState {
    pub knobs: [f32; 3],
    pub duck: bool,
    pub sound: usize,
    pub roll: bool,
    pub step: Option<usize>,
}

pub const PATTERN_KNOBS: [f32; 3] = [1.0, 0.5, 0.0];

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct UnitState {
    pub mode: Mode,
    pub on: bool,
    pub dry_wet: f32,
    pub slots: [SlotState; SLOTS],
    pub params: [f32; 3],
    pub buttons: [bool; 3],
    pub pattern: PatternState,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RackState {
    pub units: [UnitState; UNITS],
    pub assign: [[bool; UNITS]; CHANNELS],
    pub routing: [Routing; UNITS],
}

struct Slot {
    effects: [Box<dyn Effect>; CHANNELS],
    gain: [Smoothed; CHANNELS],
    quiet: [usize; CHANNELS],
}

struct Unit {
    slots: [Slot; SLOTS],
    wet: [Smoothed; CHANNELS],
    patterns: [Box<DrumMachine>; CHANNELS],
    split: [Svf; CHANNELS],
}

pub struct Rack {
    state: RackState,
    pads: pad_host::PadHost,
    mixer_fx: [mixer_fx::ChannelFx; CHANNELS],
    units: [Unit; UNITS],
    dry: Box<[f32]>,
    slot_in: Box<[f32]>,
    sent: Box<[f32]>,
    rest: Box<[f32]>,
    idle_frames: usize,
    rate: f64,
    block: u64,
    last_channel: usize,
    clock: Option<Context>,
}

const IDLE_S: f64 = 10.0;

const SILENCE: f32 = 1e-5;

impl std::fmt::Debug for Rack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rack")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum Retired {
    Effects([Box<dyn Effect>; CHANNELS]),
    Effect(Box<dyn Effect>),
    Patterns([Box<DrumMachine>; CHANNELS]),
}

impl Rack {
    pub fn new(sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate);
        let smooth = |v| Smoothed::new(v, 0.01, rate);
        let mut state = RackState::default();
        for unit in &mut state.units {
            unit.dry_wet = 1.0;
            unit.pattern.knobs = PATTERN_KNOBS;
        }
        Self {
            state,
            pads: pad_host::PadHost::new(sample_rate),
            mixer_fx: std::array::from_fn(|_| mixer_fx::ChannelFx::new(sample_rate)),
            units: std::array::from_fn(|_| Unit {
                slots: std::array::from_fn(|_| Slot {
                    effects: std::array::from_fn(|_| create(EffectKind::None, sample_rate)),
                    gain: [smooth(0.0); CHANNELS],
                    quiet: [usize::MAX / 2; CHANNELS],
                }),
                wet: [smooth(1.0); CHANNELS],
                patterns: std::array::from_fn(|_| Box::new(DrumMachine::new(&[], sample_rate))),
                split: [Svf::default(); CHANNELS],
            }),
            dry: vec![0.0; 2 * MAX_BLOCK_FRAMES].into_boxed_slice(),
            slot_in: vec![0.0; 2 * MAX_BLOCK_FRAMES].into_boxed_slice(),
            sent: vec![0.0; 2 * MAX_BLOCK_FRAMES].into_boxed_slice(),
            rest: vec![0.0; 2 * MAX_BLOCK_FRAMES].into_boxed_slice(),
            idle_frames: (IDLE_S * rate) as usize,
            rate,
            block: 0,
            last_channel: usize::MAX,
            clock: None,
        }
    }

    pub fn state(&self) -> RackState {
        self.state
    }

    #[allow(clippy::too_many_lines)]
    pub fn handle(&mut self, command: Command) -> Option<Retired> {
        match command {
            Command::Pad(channel, pad) => {
                return self.pads.handle(channel, pad).map(Retired::Effect);
            }
            Command::MixerFxSelect(..)
            | Command::MixerFxOn(..)
            | Command::MixerFxKnob(..)
            | Command::MixerFxParameter(_) => self.handle_mixer_fx(&command),
            Command::SetMode(u, mode) if u < UNITS => {
                self.state.units[u].mode = mode;
                self.reapply(u);
                self.update_gains(u);
            }
            Command::SetRouting(u, routing) if u < UNITS => {
                if self.state.routing[u] != routing {
                    self.state.routing[u] = routing;
                    for slot in &mut self.units[u].slots {
                        for e in &mut slot.effects {
                            e.reset();
                        }
                        slot.quiet = [usize::MAX / 2; CHANNELS];
                    }
                    self.reapply(u);
                    self.update_gains(u);
                }
            }
            Command::SetOn(u, on) if u < UNITS => {
                self.state.units[u].on = on;
                if self.state.units[u].mode == Mode::DrumMachine {
                    self.each_pattern(u, |p| p.set_on(on));
                }
                self.update_gains(u);
            }
            Command::SetDryWet(u, v) if u < UNITS => {
                let v = v.clamp(0.0, 1.0);
                self.state.units[u].dry_wet = v;
                self.each_pattern(u, |p| p.set_volume(v));
                self.update_gains(u);
            }
            Command::SetEffect {
                unit,
                slot,
                mut effects,
            } if unit < UNITS && slot < SLOTS => {
                let kind = effects[0].kind();
                self.state.units[unit].slots[slot].kind = kind;
                std::mem::swap(&mut self.units[unit].slots[slot].effects, &mut effects);
                let info = kind.info();
                self.state.units[unit].slots[slot].amount = info.neutral_amount;
                let defaults = info.defaults;
                if slot == 0 {
                    self.state.units[unit].params = defaults;
                    self.state.units[unit].buttons = [false; 3];
                }
                self.reapply(unit);
                self.update_gains(unit);
                return Some(Retired::Effects(effects));
            }
            Command::SetKit { .. }
            | Command::PatternKnob(..)
            | Command::PatternDuck(..)
            | Command::PatternSound(..)
            | Command::PatternStep(..)
            | Command::PatternReset(_)
            | Command::PatternClear(_)
            | Command::PatternRoll(..) => return self.handle_pattern(command),
            Command::SetAmount(u, s, v) if u < UNITS && s < SLOTS => {
                let v = v.clamp(0.0, 1.0);
                self.state.units[u].slots[s].amount = v;
                if self.state.units[u].mode == Mode::Group {
                    for e in &mut self.units[u].slots[s].effects {
                        e.set_amount(v);
                    }
                }
            }
            Command::SetSlotOn(u, s, on) if u < UNITS && s < SLOTS => {
                self.state.units[u].slots[s].on = on;
                self.update_gains(u);
            }
            Command::SetParam(u, i, v) if u < UNITS && i < 3 => {
                self.state.units[u].params[i] = v.clamp(0.0, 1.0);
                if self.state.units[u].mode == Mode::Single {
                    for e in &mut self.units[u].slots[0].effects {
                        e.set_param(i, v);
                    }
                }
            }
            Command::LfoReset => self.lfo_reset(),
            Command::SetButton(u, i, on) if u < UNITS && i < 3 => {
                self.state.units[u].buttons[i] = on;
                if self.state.units[u].mode == Mode::Single {
                    for e in &mut self.units[u].slots[0].effects {
                        e.set_button(i, on);
                    }
                }
            }
            Command::Assign { channel, unit, on } if channel < CHANNELS && unit < UNITS => {
                self.state.assign[channel][unit] = on;
                self.update_gains(unit);
            }
            Command::SetEffect { effects, .. } => return Some(Retired::Effects(effects)),
            _ => {}
        }
        None
    }

    fn lfo_reset(&mut self) {
        for unit in &mut self.units {
            for slot in &mut unit.slots {
                for e in &mut slot.effects {
                    e.lfo_reset();
                }
            }
        }
    }

    fn handle_mixer_fx(&mut self, command: &Command) {
        match *command {
            Command::MixerFxSelect(ch, effect) if ch < CHANNELS => self.mixer_fx[ch].select(effect),
            Command::MixerFxOn(ch, on) if ch < CHANNELS => self.mixer_fx[ch].set_on(on),
            Command::MixerFxKnob(ch, knob) if ch < CHANNELS => self.mixer_fx[ch].set_knob(knob),
            Command::MixerFxParameter(value) => {
                for fx in &mut self.mixer_fx {
                    fx.set_parameter(value);
                }
            }
            _ => {}
        }
    }

    fn handle_pattern(&mut self, command: Command) -> Option<Retired> {
        match command {
            Command::SetKit { unit, mut players } if unit < UNITS => {
                std::mem::swap(&mut self.units[unit].patterns, &mut players);
                let state = &mut self.state.units[unit].pattern;
                state.sound = 0;
                state.step = None;
                self.reapply_pattern(unit);
                return Some(Retired::Patterns(players));
            }
            Command::PatternKnob(u, i, v) if u < UNITS && i < 3 => {
                let v = v.clamp(0.0, 1.0);
                self.state.units[u].pattern.knobs[i] = v;
                self.each_pattern(u, |p| set_pattern_knob(p, i, v));
            }
            Command::PatternDuck(u, on) if u < UNITS => {
                self.state.units[u].pattern.duck = on;
                self.each_pattern(u, |p| p.set_duck(on));
            }
            Command::PatternSound(u, s) if u < UNITS => {
                if s < self.units[u].patterns[0].sounds() {
                    self.state.units[u].pattern.sound = s;
                    self.each_pattern(u, |p| p.set_sound(s));
                }
            }
            Command::PatternStep(u, step, on) if u < UNITS => {
                self.each_pattern(u, |p| p.set_step(step, on));
            }
            Command::PatternRoll(u, roll) if u < UNITS => {
                self.state.units[u].pattern.roll = roll;
                self.each_pattern(u, |p| p.set_roll(roll));
            }
            Command::PatternReset(u) if u < UNITS => {
                self.each_pattern(u, DrumMachine::reset_pattern);
            }
            Command::PatternClear(u) if u < UNITS => {
                self.each_pattern(u, DrumMachine::clear_pattern);
            }
            Command::SetKit { players, .. } => return Some(Retired::Patterns(players)),
            _ => {}
        }
        None
    }

    fn each_pattern(&mut self, u: usize, mut f: impl FnMut(&mut DrumMachine)) {
        for p in &mut self.units[u].patterns {
            f(p);
        }
    }

    fn reapply_pattern(&mut self, u: usize) {
        let state = self.state.units[u];
        let on = state.on && state.mode == Mode::DrumMachine;
        self.each_pattern(u, |p| {
            for (i, &v) in state.pattern.knobs.iter().enumerate() {
                set_pattern_knob(p, i, v);
            }
            p.set_duck(state.pattern.duck);
            p.set_roll(state.pattern.roll);
            p.set_sound(state.pattern.sound);
            p.set_volume(state.dry_wet);
            p.set_on(on);
        });
    }

    fn reapply(&mut self, u: usize) {
        let state = self.state.units[u];
        let unit = &mut self.units[u];
        match state.mode {
            Mode::Group => {
                for (slot, s) in unit.slots.iter_mut().zip(state.slots) {
                    for e in &mut slot.effects {
                        e.set_amount(s.amount);
                    }
                }
            }
            Mode::Single => {
                for e in &mut unit.slots[0].effects {
                    for i in 0..3 {
                        e.set_param(i, state.params[i]);
                        e.set_button(i, state.buttons[i]);
                    }
                }
            }
            Mode::DrumMachine => {}
        }
        self.reapply_pattern(u);
    }

    pub fn routing(&self, unit: usize) -> Routing {
        self.state.routing[unit]
    }

    pub fn assigned(&self, channel: usize, unit: usize) -> bool {
        self.state.assign[channel][unit]
    }

    fn fed(&self, u: usize, channel: usize) -> bool {
        match self.state.routing[u] {
            Routing::Send => channel == 0,
            Routing::Insert | Routing::PostFader => self.state.assign[channel][u],
        }
    }

    fn slot_active(&self, u: usize, slot: usize, channel: usize) -> bool {
        let state = &self.state.units[u];
        let selected = match state.mode {
            Mode::Group => state.slots[slot].on,
            Mode::Single => slot == 0,
            Mode::DrumMachine => false,
        };
        state.on && self.fed(u, channel) && selected && state.slots[slot].kind != EffectKind::None
    }

    fn listening(&self, u: usize, slot: usize, channel: usize) -> bool {
        let state = &self.state.units[u];
        let in_use = match state.mode {
            Mode::Group => true,
            Mode::Single => slot == 0,
            Mode::DrumMachine => false,
        };
        in_use && self.fed(u, channel) && self.units[u].slots[slot].effects[channel].listens()
    }

    fn update_gains(&mut self, u: usize) {
        let mut dry_wet = self.state.units[u].dry_wet;
        if self.state.units[u].mode == Mode::Single {
            let mut own = false;
            for e in &mut self.units[u].slots[0].effects {
                own = e.set_dry_wet(dry_wet);
            }
            if own {
                dry_wet = 1.0;
            }
        }
        for channel in 0..CHANNELS {
            self.units[u].wet[channel].set(dry_wet);
            for slot in 0..SLOTS {
                let active = self.slot_active(u, slot, channel);
                let s = &mut self.units[u].slots[slot];
                s.effects[channel].set_active(active);
                s.gain[channel].set(if active { 1.0 } else { 0.0 });
                if active {
                    s.quiet[channel] = 0;
                }
            }
        }
    }

    pub fn set_clock(&mut self, ctx: Context) {
        self.clock = Some(ctx);
    }

    pub fn process(&mut self, channel: usize, buf: &mut [f32], ctx: &Context) {
        if self.last_channel == usize::MAX || channel <= self.last_channel {
            self.block += 1;
        }
        self.last_channel = channel;
        self.process_stage(Routing::Insert, channel, buf, ctx);
        self.pads.process(channel, buf, ctx);
    }

    pub fn pad_on(&self, channel: usize, layer: usize) -> bool {
        self.pads.on(channel, layer)
    }

    pub fn process_mixer_fx(&mut self, channel: usize, buf: &mut [f32], ctx: &Context) {
        if let Some(fx) = self.mixer_fx.get_mut(channel) {
            fx.process(buf, ctx);
        }
    }

    pub fn process_post_fader(&mut self, channel: usize, buf: &mut [f32], ctx: &Context) {
        self.process_stage(Routing::PostFader, channel, buf, ctx);
    }

    pub fn process_send(&mut self, unit: usize, buf: &mut [f32], ctx: &Context) {
        if unit < UNITS && self.state.routing[unit] == Routing::Send {
            let n = buf.len().min(2 * MAX_BLOCK_FRAMES);
            self.process_unit(unit, 0, &mut buf[..n], ctx);
        }
    }

    fn process_stage(&mut self, stage: Routing, channel: usize, buf: &mut [f32], ctx: &Context) {
        if channel >= CHANNELS {
            return;
        }
        let n = buf.len().min(2 * MAX_BLOCK_FRAMES);
        for u in 0..UNITS {
            if self.state.routing[u] == stage {
                self.process_unit(u, channel, &mut buf[..n], ctx);
            }
        }
    }

    fn process_pattern(&mut self, u: usize, channel: usize, buf: &mut [f32], ctx: &Context) {
        if !self.fed(u, channel) {
            return;
        }
        let block = self.block;
        let ctx = &self.clock.unwrap_or(*ctx);
        let patterns = &mut self.units[u].patterns;
        if !patterns[channel].idle() {
            if !patterns[channel].running(block) && patterns[channel].on() {
                let cycle = (0..CHANNELS)
                    .filter(|&ch| ch != channel && patterns[ch].running(block))
                    .map(|ch| patterns[ch].cycle_at(block))
                    .next();
                patterns[channel].align(ctx.beat, cycle);
            }
            patterns[channel].process_in(block, buf, ctx);
        }
        let first = (0..CHANNELS).find(|&ch| self.fed(u, ch));
        if first == Some(channel) {
            self.state.units[u].pattern.step = self.units[u].patterns[channel].step();
        }
    }

    fn modifiers(&self, u: usize) -> (Option<f32>, Option<f32>) {
        let state = &self.state.units[u];
        let amount = |kind| {
            state
                .slots
                .iter()
                .find(|s| s.on && s.kind == kind)
                .map(|s| s.amount)
        };
        if state.mode == Mode::Group && state.on {
            (amount(EffectKind::ModFilter), amount(EffectKind::ModDecay))
        } else {
            (None, None)
        }
    }

    fn split_bands(&mut self, u: usize, channel: usize, buf: &mut [f32], amount: f32) -> bool {
        let Some((lows, freq)) = split_of(amount) else {
            return false;
        };
        let svf = &mut self.units[u].split[channel];
        svf.set(freq, std::f64::consts::FRAC_1_SQRT_2, self.rate);
        let n = buf.len();
        for (i, (x, r)) in buf.iter_mut().zip(&mut self.rest[..n]).enumerate() {
            let (low, _) = svf.tick(i % 2, *x);
            let high = *x - low;
            (*x, *r) = if lows { (low, high) } else { (high, low) };
        }
        true
    }

    fn process_unit(&mut self, u: usize, channel: usize, buf: &mut [f32], ctx: &Context) {
        if self.state.units[u].mode == Mode::DrumMachine {
            self.process_pattern(u, channel, buf, ctx);
        }
        let n = buf.len();
        let frames = n / 2;
        let live: [bool; SLOTS] = std::array::from_fn(|slot| {
            self.slot_active(u, slot, channel)
                || self.units[u].slots[slot].quiet[channel] < self.idle_frames
                || self.listening(u, slot, channel)
        });
        if !live.iter().any(|&l| l) {
            return;
        }
        self.dry[..n].copy_from_slice(buf);
        let (filter, decay) = self.modifiers(u);
        let decay = decay.map_or(0.0, |a| 2.0 * a - 1.0);
        for slot in 0..SLOTS {
            if !self.state.units[u].slots[slot].kind.is_modifier() {
                self.units[u].slots[slot].effects[channel].set_decay(decay);
            }
        }
        let split = filter.is_some_and(|a| self.split_bands(u, channel, buf, a));
        for slot in 0..SLOTS {
            if !live[slot] || self.state.units[u].slots[slot].kind.is_modifier() {
                continue;
            }
            let active = self.slot_active(u, slot, channel);
            let s = &mut self.units[u].slots[slot];
            let effect = &mut s.effects[channel];
            let gain = &mut s.gain[channel];
            self.slot_in[..n].copy_from_slice(buf);
            let mut tail_peak = 0.0f32;
            if effect.has_tail() {
                {
                    let (sent, _) = self.sent[..n].as_chunks_mut::<2>();
                    let (input, _) = self.slot_in[..n].as_chunks::<2>();
                    for (o, i) in sent.iter_mut().zip(input) {
                        let g = gain.tick();
                        *o = [i[0] * g, i[1] * g];
                    }
                }
                buf.copy_from_slice(&self.sent[..n]);
                effect.process(buf, ctx);
                let (out, _) = buf.as_chunks_mut::<2>();
                let (sent, _) = self.sent[..n].as_chunks::<2>();
                let (input, _) = self.slot_in[..n].as_chunks::<2>();
                for ((o, sn), i) in out.iter_mut().zip(sent).zip(input) {
                    let tail = [o[0] - sn[0], o[1] - sn[1]];
                    tail_peak = tail_peak.max(tail[0].abs()).max(tail[1].abs());
                    *o = [i[0] + tail[0], i[1] + tail[1]];
                }
            } else {
                effect.process(buf, ctx);
                let (out, _) = buf.as_chunks_mut::<2>();
                let (input, _) = self.slot_in[..n].as_chunks::<2>();
                for (o, i) in out.iter_mut().zip(input) {
                    let g = gain.tick();
                    o[0] = i[0] + (o[0] - i[0]) * g;
                    o[1] = i[1] + (o[1] - i[1]) * g;
                }
                if gain.value() > 0.0 {
                    tail_peak = 1.0;
                }
            }
            s.quiet[channel] = if active || tail_peak >= SILENCE {
                0
            } else {
                s.quiet[channel].saturating_add(frames)
            };
        }
        if split {
            for (x, r) in buf.iter_mut().zip(&self.rest[..n]) {
                *x += r;
            }
        }
        let wet = &mut self.units[u].wet[channel];
        let (out, _) = buf.as_chunks_mut::<2>();
        let (dry, _) = self.dry[..n].as_chunks::<2>();
        for (o, d) in out.iter_mut().zip(dry) {
            let w = wet.tick();
            o[0] = d[0] + (o[0] - d[0]) * w;
            o[1] = d[1] + (o[1] - d[1]) * w;
        }
    }
}

fn split_of(amount: f32) -> Option<(bool, f64)> {
    let d = f64::from(amount.clamp(0.0, 1.0)) - 0.5;
    if d.abs() < 0.01 {
        return None;
    }
    let t = 2.0 * d.abs();
    Some(if d > 0.0 {
        (true, 20_000.0 * (100.0f64 / 20_000.0).powf(t))
    } else {
        (false, 20.0 * (8_000.0f64 / 20.0).powf(t))
    })
}

fn set_pattern_knob(p: &mut DrumMachine, knob: usize, v: f32) {
    match knob {
        0 => p.set_decay(v),
        1 => p.set_pitch(v),
        _ => p.set_pattern(v),
    }
}

#[cfg(test)]
mod tests;
