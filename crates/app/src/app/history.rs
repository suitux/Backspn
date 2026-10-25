use std::time::Instant;

use backspn_engine::{DECKS, EngineState};

use crate::{App, played};

#[derive(Debug, Default)]
pub(crate) struct History {
    pub(crate) session: Option<i64>,
    pub(crate) last_deck: Option<usize>,
    pub(crate) entry: [Option<i64>; DECKS],
}

impl App {
    pub(crate) fn track_played_state(&mut self, state: &EngineState) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        let samples: [played::DeckSample; DECKS] = std::array::from_fn(|d| played::DeckSample {
            track: self.decks[d].track_id(),
            playing: state.decks[d].playing,
            fader: state.mixer.channels[d].fader,
        });
        let events = self
            .play_clock
            .tick(dt, &samples, self.settings.played_state_delay);
        for event in events {
            if let Err(e) = self.apply_play_event(event) {
                log::warn!("cannot store the played state: {e}");
            }
        }
    }

    fn apply_play_event(&mut self, event: played::PlayEvent) -> backspn_library::Result<()> {
        let Some(lib) = self.browser.library() else {
            return Ok(());
        };
        let now = backspn_library::clock::LocalTime::now();
        match event {
            played::PlayEvent::Played { deck, track } => {
                lib.mark_played(track, now)?;
                let session = match self.history.session {
                    Some(s) => s,
                    None => *self.history.session.insert(lib.start_history_session(now)?),
                };
                if self.history.last_deck == Some(deck) {
                    lib.set_last_history_entry_private(session)?;
                }
                self.history.last_deck = Some(deck);
                self.history.entry[deck] = Some(lib.add_history_entry(session, track, deck, now)?);
                self.browser.mark_played(track);
                self.browser.history_changed(session);
            }
            played::PlayEvent::Left { deck, seconds, .. } => {
                if let (Some(session), Some(position)) =
                    (self.history.session, self.history.entry[deck].take())
                {
                    lib.set_history_duration(session, position, seconds)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn close_history(&mut self) {
        let loaded: Vec<_> = self.play_clock.loaded().collect();
        for (deck, track, seconds) in loaded {
            let event = played::PlayEvent::Left {
                deck,
                track,
                seconds,
            };
            if let Err(e) = self.apply_play_event(event) {
                log::warn!("cannot store the history: {e}");
            }
        }
    }
}
