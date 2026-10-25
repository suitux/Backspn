use backspn_engine::{Command, DeckCommand, DeckState, HOTCUES, SLICE_SIZES};
use eframe::egui::{Button, ComboBox, Sense, Ui, vec2};

use crate::i18n::tr;
use crate::theme;
use crate::widgets;

use super::{ARROW, DeckView, SIZE_BUTTON, SIZES};

impl DeckView {
    #[allow(clippy::too_many_lines)]
    pub(super) fn bottom_bar(&mut self, ui: &mut Ui, state: &DeckState, out: &mut Vec<Command>) {
        let t = theme::get(ui.ctx());
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.spacing_mut().button_padding = vec2(3.0, 4.0);
            let h = 30.0;
            let icon = if state.playing {
                widgets::Icon::Pause
            } else {
                widgets::Icon::Play
            };
            if widgets::transport_icon(ui, icon, state.playing, t.play, vec2(44.0, h)).clicked() {
                out.push(self.deck(DeckCommand::TogglePlay));
            }
            let held = Sense::click_and_drag();
            let small = vec2(42.0, h);
            let cue = widgets::transport(ui, "CUE", self.cue_button.down(), t.cue, small, held);
            if let Some(down) = self.cue_button.button(&cue) {
                out.push(self.deck(if down {
                    DeckCommand::CueDown
                } else {
                    DeckCommand::CueUp
                }));
            }
            let cup = widgets::transport(ui, "CUP", self.cup_button.down(), t.cue, small, held);
            if let Some(down) = self.cup_button.button(&cup) {
                out.push(self.deck(if down {
                    DeckCommand::CupDown
                } else {
                    DeckCommand::CupUp
                }));
            }
            if widgets::transport(ui, "SLIP", state.slip, t.accent, small, Sense::click())
                .on_hover_text(tr("Slip mode"))
                .clicked()
            {
                out.push(self.deck(DeckCommand::SetSlip(!state.slip)));
            }
            let rev = widgets::transport(ui, "REV", state.reverse, t.accent, small, held)
                .on_hover_text(tr("Reverse while held"));
            if let Some(down) = self.rev_button.button(&rev) {
                out.push(self.deck(DeckCommand::Reverse(down)));
            }
            ui.add_space(2.0);

            let mut tail = 34.0 + 40.0 + 34.0 + 3.0 * 4.0 + 6.0;
            if !self.compact {
                tail += 28.0 + 4.0;
            }
            let room = ui.available_width() - tail - 2.0 * (ARROW + 4.0);
            let visible = ((room / (SIZE_BUTTON + 4.0)).floor() as usize).clamp(1, SIZES.len());
            if self.shown_size != Some((self.loop_beats, visible))
                && let Some(at) = SIZES
                    .iter()
                    .position(|(v, _)| (v - self.loop_beats).abs() < 1e-9)
            {
                self.size_offset = at.saturating_sub(visible / 2);
                self.shown_size = Some((self.loop_beats, visible));
            }
            self.size_offset = self.size_offset.min(SIZES.len() - visible);
            if ui
                .add_enabled_ui(self.size_offset > 0, |ui| {
                    ui.add_sized(vec2(ARROW, h), Button::new("<"))
                })
                .inner
                .clicked()
            {
                self.size_offset -= 1;
            }
            for &(beats, label) in &SIZES[self.size_offset..self.size_offset + visible] {
                let selected = (beats - self.loop_beats).abs() < 1e-9;
                let active = selected && state.loop_active;
                let fill = if active {
                    Some(t.play)
                } else if selected {
                    Some(t.accent)
                } else {
                    None
                };
                let response = widgets::pad(ui, label, fill, vec2(SIZE_BUTTON, h))
                    .on_hover_text(tr("Loop of this size (also the beat-jump size)"));
                if response.clicked() {
                    self.loop_beats = beats;
                    out.push(self.deck(DeckCommand::AutoLoop { beats }));
                }
            }
            if ui
                .add_enabled_ui(self.size_offset + visible < SIZES.len(), |ui| {
                    ui.add_sized(vec2(ARROW, h), Button::new(">"))
                })
                .inner
                .clicked()
            {
                self.size_offset += 1;
            }
            ui.add_space(6.0);
            if ui.add_sized(vec2(34.0, h), Button::new("IN")).clicked() {
                out.push(self.deck(DeckCommand::LoopIn));
            }
            if ui.add_sized(vec2(40.0, h), Button::new("OUT")).clicked() {
                out.push(self.deck(DeckCommand::LoopOut));
            }
            let mut looping =
                widgets::pad(ui, "⟲", state.loop_active.then_some(t.play), vec2(34.0, h));
            if state.loop_active {
                looping = looping
                    .interact(eframe::egui::Sense::drag())
                    .on_hover_text(tr(
                        "Loop on / off. Drag the loop onto a sample deck cell or onto Loops; right-click to save it",
                    ));
                Self::loop_drag_source(&looping, self.index, &mut self.pending_save_loop);
            } else {
                looping = looping.on_hover_text(tr("Loop on / off"));
            }
            if looping.clicked() {
                let command = if state.loop_end_s.is_some() {
                    DeckCommand::LoopToggle
                } else {
                    DeckCommand::AutoLoop {
                        beats: self.loop_beats,
                    }
                };
                out.push(self.deck(command));
            }
            if !self.compact
                && ui
                    .add_sized(
                        vec2(28.0, h),
                        Button::new(if self.show_pads { "˄" } else { "˅" }),
                    )
                    .on_hover_text(tr("Hotcue pads"))
                    .clicked()
            {
                self.show_pads = !self.show_pads;
            }
        });
    }

    pub(super) fn hotcue_row(&mut self, ui: &mut Ui, state: &DeckState, out: &mut Vec<Command>) {
        let t = theme::get(ui.ctx());
        ui.horizontal(|ui| {
            let freeze = state.freeze;
            let frz = widgets::toggle(ui, "FRZ", freeze.is_some(), t.accent)
                .on_hover_text(tr("Freeze: the pads play slices of the region from here"));
            if frz.clicked() {
                out.push(self.deck(DeckCommand::SetFreeze {
                    on: freeze.is_none(),
                    loop_beats: self.loop_beats,
                }));
            }
            if let Some(view) = freeze {
                self.slice_size(ui, view.size_index, out);
            }
            let gap = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - gap * 7.0) / 8.0).max(24.0);
            if let Some(view) = freeze {
                self.slice_pads(ui, view, width, out);
                return;
            }
            for n in 0..HOTCUES {
                let fill = state.hotcues[n].map(|h| t.hotcue(h.kind));
                let response = widgets::pad(ui, &(n + 1).to_string(), fill, vec2(width, 24.0))
                    .on_hover_text(tr("Click: set / jump · Right-click: delete"));
                if response.clicked() {
                    out.push(self.deck(DeckCommand::Hotcue(n)));
                }
                if response.secondary_clicked() {
                    out.push(self.deck(DeckCommand::DeleteHotcue(n)));
                }
            }
        });
    }

    fn slice_size(&mut self, ui: &mut Ui, index: usize, out: &mut Vec<Command>) {
        let label = |i: usize| match SLICE_SIZES[i] {
            0.25 => "1/4".to_owned(),
            0.5 => "1/2".to_owned(),
            b => format!("{b}"),
        };
        let mut chosen = index;
        ComboBox::from_id_salt(("slice size", self.index))
            .width(48.0)
            .selected_text(label(index))
            .show_ui(ui, |ui| {
                for i in 0..SLICE_SIZES.len() {
                    ui.selectable_value(&mut chosen, i, label(i));
                }
            })
            .response
            .on_hover_text(tr("Slice size in beats (an active loop is cut into eight)"));
        if chosen != index {
            out.push(self.deck(DeckCommand::SetSliceSize(chosen)));
        }
    }

    fn slice_pads(
        &mut self,
        ui: &mut Ui,
        view: backspn_engine::FreezeView,
        width: f32,
        out: &mut Vec<Command>,
    ) {
        let t = theme::get(ui.ctx());
        for n in 0..self.slice_buttons.len() {
            let held = view.held & (1 << n) != 0;
            let fill = if held {
                Some(t.accent)
            } else if view.current == Some(n) {
                Some(t.play)
            } else {
                None
            };
            let response = widgets::transport(
                ui,
                &(n + 1).to_string(),
                held,
                fill.unwrap_or(t.accent),
                vec2(width, 24.0),
                Sense::click_and_drag(),
            )
            .on_hover_text(tr("Plays this slice while held"));
            if view.current == Some(n) && !held {
                ui.painter().rect_stroke(
                    response.rect,
                    4.0,
                    eframe::egui::Stroke::new(2.0, t.play),
                    eframe::egui::StrokeKind::Inside,
                );
            }
            if let Some(pressed) = self.slice_buttons[n].button(&response) {
                out.push(self.deck(DeckCommand::FreezeSlice { slice: n, pressed }));
            }
        }
    }
}
