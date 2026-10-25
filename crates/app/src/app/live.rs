use backspn_engine::{Command, DECKS, DeckCommand, EngineState};

use crate::App;
use crate::sample_view::DeckKind;

const NO_INPUT: (u16, u16) = (u16::MAX, u16::MAX);

impl App {
    pub(crate) fn set_live(&mut self, deck: usize, on: bool, state: &EngineState) {
        if deck >= DECKS || self.decks[deck].live == on {
            return;
        }
        if on {
            if state.decks[deck].playing {
                let notice = crate::deck_informer::Notice::live_input_locked();
                crate::toast::warning(crate::deck_informer::toast_text(deck, &notice));
                self.decks[deck].notify(notice);
                return;
            }
            if self.deck_kind(deck) != DeckKind::Track {
                self.set_deck_kind(deck, DeckKind::Track);
            }
            self.outbox.push(Command::Deck(deck, DeckCommand::Pause));
            self.outbox.push(Command::Deck(deck, DeckCommand::Unload));
        }
        self.decks[deck].live = on;
        self.send_inputs();
    }

    pub(crate) fn set_deck_type(
        &mut self,
        deck: usize,
        deck_type: crate::status::DeckType,
        state: &EngineState,
    ) {
        use crate::status::DeckType;
        match deck_type {
            DeckType::Track => {
                self.set_live(deck, false, state);
                self.set_deck_kind(deck, DeckKind::Track);
            }
            DeckType::Sample => {
                self.set_live(deck, false, state);
                self.set_deck_kind(deck, DeckKind::SampleDeck);
            }
            DeckType::LiveInput => self.set_live(deck, true, state),
        }
    }

    pub(crate) fn toggle_live_input(&mut self, deck: usize, state: &EngineState) {
        if deck >= DECKS {
            return;
        }
        if self.decks[deck].live {
            self.set_live(deck, false, state);
            if deck >= 2 {
                self.set_deck_kind(deck, DeckKind::SampleDeck);
            }
        } else {
            self.set_live(deck, true, state);
        }
    }

    pub(crate) fn send_inputs(&mut self) {
        let routing = self.settings.input_routing;
        for d in 0..DECKS {
            let input = self.decks[d]
                .live
                .then(|| routing.decks[d].unwrap_or(NO_INPUT));
            self.outbox.push(Command::LiveInput(d, input));
        }
        let gain = self.settings.microphone_gain;
        self.outbox
            .push(Command::Microphone(routing.microphone.map(|r| (r, gain))));
        self.outbox.push(Command::SendInput(routing.send));
        let source = self.settings.recording_source.engine(&routing);
        self.outbox.push(Command::RecordSource(source));
    }
}
