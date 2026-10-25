use std::time::Instant;

use backspn_control::{MoveMode, MoveSize};
use backspn_engine::{Command, DeckCommand, DeckState};
use eframe::egui::{Button, ComboBox, Ui, vec2};

use crate::i18n::tr;
use crate::repeat;
use crate::theme;
use crate::widgets;

use super::DeckView;

const MENU_SIZES: std::ops::RangeInclusive<u8> = 2..=11;

impl DeckView {
    pub(super) fn move_panel(&mut self, ui: &mut Ui, state: &DeckState, out: &mut Vec<Command>) {
        let t = theme::get(ui.ctx());
        let h = 24.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ComboBox::from_id_salt(("move mode", self.index))
                .width(104.0)
                .selected_text(self.move_mode.label())
                .show_ui(ui, |ui| {
                    for mode in MoveMode::ALL {
                        ui.selectable_value(&mut self.move_mode, mode, mode.label());
                    }
                })
                .response
                .on_hover_text(tr(
                    "What ◀ / ▶ move: the playhead, the loop or one of its ends",
                ));
            if widgets::toggle(ui, "FINE", self.move_size == MoveSize::FINE, t.accent)
                .on_hover_text(tr("Steps of 1/32 beat"))
                .clicked()
            {
                self.move_size = MoveSize::FINE;
            }
            let in_menu = MENU_SIZES.contains(&self.move_size.index());
            ComboBox::from_id_salt(("move size", self.index))
                .width(72.0)
                .selected_text(if in_menu {
                    self.move_size.label()
                } else {
                    "beats".into()
                })
                .show_ui(ui, |ui| {
                    for size in MENU_SIZES.filter_map(MoveSize::new) {
                        ui.selectable_value(&mut self.move_size, size, size.label());
                    }
                })
                .response
                .on_hover_text(tr("Step size in beats"));
            if widgets::toggle(ui, "LOOP", self.move_size == MoveSize::LOOP, t.accent)
                .on_hover_text(tr("Steps of the loop size"))
                .clicked()
            {
                self.move_size = MoveSize::LOOP;
            }
            let mut down = None;
            for (label, steps) in [("◀", -1), ("▶", 1)] {
                let button =
                    ui.add_enabled(state.loaded, Button::new(label).min_size(vec2(32.0, h)));
                if button.is_pointer_button_down_on() {
                    down = Some(steps);
                }
            }
            if down.is_none() && ui.input(|i| i.pointer.primary_down()) {
                down = self.move_held.map(|(steps, ..)| steps);
            }
            self.hold_move(down, out);
            if down.is_some() {
                ui.ctx().request_repaint();
            }
        });
    }

    fn hold_move(&mut self, down: Option<i32>, out: &mut Vec<Command>) {
        let now = Instant::now();
        match (down, self.move_held) {
            (Some(steps), Some((held, since, done))) if held == steps => {
                let due = repeat::repeats_due(now - since);
                for _ in done..due {
                    out.extend(self.move_command(steps).map(|c| self.deck(c)));
                }
                self.move_held = Some((steps, since, due.max(done)));
            }
            (Some(steps), _) => {
                if self.move_mode == MoveMode::BeatJump {
                    out.push(self.deck(DeckCommand::SlipJump(true)));
                }
                out.extend(self.move_command(steps).map(|c| self.deck(c)));
                self.move_held = Some((steps, now, 0));
            }
            (None, Some(_)) => {
                out.push(self.deck(DeckCommand::SlipJump(false)));
                self.move_held = None;
            }
            (None, None) => {}
        }
    }
}
