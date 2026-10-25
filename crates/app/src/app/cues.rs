use backspn_control::{Action, Trigger};
use backspn_engine::{Command, CueKind, DECKS, DeckCommand, EngineState};

use crate::App;
use crate::cue_list::{self, CueList};

const MOVE_STEP_S: f64 = 0.010;

impl App {
    pub(crate) fn cue_action(
        &mut self,
        action: Action,
        trigger: Trigger,
        state: &EngineState,
    ) -> bool {
        let pressed = trigger == Trigger::Press;
        let Some(&mut d) = action.clone().deck_mut().filter(|&&mut d| d < DECKS) else {
            return false;
        };
        let s = &state.decks[d];
        let loop_beats = self.decks[d].loop_beats();
        let Some(cues) = self.decks[d].cues_mut() else {
            return is_cue_action(action);
        };
        let mut jump = None;
        let changed = match action {
            Action::Hotcue { slot, .. } if pressed => {
                cues.select_hotcue(slot);
                return false;
            }
            Action::CueStore { .. } if pressed => store(cues, cue_list::active_cue(s)),
            Action::CueSetStore { .. } if pressed => store(cues, cue_list::cue_here(s)),
            Action::CueDelete { .. } if pressed => cues.delete_selected(),
            Action::CueJump { direction, .. } => {
                let forward = match trigger {
                    Trigger::Press => direction >= 0.0,
                    Trigger::Delta(steps) if steps != 0.0 => (steps > 0.0) == (direction >= 0.0),
                    _ => return true,
                };
                jump = cues.navigate(forward, s.position_s);
                false
            }
            Action::CueMove { .. } => match trigger {
                Trigger::Delta(steps) => cues.move_selected(f64::from(steps) * MOVE_STEP_S),
                _ => return true,
            },
            Action::CueType { kind, .. } if pressed => {
                let kind = CueKind::from(kind);
                let start = cues
                    .selected()
                    .and_then(|i| cues.cues().nth(i))
                    .map(|c| c.start_s);
                let len = start.and_then(|start| cue_list::loop_length(s, start, loop_beats));
                cues.set_selected_type(kind, len)
            }
            Action::MapHotcue { slot, .. } if pressed => match cue_list::active_cue(s) {
                Some(active) => {
                    cues.map_hotcue(active, slot);
                    true
                }
                None => false,
            },
            _ => return is_cue_action(action),
        };
        let hotcues = cues.hotcues();
        if changed {
            self.outbox
                .push(Command::Deck(d, DeckCommand::SetHotcues(hotcues)));
            if let Some(grid) = s.grid {
                let grid = self.decks[d].flex(grid);
                self.outbox
                    .push(Command::Deck(d, DeckCommand::SetGrid(Some(grid))));
            }
            self.save_cues(d);
        }
        if let Some(point) = jump {
            self.outbox
                .push(Command::Deck(d, DeckCommand::JumpToCue(point)));
        }
        true
    }
}

impl App {
    pub(crate) fn pick_cue(&mut self, deck: usize, index: usize) {
        if let Some(point) = self.decks[deck].cues_mut().and_then(|c| c.select(index)) {
            self.outbox
                .push(Command::Deck(deck, DeckCommand::JumpToCue(point)));
        }
    }

    pub(crate) fn rename_cue(&mut self, deck: usize, name: &str) {
        if self.decks[deck]
            .cues_mut()
            .is_some_and(|c| c.rename_selected(name))
        {
            self.save_cues(deck);
        }
    }
}

fn store(cues: &mut CueList, active: Option<backspn_engine::HotcuePoint>) -> bool {
    active.map(|a| cues.store(a)).is_some()
}

fn is_cue_action(action: Action) -> bool {
    matches!(
        action,
        Action::CueStore { .. }
            | Action::CueSetStore { .. }
            | Action::CueDelete { .. }
            | Action::CueJump { .. }
            | Action::CueMove { .. }
            | Action::CueType { .. }
            | Action::MapHotcue { .. }
    )
}
