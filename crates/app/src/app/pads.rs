use backspn_control::pads::{self, KeySlot};
use backspn_control::{Action, MoveSize, Trigger};
use backspn_engine::{Command, DECKS, DeckCommand, EngineState};

use crate::App;

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
