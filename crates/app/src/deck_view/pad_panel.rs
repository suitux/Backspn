use backspn_control::pad_modes::PADS;
use backspn_engine::DeckState;
use eframe::egui::{Ui, vec2};

use crate::i18n::tr;
use crate::theme;
use crate::widgets;

use super::DeckView;

/// The pads of a controller that changes them with mode buttons, in the
/// mode it is in.
#[derive(Debug, Clone, PartialEq)]
pub struct PadModeView {
    pub names: Vec<String>,
    /// The mode shown.
    pub mode: usize,
    /// The mode the controller is in.
    pub controller_mode: usize,
    pub pads: [PadLabel; PADS],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PadLabel {
    pub text: String,
    pub lit: bool,
    /// The hot cue the pad plays, shown in its colour.
    pub hotcue: Option<usize>,
}

impl DeckView {
    pub(super) fn pad_mode_panel(&mut self, ui: &mut Ui, state: &DeckState) {
        let Some(view) = &self.pad_modes else {
            return;
        };
        let t = theme::get(ui.ctx());
        let mut picked = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
            for (i, name) in view.names.iter().enumerate() {
                let colour = if i == view.mode { t.accent } else { t.text_dim };
                let hint = if i == view.controller_mode {
                    tr("The mode the controller is in")
                } else {
                    tr("Show the pads of this mode; the controller's mode buttons switch back")
                };
                if widgets::chip_button(ui, name, colour)
                    .on_hover_text(hint)
                    .clicked()
                {
                    picked = Some(i);
                }
            }
        });
        if let Some(i) = picked {
            self.pad_mode_pick = (i != view.controller_mode).then_some(i);
        }
        let Some(view) = &self.pad_modes else {
            return;
        };
        ui.horizontal(|ui| {
            let gap = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - gap * 7.0) / 8.0).max(24.0);
            for pad in &view.pads {
                let cue = pad
                    .hotcue
                    .and_then(|slot| state.hotcues.get(slot).copied().flatten());
                let fill = match cue {
                    Some(h) => Some(t.hotcue(h.kind)),
                    None => pad.lit.then_some(t.accent),
                };
                widgets::pad(ui, &pad.text, fill, vec2(width, 22.0))
                    .on_hover_text(tr("What this pad does on the controller in this mode"));
            }
        });
    }
}
