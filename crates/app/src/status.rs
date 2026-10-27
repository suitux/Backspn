use std::time::{Duration, Instant};

use backspn_control::{MoveMode, MoveSize};
use backspn_engine::{
    Command, DECKS, DEFAULT_CLOCK_BPM, DeckCommand, EngineState, FX_UNITS, SyncCommand,
};
use serde::{Deserialize, Serialize};

use crate::App;
use crate::fx_view::UnitStatus;
use crate::mixer_view::MixerStatus;
use crate::sample_view::DeckKind;

const SYNC_EVERY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct DeckStatus {
    pub loop_beats: f64,
    pub move_mode: MoveMode,
    pub move_size: MoveSize,
    pub keylock: bool,
    pub key_adjust: f32,
    pub tempo: f64,
    #[serde(alias = "flux")]
    pub slip: bool,
    #[serde(alias = "flavor")]
    pub deck_kind: DeckKind,
    pub live: bool,
}

impl Default for DeckStatus {
    fn default() -> Self {
        Self {
            loop_beats: 4.0,
            move_mode: MoveMode::BeatJump,
            move_size: MoveSize::of_loop(4.0),
            keylock: true,
            key_adjust: 0.0,
            tempo: 1.0,
            slip: false,
            deck_kind: DeckKind::default(),
            live: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeckType {
    Track,
    Sample,
    LiveInput,
}

impl DeckType {
    pub const ALL: [Self; 3] = [Self::Track, Self::Sample, Self::LiveInput];

    pub fn of(deck_kind: DeckKind, live: bool) -> Self {
        if live {
            Self::LiveInput
        } else if deck_kind == DeckKind::SampleDeck {
            Self::Sample
        } else {
            Self::Track
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Track => "Track",
            Self::Sample => "Sample",
            Self::LiveInput => "Live Input",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Status {
    pub decks: [DeckStatus; DECKS],
    pub mixer: MixerStatus,
    pub clock_bpm: f64,
    pub clock_external: bool,
    pub fx: Option<[UnitStatus; FX_UNITS]>,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            decks: [DeckStatus::default(); DECKS],
            mixer: MixerStatus::default(),
            clock_bpm: DEFAULT_CLOCK_BPM,
            clock_external: false,
            fx: None,
        }
    }
}

impl App {
    pub(crate) fn restore_status(&mut self) {
        let status = self.settings.status.clone();
        self.mixer.restore(&status.mixer);
        self.outbox.extend(self.mixer.all_commands());
        for (d, deck) in status.decks.iter().enumerate() {
            let view = &mut self.decks[d];
            view.set_loop_beats(deck.loop_beats);
            view.set_move_mode(deck.move_mode);
            view.set_move_size(deck.move_size);
            self.outbox.extend([
                Command::Deck(d, DeckCommand::SetKeylock(deck.keylock)),
                Command::Deck(d, DeckCommand::SetKeyAdjust(deck.key_adjust)),
                Command::Deck(d, DeckCommand::SetTempo(deck.tempo)),
                Command::Deck(d, DeckCommand::SetSlip(deck.slip)),
            ]);
            self.set_deck_kind(d, deck.deck_kind);
            if deck.live {
                let state = self.engine_state();
                self.set_live(d, true, &state);
            }
        }
        self.clock_bpm = status.clock_bpm;
        self.outbox.extend([
            Command::Sync(SyncCommand::SetClockBpm(status.clock_bpm)),
            Command::Sync(SyncCommand::SetExternal(status.clock_external)),
        ]);
        if let Some(fx) = &status.fx {
            let mut out = Vec::new();
            self.fx.restore(fx, &mut out);
            self.outbox.extend(out);
            self.outbox.extend(self.fx.all_commands());
        }
    }

    pub(crate) fn current_status(&self, state: &EngineState) -> Status {
        Status {
            decks: std::array::from_fn(|d| DeckStatus {
                loop_beats: self.decks[d].loop_beats(),
                move_mode: self.decks[d].move_mode(),
                move_size: self.decks[d].move_size(),
                keylock: state.decks[d].keylock,
                key_adjust: state.decks[d].key_adjust,
                tempo: state.decks[d].tempo,
                slip: state.decks[d].slip,
                deck_kind: self.deck_kind(d),
                live: self.decks[d].live,
            }),
            mixer: self.mixer.status(),
            clock_bpm: state.sync.clock_bpm,
            clock_external: state.sync.external,
            fx: Some(self.fx.status()),
        }
    }

    pub(crate) fn sync_status(&mut self, state: &EngineState) {
        if self.status_synced.elapsed() >= SYNC_EVERY {
            self.status_synced = Instant::now();
            self.settings.status = self.current_status(state);
        }
    }

    pub(crate) fn save_status(&mut self) {
        let state = self.engine_state();
        self.settings.status = self.current_status(&state);
        if let Err(e) = self.settings.save() {
            crate::toast::error(format!("Cannot save the settings: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_input_wins_over_the_kind_and_the_kind_over_track() {
        assert_eq!(DeckType::of(DeckKind::Track, false), DeckType::Track);
        assert_eq!(DeckType::of(DeckKind::SampleDeck, false), DeckType::Sample);
        assert_eq!(DeckType::of(DeckKind::Track, true), DeckType::LiveInput);
        assert_eq!(
            DeckType::of(DeckKind::SampleDeck, true),
            DeckType::LiveInput
        );
    }
}
