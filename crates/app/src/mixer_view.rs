use backspn_control::{Action, Trigger};
use backspn_engine::{
    Assign, Band, ChannelState, Command, EngineState, FxCommand, MAX_KEY_ADJUST, MIXER_FX_SLOTS,
    MixerCommand, MixerFx, MixerState,
};
use eframe::egui::{self, Color32, RichText, Slider, Ui};

use crate::i18n::tr;
use crate::theme;
use crate::widgets::{self, meter};

pub const STRIP_WIDTH: f32 = 90.0;
pub const STRIP_GAP: f32 = 6.0;

const STRIP_CHROME: f32 = 112.0;
const CROSSFADER_ROW: f32 = 30.0;
const CARD_MARGINS: f32 = 22.0;
const MIN_FADER: f32 = 60.0;
const MAX_FADER: f32 = 520.0;
const COMPACT_BELOW: f32 = 250.0;

const COLUMN: f32 = 56.0;

pub fn width(channels: usize) -> f32 {
    channels as f32 * (STRIP_WIDTH + STRIP_GAP + 15.0) + 22.0
}

pub fn gain_db(knob: f32) -> f32 {
    let linear = 4.0 * knob.clamp(0.0, 1.0).powi(2);
    if linear > 0.0 {
        20.0 * linear.log10()
    } else {
        f32::NEG_INFINITY
    }
}

pub fn gain_knob(db: f32) -> f32 {
    (10f32.powf(db / 20.0) / 4.0).sqrt().clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy)]
struct ChannelControls {
    gain: f32,
    eq: [f32; 4],
    filter: f32,
    fader: f32,
    cue: bool,
    assign: Assign,
    kill: [bool; 4],
    filter_on: bool,
    fx: u8,
    balance: f32,
    mute: bool,
}

impl Default for ChannelControls {
    fn default() -> Self {
        let ChannelState {
            gain_db,
            eq,
            filter,
            fader,
            cue,
            assign,
            auto_gain_db: _,
            balance,
            mute,
        } = ChannelState::default();
        Self {
            gain: gain_knob(gain_db),
            eq,
            filter,
            fader,
            cue,
            assign,
            kill: [false; 4],
            filter_on: true,
            fx: 0,
            balance,
            mute,
        }
    }
}

impl ChannelControls {
    fn eq_sent(&self, band: usize) -> f32 {
        if self.kill[band] { 0.0 } else { self.eq[band] }
    }

    fn filter_sent(&self) -> f32 {
        if self.filter_on { self.filter } else { 0.0 }
    }
}

/// A press flips an on/off state; a latched switch on the controller sets it.
fn latch(current: bool, trigger: Trigger) -> Option<bool> {
    match trigger {
        Trigger::Press => Some(!current),
        Trigger::Set(v) => Some(v > 0.5),
        Trigger::Release | Trigger::Delta(_) | Trigger::Reset => None,
    }
}

pub(crate) fn level(current: f32, trigger: Trigger) -> Option<f32> {
    match trigger {
        Trigger::Set(v) => Some(v.clamp(0.0, 1.0)),
        Trigger::Delta(d) => Some((current + d * STEP).clamp(0.0, 1.0)),
        Trigger::Press | Trigger::Release | Trigger::Reset => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MixerStatus {
    pub gain: [f32; 4],
    pub eq: [[f32; 4]; 4],
    pub balance: [f32; 4],
    pub cue: [bool; 4],
    pub assign: [u8; 4],
    pub master_volume: f32,
    pub cue_mix: f32,
    pub cue_volume: f32,
    pub fader: [f32; 4],
    pub filter: [f32; 4],
    pub filter_on: [bool; 4],
    pub fx: [u8; 4],
    pub kill: [[bool; 4]; 4],
    pub mute: [bool; 4],
    pub crossfader: f32,
    pub master_balance: f32,
    pub limiter: bool,
    pub color_parameter: f32,
}

impl Default for MixerStatus {
    fn default() -> Self {
        MixerView::new().status()
    }
}

fn side_assign(current: Assign, side: Assign, on: bool) -> Assign {
    if on {
        side
    } else if current == side {
        Assign::Thru
    } else {
        current
    }
}

fn assign_code(assign: Assign) -> u8 {
    match assign {
        Assign::Thru => 0,
        Assign::A => 1,
        Assign::B => 2,
    }
}

fn assign_of(code: u8) -> Assign {
    match code {
        1 => Assign::A,
        2 => Assign::B,
        _ => Assign::Thru,
    }
}

const BANDS: [Band; 4] = [Band::Low, Band::Mid, Band::High, Band::MidLow];

pub struct MixerView {
    channels: [ChannelControls; 4],
    crossfader: f32,
    curve: f32,
    master: f32,
    master_balance: f32,
    cue_mix: f32,
    cue_volume: f32,
    limiter: bool,
    pub fx_units: usize,
    pub four_band: bool,
    chrome: [[f32; 2]; 2],
    strips_width: f32,
    mixer_fx_slots: [MixerFx; 4],
    held_select: Option<(u8, bool)>,
    held_on: [Option<bool>; 4],
    fx_dirty: [bool; 4],
    color_parameter: f32,
    parameter_dirty: bool,
    pub meter_gain: f32,
    lock: MixerLock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct MixerLock {
    pub channel: bool,
    pub eq_gain: bool,
    pub master: bool,
    pub fx: bool,
}

impl MixerLock {
    pub fn locks(self, action: Action) -> bool {
        match action {
            Action::Volume { .. } | Action::Balance { .. } | Action::Crossfader => self.channel,
            Action::Gain { .. } | Action::Eq { .. } | Action::EqKill { .. } => self.eq_gain,
            Action::MasterVolume => self.master,
            Action::Filter { .. } | Action::MixerFxOn { .. } | Action::MixerFxSelect { .. } => {
                self.fx
            }
            _ => false,
        }
    }
}

pub fn channel_meter_gain(headroom_db: f32, apply: bool) -> f32 {
    const MINUS_3_DB: f32 = 0.707_95;
    if apply {
        MINUS_3_DB * 10f32.powf(headroom_db * 0.05)
    } else {
        MINUS_3_DB
    }
}

pub const MIXER_FX_COLOURS: [Color32; 5] = [
    Color32::from_rgb(255, 170, 90),
    Color32::from_rgb(235, 70, 70),
    Color32::from_rgb(80, 200, 100),
    Color32::from_rgb(70, 130, 240),
    Color32::from_rgb(240, 210, 60),
];

impl MixerView {
    pub fn new() -> Self {
        let defaults = MixerState::default();
        let mut channels = [ChannelControls::default(); 4];
        channels[0].assign = Assign::A;
        channels[1].assign = Assign::B;
        channels[2].assign = Assign::A;
        channels[3].assign = Assign::B;
        Self {
            channels,
            crossfader: defaults.crossfader,
            curve: defaults.crossfader_curve,
            master: defaults.master_volume,
            master_balance: defaults.master_balance,
            cue_mix: defaults.cue_mix,
            cue_volume: defaults.cue_volume,
            limiter: defaults.limiter,
            fx_units: 2,
            four_band: false,
            chrome: [[STRIP_CHROME; 2], [STRIP_CHROME + CROSSFADER_ROW; 2]],
            strips_width: 0.0,
            mixer_fx_slots: MIXER_FX_SLOTS,
            held_select: None,
            held_on: [None; 4],
            fx_dirty: [false; 4],
            color_parameter: 0.5,
            parameter_dirty: false,
            meter_gain: channel_meter_gain(backspn_engine::DEFAULT_HEADROOM_DB, false),
            lock: MixerLock::default(),
        }
    }

    pub fn set_lock(&mut self, lock: MixerLock, out: &mut Vec<Command>) {
        if lock.channel && !self.lock.channel {
            let defaults = ChannelControls::default();
            for c in &mut self.channels {
                c.fader = defaults.fader;
                c.eq = defaults.eq;
                c.kill = defaults.kill;
                c.gain = defaults.gain;
                c.balance = defaults.balance;
            }
            self.crossfader = MixerState::default().crossfader;
            out.extend(self.all_commands());
        }
        self.lock = lock;
    }

    fn fx_commands(&self, i: usize, out: &mut Vec<Command>) {
        let c = &self.channels[i];
        let effect = (c.fx > 0).then(|| self.mixer_fx_slots[usize::from(c.fx - 1).min(3)]);
        let filter = if effect.is_some() {
            0.0
        } else {
            c.filter_sent()
        };
        out.push(Command::Mixer(MixerCommand::Filter(i, filter)));
        out.push(Command::Fx(FxCommand::MixerFxSelect(i, effect)));
        out.push(Command::Fx(FxCommand::MixerFxOn(i, c.filter_on)));
        out.push(Command::Fx(FxCommand::MixerFxKnob(i, c.filter)));
    }

    pub fn set_mixer_fx_slots(&mut self, slots: [MixerFx; 4], out: &mut Vec<Command>) {
        if slots != self.mixer_fx_slots {
            self.mixer_fx_slots = slots;
            for i in 0..4 {
                if self.channels[i].fx > 0 {
                    self.fx_commands(i, out);
                }
            }
        }
    }

    pub fn mixer_fx(&self) -> [(u8, bool); 4] {
        self.channels.map(|c| (c.fx, c.filter_on))
    }

    pub fn default_knobs() -> backspn_control::feedback::MixerKnobs {
        Self::new().knobs()
    }

    pub fn knobs(&self) -> backspn_control::feedback::MixerKnobs {
        backspn_control::feedback::MixerKnobs {
            fader: self.channels.map(|c| c.fader),
            gain: self.channels.map(|c| c.gain),
            eq: self.channels.map(|c| c.eq),
            kill: self.channels.map(|c| c.kill),
            filter: self.channels.map(|c| f32::midpoint(c.filter, 1.0)),
            crossfader: self.crossfader,
            master: self.master,
            cue_volume: self.cue_volume,
            cue_mix: self.cue_mix,
        }
    }

    fn fx_label(&self, i: usize) -> &'static str {
        match self.channels[i].fx {
            0 => "FLTR",
            n => self.mixer_fx_slots[usize::from(n - 1).min(3)].short(),
        }
    }

    pub fn all_commands(&self) -> Vec<Command> {
        let mut out = Vec::new();
        let mut send = |c: MixerCommand| out.push(Command::Mixer(c));
        for (i, c) in self.channels.iter().enumerate() {
            send(MixerCommand::Gain(i, gain_db(c.gain)));
            for (n, band) in BANDS.into_iter().enumerate() {
                send(MixerCommand::Eq(i, band, c.eq_sent(n)));
            }
            send(MixerCommand::Fader(i, c.fader));
            send(MixerCommand::Cue(i, c.cue));
            send(MixerCommand::Assign(i, c.assign));
            send(MixerCommand::Balance(i, c.balance));
            send(MixerCommand::Mute(i, c.mute));
        }
        send(MixerCommand::Crossfader(self.crossfader));
        send(MixerCommand::MasterBalance(self.master_balance));
        send(MixerCommand::CrossfaderCurve(self.curve));
        send(MixerCommand::MasterVolume(self.master));
        send(MixerCommand::CueMix(self.cue_mix));
        send(MixerCommand::CueVolume(self.cue_volume));
        send(MixerCommand::Limiter(self.limiter));
        for i in 0..4 {
            self.fx_commands(i, &mut out);
        }
        out.push(Command::Fx(FxCommand::MixerFxParameter(
            self.color_parameter,
        )));
        out
    }

    pub fn initial_commands(&self) -> Vec<Command> {
        self.channels
            .iter()
            .enumerate()
            .map(|(i, c)| Command::Mixer(MixerCommand::Assign(i, c.assign)))
            .collect()
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        channels: &[usize],
        (crossfader, height): (bool, f32),
        state: &EngineState,
        out: &mut Vec<Command>,
        actions: &mut Vec<(Action, Trigger)>,
    ) {
        let mut send = |c: MixerCommand| out.push(Command::Mixer(c));
        let compact = height < COMPACT_BELOW;
        let chrome = self.chrome[usize::from(crossfader)][usize::from(compact)];
        let fader = (height - chrome).clamp(MIN_FADER, MAX_FADER);
        let strips_width = self.strips_width;
        let card = theme::card(&theme::get(ui.ctx())).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height((height - CARD_MARGINS).max(0.0));
            let start = ui.cursor().top();
            let row = ui.horizontal_top(|ui| {
                let pad = ((ui.available_width() - strips_width) / 2.0).max(0.0);
                ui.add_space(pad);
                for (n, &i) in channels.iter().enumerate() {
                    self.strip(ui, i, (fader, compact), state, &mut send, actions);
                    if n + 1 < channels.len() {
                        ui.add_space(4.0);
                    }
                }
                pad
            });
            self.strips_width = row.response.rect.width() - row.inner;
            if crossfader {
                self.crossfader_row(ui, &mut send);
            }
            ui.cursor().top() - start - ui.spacing().item_spacing.y
        });
        self.chrome[usize::from(crossfader)][usize::from(compact)] =
            card.inner + CARD_MARGINS - fader;
        for i in 0..4 {
            if std::mem::take(&mut self.fx_dirty[i]) {
                self.fx_commands(i, out);
            }
        }
        if std::mem::take(&mut self.parameter_dirty) {
            out.push(Command::Fx(FxCommand::MixerFxParameter(
                self.color_parameter,
            )));
        }
    }

    pub fn natural_width(&self) -> f32 {
        if self.strips_width > 0.0 {
            self.strips_width + CARD_MARGINS
        } else {
            width(2)
        }
    }

    pub fn row_height(&self, crossfader: bool, decks: f32) -> f32 {
        let chrome = self.chrome[usize::from(crossfader)];
        let compact = decks.max(chrome[1] + MIN_FADER);
        if compact < COMPACT_BELOW {
            compact
        } else {
            compact.max(chrome[0] + MIN_FADER)
        }
    }

    fn crossfader_row(&mut self, ui: &mut Ui, send: &mut impl FnMut(MixerCommand)) {
        {
            let strips = ui.min_rect().width();
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let t = theme::get(ui.ctx());
                ui.label(RichText::new("A").strong().color(t.deck(0)));
                ui.spacing_mut().slider_width = strips - 40.0;
                let xf = ui.add_enabled(
                    !self.lock.channel,
                    Slider::new(&mut self.crossfader, 0.0..=1.0)
                        .show_value(false)
                        .trailing_fill(false),
                );
                if xf.changed() {
                    send(MixerCommand::Crossfader(self.crossfader));
                }
                if xf.double_clicked() {
                    self.crossfader = 0.5;
                    send(MixerCommand::Crossfader(0.5));
                }
                ui.label(RichText::new("B").strong().color(t.deck(1)));
            });
        }
    }

    pub fn main_volume(&mut self, ui: &mut Ui, state: &MixerState, out: &mut Vec<Command>) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let moved = ui
                .add_enabled_ui(!self.lock.master, |ui| {
                    widgets::knob(ui, &mut self.master, 0.0..=1.0, 0.8, "MAIN").changed()
                })
                .inner;
            if moved {
                out.push(Command::Mixer(MixerCommand::MasterVolume(self.master)));
            }
            meter(ui, &state.master_peak, 40.0);
        });
    }

    pub fn limiter(&mut self, ui: &mut Ui, state: &MixerState, out: &mut Vec<Command>) {
        let t = theme::get(ui.ctx());
        if widgets::toggle(ui, "LIMITER", self.limiter, t.danger)
            .on_hover_text(format!("Gain reduction: {:.1} dB", state.limiter_db))
            .clicked()
        {
            self.limiter = !self.limiter;
            out.push(Command::Mixer(MixerCommand::Limiter(self.limiter)));
        }
    }

    pub fn status(&self) -> MixerStatus {
        MixerStatus {
            gain: self.channels.map(|c| c.gain),
            eq: self.channels.map(|c| c.eq),
            balance: self.channels.map(|c| c.balance),
            cue: self.channels.map(|c| c.cue),
            assign: self.channels.map(|c| assign_code(c.assign)),
            master_volume: self.master,
            cue_mix: self.cue_mix,
            cue_volume: self.cue_volume,
            fader: self.channels.map(|c| c.fader),
            filter: self.channels.map(|c| c.filter),
            filter_on: self.channels.map(|c| c.filter_on),
            fx: self.channels.map(|c| c.fx),
            kill: self.channels.map(|c| c.kill),
            mute: self.channels.map(|c| c.mute),
            crossfader: self.crossfader,
            master_balance: self.master_balance,
            limiter: self.limiter,
            color_parameter: self.color_parameter,
        }
    }

    pub fn restore(&mut self, status: &MixerStatus) {
        for (i, c) in self.channels.iter_mut().enumerate() {
            c.gain = status.gain[i].clamp(0.0, 1.0);
            c.eq = status.eq[i].map(|v| v.clamp(0.0, 1.0));
            c.balance = status.balance[i].clamp(-1.0, 1.0);
            c.cue = status.cue[i];
            c.assign = assign_of(status.assign[i]);
            c.fader = status.fader[i].clamp(0.0, 1.0);
            c.filter = status.filter[i].clamp(-1.0, 1.0);
            c.filter_on = status.filter_on[i];
            c.fx = status.fx[i].min(4);
            c.kill = status.kill[i];
            c.mute = status.mute[i];
        }
        self.crossfader = status.crossfader.clamp(0.0, 1.0);
        self.master_balance = status.master_balance.clamp(-1.0, 1.0);
        self.limiter = status.limiter;
        self.color_parameter = status.color_parameter.clamp(0.0, 1.0);
        self.master = status.master_volume.clamp(0.0, 1.0);
        self.cue_mix = status.cue_mix.clamp(0.0, 1.0);
        self.cue_volume = status.cue_volume.clamp(0.0, 1.0);
    }

    pub fn crossfader(&self) -> f32 {
        self.crossfader
    }

    pub fn curve(&self) -> f32 {
        self.curve
    }

    pub fn set_curve(&mut self, curve: f32, out: &mut Vec<Command>) {
        self.curve = curve.clamp(0.0, 1.0);
        out.push(Command::Mixer(MixerCommand::CrossfaderCurve(self.curve)));
    }

    fn strip(
        &mut self,
        ui: &mut Ui,
        i: usize,
        (fader, compact): (f32, bool),
        state: &EngineState,
        send: &mut impl FnMut(MixerCommand),
        actions: &mut Vec<(Action, Trigger)>,
    ) {
        let t = theme::get(ui.ctx());
        let right = i % 2 == 1;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if right {
                    self.inner_column(ui, i, (fader, compact, right), state, send);
                    self.outer_column(ui, i, compact, state, send, actions);
                } else {
                    self.outer_column(ui, i, compact, state, send, actions);
                    self.inner_column(ui, i, (fader, compact, right), state, send);
                }
            });
            let c = &mut self.channels[i];
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.add_space(6.0);
                for (assign, label) in [
                    (Assign::A, " A "),
                    (Assign::Thru, " · "),
                    (Assign::B, " B "),
                ] {
                    if widgets::toggle(ui, label, c.assign == assign, t.text_dim)
                        .on_hover_text(tr("Crossfader side"))
                        .clicked()
                    {
                        c.assign = assign;
                        send(MixerCommand::Assign(i, assign));
                    }
                }
            });
        });
    }

    fn outer_column(
        &mut self,
        ui: &mut Ui,
        i: usize,
        compact: bool,
        state: &EngineState,
        send: &mut impl FnMut(MixerCommand),
        actions: &mut Vec<(Action, Trigger)>,
    ) {
        let t = theme::get(ui.ctx());
        let knob = if compact {
            widgets::knob_compact
        } else {
            widgets::knob
        };
        let label = self.fx_label(i);
        let fx_units = self.fx_units;
        let mut fx_changed = false;
        let color = self.channels[i].fx > 0
            && self.mixer_fx_slots[usize::from(self.channels[i].fx - 1).min(3)].is_color();
        let parameter = &mut self.color_parameter;
        let parameter_dirty = &mut self.parameter_dirty;
        let lock = self.lock;
        let c = &mut self.channels[i];
        let fx = usize::from(c.fx.min(4));
        ui.vertical(|ui| {
            ui.set_width(COLUMN);
            ui.spacing_mut().item_spacing.y = 0.0;
            let gain = c.gain;
            let (knob_moved, dot_clicked) = knob_dot(
                ui,
                (compact, false),
                (&mut c.gain, 0.0..=1.0, 0.5, "GAIN"),
                (c.cue, t.sync, "Headphone cue"),
            );
            if knob_moved && lock.eq_gain {
                c.gain = gain;
            } else if knob_moved {
                send(MixerCommand::Gain(i, gain_db(c.gain)));
            }
            if dot_clicked {
                c.cue = !c.cue;
                send(MixerCommand::Cue(i, c.cue));
            }
            let mut param_knob = |ui: &mut Ui| {
                if color
                    && knob(ui, parameter, 0.0..=1.0, 0.5, "PARAM")
                        .on_hover_text(tr("Color FX parameter"))
                        .changed()
                {
                    *parameter_dirty = true;
                }
            };
            let (knob_moved, dot_clicked, picked) = ui
                .add_enabled_ui(!lock.fx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let (r, picked) = fx_knob(
                            ui,
                            compact,
                            &mut c.filter,
                            label,
                            (c.filter_on, MIXER_FX_COLOURS[fx]),
                        );
                        param_knob(ui);
                        (r.0, r.1, picked)
                    })
                    .inner
                })
                .inner;
            if dot_clicked {
                c.filter_on = !c.filter_on;
            }
            if picked {
                c.fx = (c.fx + 1) % 5;
            }
            fx_changed = knob_moved || dot_clicked || picked;
            ui.add_space(4.0);
            let assigned: Vec<bool> = (0..fx_units).map(|u| state.fx.assign[i][u]).collect();
            ui.horizontal(|ui| {
                ui.add_space((COLUMN - widgets::TOGGLE_GRID_WIDTH) / 2.0);
                if let Some(unit) =
                    widgets::toggle_grid(ui, &assigned, t.cue, "Route through FX unit")
                {
                    actions.push((Action::FxAssign { channel: i, unit }, Trigger::Press));
                }
            });
            ui.vertical_centered(|ui| widgets::caption(ui, "FX"));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let deck = state.decks.get(i);
                let mut key = deck.map_or(0.0, |d| d.key_adjust / MAX_KEY_ADJUST);
                if knob(ui, &mut key, -1.0..=1.0, 0.0, "KEY").changed() {
                    let set = Trigger::Set(f32::midpoint(key, 1.0));
                    actions.push((Action::KeyAdjust { deck: i }, set));
                }
                let keylock = deck.is_some_and(|d| d.keylock);
                if widgets::dot(ui, keylock, t.sync, "Keylock").clicked() {
                    actions.push((Action::Keylock { deck: i }, Trigger::Press));
                }
            });
        });
        self.fx_dirty[i] |= fx_changed;
    }

    fn inner_column(
        &mut self,
        ui: &mut Ui,
        i: usize,
        (fader, compact, right): (f32, bool, bool),
        state: &EngineState,
        send: &mut impl FnMut(MixerCommand),
    ) {
        let t = theme::get(ui.ctx());
        let lock = self.lock;
        let c = &mut self.channels[i];
        ui.vertical(|ui| {
            ui.set_width(COLUMN);
            ui.spacing_mut().item_spacing.y = 0.0;
            let bands: &[(Band, &str)] = if self.four_band {
                &[
                    (Band::High, "HI"),
                    (Band::Mid, "MID"),
                    (Band::MidLow, "MID LO"),
                    (Band::Low, "LO"),
                ]
            } else {
                &[(Band::High, "HI"), (Band::Mid, "MID"), (Band::Low, "LO")]
            };
            for &(band, label) in bands {
                let n = band as usize;
                let hint = format!("Kill {label}");
                let (knob_moved, dot_clicked) = ui
                    .add_enabled_ui(!lock.eq_gain, |ui| {
                        knob_dot(
                            ui,
                            (compact, false),
                            (&mut c.eq[n], 0.0..=1.0, 0.5, label),
                            (c.kill[n], t.danger, &hint),
                        )
                    })
                    .inner;
                if dot_clicked {
                    c.kill[n] = !c.kill[n];
                }
                if knob_moved || dot_clicked {
                    send(MixerCommand::Eq(i, band, c.eq_sent(n)));
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add_space(6.0);
                let level = [state.mixer.channel_peaks[i] * self.meter_gain];
                if right {
                    meter(ui, &level, fader);
                }
                if ui
                    .add_enabled_ui(!lock.channel, |ui| {
                        widgets::fader(ui, &mut c.fader, fader, 1.0).changed()
                    })
                    .inner
                {
                    send(MixerCommand::Fader(i, c.fader));
                }
                if !right {
                    meter(ui, &level, fader);
                }
            });
        });
    }
}

fn fx_knob(
    ui: &mut Ui,
    compact: bool,
    value: &mut f32,
    label: &str,
    (on, colour): (bool, eframe::egui::Color32),
) -> ((bool, bool), bool) {
    let caption = if compact { 7.5 } else { 9.0 };
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let r = knob_dot(
            ui,
            (compact, true),
            (value, -1.0..=1.0, 0.0, label),
            (on, colour, "Filter / mixer FX on / off"),
        );
        let pick = ui
            .add_sized(
                [widgets::KNOB_WIDTH, caption + 3.0],
                egui::Label::new(RichText::new(label).size(caption).color(colour))
                    .sense(egui::Sense::click()),
            )
            .on_hover_text(tr("Filter or mixer FX 1–4 (click to change)"));
        (r, pick.clicked())
    })
    .inner
}

fn knob_dot(
    ui: &mut Ui,
    (compact, captioned): (bool, bool),
    (value, range, default, label): (&mut f32, std::ops::RangeInclusive<f32>, f32, &str),
    (on, color, hint): (bool, eframe::egui::Color32, &str),
) -> (bool, bool) {
    let knob = if captioned {
        widgets::knob_bare
    } else if compact {
        widgets::knob_compact
    } else {
        widgets::knob
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let moved = knob(ui, value, range, default, label).changed();
        (moved, widgets::dot(ui, on, color, hint).clicked())
    })
    .inner
}

const STEP: f32 = backspn_control::RELATIVE_STEP;

impl MixerView {
    fn apply_mixer_fx(&mut self, action: Action, trigger: Trigger, out: &mut Vec<Command>) -> bool {
        match action {
            Action::MixerFxOn { channel } if channel < 4 => match trigger {
                Trigger::Press => {
                    let picked = self.held_select.as_mut().map(|(slot, taken)| {
                        *taken = true;
                        self.channels[channel].fx = *slot;
                    });
                    self.held_on[channel] = Some(picked.is_some());
                    if picked.is_some() {
                        self.fx_commands(channel, out);
                    }
                }
                Trigger::Release => {
                    let picked = self.held_on[channel].take();
                    if picked == Some(false) {
                        self.channels[channel].filter_on = !self.channels[channel].filter_on;
                        self.fx_commands(channel, out);
                    }
                }
                // A latched switch on the controller says where it is.
                Trigger::Set(v) => {
                    self.channels[channel].filter_on = v > 0.5;
                    self.fx_commands(channel, out);
                }
                _ => {}
            },
            Action::MixerFxSelect { slot } if slot <= 4 => {
                let slot = slot as u8;
                match trigger {
                    Trigger::Press => {
                        let held: Vec<usize> =
                            (0..4).filter(|&i| self.held_on[i].is_some()).collect();
                        for &i in &held {
                            self.held_on[i] = Some(true);
                            self.channels[i].fx = slot;
                            self.fx_commands(i, out);
                        }
                        self.held_select = Some((slot, !held.is_empty()));
                    }
                    Trigger::Release => {
                        if let Some((held, taken)) = self.held_select.take()
                            && held == slot
                            && !taken
                        {
                            for i in 0..4 {
                                self.channels[i].fx = slot;
                                self.fx_commands(i, out);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => return false,
        }
        true
    }

    fn apply_balance_mute(
        &mut self,
        action: Action,
        trigger: Trigger,
        out: &mut Vec<Command>,
    ) -> bool {
        let mut send = |c: MixerCommand| out.push(Command::Mixer(c));
        match action {
            Action::Balance { channel } if channel < 4 => {
                let knob = f32::midpoint(self.channels[channel].balance, 1.0);
                if let Some(v) = level(knob, trigger) {
                    self.channels[channel].balance = v * 2.0 - 1.0;
                    send(MixerCommand::Balance(channel, v * 2.0 - 1.0));
                }
            }
            Action::MasterBalance => {
                if let Some(v) = level(f32::midpoint(self.master_balance, 1.0), trigger) {
                    self.master_balance = v * 2.0 - 1.0;
                    send(MixerCommand::MasterBalance(self.master_balance));
                }
            }
            Action::Mute { channel } if channel < 4 && trigger == Trigger::Press => {
                self.channels[channel].mute = !self.channels[channel].mute;
                send(MixerCommand::Mute(channel, self.channels[channel].mute));
            }
            _ => return false,
        }
        true
    }

    /// A press flips the band's cut; a switch sets it.
    fn apply_kill(&mut self, action: Action, trigger: Trigger, out: &mut Vec<Command>) -> bool {
        let Action::EqKill { channel, band } = action else {
            return false;
        };
        let Some(c) = self.channels.get_mut(channel).filter(|_| band < 4) else {
            return false;
        };
        let kill = match trigger {
            Trigger::Press => !c.kill[band],
            Trigger::Set(v) => v > 0.5,
            _ => return true,
        };
        c.kill[band] = kill;
        out.push(Command::Mixer(MixerCommand::Eq(
            channel,
            BANDS[band],
            c.eq_sent(band),
        )));
        true
    }

    pub fn apply(&mut self, action: Action, trigger: Trigger, out: &mut Vec<Command>) -> bool {
        self.lock.locks(action)
            || self.apply_kill(action, trigger, out)
            || self.apply_unlocked(action, trigger, out)
    }

    fn apply_unlocked(&mut self, action: Action, trigger: Trigger, out: &mut Vec<Command>) -> bool {
        if self.apply_mixer_fx(action, trigger, out)
            || self.apply_balance_mute(action, trigger, out)
        {
            return true;
        }
        let mut send = |c: MixerCommand| out.push(Command::Mixer(c));
        match action {
            Action::Volume { channel } if channel < 4 => {
                if let Some(v) = level(self.channels[channel].fader, trigger) {
                    self.channels[channel].fader = v;
                    send(MixerCommand::Fader(channel, v));
                }
            }
            Action::Gain { channel } if channel < 4 => {
                if let Some(v) = level(self.channels[channel].gain, trigger) {
                    self.channels[channel].gain = v;
                    send(MixerCommand::Gain(channel, gain_db(v)));
                }
            }
            Action::Eq { channel, band } if channel < 4 && band < 4 => {
                if let Some(v) = level(self.channels[channel].eq[band], trigger) {
                    self.channels[channel].eq[band] = v;
                    let sent = self.channels[channel].eq_sent(band);
                    let band = BANDS[band];
                    send(MixerCommand::Eq(channel, band, sent));
                }
            }
            Action::Filter { channel } if channel < 4 => {
                let knob = f32::midpoint(self.channels[channel].filter, 1.0);
                if let Some(v) = level(knob, trigger) {
                    self.channels[channel].filter = v * 2.0 - 1.0;
                    self.fx_commands(channel, out);
                }
            }
            Action::MonitorCue { channel } if channel < 4 => {
                if let Some(on) = latch(self.channels[channel].cue, trigger) {
                    self.channels[channel].cue = on;
                    send(MixerCommand::Cue(channel, on));
                }
            }
            Action::Crossfader => {
                if let Some(v) = level(self.crossfader, trigger) {
                    self.crossfader = v;
                    send(MixerCommand::Crossfader(v));
                }
            }
            Action::XfaderAssign { channel } if channel < 4 => {
                if let Trigger::Set(v) = trigger {
                    let assign = match v {
                        v if v < 0.25 => Assign::B,
                        v if v < 0.75 => Assign::Thru,
                        _ => Assign::A,
                    };
                    self.channels[channel].assign = assign;
                    send(MixerCommand::Assign(channel, assign));
                }
            }
            Action::XfaderSide { channel, right } if channel < 4 => {
                let side = if right { Assign::B } else { Assign::A };
                let current = self.channels[channel].assign;
                let on = match trigger {
                    Trigger::Press => current != side,
                    Trigger::Set(v) => v > 0.5,
                    _ => return true,
                };
                let assign = side_assign(current, side, on);
                self.channels[channel].assign = assign;
                send(MixerCommand::Assign(channel, assign));
            }
            Action::XfaderCurve => {
                if let Trigger::Set(v) = trigger {
                    self.curve = match v {
                        v if v < 0.25 => 1.0,
                        v if v < 0.75 => 0.0,
                        _ => 0.5,
                    };
                    send(MixerCommand::CrossfaderCurve(self.curve));
                }
            }
            Action::MasterVolume => {
                if let Some(v) = level(self.master, trigger) {
                    self.master = v;
                    send(MixerCommand::MasterVolume(v));
                }
            }
            Action::CueVolume => {
                if let Some(v) = level(self.cue_volume, trigger) {
                    self.cue_volume = v;
                    send(MixerCommand::CueVolume(v));
                }
            }
            Action::CueMix => {
                if let Some(v) = level(self.cue_mix, trigger) {
                    self.cue_mix = v;
                    send(MixerCommand::CueMix(v));
                }
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_external_mixer_resets_and_locks_the_controls_it_takes_over() {
        let mut m = MixerView::new();
        let mut out = Vec::new();
        m.apply(Action::Volume { channel: 0 }, Trigger::Set(0.2), &mut out);
        m.apply(Action::Crossfader, Trigger::Set(0.1), &mut out);
        m.apply(Action::MasterVolume, Trigger::Set(0.3), &mut out);
        out.clear();
        let lock = MixerLock {
            channel: true,
            eq_gain: true,
            master: true,
            fx: false,
        };
        m.set_lock(lock, &mut out);
        assert!((m.channels[0].fader - 1.0).abs() < f32::EPSILON);
        assert!((m.crossfader - 0.5).abs() < f32::EPSILON);
        assert!((m.master - 0.3).abs() < f32::EPSILON);
        assert!(!out.is_empty());
        out.clear();
        for action in [
            Action::Volume { channel: 0 },
            Action::Eq {
                channel: 0,
                band: 0,
            },
            Action::Gain { channel: 0 },
            Action::MasterVolume,
            Action::Crossfader,
        ] {
            assert!(m.apply(action, Trigger::Set(0.0), &mut out));
        }
        assert!(out.is_empty(), "{out:?}");
        m.apply(Action::Filter { channel: 0 }, Trigger::Set(0.9), &mut out);
        assert!(!out.is_empty());
    }

    #[test]
    fn channel_meters_show_minus_3_db_and_the_headroom_when_asked() {
        assert!((channel_meter_gain(-3.0, false) - 0.707_95).abs() < 1e-5);
        assert!((channel_meter_gain(-3.0, true) - 0.501_2).abs() < 1e-3);
        assert!((channel_meter_gain(0.0, true) - 0.707_95).abs() < 1e-5);
    }

    fn mixer(out: &[Command]) -> Vec<MixerCommand> {
        out.iter()
            .filter_map(|c| match c {
                Command::Mixer(m) => Some(*m),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn knobs_for_out_mappings_are_where_the_controls_set_them() {
        let mut m = MixerView::new();
        let mut out = Vec::new();
        m.apply(Action::Volume { channel: 2 }, Trigger::Set(0.4), &mut out);
        m.apply(Action::Filter { channel: 1 }, Trigger::Set(0.8), &mut out);
        m.apply(Action::Crossfader, Trigger::Set(0.1), &mut out);
        let k = m.knobs();
        assert!((k.fader[2] - 0.4).abs() < 1e-6 && (k.crossfader - 0.1).abs() < 1e-6);
        assert!((k.filter[1] - 0.8).abs() < 1e-6 && (k.filter[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn faders_and_knobs_set_and_step() {
        let mut m = MixerView::new();
        let mut out = Vec::new();
        assert!(m.apply(Action::Volume { channel: 1 }, Trigger::Set(0.25), &mut out));
        assert!(m.apply(
            Action::Volume { channel: 1 },
            Trigger::Delta(16.0),
            &mut out
        ));
        assert_eq!(
            mixer(&out),
            [MixerCommand::Fader(1, 0.25), MixerCommand::Fader(1, 0.5)]
        );
        out.clear();
        m.apply(Action::Filter { channel: 0 }, Trigger::Set(1.0), &mut out);
        m.apply(Action::Gain { channel: 0 }, Trigger::Set(0.5), &mut out);
        assert_eq!(
            mixer(&out),
            [MixerCommand::Filter(0, 1.0), MixerCommand::Gain(0, 0.0)]
        );
    }

    #[test]
    fn gain_knob_follows_its_curve() {
        assert!(gain_db(0.5).abs() < 1e-5, "centre: 0 dB");
        assert!((gain_db(1.0) - 12.04).abs() < 0.01, "top: ×4");
        assert!((gain_db(0.25) + 12.04).abs() < 0.01, "a quarter: ÷4");
        assert_eq!(gain_db(0.0), f32::NEG_INFINITY, "bottom: silence");
        for db in [-12.0, 0.0, 6.0] {
            assert!((gain_db(gain_knob(db)) - db).abs() < 1e-4);
        }
    }

    #[test]
    fn mixer_fx_select_buttons() {
        let mut m = MixerView::new();
        let mut out = Vec::new();
        let select = |slot| Action::MixerFxSelect { slot };
        m.apply(select(2), Trigger::Press, &mut out);
        m.apply(select(2), Trigger::Release, &mut out);
        assert!(m.mixer_fx().iter().all(|&(fx, _)| fx == 2));
        m.apply(select(4), Trigger::Press, &mut out);
        m.apply(Action::MixerFxOn { channel: 1 }, Trigger::Press, &mut out);
        m.apply(Action::MixerFxOn { channel: 1 }, Trigger::Release, &mut out);
        m.apply(select(4), Trigger::Release, &mut out);
        assert_eq!(m.mixer_fx().map(|(fx, _)| fx), [2, 4, 2, 2]);
        let on = m.mixer_fx()[2].1;
        m.apply(Action::MixerFxOn { channel: 2 }, Trigger::Press, &mut out);
        m.apply(select(1), Trigger::Press, &mut out);
        m.apply(select(1), Trigger::Release, &mut out);
        m.apply(Action::MixerFxOn { channel: 2 }, Trigger::Release, &mut out);
        assert_eq!(m.mixer_fx().map(|(fx, _)| fx), [2, 4, 1, 2]);
        assert_eq!(m.mixer_fx()[2].1, on);
        assert!(m.mixer_fx()[1].1, "picking with it held: no switch");
        let on = m.mixer_fx()[0].1;
        m.apply(Action::MixerFxOn { channel: 0 }, Trigger::Press, &mut out);
        assert_eq!(m.mixer_fx()[0].1, on);
        m.apply(Action::MixerFxOn { channel: 0 }, Trigger::Release, &mut out);
        assert_eq!(m.mixer_fx()[0].1, !on);
        out.clear();
        m.apply(Action::Filter { channel: 1 }, Trigger::Set(1.0), &mut out);
        assert!(out.iter().any(|c| matches!(
            c,
            Command::Fx(FxCommand::MixerFxSelect(1, Some(MixerFx::TimeGater)))
        )));
        assert!(
            out.iter()
                .any(|c| matches!(c, Command::Mixer(MixerCommand::Filter(1, f)) if *f == 0.0))
        );
    }

    #[test]
    fn latched_switches_set_cue_and_filter() {
        let mut m = MixerView::new();
        let mut out = Vec::new();
        m.apply(
            Action::MonitorCue { channel: 1 },
            Trigger::Set(1.0),
            &mut out,
        );
        m.apply(
            Action::MonitorCue { channel: 1 },
            Trigger::Set(1.0),
            &mut out,
        );
        m.apply(
            Action::MonitorCue { channel: 1 },
            Trigger::Set(0.0),
            &mut out,
        );
        assert_eq!(
            mixer(&out),
            [
                MixerCommand::Cue(1, true),
                MixerCommand::Cue(1, true),
                MixerCommand::Cue(1, false)
            ]
        );
        m.apply(
            Action::MixerFxOn { channel: 2 },
            Trigger::Set(0.0),
            &mut out,
        );
        assert!(!m.channels[2].filter_on);
        m.apply(
            Action::MixerFxOn { channel: 2 },
            Trigger::Set(1.0),
            &mut out,
        );
        assert!(m.channels[2].filter_on);
        m.apply(
            Action::MixerFxOn { channel: 2 },
            Trigger::Set(1.0),
            &mut out,
        );
        assert!(m.channels[2].filter_on, "a switch sets, it does not toggle");
    }

    #[test]
    fn cue_toggles_and_presses_only() {
        let mut m = MixerView::new();
        let mut out = Vec::new();
        m.apply(Action::MonitorCue { channel: 2 }, Trigger::Press, &mut out);
        m.apply(
            Action::MonitorCue { channel: 2 },
            Trigger::Release,
            &mut out,
        );
        m.apply(Action::MonitorCue { channel: 2 }, Trigger::Press, &mut out);
        assert_eq!(
            mixer(&out),
            [MixerCommand::Cue(2, true), MixerCommand::Cue(2, false)]
        );
    }

    #[test]
    fn crossfader_switches() {
        let mut m = MixerView::new();
        let mut out = Vec::new();
        for v in [0.0, 0.5, 1.0] {
            m.apply(
                Action::XfaderAssign { channel: 3 },
                Trigger::Set(v),
                &mut out,
            );
            m.apply(Action::XfaderCurve, Trigger::Set(v), &mut out);
        }
        assert_eq!(
            mixer(&out),
            [
                MixerCommand::Assign(3, Assign::B),
                MixerCommand::CrossfaderCurve(1.0),
                MixerCommand::Assign(3, Assign::Thru),
                MixerCommand::CrossfaderCurve(0.0),
                MixerCommand::Assign(3, Assign::A),
                MixerCommand::CrossfaderCurve(0.5),
            ]
        );
    }

    #[test]
    fn eq_kill_toggles_and_follows_a_switch() {
        let mut m = MixerView::new();
        let mut out = Vec::new();
        let kill = Action::EqKill {
            channel: 0,
            band: 0,
        };
        assert!(m.apply(kill, Trigger::Press, &mut out));
        assert_eq!(mixer(&out), [MixerCommand::Eq(0, Band::Low, 0.0)]);
        assert!(m.knobs().kill[0][0]);
        out.clear();
        m.apply(kill, Trigger::Set(0.0), &mut out);
        assert_eq!(mixer(&out), [MixerCommand::Eq(0, Band::Low, 0.5)]);
        assert!(!m.knobs().kill[0][0]);
        out.clear();
        m.apply(kill, Trigger::Release, &mut out);
        assert!(out.is_empty());
        assert!(
            MixerLock {
                eq_gain: true,
                ..MixerLock::default()
            }
            .locks(kill)
        );
    }

    #[test]
    fn other_actions_are_not_mixer_actions() {
        let mut out = Vec::new();
        assert!(!MixerView::new().apply(Action::PlayPause { deck: 0 }, Trigger::Press, &mut out));
        assert!(out.is_empty());
    }

    #[test]
    fn side_buttons_assign_and_release() {
        assert_eq!(side_assign(Assign::Thru, Assign::A, true), Assign::A);
        assert_eq!(side_assign(Assign::B, Assign::A, true), Assign::A);
        assert_eq!(side_assign(Assign::A, Assign::A, false), Assign::Thru);
        assert_eq!(side_assign(Assign::B, Assign::A, false), Assign::B);
    }

    #[test]
    fn all_commands_restore_every_control() {
        let mut m = MixerView::new();
        let mut out = Vec::new();
        m.apply(
            Action::Eq {
                channel: 1,
                band: 2,
            },
            Trigger::Set(0.0),
            &mut out,
        );
        let all = mixer(&m.all_commands());
        assert_eq!(all.len(), 4 * 11 + 7);
        assert!(all.contains(&MixerCommand::Eq(1, Band::High, 0.0)));
        assert!(all.contains(&MixerCommand::Assign(0, Assign::A)));
    }

    #[test]
    fn status_restores_the_controls() {
        let mut m = MixerView::new();
        let mut out = Vec::new();
        m.apply(Action::Gain { channel: 2 }, Trigger::Set(0.7), &mut out);
        m.apply(Action::Mute { channel: 1 }, Trigger::Press, &mut out);
        m.apply(Action::Balance { channel: 0 }, Trigger::Set(0.25), &mut out);
        m.color_parameter = 0.8;
        let status = m.status();
        let mut fresh = MixerView::new();
        fresh.restore(&status);
        assert_eq!(fresh.status(), status);
        assert!(
            fresh.all_commands().iter().any(|c| matches!(
                c,
                Command::Fx(FxCommand::MixerFxParameter(v)) if (*v - 0.8).abs() < 1e-6
            )),
            "the Color FX parameter goes to the engine"
        );
        assert!((status.gain[2] - 0.7).abs() < 1e-6);
        assert!((status.balance[0] + 0.5).abs() < 1e-6);
        assert_eq!(status.assign, [1, 2, 1, 2]);
    }

    #[test]
    fn status_keeps_faders_filters_crossfader_and_mute() {
        let mut m = MixerView::new();
        let mut out = Vec::new();
        m.apply(Action::Volume { channel: 3 }, Trigger::Set(0.4), &mut out);
        m.apply(Action::Filter { channel: 0 }, Trigger::Set(0.8), &mut out);
        m.apply(Action::Crossfader, Trigger::Set(0.9), &mut out);
        m.apply(Action::Mute { channel: 2 }, Trigger::Press, &mut out);
        let status = m.status();
        let mut fresh = MixerView::new();
        fresh.restore(&status);
        assert_eq!(fresh.status(), status);
        assert!((status.fader[3] - 0.4).abs() < 1e-6);
        assert!((status.filter[0] - 0.6).abs() < 1e-6);
        assert!((status.crossfader - 0.9).abs() < 1e-6);
        assert_eq!(status.mute, [false, false, true, false]);
        let old: MixerStatus = serde_json::from_str(r#"{"master_volume": 0.5}"#).unwrap();
        assert_eq!(old.fader, MixerStatus::default().fader);
    }
}
