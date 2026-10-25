use backspn_control::Action;
use backspn_engine::{CueKind, DeckState, HOTCUES};
use backspn_library::CuePoint;
use eframe::egui::{Button, ComboBox, TextEdit, Ui, vec2};

use crate::cue_list::CueList;
use crate::i18n::tr;
use crate::theme;
use crate::widgets;

use super::{DeckEvent, DeckView};

const TYPES: [(CueKind, &str); 5] = [
    (CueKind::Cue, "Cue"),
    (CueKind::FadeIn, "Fade-In"),
    (CueKind::FadeOut, "Fade-Out"),
    (CueKind::Load, "Load"),
    (CueKind::Loop, "Loop"),
];

pub(super) fn position_label(seconds: f64) -> String {
    let tenths = (seconds.max(0.0) * 10.0).round() as u64;
    format!(
        "{:02}:{:02}.{}",
        tenths / 600,
        tenths / 10 % 60,
        tenths % 10
    )
}

pub(super) fn list_entry(index: usize, name: &str) -> String {
    format!("{index:02}: {name}")
}

fn type_name(kind: i32) -> &'static str {
    TYPES
        .iter()
        .find(|(k, _)| i32::from(*k) == kind)
        .map_or("Grid", |(_, name)| name)
}

impl DeckView {
    pub(super) fn cue_panel(&mut self, ui: &mut Ui, state: &DeckState) {
        let mut events = Vec::new();
        let (entries, selected): (Vec<(usize, String)>, Option<CuePoint>) = self
            .cues()
            .map(|list| {
                let entries = list
                    .cues()
                    .enumerate()
                    .filter(|(_, c)| c.kind != CuePoint::GRID)
                    .map(|(i, c)| (i, list_entry(i, &c.name)))
                    .collect();
                (entries, list.selected_cue().cloned())
            })
            .unwrap_or_default();
        let selected_index = self.cues().and_then(CueList::selected);
        self.cue_info_row(
            ui,
            state,
            (&entries, selected_index, selected.as_ref()),
            &mut events,
        );
        self.cue_button_row(ui, state, selected.is_some(), &mut events);
        self.events.extend(events);
    }

    fn cue_info_row(
        &mut self,
        ui: &mut Ui,
        state: &DeckState,
        (entries, selected_index, selected): (&[(usize, String)], Option<usize>, Option<&CuePoint>),
        events: &mut Vec<DeckEvent>,
    ) {
        let deck = self.index;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let shown = selected_index
                .and_then(|i| entries.iter().find(|(j, _)| *j == i))
                .map_or("---", |(_, e)| e.as_str());
            ComboBox::from_id_salt(("cue list", deck))
                .width(110.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (i, entry) in entries {
                        if ui
                            .selectable_label(selected_index == Some(*i), entry)
                            .clicked()
                        {
                            events.push(DeckEvent::CuePick { deck, index: *i });
                        }
                    }
                });
            let mut name = self.cue_name.clone().unwrap_or_else(|| {
                selected
                    .as_ref()
                    .map(|c| c.name.clone())
                    .unwrap_or_default()
            });
            let edit = ui.add_enabled(
                selected.is_some(),
                TextEdit::singleline(&mut name).desired_width(110.0),
            );
            if edit.changed() {
                self.cue_name = Some(name.clone());
            }
            if edit.lost_focus() {
                if let Some(name) = self.cue_name.take() {
                    events.push(DeckEvent::CueRename { deck, name });
                }
            } else if !edit.has_focus() {
                self.cue_name = None;
            }
            let kind = selected.map(|c| c.kind);
            ui.add_enabled_ui(selected.is_some(), |ui| {
                ComboBox::from_id_salt(("cue type", deck))
                    .width(80.0)
                    .selected_text(kind.map_or("Floating", type_name))
                    .show_ui(ui, |ui| {
                        for (k, label) in TYPES {
                            let id = i32::from(k);
                            if ui.selectable_label(kind == Some(id), label).clicked() {
                                events.push(DeckEvent::Action(Action::CueType { deck, kind: id }));
                            }
                        }
                    });
            });
            let at = selected.map_or(state.cue_s, |c| c.start_s);
            ui.label(if state.loaded {
                position_label(at)
            } else {
                "---".to_owned()
            });
        });
    }

    fn cue_button_row(
        &mut self,
        ui: &mut Ui,
        state: &DeckState,
        has_selection: bool,
        events: &mut Vec<DeckEvent>,
    ) {
        let t = theme::get(ui.ctx());
        let deck = self.index;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let h = 24.0;
            if ui
                .add_sized(vec2(28.0, h), Button::new("⏮"))
                .on_hover_text(tr("Previous cue"))
                .clicked()
            {
                events.push(DeckEvent::Action(Action::CueJump {
                    deck,
                    direction: -1.0,
                }));
            }
            if ui
                .add_sized(vec2(28.0, h), Button::new("⏭"))
                .on_hover_text(tr("Next cue"))
                .clicked()
            {
                events.push(DeckEvent::Action(Action::CueJump {
                    deck,
                    direction: 1.0,
                }));
            }
            if ui
                .add_sized(vec2(52.0, h), Button::new("STORE"))
                .on_hover_text(tr("Store the cue point or loop on the next free hotcue"))
                .clicked()
            {
                events.push(DeckEvent::Action(Action::CueStore { deck }));
            }
            if widgets::toggle(ui, "MAP", self.cue_map, t.accent)
                .on_hover_text(tr(
                    "Then click a hotcue to store the cue point or loop on it",
                ))
                .clicked()
            {
                self.cue_map = !self.cue_map;
            }
            if ui
                .add_enabled(has_selection, Button::new("DELETE").min_size(vec2(52.0, h)))
                .clicked()
            {
                events.push(DeckEvent::Action(Action::CueDelete { deck }));
            }
            let gap = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - gap * 7.0) / 8.0).clamp(20.0, 34.0);
            for slot in 0..HOTCUES {
                let fill = state.hotcues[slot].map(|h| t.hotcue(h.kind));
                if widgets::pad(ui, &(slot + 1).to_string(), fill, vec2(width, h)).clicked() {
                    if self.cue_map {
                        self.cue_map = false;
                        events.push(DeckEvent::Action(Action::MapHotcue { deck, slot }));
                    } else {
                        events.push(DeckEvent::Action(Action::Hotcue { deck, slot }));
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(position_label(83.44), "01:23.4");
        assert_eq!(position_label(59.96), "01:00.0");
        assert_eq!(position_label(-1.0), "00:00.0");
        assert_eq!(list_entry(3, "Drop"), "03: Drop");
        assert_eq!(type_name(2), "Fade-Out");
        assert_eq!(type_name(4), "Grid");
    }
}
