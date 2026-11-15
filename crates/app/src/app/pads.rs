use backspn_control::pads::{self, KeySlot};
use backspn_control::{Action, MoveSize, Trigger};
use backspn_engine::pad_fx;
use backspn_engine::{Command, DECKS, DeckCommand, EngineState};

use crate::App;
use crate::controls::PadPanel;
use crate::deck_view::{PadLabel, PadModeView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PadPages {
    pub key_shift: u8,
    pub beat_jump: u8,
    pub keyboard: u8,
    pub key_cue: Option<usize>,
}

impl Default for PadPages {
    fn default() -> Self {
        Self {
            key_shift: pads::KEY_PAGE_START,
            beat_jump: pads::BEAT_JUMP_PAGE_START,
            keyboard: pads::KEY_PAGE_START,
            key_cue: None,
        }
    }
}

impl App {
    fn key_cue(&mut self, deck: usize, state: &EngineState) -> Option<usize> {
        let cue = self.pad_pages[deck].key_cue?;
        let exists = state.decks[deck]
            .hotcues
            .get(cue)
            .is_some_and(Option::is_some);
        if !exists {
            self.pad_pages[deck].key_cue = None;
        }
        exists.then_some(cue)
    }

    fn keyboard_pad(
        &mut self,
        deck: usize,
        pad: usize,
        trigger: Trigger,
        state: &EngineState,
    ) -> Option<(Action, Trigger)> {
        let Some(cue) = self.key_cue(deck, state) else {
            let exists = state.decks[deck]
                .hotcues
                .get(pad)
                .is_some_and(Option::is_some);
            if !exists {
                return Some((Action::Hotcue { deck, slot: pad }, trigger));
            }
            if trigger == Trigger::Press {
                self.pad_pages[deck].key_cue = Some(pad);
            }
            return None;
        };
        if trigger != Trigger::Press {
            return None;
        }
        let slot = pads::key_slot(self.pad_pages[deck].keyboard, pad);
        self.key_slot_action(deck, slot, Some(cue), trigger)
    }

    fn key_slot_action(
        &mut self,
        deck: usize,
        slot: KeySlot,
        cue: Option<usize>,
        trigger: Trigger,
    ) -> Option<(Action, Trigger)> {
        let set = |app: &mut Self, k: f32| {
            app.outbox
                .push(Command::Deck(deck, DeckCommand::SetKeyAdjust(k)));
        };
        match slot {
            KeySlot::Semitone(k) => {
                set(self, k as f32);
                cue.map(|slot| (Action::Hotcue { deck, slot }, trigger))
            }
            KeySlot::Up | KeySlot::Down => {
                let direction = if slot == KeySlot::Up { 1.0 } else { -1.0 };
                Some((Action::KeySemitone { deck, direction }, trigger))
            }
            KeySlot::Reset => {
                set(self, 0.0);
                None
            }
            KeySlot::Sync | KeySlot::Empty => None,
        }
    }

    /// Shows on each deck what its controller's pads do, while the
    /// controller has pad modes.
    pub(crate) fn show_pad_modes(&mut self) {
        let Some(controls) = &self.controls else {
            return;
        };
        let picked = std::array::from_fn(|d| self.decks[d].pad_mode_pick);
        for (d, panel) in controls.pad_panels(picked).into_iter().enumerate() {
            let view = &mut self.decks[d];
            if let Some(p) = &panel {
                view.pad_mode_pick =
                    picked_after(view.pad_mode_pick, view.pad_mode_seen, p.controller_mode);
                view.pad_mode_seen = Some(p.controller_mode);
            }
            view.pad_modes = panel.map(|p| pad_mode_view(p, self.pad_pages[d]));
        }
    }

    pub(super) fn pad_layer_action(
        &mut self,
        action: Action,
        trigger: Trigger,
        state: &EngineState,
    ) -> Option<(Action, Trigger)> {
        let pressed = trigger == Trigger::Press;
        let playing = |d: usize| state.decks.get(d).is_some_and(|s| s.playing);
        let loaded = |d: usize| state.decks.get(d).is_some_and(|s| s.duration_s > 0.0);
        match action {
            Action::TempoRangeCycle { deck } if deck < DECKS => {
                if pressed {
                    let tempo = state.decks[deck].tempo as f32 - 1.0;
                    let view = &mut self.decks[deck];
                    view.set_tempo_range(pads::next_tempo_range(view.tempo_range(), tempo));
                }
                None
            }
            Action::FastLoopSize { deck, beats } if deck < DECKS => {
                if pressed {
                    self.outbox
                        .push(Command::Deck(deck, DeckCommand::FastLoop { beats }));
                }
                None
            }
            Action::FaderStart { deck, play: true } => {
                (pressed && !playing(deck)).then_some((Action::PlayPause { deck }, trigger))
            }
            Action::FaderStart { deck, play: false } => (trigger == Trigger::Release
                || playing(deck))
            .then_some((Action::Cue { deck }, trigger)),
            Action::KeyShiftPage { deck, direction } if deck < DECKS => {
                if pressed {
                    let p = &mut self.pad_pages[deck].key_shift;
                    *p = pads::key_page(*p, direction);
                }
                None
            }
            Action::KeyShiftPad { deck, pad } if deck < DECKS => {
                if !pressed || !loaded(deck) {
                    return None;
                }
                let slot = pads::key_slot(self.pad_pages[deck].key_shift, pad);
                self.key_slot_action(deck, slot, None, trigger)
            }
            Action::KeyboardPage { deck, direction } if deck < DECKS => {
                if pressed {
                    let p = &mut self.pad_pages[deck].keyboard;
                    *p = pads::key_page(*p, direction);
                }
                None
            }
            Action::KeyboardDelete { deck } if deck < DECKS => {
                let cue = self.key_cue(deck, state)?;
                pressed.then(|| {
                    self.pad_pages[deck].key_cue = None;
                    (Action::DeleteHotcue { deck, slot: cue }, trigger)
                })
            }
            Action::KeyboardPad { deck, pad } if deck < DECKS => {
                if !loaded(deck) {
                    return None;
                }
                self.keyboard_pad(deck, pad, trigger, state)
            }
            Action::BeatJumpPage {
                deck,
                direction,
                far,
            } if deck < DECKS => {
                if pressed {
                    let p = &mut self.pad_pages[deck].beat_jump;
                    *p = pads::beat_jump_page(*p, direction, far);
                }
                None
            }
            Action::PadFx { .. } | Action::PadFxBeat { .. } => {
                self.pad_fx_action(action, trigger);
                None
            }
            Action::BeatJumpPad { deck, pad } if deck < DECKS => {
                let (size, forward) = pads::beat_jump(self.pad_pages[deck].beat_jump, pad)?;
                let fine = MoveSize::FINE.beats().unwrap_or(0.0);
                let beats = size.unwrap_or(if forward { fine } else { -fine });
                Some((Action::BeatJump { deck, beats }, trigger))
            }
            Action::TempoRangeCycle { .. }
            | Action::FastLoopSize { .. }
            | Action::KeyShiftPage { .. }
            | Action::KeyShiftPad { .. }
            | Action::BeatJumpPage { .. }
            | Action::BeatJumpPad { .. }
            | Action::KeyboardPad { .. }
            | Action::KeyboardPage { .. }
            | Action::KeyboardDelete { .. } => None,
            a => Some((a, trigger)),
        }
    }
}

/// The mode picked on screen stays until the controller changes mode.
fn picked_after(picked: Option<usize>, seen: Option<usize>, controller: usize) -> Option<usize> {
    picked.filter(|_| seen.is_none_or(|s| s == controller))
}

/// A controller's pad panel as the deck shows it.
pub(crate) fn pad_mode_view(panel: PadPanel, pages: PadPages) -> PadModeView {
    PadModeView {
        names: panel.names,
        mode: panel.mode,
        controller_mode: panel.controller_mode,
        pads: std::array::from_fn(|i| {
            panel.pads[i].map_or_else(PadLabel::default, |action| PadLabel {
                text: pad_label(action, pages),
                lit: panel.lit[i],
                hotcue: hotcue_of(action, pages),
            })
        }),
    }
}

/// The hot cue a pad plays, to show its colour.
fn hotcue_of(action: Action, pages: PadPages) -> Option<usize> {
    match action {
        Action::Hotcue { slot, .. } => Some(slot),
        Action::KeyboardPad { pad, .. } if pages.key_cue.is_none() => Some(pad),
        _ => None,
    }
}

/// What a pad does, in a word or two.
fn pad_label(action: Action, pages: PadPages) -> String {
    let key = |page, pad| match pads::key_slot(page, pad) {
        KeySlot::Semitone(0) => "0".to_owned(),
        KeySlot::Semitone(k) => format!("{k:+}"),
        KeySlot::Up => "▲".to_owned(),
        KeySlot::Down => "▼".to_owned(),
        KeySlot::Reset => "RESET".to_owned(),
        KeySlot::Sync => "SYNC".to_owned(),
        KeySlot::Empty => String::new(),
    };
    match action {
        Action::Hotcue { slot, .. } => (slot + 1).to_string(),
        Action::PadFx { bank, pad, .. } => pad_fx::factory(bank, pad)
            .map_or_else(|| format!("FX {}", pad + 1), |p| p.kind.name().to_owned()),
        Action::BeatJumpPad { pad, .. } => pads::beat_jump(pages.beat_jump, pad)
            .map(|(size, forward)| {
                let arrow = if forward { "▶" } else { "◀" };
                let beats = size.map_or(MoveSize::FINE.beats().unwrap_or(0.0), f64::abs);
                format!("{arrow} {}", crate::screen::beats_label(beats))
            })
            .unwrap_or_default(),
        Action::SampleCell { slot, cell, .. } => format!("{}·{}", slot + 1, cell + 1),
        Action::KeyboardPad { pad, .. } if pages.key_cue.is_none() => (pad + 1).to_string(),
        Action::KeyboardPad { pad, .. } => key(pages.keyboard, pad),
        Action::KeyShiftPad { pad, .. } => key(pages.key_shift, pad),
        Action::LoopSize { beats, .. } => crate::screen::beats_label(beats),
        a => a.label(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mode_picked_on_screen_lasts_until_the_controller_switches() {
        assert_eq!(picked_after(Some(2), Some(0), 0), Some(2));
        assert_eq!(picked_after(Some(2), None, 0), Some(2));
        assert_eq!(
            picked_after(Some(2), Some(0), 5),
            None,
            "a mode button pressed"
        );
        assert_eq!(picked_after(None, Some(0), 5), None);
    }

    #[test]
    fn pads_say_what_they_do() {
        let pages = PadPages::default();
        let label = |action| pad_label(action, pages);
        assert_eq!(label(Action::Hotcue { deck: 0, slot: 2 }), "3");
        assert_eq!(
            label(Action::LoopSize {
                deck: 0,
                beats: 0.25
            }),
            "1/4"
        );
        assert_eq!(label(Action::BeatJumpPad { deck: 0, pad: 0 }), "◀ 1");
        assert_eq!(label(Action::BeatJumpPad { deck: 0, pad: 1 }), "▶ 1");
        assert_eq!(
            label(Action::SampleCell {
                deck: 2,
                slot: 1,
                cell: 0
            }),
            "2·1"
        );
        assert_eq!(label(Action::KeyShiftPad { deck: 0, pad: 3 }), "+7");
        assert_eq!(
            label(Action::KeyboardPad { deck: 0, pad: 3 }),
            "4",
            "pick a cue first"
        );
        let chosen = PadPages {
            key_cue: Some(0),
            ..pages
        };
        assert_eq!(
            pad_label(Action::KeyboardPad { deck: 0, pad: 4 }, chosen),
            "0"
        );
        assert_eq!(
            label(Action::PadFx {
                deck: 0,
                bank: 0,
                pad: 0
            }),
            pad_fx::factory(0, 0).unwrap().kind.name(),
            "the effect on the pad"
        );

        let panel = PadPanel {
            names: vec!["HOT CUE".into()],
            mode: 0,
            controller_mode: 0,
            pads: std::array::from_fn(|slot| {
                (slot < 2).then_some(Action::Hotcue { deck: 0, slot })
            }),
            lit: [true, false, false, false, false, false, false, false],
        };
        let view = pad_mode_view(panel, pages);
        assert_eq!(
            view.pads[0],
            PadLabel {
                text: "1".into(),
                lit: true,
                hotcue: Some(0)
            }
        );
        assert_eq!(
            view.pads[5],
            PadLabel::default(),
            "a pad with no job stays blank"
        );
    }
}
