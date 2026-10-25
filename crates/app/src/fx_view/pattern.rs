use backspn_control::{Action, Trigger};
use backspn_engine::fx_pattern::{PATTERNS, STEPS, Steps, pattern_index, pitch_semitones};
use backspn_engine::{Command, DrumMachine, FX_CHANNELS, FX_UNITS, FxCommand, FxMode};
use eframe::egui::{self, ComboBox, RichText, Ui};

use super::FxView;
use crate::i18n::tr;
use crate::kits::Kit;
use crate::modal::{self, Ask};
use crate::theme;
use crate::widgets::{self, knob};

const KNOBS: [&str; 3] = ["DECAY", "PITCH", "PTRN"];
const SHOWN: [usize; 3] = [2, 1, 0];
const BUTTONS: [&str; 3] = ["DUCK", "NEXT", "PREV"];
const DUCK: usize = 0;
pub(crate) const BUTTONS_SHOWN: [usize; 3] = [2, 1, 0];

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct PatternStatus {
    knobs: [f32; 3],
    duck: bool,
    sound: usize,
    live: Vec<[Steps; PATTERNS]>,
}

impl Default for PatternStatus {
    fn default() -> Self {
        Self {
            knobs: backspn_engine::PATTERN_KNOBS,
            duck: false,
            sound: 0,
            live: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct PatternControls {
    kit: Option<(String, Kit)>,
    pub(super) knobs: [f32; 3],
    duck: bool,
    sound: usize,
    rolling: bool,
    saved: Vec<[Steps; PATTERNS]>,
    live: Vec<[Steps; PATTERNS]>,
    step: Option<usize>,
    save_ask: Option<Ask<()>>,
}

impl PatternControls {
    pub(super) fn new() -> Self {
        Self {
            knobs: backspn_engine::PATTERN_KNOBS,
            ..Self::default()
        }
    }

    fn pattern(&self) -> usize {
        pattern_index(self.knobs[2])
    }

    pub(super) fn status(&self) -> PatternStatus {
        PatternStatus {
            knobs: self.knobs,
            duck: self.duck,
            sound: self.sound,
            live: self.live.clone(),
        }
    }

    pub(super) fn clear_kit(&mut self) {
        self.kit = None;
    }

    pub(super) fn restore(&mut self, status: &PatternStatus) {
        self.knobs = status.knobs.map(|v| v.clamp(0.0, 1.0));
        self.duck = status.duck;
        if status.sound < self.live.len() {
            self.sound = status.sound;
        }
        let fits = status.live.len() == self.live.len()
            && status.live.iter().zip(&self.saved).all(|(live, saved)| {
                live.iter()
                    .zip(saved)
                    .all(|(l, s)| l.per_beat == s.per_beat)
            });
        if fits {
            self.live.clone_from(&status.live);
        }
    }

    fn sound_name(&self) -> &str {
        self.kit
            .as_ref()
            .and_then(|(_, k)| k.sounds.get(self.sound))
            .map_or("—", |s| s.name.as_str())
    }

    fn live_steps(&self) -> Steps {
        self.live
            .get(self.sound)
            .map_or_else(Steps::default, |p| p[self.pattern()])
    }

    fn customized(&self) -> bool {
        let p = self.pattern();
        match (self.live.get(self.sound), self.saved.get(self.sound)) {
            (Some(live), Some(saved)) => live[p] != saved[p],
            _ => false,
        }
    }

    fn unsaved(&self) -> bool {
        self.live != self.saved
    }

    fn pattern_label(&self) -> String {
        format!("{} P{}", self.sound_name(), self.pattern() + 1)
    }
}

fn merge_steps(
    live: &mut [[Steps; PATTERNS]],
    old: &[[Steps; PATTERNS]],
    new: &[[Steps; PATTERNS]],
) -> bool {
    if live.len() != new.len() || old.len() != new.len() {
        return false;
    }
    for ((live, old), new) in live.iter_mut().zip(old).zip(new) {
        for n in 0..PATTERNS {
            if live[n] == old[n] || live[n].per_beat != new[n].per_beat {
                live[n] = new[n];
            }
        }
    }
    true
}

pub fn resolution_label(per_beat: u32) -> String {
    format!("1/{per_beat}")
}

fn kit_steps(kit: &Kit) -> Vec<[Steps; PATTERNS]> {
    kit.sounds.iter().map(|s| s.patterns).collect()
}

impl FxView {
    pub fn mapped_steps(&self, u: usize) -> (u16, u8) {
        let Some(p) = self.patterns.get(u) else {
            return (0, 0);
        };
        let live = p.live_steps();
        let cells = live.cells();
        let bits = (0..STEPS)
            .filter(|&step| live.on(step * cells / STEPS))
            .fold(0u16, |bits, step| bits | 1 << step);
        (bits, u8::try_from(cells).unwrap_or(u8::MAX))
    }

    pub fn kit_ids(&self) -> [String; FX_UNITS] {
        std::array::from_fn(|u| {
            self.patterns[u]
                .kit
                .as_ref()
                .map_or_else(|| self.kit_choice[u].clone(), |(id, _)| id.clone())
        })
    }

    fn players(&mut self, u: usize) -> Option<[Box<DrumMachine>; FX_CHANNELS]> {
        let id = self.patterns[u].kit.as_ref()?.0.clone();
        let sounds = self.kits.sounds(&id);
        let live = self.patterns[u].live.clone();
        let rate = self.sample_rate;
        Some(std::array::from_fn(|_| {
            let mut p = DrumMachine::new(&sounds, rate);
            for (s, patterns) in live.iter().enumerate() {
                for (n, &steps) in patterns.iter().enumerate() {
                    p.set_steps(s, n, steps);
                }
            }
            Box::new(p)
        }))
    }

    fn send_kit(&mut self, u: usize, out: &mut Vec<Command>) {
        if let Some(players) = self.players(u) {
            out.push(Command::Fx(FxCommand::SetKit { unit: u, players }));
            out.push(Command::Fx(FxCommand::PatternSound(
                u,
                self.patterns[u].sound,
            )));
        }
    }

    pub fn load_kit(&mut self, u: usize, id: &str, out: &mut Vec<Command>) {
        let Some(stored) = self.kits.find(id).cloned() else {
            return;
        };
        let steps = kit_steps(&stored.kit);
        let [decay, pitch, pattern, volume] = stored.kit.knobs;
        let p = &mut self.patterns[u];
        p.saved.clone_from(&steps);
        p.live = steps;
        p.sound = 0;
        p.knobs = [decay, pitch, pattern];
        p.kit = Some((stored.id.clone(), stored.kit));
        self.kit_choice[u] = stored.id;
        self.send_kit(u, out);
        for (i, v) in self.patterns[u].knobs.into_iter().enumerate() {
            out.push(Command::Fx(FxCommand::PatternKnob(u, i, v)));
        }
        self.units[u].dry_wet = volume;
        out.push(Command::Fx(FxCommand::SetDryWet(u, volume)));
    }

    pub(super) fn ensure_kit(&mut self, u: usize, out: &mut Vec<Command>) {
        if self.units[u].mode != FxMode::DrumMachine || self.patterns[u].kit.is_some() {
            return;
        }
        let id = Some(self.kit_choice[u].clone())
            .filter(|id| self.kits.find(id).is_some())
            .or_else(|| self.kits.kits().first().map(|k| k.id.clone()));
        if let Some(id) = id {
            self.load_kit(u, &id, out);
        }
    }

    pub fn refresh_kits(&mut self, out: &mut Vec<Command>) {
        for u in 0..self.patterns.len() {
            let Some((id, old)) = self.patterns[u].kit.clone() else {
                continue;
            };
            let Some(new) = self.kits.find(&id).map(|k| k.kit.clone()) else {
                self.patterns[u] = PatternControls {
                    knobs: self.patterns[u].knobs,
                    ..PatternControls::default()
                };
                self.ensure_kit(u, out);
                continue;
            };
            if new == old {
                continue;
            }
            let p = &mut self.patterns[u];
            let saved = kit_steps(&new);
            if !merge_steps(&mut p.live, &p.saved, &saved) {
                p.live.clone_from(&saved);
                p.sound = 0;
            }
            p.saved = saved;
            p.kit = Some((id, new));
            self.send_kit(u, out);
        }
    }

    pub fn set_pattern_steps(&mut self, state: &backspn_engine::RackState) {
        for (p, unit) in self.patterns.iter_mut().zip(&state.units) {
            p.step = unit.pattern.step.filter(|_| unit.on);
        }
    }

    pub(super) fn pattern_commands(&mut self, out: &mut Vec<Command>) {
        for u in 0..self.patterns.len() {
            self.send_kit(u, out);
            let p = &self.patterns[u];
            for (i, &v) in p.knobs.iter().enumerate() {
                out.push(Command::Fx(FxCommand::PatternKnob(u, i, v)));
            }
            out.push(Command::Fx(FxCommand::PatternDuck(u, p.duck)));
        }
    }

    fn set_pattern_knob(&mut self, u: usize, i: usize, v: f32, out: &mut Vec<Command>) {
        self.patterns[u].knobs[i] = v.clamp(0.0, 1.0);
        out.push(Command::Fx(FxCommand::PatternKnob(
            u,
            i,
            self.patterns[u].knobs[i],
        )));
    }

    fn toggle_duck(&mut self, u: usize, out: &mut Vec<Command>) {
        let p = &mut self.patterns[u];
        p.duck = !p.duck;
        out.push(Command::Fx(FxCommand::PatternDuck(u, p.duck)));
    }

    fn set_roll(&mut self, u: usize, on: bool, out: &mut Vec<Command>) {
        let p = &mut self.patterns[u];
        if p.rolling != on {
            p.rolling = on;
            out.push(Command::Fx(FxCommand::PatternRoll(u, on)));
        }
    }

    fn step_sound(&mut self, u: usize, by: isize, out: &mut Vec<Command>) {
        let count = self.patterns[u].live.len();
        if count == 0 {
            return;
        }
        let sound = (self.patterns[u].sound as isize + by).rem_euclid(count as isize) as usize;
        self.select_sound(u, sound, out);
    }

    fn select_sound(&mut self, u: usize, sound: usize, out: &mut Vec<Command>) {
        let p = &mut self.patterns[u];
        if sound < p.live.len() {
            p.sound = sound;
            out.push(Command::Fx(FxCommand::PatternSound(u, sound)));
        }
    }

    fn toggle_step(&mut self, u: usize, step: usize, out: &mut Vec<Command>) {
        let p = &mut self.patterns[u];
        let n = p.pattern();
        if let Some(live) = p.live.get_mut(p.sound)
            && step < live[n].cells()
        {
            let on = !live[n].on(step);
            live[n].set(step, on);
            out.push(Command::Fx(FxCommand::PatternStep(u, step, on)));
        }
    }

    fn reset_pattern(&mut self, u: usize, out: &mut Vec<Command>) {
        let p = &mut self.patterns[u];
        let n = p.pattern();
        if let (Some(live), Some(saved)) = (p.live.get_mut(p.sound), p.saved.get(p.sound)) {
            live[n] = saved[n];
            out.push(Command::Fx(FxCommand::PatternReset(u)));
        }
    }

    fn clear_pattern(&mut self, u: usize, out: &mut Vec<Command>) {
        let p = &mut self.patterns[u];
        let n = p.pattern();
        if let Some(live) = p.live.get_mut(p.sound) {
            live[n].bits = 0;
            out.push(Command::Fx(FxCommand::PatternClear(u)));
        }
    }

    fn save_pattern(&mut self, u: usize, out: &mut Vec<Command>) -> anyhow::Result<()> {
        let Some(id) = self.patterns[u].kit.as_ref().map(|(id, _)| id.clone()) else {
            return Ok(());
        };
        let Some(mut kit) = self.kits.find(&id).map(|k| k.kit.clone()) else {
            return Ok(());
        };
        let p = &mut self.patterns[u];
        for (sound, live) in kit.sounds.iter_mut().zip(&p.live) {
            sound.patterns = *live;
        }
        self.kits.save(&id, &kit)?;
        p.saved = kit_steps(&kit);
        p.kit = Some((id, kit));
        self.send_kit(u, out);
        self.refresh_kits(out);
        Ok(())
    }

    fn confirm_save_pattern(&mut self, u: usize) {
        let p = &mut self.patterns[u];
        let Some((_, kit)) = &p.kit else {
            return;
        };
        p.save_ask = Some(Ask::yes_no(
            "Save pattern",
            format!(
                "Overwrite the kit “{}” with its steps as they are now?",
                kit.name
            ),
            (),
        ));
    }

    fn step_kit(&mut self, u: usize, by: isize, out: &mut Vec<Command>) {
        let ids: Vec<String> = self.kits.kits().iter().map(|k| k.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        let current = self.patterns[u]
            .kit
            .as_ref()
            .and_then(|(id, _)| ids.iter().position(|i| i == id));
        let next = current.map_or(0, |c| {
            (c as isize + by).rem_euclid(ids.len() as isize) as usize
        });
        self.load_kit(u, &ids[next], out);
    }

    fn kit_combo(&mut self, ui: &mut Ui, u: usize, width: f32, out: &mut Vec<Command>) {
        let current = self.patterns[u]
            .kit
            .as_ref()
            .map(|(id, k)| (id.clone(), k.name.clone()));
        let mut picked = None;
        ComboBox::from_id_salt(("pattern-kit", u))
            .width(width)
            .truncate()
            .selected_text(
                RichText::new(current.as_ref().map_or("No kit", |(_, n)| n.as_str())).size(12.0),
            )
            .show_ui(ui, |ui| {
                if self.kits.kits().is_empty() {
                    ui.label(RichText::new(tr("No kits: make one in Preferences.")).weak());
                }
                for k in self.kits.kits() {
                    let on = current.as_ref().is_some_and(|(id, _)| *id == k.id);
                    if ui.selectable_label(on, &k.kit.name).clicked() {
                        picked = Some(k.id.clone());
                    }
                }
            });
        if let Some(id) = picked {
            self.load_kit(u, &id, out);
        }
    }

    fn sound_combo(&mut self, ui: &mut Ui, u: usize, out: &mut Vec<Command>) {
        let p = &self.patterns[u];
        let mut picked = None;
        ComboBox::from_id_salt(("pattern-sound", u))
            .truncate()
            .selected_text(RichText::new(p.sound_name()).size(12.0).strong())
            .show_ui(ui, |ui| {
                let sounds = p.kit.as_ref().map_or(&[][..], |(_, k)| &k.sounds[..]);
                for (i, s) in sounds.iter().enumerate() {
                    if ui.selectable_label(i == p.sound, &s.name).clicked() {
                        picked = Some(i);
                    }
                }
            });
        if let Some(i) = picked {
            self.select_sound(u, i, out);
        }
    }

    fn pattern_buttons(&mut self, ui: &mut Ui, u: usize, out: &mut Vec<Command>) {
        let t = theme::get(ui.ctx());
        for b in BUTTONS_SHOWN {
            let on = b == DUCK && self.patterns[u].duck;
            let r = widgets::toggle(ui, BUTTONS[b], on, t.accent);
            let r = if b == DUCK {
                r.on_hover_text(tr("Duck: each hit dips the deck"))
            } else {
                r
            };
            if r.clicked() {
                self.press_pattern_button(u, b, out);
            }
        }
    }

    fn press_pattern_button(&mut self, u: usize, b: usize, out: &mut Vec<Command>) {
        match b {
            DUCK => self.toggle_duck(u, out),
            1 => self.step_sound(u, 1, out),
            _ => self.step_sound(u, -1, out),
        }
    }

    fn pattern_steps(&mut self, ui: &mut Ui, u: usize, width: f32, out: &mut Vec<Command>) {
        let t = theme::get(ui.ctx());
        if modal::show(ui.ctx(), &mut self.patterns[u].save_ask).is_some()
            && let Err(e) = self.save_pattern(u, out)
        {
            crate::toast::error(format!("Cannot save the kit: {e}"));
        }
        let p = &self.patterns[u];
        let (steps, step, customized, unsaved) =
            (p.live_steps(), p.step, p.customized(), p.unsaved());
        ui.allocate_ui_with_layout(
            egui::vec2(width, 0.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                let lit = if unsaved {
                    widgets::Lit::On
                } else {
                    widgets::Lit::Off
                };
                if widgets::toggle_lit(ui, "SAVE", lit, t.accent)
                    .on_hover_text(tr("Overwrite the kit with these steps"))
                    .clicked()
                {
                    self.confirm_save_pattern(u);
                }
                if widgets::toggle(ui, "CLEAR", false, t.accent).clicked() {
                    self.clear_pattern(u, out);
                }
                let lit = if customized {
                    widgets::Lit::On
                } else {
                    widgets::Lit::Off
                };
                if widgets::toggle_lit(ui, "RESET", lit, t.accent)
                    .on_hover_text(tr("Back to the kit's steps"))
                    .clicked()
                {
                    self.reset_pattern(u, out);
                }
                let left = (ui.available_width() - ui.spacing().item_spacing.x).max(100.0);
                if let Some(i) =
                    widgets::step_row(ui, steps.bits, steps.cells(), step, t.accent, left)
                {
                    self.toggle_step(u, i, out);
                }
            },
        );
    }

    pub(super) fn pattern_full(
        &mut self,
        ui: &mut Ui,
        u: usize,
        width: f32,
        out: &mut Vec<Command>,
    ) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                self.kit_combo(ui, u, 150.0, out);
                self.sound_combo(ui, u, out);
                let pattern = self.patterns[u].pattern() + 1;
                ui.label(RichText::new(format!("P{pattern}")).strong());
            });
            ui.horizontal_top(|ui| {
                for i in SHOWN {
                    let defaults = backspn_engine::PATTERN_KNOBS;
                    let mut v = self.patterns[u].knobs[i];
                    let r = knob(ui, &mut v, 0.0..=1.0, defaults[i], KNOBS[i]);
                    let r = match i {
                        1 => r.on_hover_text(format!("{:+.2} st", pitch_semitones(v))),
                        2 => r.on_hover_text(self.patterns[u].pattern_label()),
                        _ => r,
                    };
                    if r.changed() {
                        self.set_pattern_knob(u, i, v, out);
                    }
                }
                ui.vertical(|ui| {
                    ui.horizontal(|ui| self.pattern_buttons(ui, u, out));
                });
            });
            self.pattern_steps(ui, u, width, out);
        });
    }

    pub(super) fn pattern_compact(
        &mut self,
        ui: &mut Ui,
        u: usize,
        width: f32,
        out: &mut Vec<Command>,
    ) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                self.kit_combo(ui, u, 96.0, out);
                for i in SHOWN {
                    let mut v = self.patterns[u].knobs[i];
                    ui.spacing_mut().slider_width = 44.0;
                    let r = ui
                        .add(egui::Slider::new(&mut v, 0.0..=1.0).show_value(false))
                        .on_hover_text(match i {
                            2 => self.patterns[u].pattern_label(),
                            _ => KNOBS[i].to_owned(),
                        });
                    if r.changed() {
                        self.set_pattern_knob(u, i, v, out);
                    }
                }
                self.pattern_buttons(ui, u, out);
            });
            self.pattern_steps(ui, u, width, out);
        });
    }

    pub(super) fn apply_pattern(
        &mut self,
        action: Action,
        trigger: Trigger,
        out: &mut Vec<Command>,
    ) -> bool {
        let pressed = trigger == Trigger::Press;
        let level = |current: f32| match trigger {
            Trigger::Set(v) => Some(v.clamp(0.0, 1.0)),
            Trigger::Delta(d) => Some((current + d / 64.0).clamp(0.0, 1.0)),
            Trigger::Press | Trigger::Release | Trigger::Reset => None,
        };
        let units = self.patterns.len();
        let pattern_mode =
            |s: &Self, unit: usize| unit < units && s.units[unit].mode == FxMode::DrumMachine;
        match action {
            Action::FxKnob { unit, knob } if knob < 3 && pattern_mode(self, unit) => {
                let i = SHOWN[knob];
                if let Some(v) = level(self.patterns[unit].knobs[i]) {
                    self.set_pattern_knob(unit, i, v, out);
                }
            }
            Action::FxButton { unit, button } if button < 3 && pattern_mode(self, unit) => {
                if pressed {
                    self.press_pattern_button(unit, BUTTONS_SHOWN[button], out);
                }
            }
            Action::PatternStep { unit, step } if unit < units => {
                if pressed && step < STEPS {
                    let cells = self.patterns[unit].live_steps().cells();
                    self.toggle_step(unit, step * cells / STEPS, out);
                }
            }
            Action::PatternReset { unit } if unit < units => {
                if pressed {
                    self.reset_pattern(unit, out);
                }
            }
            Action::PatternClear { unit } if unit < units => {
                if pressed {
                    self.clear_pattern(unit, out);
                }
            }
            Action::PatternRoll { unit } if unit < units => match trigger {
                Trigger::Press => self.set_roll(unit, true, out),
                Trigger::Release => self.set_roll(unit, false, out),
                _ => {}
            },
            Action::PatternKit { unit } if unit < units => match trigger {
                Trigger::Press => self.step_kit(unit, 1, out),
                Trigger::Delta(d) if d != 0.0 => self.step_kit(unit, d.signum() as isize, out),
                _ => {}
            },
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kits::{KitSound, Steps};

    fn kit(sounds: usize) -> Kit {
        Kit {
            name: "Test".into(),
            author: String::new(),
            knobs: [0.8, 0.5, 0.0, 0.7],
            sounds: (0..sounds)
                .map(|i| KitSound {
                    name: format!("S{i}"),
                    file: format!("s{i}.wav"),
                    level_db: 0.0,
                    patterns: [Steps {
                        bits: 1 << i,
                        per_beat: 8,
                    }; PATTERNS],
                })
                .collect(),
        }
    }

    fn view_with(kit: Kit) -> FxView {
        let mut v = FxView::new(48_000, crate::kits::KitLibrary::none());
        let p = &mut v.patterns[0];
        p.saved = kit_steps(&kit);
        p.live = p.saved.clone();
        p.kit = Some(("test".into(), kit));
        v.units[0].mode = FxMode::DrumMachine;
        v
    }

    fn names(out: &[Command]) -> Vec<String> {
        out.iter()
            .filter_map(|c| match c {
                Command::Fx(FxCommand::SetKit { unit, .. }) => Some(format!("SetKit({unit})")),
                Command::Fx(f) => Some(format!("{f:?}")),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn steps_reset_and_clear_follow_the_playing_pattern() {
        let mut v = view_with(kit(2));
        let mut out = Vec::new();
        v.toggle_step(0, 3, &mut out);
        assert_eq!(v.patterns[0].live_steps().bits, 0b1001);
        assert!(v.patterns[0].customized());
        v.reset_pattern(0, &mut out);
        assert!(!v.patterns[0].customized());
        v.clear_pattern(0, &mut out);
        assert_eq!(v.patterns[0].live_steps().bits, 0);
        assert_eq!(
            names(&out),
            [
                "PatternStep(0, 3, true)",
                "PatternReset(0)",
                "PatternClear(0)"
            ]
        );
    }

    #[test]
    fn save_overwrites_the_kit_with_the_live_steps() {
        let dir = std::env::temp_dir().join(format!("backspn-save-kit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut kits = crate::kits::KitLibrary::open(dir.clone());
        kits.save("test", &kit(2)).unwrap();
        let mut v = FxView::new(48_000, kits);
        v.units[0].mode = FxMode::DrumMachine;
        v.units[1].mode = FxMode::DrumMachine;
        let mut out = Vec::new();
        v.load_kit(0, "test", &mut out);
        v.load_kit(1, "test", &mut out);
        v.toggle_step(0, 3, &mut out);
        assert!(v.patterns[0].unsaved());

        out.clear();
        v.save_pattern(0, &mut out).unwrap();
        assert!(!v.patterns[0].unsaved());
        assert!(
            !v.patterns[0].customized(),
            "RESET goes back to the saved steps"
        );
        assert_eq!(v.patterns[0].live_steps().bits, 0b1001);
        let stored = crate::kits::KitLibrary::open(dir.clone());
        assert_eq!(
            stored.find("test").unwrap().kit.sounds[0].patterns[0].bits,
            0b1001
        );
        assert_eq!(v.patterns[1].live_steps().bits, 0b1001);
        assert_eq!(
            names(&out),
            [
                "SetKit(0)",
                "PatternSound(0, 0)",
                "SetKit(1)",
                "PatternSound(1, 0)"
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_sound_picked_plays_and_one_past_the_kit_is_ignored() {
        let dir = std::env::temp_dir().join(format!("backspn-fx-sound-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut kits = crate::kits::KitLibrary::open(dir.clone());
        kits.save("test", &kit(3)).unwrap();
        let mut v = FxView::new(48_000, kits);
        let mut out = Vec::new();
        v.set_mode(0, FxMode::DrumMachine, &mut out);
        let mut out = Vec::new();
        v.select_sound(0, 2, &mut out);
        assert_eq!(v.patterns[0].sound, 2);
        assert_eq!(names(&out), ["PatternSound(0, 2)"]);
        let mut out = Vec::new();
        v.select_sound(0, 3, &mut out);
        assert_eq!(v.patterns[0].sound, 2);
        assert!(out.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_restored_session_brings_back_units_and_the_drum_machine() {
        let dir = std::env::temp_dir().join(format!("backspn-fx-status-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut kits = crate::kits::KitLibrary::open(dir.clone());
        kits.save("test", &kit(3)).unwrap();
        let mut v = FxView::new(48_000, kits);
        let mut out = Vec::new();
        v.set_mode(0, FxMode::DrumMachine, &mut out);
        v.set_pattern_knob(0, 1, 0.9, &mut out);
        v.toggle_duck(0, &mut out);
        v.step_sound(0, 1, &mut out);
        v.toggle_step(0, 5, &mut out);
        v.units[0].on = true;
        v.units[0].dry_wet = 0.3;
        v.units[1].amounts[2] = 0.7;
        v.units[1].slot_on[2] = true;
        let status = v.status();

        let kits = crate::kits::KitLibrary::open(dir.clone());
        let mut fresh = FxView::new(48_000, kits);
        fresh.kit_choice[0] = "test".into();
        let mut out = Vec::new();
        fresh.restore(&status, &mut out);
        assert_eq!(fresh.status(), status);
        let p = &fresh.patterns[0];
        assert_eq!((p.sound, p.duck), (1, true));
        assert!(p.customized(), "the live step is still there");
        let all = names(&fresh.all_commands());
        assert!(all.contains(&"SetOn(0, true)".to_owned()));
        assert!(all.contains(&"SetDryWet(0, 0.3)".to_owned()));
        assert!(all.contains(&"PatternDuck(0, true)".to_owned()));
        assert!(all.contains(&"SetSlotOn(1, 2, true)".to_owned()));

        let mut changed = status.clone();
        changed[0].pattern.live.pop();
        let kits = crate::kits::KitLibrary::open(dir.clone());
        let mut other = FxView::new(48_000, kits);
        other.kit_choice[0] = "test".into();
        other.restore(&changed, &mut Vec::new());
        assert!(!other.patterns[0].customized());
        assert!((other.patterns[0].knobs[1] - 0.9).abs() < 1e-6);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mapped_knobs_and_buttons_drive_the_drum_machine() {
        let mut v = view_with(kit(3));
        let mut out = Vec::new();
        v.apply(
            Action::FxKnob { unit: 0, knob: 0 },
            Trigger::Set(0.3),
            &mut out,
        );
        v.apply(
            Action::FxKnob { unit: 0, knob: 2 },
            Trigger::Set(0.6),
            &mut out,
        );
        v.apply(
            Action::FxButton { unit: 0, button: 0 },
            Trigger::Press,
            &mut out,
        );
        v.apply(
            Action::FxButton { unit: 0, button: 2 },
            Trigger::Press,
            &mut out,
        );
        assert_eq!(
            names(&out),
            [
                "PatternKnob(0, 2, 0.3)",
                "PatternKnob(0, 0, 0.6)",
                "PatternSound(0, 2)",
                "PatternDuck(0, true)"
            ]
        );
        assert_eq!(v.patterns[0].pattern_label(), "S2 P3");
    }

    #[test]
    fn roll_is_held_with_its_patterns_size() {
        let mut v = view_with(kit(1));
        let mut out = Vec::new();
        v.apply(Action::PatternRoll { unit: 0 }, Trigger::Press, &mut out);
        v.apply(Action::PatternRoll { unit: 0 }, Trigger::Press, &mut out);
        v.apply(Action::PatternRoll { unit: 0 }, Trigger::Release, &mut out);
        assert_eq!(
            names(&out),
            ["PatternRoll(0, true)", "PatternRoll(0, false)"]
        );
        assert_eq!(v.patterns[0].live_steps().per_beat, 8);
        assert_eq!(resolution_label(16), "1/16");
        v.apply(
            Action::PatternStep { unit: 0, step: 3 },
            Trigger::Press,
            &mut out,
        );
        assert!(names(&out).contains(&"PatternStep(0, 6, true)".to_owned()));
    }

    #[test]
    fn controllers_see_the_sixteen_mapped_steps() {
        let mut v = view_with(kit(1));
        let mut out = Vec::new();
        assert_eq!(v.mapped_steps(0), (0b1, 32), "cell 0 of 32");
        v.apply(
            Action::PatternStep { unit: 0, step: 3 },
            Trigger::Press,
            &mut out,
        );
        assert_eq!(v.mapped_steps(0), (0b1001, 32));
        assert_eq!(v.mapped_steps(9), (0, 0));
    }

    #[test]
    fn edited_kits_keep_live_changes() {
        let at = |bits: u64, per_beat: u32| Steps { bits, per_beat };
        let old = [[at(0b1, 4); PATTERNS]];
        let mut live = old;
        live[0][0] = at(0b10_0001, 4);
        live[0][2] = at(0b11, 4);
        let mut new = old;
        new[0][0] = at(0b11, 4);
        new[0][1] = at(0b111, 4);
        new[0][2] = at(0b1, 8);
        assert!(merge_steps(&mut live, &old, &new));
        assert_eq!(live[0][0], at(0b10_0001, 4), "changed live: stays");
        assert_eq!(live[0][1], at(0b111, 4), "untouched: the kit's");
        assert_eq!(live[0][2], at(0b1, 8), "new resolution: the kit's");
        assert!(!merge_steps(&mut live, &old, &[]), "sounds changed");
    }

    #[test]
    fn a_kit_gone_from_the_library_is_dropped() {
        let mut v = view_with(kit(2));
        let mut out = Vec::new();
        v.refresh_kits(&mut out);
        assert!(v.patterns[0].kit.is_none());
        assert!(out.is_empty(), "no other kit to load");
    }

    #[test]
    fn an_off_unit_shows_no_playing_step() {
        let mut v = view_with(kit(1));
        let mut state = backspn_engine::RackState::default();
        state.units[0].pattern.step = Some(5);
        v.set_pattern_steps(&state);
        assert_eq!(v.patterns[0].step, None, "off");
        state.units[0].on = true;
        v.set_pattern_steps(&state);
        assert_eq!(v.patterns[0].step, Some(5));
    }
}
