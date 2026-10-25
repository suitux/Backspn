mod pattern;

use std::collections::HashMap;

use backspn_engine::{
    Command, DECKS, EffectKind, FX_SLOTS, FX_UNITS, FxCommand, FxMode, FxRouting, create_effect,
};
use eframe::egui::{self, ComboBox, RichText, Ui};

use backspn_control::{Action, Trigger, deck_letter};

use crate::i18n::tr;
use crate::kits::KitLibrary;
use crate::theme;
use crate::widgets::{self, knob};
pub use pattern::resolution_label;
use pattern::{PatternControls, PatternStatus};

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UnitStatus {
    controls: UnitControls,
    pattern: PatternStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct UnitControls {
    mode: FxMode,
    on: bool,
    dry_wet: f32,
    kinds: [EffectKind; FX_SLOTS],
    amounts: [f32; FX_SLOTS],
    slot_on: [bool; FX_SLOTS],
    params: [f32; 3],
    buttons: [bool; 3],
    assign: [bool; DECKS],
}

impl Default for UnitControls {
    fn default() -> Self {
        Self {
            mode: FxMode::Group,
            on: false,
            dry_wet: 1.0,
            kinds: [EffectKind::None; FX_SLOTS],
            amounts: [0.0; FX_SLOTS],
            slot_on: [false; FX_SLOTS],
            params: [0.0; 3],
            buttons: [false; 3],
            assign: [false; DECKS],
        }
    }
}

pub fn mode_name(mode: FxMode) -> &'static str {
    match mode {
        FxMode::Group => "Group",
        FxMode::Single => "Single",
        FxMode::DrumMachine => "Drum Machine",
    }
}

pub fn mode_select(
    ui: &mut Ui,
    id: impl egui::AsIdSalt,
    mode: &mut FxMode,
    width: f32,
    text_size: f32,
) -> bool {
    let before = *mode;
    ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(RichText::new(mode_name(*mode)).size(text_size))
        .show_ui(ui, |ui| {
            for m in [FxMode::Group, FxMode::Single, FxMode::DrumMachine] {
                ui.selectable_value(mode, m, mode_name(m));
            }
        });
    *mode != before
}

fn effect_matches(selection: &[EffectKind], query: &str, group: bool) -> Vec<EffectKind> {
    let query = query.trim().to_lowercase();
    let offered = selection
        .iter()
        .copied()
        .filter(|k| group || !k.is_modifier());
    if query.is_empty() {
        return std::iter::once(EffectKind::None).chain(offered).collect();
    }
    offered
        .filter(|k| k.info().name.to_lowercase().contains(&query))
        .collect()
}

const SINGLE_SELECTOR: usize = 9;

const SEARCH_LIST_MAX: f32 = 260.0;

const COMPACT_HEADER: f32 = 150.0;

#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub amount: Option<f32>,
    pub slot_on: Option<bool>,
    pub single: Option<SingleSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SingleSnapshot {
    pub params: [f32; 3],
    pub buttons: [bool; 3],
    pub dry_wet: f32,
    pub on: bool,
}

pub struct FxView {
    units: [UnitControls; FX_UNITS],
    routing: [FxRouting; FX_UNITS],
    sample_rate: u32,
    pub selection: Vec<EffectKind>,
    pub restore: bool,
    pub snapshots: HashMap<EffectKind, Snapshot>,
    pub kits: KitLibrary,
    patterns: [PatternControls; FX_UNITS],
    pub kit_choice: [String; FX_UNITS],
    pub natural_heights: [f32; 2],
    defaults: [KnobDefaults; FX_UNITS],
}

#[derive(Debug, Clone, Copy, Default)]
struct KnobDefaults {
    dry_wet: f32,
    knobs: [f32; 3],
}

impl FxView {
    pub fn new(sample_rate: u32, kits: KitLibrary) -> Self {
        let mut units = [UnitControls::default(); FX_UNITS];
        for (u, kinds) in [
            [EffectKind::Delay, EffectKind::Reverb, EffectKind::Filter],
            [
                EffectKind::Flanger,
                EffectKind::Gater,
                EffectKind::BeatChopper,
            ],
        ]
        .into_iter()
        .enumerate()
        {
            units[u].kinds = kinds;
            units[u].amounts = kinds.map(|k| k.info().neutral_amount);
            units[u].assign[u] = true;
        }
        Self {
            units,
            routing: [FxRouting::Insert; FX_UNITS],
            sample_rate,
            selection: EffectKind::ALL[1..].to_vec(),
            restore: false,
            snapshots: HashMap::new(),
            kits,
            patterns: std::array::from_fn(|_| PatternControls::new()),
            kit_choice: std::array::from_fn(|_| String::new()),
            natural_heights: [0.0; 2],
            defaults: [KnobDefaults::default(); FX_UNITS],
        }
    }

    pub fn modes(&self) -> [FxMode; FX_UNITS] {
        self.units.map(|c| c.mode)
    }

    pub fn effects(&self) -> [[EffectKind; FX_SLOTS]; FX_UNITS] {
        self.units.map(|c| c.kinds)
    }

    pub fn set_effects(
        &mut self,
        effects: &[[EffectKind; FX_SLOTS]; FX_UNITS],
        out: &mut Vec<Command>,
    ) {
        for (u, kinds) in effects.iter().enumerate() {
            for (slot, &kind) in kinds.iter().enumerate() {
                if self.units[u].kinds[slot] != kind {
                    let single = self.units[u].mode == FxMode::Single && slot == 0;
                    self.change_effect(u, (!single).then_some(slot), kind, out);
                }
            }
        }
    }

    pub fn set_kit(&mut self, u: usize, id: &str, out: &mut Vec<Command>) {
        if u >= FX_UNITS || id.is_empty() || self.kit_ids()[u] == id {
            return;
        }
        if self.units[u].mode == FxMode::DrumMachine {
            self.load_kit(u, id, out);
        } else {
            id.clone_into(&mut self.kit_choice[u]);
        }
    }

    pub fn set_mode(&mut self, u: usize, mode: FxMode, out: &mut Vec<Command>) {
        if u < FX_UNITS && self.units[u].mode != mode {
            self.units[u].mode = mode;
            out.push(Command::Fx(FxCommand::SetMode(u, mode)));
            self.ensure_kit(u, out);
        }
    }

    pub fn set_routing(&mut self, u: usize, routing: FxRouting, out: &mut Vec<Command>) {
        if u < FX_UNITS && self.routing[u] != routing {
            self.routing[u] = routing;
            out.push(Command::Fx(FxCommand::SetRouting(u, routing)));
        }
    }

    fn header(&mut self, ui: &mut Ui, u: usize, compact: bool, out: &mut Vec<Command>) {
        let t = theme::get(ui.ctx());
        let (name_size, mode_width, mode_text) = if compact {
            (13.0, 58.0, 11.0)
        } else {
            (14.0, 96.0, 12.0)
        };
        let name = ui
            .add(
                egui::Label::new(
                    RichText::new(format!("FX{}", u + 1))
                        .strong()
                        .size(name_size)
                        .color(t.text),
                )
                .sense(egui::Sense::click()),
            )
            .on_hover_text(tr("Right-click: save or load the snapshot"));
        name.context_menu(|ui| {
            if ui.button(tr("Save Snapshot")).clicked() {
                self.store_snapshot(u);
                ui.close();
            }
            if ui.button(tr("Load Snapshot")).clicked() {
                self.load_snapshot(u, out);
                ui.close();
            }
        });
        let c = &mut self.units[u];
        if widgets::toggle(ui, "ON", c.on, t.accent).clicked() {
            c.on = !c.on;
            out.push(Command::Fx(FxCommand::SetOn(u, c.on)));
        }
        if !compact {
            widgets::divider(ui, 18.0);
        }
        let mut mode = c.mode;
        if mode_select(ui, ("fx-mode", u), &mut mode, mode_width, mode_text) {
            self.set_mode(u, mode, out);
        }
    }

    fn assign_toggles(&mut self, ui: &mut Ui, u: usize, out: &mut Vec<Command>) {
        let t = theme::get(ui.ctx());
        let c = &mut self.units[u];
        for deck in 0..DECKS {
            if widgets::toggle(
                ui,
                &deck_letter(deck).to_string(),
                c.assign[deck],
                t.deck(deck),
            )
            .on_hover_text(tr("Route this deck through the unit"))
            .clicked()
            {
                c.assign[deck] = !c.assign[deck];
                out.push(Command::Fx(FxCommand::Assign {
                    channel: deck,
                    unit: u,
                    on: c.assign[deck],
                }));
            }
        }
    }

    fn dry_wet_label(&self, u: usize) -> &'static str {
        if self.routing[u] == FxRouting::Send {
            "RET"
        } else if self.modes()[u] == FxMode::DrumMachine {
            "VOL"
        } else {
            "D/W"
        }
    }

    fn assign_held(
        &mut self,
        channel: usize,
        unit: usize,
        trigger: Trigger,
        out: &mut Vec<Command>,
    ) {
        let on = match trigger {
            Trigger::Press => true,
            Trigger::Release => false,
            _ => return,
        };
        let c = &mut self.units[unit];
        if c.assign[channel] != on {
            c.assign[channel] = on;
            out.push(Command::Fx(FxCommand::Assign { channel, unit, on }));
        }
    }

    fn select_effect(&mut self, u: usize, slot: usize, trigger: Trigger, out: &mut Vec<Command>) {
        let single = match self.units[u].mode {
            FxMode::Group => false,
            FxMode::Single if slot == 0 => true,
            _ => return,
        };
        let list: Vec<EffectKind> = std::iter::once(EffectKind::None)
            .chain(self.selection.iter().copied())
            .collect();
        let Some(i) = effect_after(&list, self.units[u].kinds[slot], trigger) else {
            return;
        };
        self.change_effect(u, (!single).then_some(slot), list[i], out);
    }

    fn change_effect(
        &mut self,
        u: usize,
        slot: Option<usize>,
        kind: EffectKind,
        out: &mut Vec<Command>,
    ) {
        let index = slot.unwrap_or(0);
        let c = &mut self.units[u];
        let info = kind.info();
        if let Some(s) = slot {
            c.amounts[s] = info.neutral_amount;
        } else {
            c.params = info.defaults;
            c.buttons = [false; 3];
        }
        c.kinds[index] = kind;
        out.push(self.set_effect(u, index, kind));
        if self.restore && kind != EffectKind::None {
            match slot {
                Some(s) => self.recall_slot(u, s, out),
                None => self.recall_single(u, out),
            }
        }
    }

    pub fn store_snapshot(&mut self, u: usize) {
        let Some(c) = self.units.get(u).copied() else {
            return;
        };
        match c.mode {
            FxMode::Single if c.kinds[0] != EffectKind::None => {
                let mut buttons = c.buttons;
                if c.kinds[0].mirrored() {
                    buttons[0] = false;
                }
                self.snapshots.entry(c.kinds[0]).or_default().single = Some(SingleSnapshot {
                    params: c.params,
                    buttons,
                    dry_wet: c.dry_wet,
                    on: c.on,
                });
            }
            FxMode::Group => {
                for s in 0..FX_SLOTS {
                    if c.kinds[s] != EffectKind::None {
                        let snap = self.snapshots.entry(c.kinds[s]).or_default();
                        snap.amount = Some(c.amounts[s]);
                        snap.slot_on = Some(c.slot_on[s]);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn load_snapshot(&mut self, u: usize, out: &mut Vec<Command>) {
        match self.units.get(u).map(|c| c.mode) {
            Some(FxMode::Single) => self.recall_single(u, out),
            Some(FxMode::Group) => {
                for s in 0..FX_SLOTS {
                    self.recall_slot(u, s, out);
                }
            }
            _ => {}
        }
    }

    fn recall_single(&mut self, u: usize, out: &mut Vec<Command>) {
        let kind = self.units[u].kinds[0];
        if kind == EffectKind::None {
            return;
        }
        let snap = self.snapshots.get(&kind).and_then(|s| s.single);
        let c = &mut self.units[u];
        let mut send = |command| out.push(Command::Fx(command));
        if let Some(snap) = snap {
            c.params = snap.params;
            c.buttons = snap.buttons;
            c.dry_wet = snap.dry_wet;
            c.on = snap.on;
            self.defaults[u] = KnobDefaults {
                dry_wet: snap.dry_wet,
                knobs: snap.params,
            };
            send(FxCommand::SetDryWet(u, c.dry_wet));
            send(FxCommand::SetOn(u, c.on));
        } else {
            c.params = kind.info().defaults;
            c.buttons = [false; 3];
        }
        for i in 0..3 {
            send(FxCommand::SetParam(u, i, c.params[i]));
            send(FxCommand::SetButton(u, i, c.buttons[i]));
        }
    }

    fn recall_slot(&mut self, u: usize, s: usize, out: &mut Vec<Command>) {
        let kind = self.units[u].kinds[s];
        if kind == EffectKind::None {
            return;
        }
        let snap = self.snapshots.get(&kind).copied().unwrap_or_default();
        if let Some(amount) = snap.amount {
            self.defaults[u].knobs[s] = amount;
        }
        let c = &mut self.units[u];
        c.amounts[s] = snap.amount.unwrap_or(kind.info().neutral_amount);
        out.push(Command::Fx(FxCommand::SetAmount(u, s, c.amounts[s])));
        if let Some(on) = snap.slot_on {
            c.slot_on[s] = on;
            out.push(Command::Fx(FxCommand::SetSlotOn(u, s, on)));
        }
    }

    fn press_button(&mut self, u: usize, i: usize, out: &mut Vec<Command>) {
        let c = &mut self.units[u];
        if i == 0 && c.kinds[0].mirrored() {
            self.load_snapshot(u, out);
            return;
        }
        c.buttons[i] = !c.buttons[i];
        out.push(Command::Fx(FxCommand::SetButton(u, i, c.buttons[i])));
    }

    pub fn status(&self) -> [UnitStatus; FX_UNITS] {
        std::array::from_fn(|u| UnitStatus {
            controls: self.units[u],
            pattern: self.patterns[u].status(),
        })
    }

    pub fn restore(&mut self, status: &[UnitStatus; FX_UNITS], out: &mut Vec<Command>) {
        for (u, saved) in status.iter().enumerate() {
            self.units[u] = saved.controls;
            if saved.controls.mode == FxMode::DrumMachine {
                self.patterns[u].clear_kit();
                self.ensure_kit(u, out);
                self.patterns[u].restore(&saved.pattern);
                self.units[u].dry_wet = saved.controls.dry_wet;
            }
        }
    }

    pub fn initial_commands(&self) -> Vec<Command> {
        let mut out = Vec::new();
        for (u, unit) in self.units.iter().enumerate() {
            for (slot, &kind) in unit.kinds.iter().enumerate() {
                if kind != EffectKind::None {
                    out.push(self.set_effect(u, slot, kind));
                }
            }
            for (deck, &on) in unit.assign.iter().enumerate() {
                if on {
                    out.push(Command::Fx(FxCommand::Assign {
                        channel: deck,
                        unit: u,
                        on,
                    }));
                }
            }
        }
        out
    }

    pub fn all_commands(&mut self) -> Vec<Command> {
        let mut out = self.initial_commands();
        for (u, c) in self.units.iter().enumerate() {
            let mut send = |cmd: FxCommand| out.push(Command::Fx(cmd));
            send(FxCommand::SetRouting(u, self.routing[u]));
            send(FxCommand::SetMode(u, c.mode));
            send(FxCommand::SetOn(u, c.on));
            send(FxCommand::SetDryWet(u, c.dry_wet));
            for slot in 0..FX_SLOTS {
                send(FxCommand::SetAmount(u, slot, c.amounts[slot]));
                send(FxCommand::SetSlotOn(u, slot, c.slot_on[slot]));
            }
            for i in 0..3 {
                send(FxCommand::SetParam(u, i, c.params[i]));
                send(FxCommand::SetButton(u, i, c.buttons[i]));
            }
        }
        self.pattern_commands(&mut out);
        out
    }

    fn set_effect(&self, unit: usize, slot: usize, kind: EffectKind) -> Command {
        Command::Fx(FxCommand::SetEffect {
            unit,
            slot,
            effects: std::array::from_fn(|_| create_effect(kind, self.sample_rate)),
        })
    }

    pub fn side(&mut self, ui: &mut Ui, side: usize, units: usize, out: &mut Vec<Command>) -> f32 {
        let units = units.min(FX_UNITS);
        theme::card(&theme::get(ui.ctx()))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if units >= 4 {
                    for (n, u) in [side, side + 2].into_iter().enumerate() {
                        if n > 0 {
                            ui.separator();
                        }
                        self.compact_unit(ui, u, out);
                    }
                } else if side < units {
                    self.unit(ui, side, out);
                }
                let natural = ui.min_rect().height();
                self.natural_heights[side] = natural;
                widgets::pad_to_height(ui, self.natural_heights[1 - side]);
            })
            .response
            .rect
            .height()
    }

    fn compact_unit(&mut self, ui: &mut Ui, u: usize, out: &mut Vec<Command>) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(COMPACT_HEADER);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    self.header(ui, u, true, out);
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    self.assign_toggles(ui, u, out);
                });
            });
            let hint = if self.routing[u] == FxRouting::Send {
                "Return level"
            } else {
                "Dry / wet"
            };
            let c = &mut self.units[u];
            if widgets::knob_compact(ui, &mut c.dry_wet, 0.0..=1.0, 1.0, hint).changed() {
                out.push(Command::Fx(FxCommand::SetDryWet(u, c.dry_wet)));
            }
            let gap = ui.spacing().item_spacing.x;
            let slot_width =
                ((ui.available_width() - gap * (FX_SLOTS - 1) as f32) / FX_SLOTS as f32).max(60.0);
            match self.units[u].mode {
                FxMode::Group => {
                    for slot in 0..FX_SLOTS {
                        self.compact_slot(ui, u, slot, slot_width, out);
                    }
                }
                FxMode::Single => self.compact_single(ui, u, slot_width, out),
                FxMode::DrumMachine => {
                    let width = ui.available_width();
                    self.pattern_compact(ui, u, width, out);
                }
            }
        });
    }

    fn compact_slot(
        &mut self,
        ui: &mut Ui,
        u: usize,
        slot: usize,
        width: f32,
        out: &mut Vec<Command>,
    ) {
        ui.vertical(|ui| {
            ui.set_width(width);
            let c = &mut self.units[u];
            let kind = c.kinds[slot];
            ui.add_enabled_ui(kind != EffectKind::None, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.spacing_mut().slider_width = (width - 22.0).max(20.0);
                    let slider = ui.add(
                        egui::Slider::new(&mut c.amounts[slot], 0.0..=1.0)
                            .show_value(false)
                            .trailing_fill(true),
                    );
                    if slider.changed() {
                        out.push(Command::Fx(FxCommand::SetAmount(u, slot, c.amounts[slot])));
                    }
                    if slider.double_clicked() {
                        c.amounts[slot] = kind.info().neutral_amount;
                        out.push(Command::Fx(FxCommand::SetAmount(u, slot, c.amounts[slot])));
                    }
                    let t = theme::get(ui.ctx());
                    if widgets::dot(ui, c.slot_on[slot], t.accent, "Effect on / off").clicked() {
                        c.slot_on[slot] = !c.slot_on[slot];
                        out.push(Command::Fx(FxCommand::SetSlotOn(u, slot, c.slot_on[slot])));
                    }
                });
            });
            let current = self.units[u].kinds[slot];
            if let Some(kind) = self.effect_combo(ui, (u, slot), current, width - 8.0) {
                self.change_effect(u, Some(slot), kind, out);
            }
        });
    }

    fn compact_single(&mut self, ui: &mut Ui, u: usize, slot_width: f32, out: &mut Vec<Command>) {
        ui.vertical(|ui| {
            ui.set_width(slot_width);
            let current = self.units[u].kinds[0];
            if let Some(kind) =
                self.effect_combo(ui, (u, SINGLE_SELECTOR), current, slot_width - 8.0)
            {
                self.change_effect(u, None, kind, out);
            }
        });
        let c = &mut self.units[u];
        let info = c.kinds[0].info();
        let mut pressed = None;
        for i in 0..3 {
            if info.params[i].is_empty() && info.buttons[i].is_empty() {
                continue;
            }
            ui.vertical(|ui| {
                ui.set_width(slot_width);
                ui.spacing_mut().slider_width = (slot_width - 22.0).max(20.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if !info.params[i].is_empty()
                        && ui
                            .add(egui::Slider::new(&mut c.params[i], 0.0..=1.0).show_value(false))
                            .on_hover_text(info.params[i])
                            .changed()
                    {
                        out.push(Command::Fx(FxCommand::SetParam(u, i, c.params[i])));
                    }
                    if !info.buttons[i].is_empty() {
                        let t = theme::get(ui.ctx());
                        if widgets::dot(ui, c.buttons[i], t.accent, info.buttons[i]).clicked() {
                            pressed = Some(i);
                        }
                    }
                });
                widgets::caption(ui, info.params[i]);
            });
        }
        if let Some(i) = pressed {
            self.press_button(u, i, out);
        }
    }

    fn unit(&mut self, ui: &mut Ui, u: usize, out: &mut Vec<Command>) {
        ui.horizontal(|ui| {
            self.header(ui, u, false, out);
            widgets::divider(ui, 18.0);
            ui.spacing_mut().item_spacing.x = 3.0;
            self.assign_toggles(ui, u, out);
        });
        ui.horizontal_top(|ui| {
            let label = self.dry_wet_label(u);
            let c = &mut self.units[u];
            if knob(ui, &mut c.dry_wet, 0.0..=1.0, 1.0, label).changed() {
                out.push(Command::Fx(FxCommand::SetDryWet(u, c.dry_wet)));
            }
            widgets::divider(ui, 60.0);
            let gap = ui.spacing().item_spacing.x;
            let slot_width =
                ((ui.available_width() - gap * (FX_SLOTS - 1) as f32) / FX_SLOTS as f32).max(90.0);
            match self.units[u].mode {
                FxMode::Group => {
                    for slot in 0..FX_SLOTS {
                        self.group_slot(ui, u, slot, slot_width, out);
                    }
                }
                FxMode::Single => self.single(ui, u, slot_width, out),
                FxMode::DrumMachine => {
                    let width = ui.available_width();
                    self.pattern_full(ui, u, width, out);
                }
            }
        });
    }

    fn effect_combo(
        &self,
        ui: &mut Ui,
        id: (usize, usize),
        current: EffectKind,
        width: f32,
    ) -> Option<EffectKind> {
        let search_id = egui::Id::unique(("fx-search", id.0, id.1));
        let mut query: String = ui.data_mut(|d| d.get_temp(search_id).unwrap_or_default());
        let mut selected = current;
        let popup = ComboBox::from_id_salt(("fx", id.0, id.1))
            .width(width)
            .truncate()
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .height(f32::INFINITY)
            .selected_text(RichText::new(current.short_name()).size(12.0))
            .show_ui(ui, |ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .hint_text("Search…")
                        .desired_width(f32::INFINITY),
                );
                if !search.has_focus() && !search.lost_focus() {
                    search.request_focus();
                }
                let matches = effect_matches(&self.selection, &query, id.1 != SINGLE_SELECTOR);
                if search.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && let Some(&first) = matches.first()
                {
                    selected = first;
                    ui.close();
                }
                ui.separator();
                let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
                let full = (row * (self.selection.len() + 1) as f32).min(SEARCH_LIST_MAX);
                egui::ScrollArea::vertical()
                    .max_height(SEARCH_LIST_MAX)
                    .show(ui, |ui| {
                        ui.set_min_height(full);
                        for kind in matches {
                            if ui
                                .selectable_label(kind == selected, kind.info().name)
                                .clicked()
                            {
                                selected = kind;
                                ui.close();
                            }
                        }
                    });
            });
        if current.short_name() != current.info().name {
            popup.response.on_hover_text(current.info().name);
        }
        if popup.inner.is_none() {
            query.clear();
        }
        ui.data_mut(|d| d.insert_temp(search_id, query));
        (selected != current).then_some(selected)
    }

    fn group_slot(
        &mut self,
        ui: &mut Ui,
        u: usize,
        slot: usize,
        width: f32,
        out: &mut Vec<Command>,
    ) {
        ui.vertical(|ui| {
            ui.set_width(width);
            let current = self.units[u].kinds[slot];
            if let Some(kind) = self.effect_combo(ui, (u, slot), current, width - 8.0) {
                self.change_effect(u, Some(slot), kind, out);
            }
            let c = &mut self.units[u];
            let kind = c.kinds[slot];
            ui.add_enabled_ui(kind != EffectKind::None, |ui| {
                ui.horizontal(|ui| {
                    if knob(
                        ui,
                        &mut c.amounts[slot],
                        0.0..=1.0,
                        kind.info().neutral_amount,
                        "AMOUNT",
                    )
                    .changed()
                    {
                        out.push(Command::Fx(FxCommand::SetAmount(u, slot, c.amounts[slot])));
                    }
                    let t = theme::get(ui.ctx());
                    if widgets::toggle(ui, "ON", c.slot_on[slot], t.accent).clicked() {
                        c.slot_on[slot] = !c.slot_on[slot];
                        out.push(Command::Fx(FxCommand::SetSlotOn(u, slot, c.slot_on[slot])));
                    }
                });
            });
        });
    }

    fn single(&mut self, ui: &mut Ui, u: usize, slot_width: f32, out: &mut Vec<Command>) {
        ui.vertical(|ui| {
            let current = self.units[u].kinds[0];
            if let Some(kind) =
                self.effect_combo(ui, (u, SINGLE_SELECTOR), current, slot_width - 8.0)
            {
                self.change_effect(u, None, kind, out);
            }
            let c = &mut self.units[u];
            let info = c.kinds[0].info();
            let mut pressed = None;
            ui.horizontal(|ui| {
                for i in 0..3 {
                    if info.params[i].is_empty() && info.buttons[i].is_empty() {
                        continue;
                    }
                    ui.vertical(|ui| {
                        if !info.params[i].is_empty()
                            && knob(
                                ui,
                                &mut c.params[i],
                                0.0..=1.0,
                                info.defaults[i],
                                info.params[i],
                            )
                            .changed()
                        {
                            out.push(Command::Fx(FxCommand::SetParam(u, i, c.params[i])));
                        }
                        if !info.buttons[i].is_empty()
                            && widgets::toggle(
                                ui,
                                info.buttons[i],
                                c.buttons[i],
                                theme::get(ui.ctx()).accent,
                            )
                            .clicked()
                        {
                            pressed = Some(i);
                        }
                    });
                }
            });
            if let Some(i) = pressed {
                self.press_button(u, i, out);
            }
        });
    }
}

impl FxView {
    pub fn apply(&mut self, action: Action, trigger: Trigger, out: &mut Vec<Command>) -> bool {
        if self.apply_pattern(action, trigger, out) {
            return true;
        }
        let trigger = match (trigger, action) {
            (Trigger::Reset, Action::FxDryWet { unit }) if unit < FX_UNITS => {
                Trigger::Set(self.defaults[unit].dry_wet)
            }
            (Trigger::Reset, Action::FxKnob { unit, knob }) if unit < FX_UNITS && knob < 3 => {
                Trigger::Set(self.defaults[unit].knobs[knob])
            }
            (t, _) => t,
        };
        let level = |current: f32| match trigger {
            Trigger::Set(v) => Some(v.clamp(0.0, 1.0)),
            Trigger::Delta(d) => Some((current + d / 64.0).clamp(0.0, 1.0)),
            Trigger::Press | Trigger::Release | Trigger::Reset => None,
        };
        let pressed = trigger == Trigger::Press;
        let mut send = |c: FxCommand| out.push(Command::Fx(c));
        match action {
            Action::FxOn { unit } if unit < FX_UNITS => {
                let c = &mut self.units[unit];
                if let Some(on) = switched(c.on, trigger) {
                    c.on = on;
                    send(FxCommand::SetOn(unit, c.on));
                }
            }
            Action::FxDryWet { unit } if unit < FX_UNITS => {
                if let Some(v) = level(self.units[unit].dry_wet) {
                    self.units[unit].dry_wet = v;
                    send(FxCommand::SetDryWet(unit, v));
                }
            }
            Action::FxKnob { unit, knob } if unit < FX_UNITS && knob < 3 => {
                let c = &mut self.units[unit];
                match c.mode {
                    FxMode::Group => {
                        if let Some(v) = level(c.amounts[knob]) {
                            c.amounts[knob] = v;
                            send(FxCommand::SetAmount(unit, knob, v));
                        }
                    }
                    FxMode::Single => {
                        if let Some(v) = level(c.params[knob]) {
                            c.params[knob] = v;
                            send(FxCommand::SetParam(unit, knob, v));
                        }
                    }
                    FxMode::DrumMachine => {}
                }
            }
            Action::FxAssign { channel, unit } if unit < FX_UNITS && channel < 4 => {
                let c = &mut self.units[unit];
                if let Some(on) = switched(c.assign[channel], trigger) {
                    c.assign[channel] = on;
                    send(FxCommand::Assign {
                        channel,
                        unit,
                        on: c.assign[channel],
                    });
                }
            }
            Action::FxAssignHeld { channel, unit } if unit < FX_UNITS && channel < 4 => {
                self.assign_held(channel, unit, trigger, out);
            }
            Action::FxButton { unit, button } if unit < FX_UNITS && button < 3 => {
                let c = &mut self.units[unit];
                match c.mode {
                    FxMode::Group => {
                        if let Some(on) = switched(c.slot_on[button], trigger) {
                            c.slot_on[button] = on;
                            send(FxCommand::SetSlotOn(unit, button, on));
                        }
                    }
                    FxMode::Single if pressed => self.press_button(unit, button, out),
                    FxMode::Single | FxMode::DrumMachine => {}
                }
            }
            Action::FxSelect { unit, slot } if unit < FX_UNITS && slot < FX_SLOTS => {
                self.select_effect(unit, slot, trigger, out);
            }
            Action::FxLfoReset if pressed => out.push(Command::Fx(FxCommand::LfoReset)),
            Action::FxSnapshotStore { unit } if unit < FX_UNITS => {
                if pressed {
                    self.store_snapshot(unit);
                }
            }
            Action::FxSnapshotLoad { unit } if unit < FX_UNITS => {
                if pressed {
                    self.load_snapshot(unit, out);
                }
            }
            Action::FxMode { unit } if unit < FX_UNITS => {
                if let Some(mode) = mode_after(self.units[unit].mode, trigger) {
                    self.set_mode(unit, mode, out);
                }
            }
            _ => return false,
        }
        true
    }
}

fn mode_after(current: FxMode, trigger: Trigger) -> Option<FxMode> {
    const MODES: [FxMode; 3] = [FxMode::Group, FxMode::Single, FxMode::DrumMachine];
    let steps = match trigger {
        Trigger::Delta(steps) => steps.round() as i32,
        Trigger::Press => 1,
        _ => return None,
    };
    let at = MODES.iter().position(|&m| m == current).unwrap_or(0) as i32;
    (steps != 0).then(|| MODES[(at + steps).rem_euclid(3) as usize])
}

fn switched(current: bool, trigger: Trigger) -> Option<bool> {
    match trigger {
        Trigger::Press => Some(!current),
        Trigger::Set(v) => Some(v > 0.5),
        _ => None,
    }
}

fn effect_after(list: &[EffectKind], current: EffectKind, trigger: Trigger) -> Option<usize> {
    let n = list.len();
    let at = list.iter().position(|k| *k == current).unwrap_or(0);
    match trigger {
        Trigger::Delta(d) if d.round() != 0.0 => {
            Some((at as i64 + d.round() as i64).rem_euclid(n as i64) as usize)
        }
        Trigger::Set(v) => Some(((v.clamp(0.0, 1.0) * (n - 1) as f32).round() as usize).min(n - 1)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unit_mode_selector_wraps_round() {
        assert_eq!(
            mode_after(FxMode::Group, Trigger::Press),
            Some(FxMode::Single)
        );
        assert_eq!(
            mode_after(FxMode::DrumMachine, Trigger::Delta(1.0)),
            Some(FxMode::Group)
        );
        assert_eq!(
            mode_after(FxMode::Group, Trigger::Delta(-1.0)),
            Some(FxMode::DrumMachine)
        );
        assert_eq!(mode_after(FxMode::Single, Trigger::Release), None);
    }

    fn fx(out: &[Command]) -> Vec<String> {
        out.iter()
            .filter_map(|c| match c {
                Command::Fx(FxCommand::SetEffect { unit, slot, .. }) => {
                    Some(format!("SetEffect({unit}, {slot})"))
                }
                Command::Fx(f) => Some(format!("{f:?}")),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn units_start_on_decks_a_and_b() {
        let v = FxView::new(48_000, KitLibrary::none());
        let init = fx(&v.initial_commands());
        assert!(init.contains(&"Assign { channel: 0, unit: 0, on: true }".to_owned()));
        assert!(init.contains(&"Assign { channel: 1, unit: 1, on: true }".to_owned()));
        assert_eq!(
            init.iter().filter(|c| c.starts_with("SetEffect")).count(),
            6
        );
    }

    #[test]
    fn switch_positions_set_buttons() {
        let mut v = FxView::new(48_000, KitLibrary::none());
        let mut out = Vec::new();
        let on = Action::FxOn { unit: 0 };
        v.apply(on, Trigger::Set(1.0), &mut out);
        v.apply(on, Trigger::Set(1.0), &mut out);
        assert!(v.units[0].on, "set twice: still on");
        v.apply(on, Trigger::Set(0.0), &mut out);
        assert!(!v.units[0].on);
        let assign = Action::FxAssign {
            channel: 1,
            unit: 1,
        };
        v.apply(assign, Trigger::Set(1.0), &mut out);
        v.apply(assign, Trigger::Set(1.0), &mut out);
        assert!(v.units[1].assign[1]);
        let slot = Action::FxButton { unit: 0, button: 2 };
        v.apply(slot, Trigger::Set(1.0), &mut out);
        v.apply(slot, Trigger::Set(1.0), &mut out);
        assert!(v.units[0].slot_on[2]);
    }

    #[test]
    fn buttons_toggle_on_press() {
        let mut v = FxView::new(48_000, KitLibrary::none());
        let mut out = Vec::new();
        v.apply(Action::FxOn { unit: 1 }, Trigger::Press, &mut out);
        v.apply(Action::FxOn { unit: 1 }, Trigger::Release, &mut out);
        v.apply(
            Action::FxAssign {
                channel: 2,
                unit: 0,
            },
            Trigger::Press,
            &mut out,
        );
        v.apply(
            Action::FxButton { unit: 0, button: 1 },
            Trigger::Press,
            &mut out,
        );
        assert_eq!(
            fx(&out),
            [
                "SetOn(1, true)",
                "Assign { channel: 2, unit: 0, on: true }",
                "SetSlotOn(0, 1, true)"
            ]
        );
    }

    #[test]
    fn knobs_follow_the_unit_mode() {
        let mut v = FxView::new(48_000, KitLibrary::none());
        let mut out = Vec::new();
        v.apply(
            Action::FxKnob { unit: 0, knob: 2 },
            Trigger::Set(0.75),
            &mut out,
        );
        v.units[0].mode = FxMode::Single;
        v.apply(
            Action::FxKnob { unit: 0, knob: 2 },
            Trigger::Set(0.25),
            &mut out,
        );
        v.apply(
            Action::FxDryWet { unit: 0 },
            Trigger::Delta(-32.0),
            &mut out,
        );
        assert_eq!(
            fx(&out),
            [
                "SetAmount(0, 2, 0.75)",
                "SetParam(0, 2, 0.25)",
                "SetDryWet(0, 0.5)"
            ]
        );
    }

    #[test]
    fn out_of_range_units_are_ignored() {
        let mut v = FxView::new(48_000, KitLibrary::none());
        let mut out = Vec::new();
        assert!(!v.apply(Action::FxOn { unit: FX_UNITS }, Trigger::Press, &mut out));
        assert!(!v.apply(Action::Crossfader, Trigger::Set(0.5), &mut out));
    }

    #[test]
    fn all_commands_restore_units() {
        let mut v = FxView::new(48_000, KitLibrary::none());
        let all = fx(&v.all_commands());
        for u in 0..FX_UNITS {
            assert!(all.contains(&format!("SetOn({u}, false)")));
            assert!(all.contains(&format!("SetMode({u}, Group)")));
        }
    }

    #[test]
    fn switching_recalls_the_snapshot_when_enabled() {
        for restore in [false, true] {
            let mut v = FxView::new(48_000, KitLibrary::none());
            v.restore = restore;
            let mut out = Vec::new();
            v.units[0].amounts[0] = 0.8;
            v.units[0].slot_on[0] = true;
            v.store_snapshot(0);
            v.change_effect(0, Some(0), EffectKind::Gater, &mut out);
            v.change_effect(0, Some(0), EffectKind::Delay, &mut out);
            let expected = if restore { 0.8 } else { 0.0 };
            assert!(
                (v.units[0].amounts[0] - expected).abs() < 1e-6,
                "restore {restore}"
            );
            assert!(v.units[0].slot_on[0], "ON as it was or as kept");
        }
    }

    #[test]
    fn held_fx_assign_follows_the_controller() {
        let mut v = FxView::new(48_000, KitLibrary::none());
        let mut out = Vec::new();
        let held = Action::FxAssignHeld {
            channel: 2,
            unit: 1,
        };
        v.apply(held, Trigger::Press, &mut out);
        assert!(v.units[1].assign[2]);
        out.clear();
        v.apply(held, Trigger::Press, &mut out);
        assert!(
            v.units[1].assign[2] && out.is_empty(),
            "already on: nothing sent"
        );
        v.apply(held, Trigger::Release, &mut out);
        assert!(!v.units[1].assign[2]);
    }

    #[test]
    fn effect_select_steps_through_the_selection_and_wraps() {
        let mut v = FxView::new(48_000, KitLibrary::none());
        v.selection = vec![EffectKind::Delay, EffectKind::Reverb];
        let mut out = Vec::new();
        v.set_mode(2, FxMode::Group, &mut out);
        let select = Action::FxSelect { unit: 2, slot: 1 };
        v.apply(select, Trigger::Delta(1.0), &mut out);
        assert_eq!(v.units[2].kinds[1], EffectKind::Delay);
        v.apply(select, Trigger::Delta(2.0), &mut out);
        assert_eq!(v.units[2].kinds[1], EffectKind::None, "wraps round");
        v.apply(select, Trigger::Delta(-1.0), &mut out);
        assert_eq!(v.units[2].kinds[1], EffectKind::Reverb);
        v.set_mode(3, FxMode::Single, &mut out);
        v.apply(
            Action::FxSelect { unit: 3, slot: 0 },
            Trigger::Set(0.5),
            &mut out,
        );
        assert_eq!(v.units[3].kinds[0], EffectKind::Delay);
        v.apply(
            Action::FxSelect { unit: 3, slot: 1 },
            Trigger::Delta(1.0),
            &mut out,
        );
        assert_eq!(v.units[3].kinds[1], EffectKind::None, "Single has one");
    }

    #[test]
    fn snapshots_are_per_effect_and_rst_recalls_them() {
        let mut v = FxView::new(48_000, KitLibrary::none());
        let mut out = Vec::new();
        v.set_mode(1, FxMode::Single, &mut out);
        v.change_effect(1, None, EffectKind::Delay, &mut out);
        v.units[1].params = [0.1, 0.2, 0.3];
        v.units[1].buttons = [false, true, false];
        v.units[1].dry_wet = 0.4;
        v.store_snapshot(1);
        v.set_mode(2, FxMode::Single, &mut out);
        v.change_effect(2, None, EffectKind::Delay, &mut out);
        v.load_snapshot(2, &mut out);
        assert_eq!(v.units[2].params, [0.1, 0.2, 0.3]);
        assert_eq!(v.units[2].buttons, [false, true, false]);
        assert!((v.units[2].dry_wet - 0.4).abs() < 1e-6);
        let knob = |knob| Action::FxKnob { unit: 2, knob };
        v.apply(knob(1), Trigger::Set(0.8), &mut out);
        v.apply(knob(1), Trigger::Reset, &mut out);
        v.apply(Action::FxDryWet { unit: 2 }, Trigger::Set(1.0), &mut out);
        v.apply(Action::FxDryWet { unit: 2 }, Trigger::Reset, &mut out);
        assert!((v.units[2].params[1] - 0.2).abs() < 1e-6);
        assert!((v.units[2].dry_wet - 0.4).abs() < 1e-6);
        v.apply(Action::FxDryWet { unit: 3 }, Trigger::Set(1.0), &mut out);
        v.apply(Action::FxDryWet { unit: 3 }, Trigger::Reset, &mut out);
        assert_eq!(v.units[3].dry_wet, 0.0);
        v.units[1].params = [0.9; 3];
        out.clear();
        v.apply(
            Action::FxButton { unit: 1, button: 0 },
            Trigger::Press,
            &mut out,
        );
        assert_eq!(v.units[1].params, [0.1, 0.2, 0.3]);
        assert!(!v.units[1].buttons[0], "RST does not latch");
        v.change_effect(1, None, EffectKind::Reverb, &mut out);
        v.units[1].params = [0.9; 3];
        v.load_snapshot(1, &mut out);
        assert_eq!(v.units[1].params, EffectKind::Reverb.info().defaults);
        v.change_effect(1, None, EffectKind::Echo, &mut out);
        v.apply(
            Action::FxButton { unit: 1, button: 0 },
            Trigger::Press,
            &mut out,
        );
        assert!(v.units[1].buttons[0]);
    }

    #[test]
    fn routing_and_modes_are_sent_once_and_name_the_return() {
        let mut v = FxView::new(48_000, KitLibrary::none());
        let mut out = Vec::new();
        v.set_routing(2, FxRouting::Send, &mut out);
        v.set_routing(2, FxRouting::Send, &mut out);
        v.set_mode(3, FxMode::Single, &mut out);
        assert_eq!(fx(&out), ["SetRouting(2, Send)", "SetMode(3, Single)"]);
        assert_eq!(v.dry_wet_label(2), "RET");
        assert_eq!(v.dry_wet_label(0), "D/W");
        assert_eq!(v.modes()[3], FxMode::Single);
        v.set_mode(1, FxMode::DrumMachine, &mut out);
        assert_eq!(v.dry_wet_label(1), "VOL");
    }

    #[test]
    fn effect_search_ignores_case_and_keeps_the_order() {
        let selection = [EffectKind::Reverb, EffectKind::Delay, EffectKind::Gater];
        assert_eq!(
            effect_matches(&selection, "", true).len(),
            4,
            "none plus all"
        );
        assert_eq!(
            effect_matches(&selection, " E", true),
            [EffectKind::Reverb, EffectKind::Delay, EffectKind::Gater]
        );
        assert_eq!(effect_matches(&selection, "del", true), [EffectKind::Delay]);
        assert_eq!(effect_matches(&selection, "zzz", true), []);
        let selection = [
            EffectKind::Delay,
            EffectKind::ModFilter,
            EffectKind::ModDecay,
        ];
        assert_eq!(effect_matches(&selection, "modifier", true).len(), 2);
        assert_eq!(
            effect_matches(&selection, "", false),
            [EffectKind::None, EffectKind::Delay]
        );
    }
}
