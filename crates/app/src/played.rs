pub const AUDIBLE_FADER: f32 = 0.0625;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeckSample {
    pub track: Option<i64>,
    pub playing: bool,
    pub fader: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayEvent {
    Played {
        deck: usize,
        track: i64,
    },
    Left {
        deck: usize,
        track: i64,
        seconds: f64,
    },
}

#[derive(Debug, Clone, Default)]
pub struct PlayClock {
    decks: [(Option<i64>, f64); 4],
}

impl PlayClock {
    pub fn tick(&mut self, dt: f64, decks: &[DeckSample], delay: f64) -> Vec<PlayEvent> {
        let mut events = Vec::new();
        for (deck, (sample, (track, seconds))) in decks.iter().zip(&mut self.decks).enumerate() {
            if sample.track != *track {
                if let Some(old) = *track
                    && *seconds > 0.0
                {
                    events.push(PlayEvent::Left {
                        deck,
                        track: old,
                        seconds: *seconds,
                    });
                }
                *track = sample.track;
                *seconds = 0.0;
            }
            let Some(id) = *track else { continue };
            if !sample.playing || sample.fader <= AUDIBLE_FADER {
                continue;
            }
            let before = *seconds;
            *seconds += dt;
            if before <= delay && *seconds > delay {
                events.push(PlayEvent::Played { deck, track: id });
            }
        }
        events
    }

    pub fn loaded(&self) -> impl Iterator<Item = (usize, i64, f64)> + '_ {
        self.decks
            .iter()
            .enumerate()
            .filter_map(|(d, (t, s))| t.map(|t| (d, t, *s)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(track: Option<i64>, playing: bool, fader: f32) -> DeckSample {
        DeckSample {
            track,
            playing,
            fader,
        }
    }

    #[test]
    fn a_track_is_played_once_after_the_delay_of_audible_play() {
        let mut clock = PlayClock::default();
        let a = [deck(Some(7), true, 1.0)];
        let mut played = Vec::new();
        for _ in 0..30 {
            played.extend(clock.tick(0.5, &a, 10.0));
        }
        assert_eq!(played, [PlayEvent::Played { deck: 0, track: 7 }]);
    }

    #[test]
    fn closed_fader_or_pause_does_not_count() {
        let mut clock = PlayClock::default();
        for _ in 0..40 {
            assert_eq!(clock.tick(0.5, &[deck(Some(7), true, 0.05)], 10.0), []);
            assert_eq!(clock.tick(0.5, &[deck(Some(7), false, 1.0)], 10.0), []);
        }
        assert_eq!(clock.loaded().next(), Some((0, 7, 0.0)));
    }

    #[test]
    fn loading_another_track_reports_how_long_the_last_was_heard() {
        let mut clock = PlayClock::default();
        clock.tick(3.0, &[deck(Some(7), true, 1.0)], 10.0);
        let events = clock.tick(1.0, &[deck(Some(8), true, 1.0)], 10.0);
        assert_eq!(
            events,
            [PlayEvent::Left {
                deck: 0,
                track: 7,
                seconds: 3.0
            }]
        );
        assert_eq!(
            clock.loaded().next(),
            Some((0, 8, 1.0)),
            "new track counts from zero"
        );
    }
}
