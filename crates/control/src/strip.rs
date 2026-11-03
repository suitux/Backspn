//! What a two-half touch strip does with the fingers on it. In its usual
//! transport mode each half drives the deck of its side; with a finger held on
//! the strip, a press of a loop encoder or of an effect button hands the whole
//! strip to that loop or that effect, and a second press hands it back.

use crate::feedback::{DeckFeedback, Feedback};
use crate::{Action, Trigger};

/// Loop sizes along the strip in loop mode, from 1/32 to 32 beats.
pub const LOOP_ZONES: usize = 11;
/// The zone of one beat.
const ONE_BEAT_ZONE: i32 = 5;
/// The ends of the strip that still read as fully off or fully on in effect mode.
const FX_MARGIN: f32 = 0.05;
const LIT: u8 = 0x7F;
const DIM: u8 = 0x0A;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Transport,
    /// The strip sets a held loop on the deck, its size by where the finger is.
    Loop { deck: usize },
    /// The strip sets one effect knob and holds its button on while touched.
    Fx { unit: usize, slot: usize },
}

/// What a press can hand the strip to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Loop { deck: usize },
    Fx { unit: usize, slot: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Half {
    deck: usize,
    scratching: bool,
    seeking: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Strip {
    mode: Mode,
    touched: bool,
    zone: Option<usize>,
    fx_value: f32,
    halves: [Half; 2],
}

/// The LEDs under a strip: two per position, a warm colour then a cool one.
#[derive(Debug, Clone, Copy)]
pub struct Bar {
    pub first: usize,
    pub positions: usize,
}

pub fn loop_beats(zone: usize) -> f64 {
    let zone = i32::try_from(zone.min(LOOP_ZONES - 1)).unwrap_or(0);
    2f64.powi(zone - ONE_BEAT_ZONE)
}

pub fn loop_zone(share: f32) -> usize {
    ((share.clamp(0.0, 1.0) * LOOP_ZONES as f32) as usize).min(LOOP_ZONES - 1)
}

fn fx_value(share: f32) -> f32 {
    ((share - FX_MARGIN) / (1.0 - 2.0 * FX_MARGIN)).clamp(0.0, 1.0)
}

impl Strip {
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// A press of a loop encoder or an effect button. Returns what to send
    /// when the strip takes the press, `None` when the press is not its own.
    pub fn press(&mut self, target: Target) -> Option<Vec<(Action, Trigger)>> {
        let mine = match (self.mode, target) {
            (Mode::Loop { deck }, Target::Loop { deck: d }) => deck == d,
            (Mode::Fx { unit, slot }, Target::Fx { unit: u, slot: s }) => unit == u && slot == s,
            _ => false,
        };
        if mine {
            let out = self.leave();
            self.mode = Mode::Transport;
            return Some(out);
        }
        if self.mode != Mode::Transport || !self.touched {
            return None;
        }
        let mut out = self.leave();
        self.mode = match target {
            Target::Loop { deck } => Mode::Loop { deck },
            Target::Fx { unit, slot } => Mode::Fx { unit, slot },
        };
        if let Mode::Fx { unit, slot } = self.mode {
            out.push((Action::FxButton { unit, button: slot }, Trigger::Set(1.0)));
        }
        Some(out)
    }

    /// Ends whatever the fingers are doing in the current mode.
    fn leave(&mut self) -> Vec<(Action, Trigger)> {
        let mut out = Vec::new();
        match self.mode {
            Mode::Transport => {
                for half in &mut self.halves {
                    if half.scratching {
                        out.push((Action::JogTouch { deck: half.deck }, Trigger::Release));
                    }
                    half.scratching = false;
                    half.seeking = false;
                }
            }
            Mode::Loop { deck } => {
                if self.zone.take().is_some() {
                    out.push((Action::LoopActive { deck }, Trigger::Press));
                }
            }
            Mode::Fx { unit, slot } if self.touched => {
                out.push((Action::FxButton { unit, button: slot }, Trigger::Set(0.0)));
            }
            Mode::Fx { .. } => {}
        }
        out
    }

    /// A finger landing on the strip or the last one lifting, anywhere on it.
    pub fn touch(&mut self, down: bool) -> Vec<(Action, Trigger)> {
        self.touched = down;
        match self.mode {
            Mode::Loop { deck } if !down => self
                .zone
                .take()
                .map(|_| (Action::LoopActive { deck }, Trigger::Press))
                .into_iter()
                .collect(),
            Mode::Transport | Mode::Loop { .. } => Vec::new(),
            Mode::Fx { unit, slot } => vec![(
                Action::FxButton { unit, button: slot },
                Trigger::Set(if down { 1.0 } else { 0.0 }),
            )],
        }
    }

    /// Where the finger is along the whole strip, 0 at the left end.
    pub fn position(&mut self, share: f32) -> Vec<(Action, Trigger)> {
        match self.mode {
            Mode::Transport => Vec::new(),
            Mode::Loop { deck } => {
                let zone = loop_zone(share);
                if self.zone == Some(zone) {
                    return Vec::new();
                }
                self.zone = Some(zone);
                let beats = loop_beats(zone);
                vec![(Action::LoopSize { deck, beats }, Trigger::Press)]
            }
            Mode::Fx { unit, slot } => {
                self.fx_value = fx_value(share);
                vec![(
                    Action::FxKnob { unit, knob: slot },
                    Trigger::Set(self.fx_value),
                )]
            }
        }
    }

    /// A finger landing on or lifting from one half in transport mode. With
    /// SHIFT it scratches a playing deck and seeks a stopped one; without, the
    /// slide bends a playing deck and scratches a stopped one.
    pub fn half_touch(
        &mut self,
        side: usize,
        deck: usize,
        down: bool,
        shift: bool,
        playing: bool,
    ) -> Vec<(Action, Trigger)> {
        if self.mode != Mode::Transport {
            return Vec::new();
        }
        let half = &mut self.halves[side.min(1)];
        let mut out = Vec::new();
        if half.scratching {
            out.push((Action::JogTouch { deck: half.deck }, Trigger::Release));
        }
        *half = Half {
            deck,
            scratching: down && shift && playing,
            seeking: down && shift && !playing,
        };
        if half.scratching {
            out.push((Action::JogTouch { deck }, Trigger::Press));
        }
        out
    }

    pub fn half_turn(&self, side: usize, deck: usize, turns: f32) -> Vec<(Action, Trigger)> {
        let half = self.halves[side.min(1)];
        if self.mode != Mode::Transport || half.seeking {
            return Vec::new();
        }
        vec![(Action::JogTurn { deck }, Trigger::Delta(turns))]
    }

    /// Where the finger is along its half, 0 at the half's left end.
    pub fn half_position(&self, side: usize, share: f32) -> Vec<(Action, Trigger)> {
        let half = self.halves[side.min(1)];
        if self.mode != Mode::Transport || !half.seeking {
            return Vec::new();
        }
        vec![(
            Action::Seek { deck: half.deck },
            Trigger::Set(share.clamp(0.0, 1.0)),
        )]
    }

    /// The bar's LEDs as (warm, cool) levels per position.
    pub fn bar(&self, feedback: &Feedback, layer: [usize; 2], positions: usize) -> Vec<(u8, u8)> {
        let mut bar = vec![(0, 0); positions];
        match self.mode {
            Mode::Transport => {
                let half = positions / 2;
                for (side, &deck) in layer.iter().enumerate() {
                    let deck = &feedback.decks[deck.min(feedback.decks.len() - 1)];
                    let first = side * (positions - half);
                    let filled = played(deck, half);
                    let warning = deck.playing && deck.end_warning;
                    for i in 0..half.min(filled) {
                        bar[first + i] = if warning { (LIT, 0) } else { (0, LIT) };
                    }
                }
                if positions % 2 == 1 {
                    bar[half] = (0, DIM);
                }
            }
            Mode::Loop { .. } => {
                for zone in 0..LOOP_ZONES {
                    let at = zone * (positions - 1) / (LOOP_ZONES - 1);
                    bar[at] = (0, if self.zone == Some(zone) { LIT } else { DIM });
                }
            }
            Mode::Fx { unit, slot } => {
                let on = feedback
                    .fx
                    .get(unit)
                    .and_then(|f| f.buttons.get(slot))
                    .copied()
                    .unwrap_or(false);
                let filled = (self.fx_value * positions as f32).round() as usize;
                for level in bar.iter_mut().take(filled.max(1)) {
                    *level = if on { (LIT, 0) } else { (DIM, 0) };
                }
            }
        }
        bar
    }
}

/// How many of `half` positions the deck's played share fills.
/// How many of `half` positions the deck has played.
pub fn played(deck: &DeckFeedback, half: usize) -> usize {
    if !deck.loaded || deck.duration_s <= 0.0 {
        return 0;
    }
    let share = (deck.position_s / deck.duration_s).clamp(0.0, 1.0);
    ((share * half as f64).ceil() as usize).min(half)
}

pub fn write_bar(levels: &[(u8, u8)], bar: Bar, leds: &mut [u8]) {
    for (i, &(warm, cool)) in levels.iter().enumerate().take(bar.positions) {
        let at = bar.first + 2 * i;
        if let Some(pair) = leds.get_mut(at..at + 2) {
            pair.copy_from_slice(&[warm, cool]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_run_from_a_32nd_to_32_beats() {
        assert!((loop_beats(0) - 1.0 / 32.0).abs() < 1e-12);
        assert!((loop_beats(5) - 1.0).abs() < 1e-12);
        assert!((loop_beats(10) - 32.0).abs() < 1e-12);
        assert_eq!(loop_zone(0.0), 0);
        assert_eq!(loop_zone(0.5), 5);
        assert_eq!(loop_zone(1.0), 10);
    }

    #[test]
    fn transport_bends_or_scratches_and_seeks_with_shift() {
        let mut s = Strip::default();
        assert_eq!(s.half_touch(0, 0, true, false, true), []);
        assert_eq!(
            s.half_turn(0, 0, 0.1),
            [(Action::JogTurn { deck: 0 }, Trigger::Delta(0.1))],
            "playing: the slide bends"
        );
        assert_eq!(
            s.half_touch(1, 1, true, true, true),
            [(Action::JogTouch { deck: 1 }, Trigger::Press)],
            "SHIFT on a playing deck scratches"
        );
        assert_eq!(
            s.half_touch(1, 1, false, true, true),
            [(Action::JogTouch { deck: 1 }, Trigger::Release)]
        );
        assert_eq!(s.half_touch(0, 0, true, true, false), []);
        assert_eq!(s.half_turn(0, 0, 0.1), [], "seeking ignores the slide");
        assert_eq!(
            s.half_position(0, 0.25),
            [(Action::Seek { deck: 0 }, Trigger::Set(0.25))],
            "SHIFT on a stopped deck seeks"
        );
        s.half_touch(0, 0, false, true, false);
        assert_eq!(s.half_position(0, 0.25), []);
    }

    #[test]
    fn a_held_finger_hands_the_strip_to_a_loop_and_back() {
        let mut s = Strip::default();
        assert_eq!(
            s.press(Target::Loop { deck: 1 }),
            None,
            "no finger: not ours"
        );
        s.touch(true);
        assert_eq!(s.press(Target::Loop { deck: 1 }), Some(vec![]));
        assert_eq!(s.mode(), Mode::Loop { deck: 1 });
        s.touch(false);
        s.touch(true);
        assert_eq!(
            s.position(0.5),
            [(
                Action::LoopSize {
                    deck: 1,
                    beats: 1.0
                },
                Trigger::Press
            )]
        );
        assert_eq!(s.position(0.52), [], "same zone");
        assert_eq!(
            s.position(1.0),
            [(
                Action::LoopSize {
                    deck: 1,
                    beats: 32.0
                },
                Trigger::Press
            )]
        );
        assert_eq!(
            s.touch(false),
            [(Action::LoopActive { deck: 1 }, Trigger::Press)],
            "lifting ends the loop"
        );
        assert_eq!(
            s.press(Target::Loop { deck: 0 }),
            None,
            "another deck's encoder"
        );
        assert_eq!(s.press(Target::Loop { deck: 1 }), Some(vec![]));
        assert_eq!(s.mode(), Mode::Transport);
    }

    #[test]
    fn effect_mode_sets_the_knob_and_holds_the_button() {
        let mut s = Strip::default();
        s.touch(true);
        assert_eq!(
            s.press(Target::Fx { unit: 0, slot: 2 }),
            Some(vec![(
                Action::FxButton { unit: 0, button: 2 },
                Trigger::Set(1.0)
            )])
        );
        assert_eq!(
            s.position(0.5),
            [(Action::FxKnob { unit: 0, knob: 2 }, Trigger::Set(0.5))]
        );
        assert_eq!(
            s.position(0.99),
            [(Action::FxKnob { unit: 0, knob: 2 }, Trigger::Set(1.0))],
            "the end reads as full"
        );
        assert_eq!(
            s.touch(false),
            [(Action::FxButton { unit: 0, button: 2 }, Trigger::Set(0.0))]
        );
        assert_eq!(s.half_turn(0, 0, 0.1), [], "the halves wait");
        assert_eq!(s.press(Target::Fx { unit: 0, slot: 2 }), Some(vec![]));
        assert_eq!(s.mode(), Mode::Transport);
    }

    #[test]
    fn taking_the_strip_ends_a_scratch() {
        let mut s = Strip::default();
        s.touch(true);
        s.half_touch(0, 0, true, true, true);
        assert_eq!(
            s.press(Target::Loop { deck: 0 }),
            Some(vec![(Action::JogTouch { deck: 0 }, Trigger::Release)])
        );
    }

    #[test]
    fn the_bar_shows_progress_zones_or_the_effect_value() {
        let mut f = Feedback::default();
        f.decks[0].loaded = true;
        f.decks[0].duration_s = 100.0;
        f.decks[0].position_s = 50.0;
        let mut s = Strip::default();
        let bar = s.bar(&f, [0, 1], 21);
        assert_eq!(&bar[..5], &[(0, LIT); 5]);
        assert_eq!(bar[5], (0, 0));
        assert_eq!(bar[10], (0, DIM), "the middle");
        assert!(bar[11..].iter().all(|&l| l == (0, 0)), "nothing on deck B");
        s.touch(true);
        s.press(Target::Loop { deck: 0 });
        s.position(1.0);
        let bar = s.bar(&f, [0, 1], 21);
        assert_eq!(bar[20], (0, LIT), "the held zone");
        assert_eq!(bar[0], (0, DIM));
        assert_eq!(bar[1], (0, 0), "between zones");
        s.touch(false);
        s.press(Target::Loop { deck: 0 });
        s.touch(true);
        s.press(Target::Fx { unit: 1, slot: 0 });
        s.position(0.5);
        f.fx[1].buttons[0] = true;
        let bar = s.bar(&f, [0, 1], 21);
        assert_eq!(bar[0], (LIT, 0));
        assert_eq!(bar[20], (0, 0));
    }

    #[test]
    fn the_bar_writes_two_leds_per_position() {
        let mut leds = [0u8; 8];
        write_bar(
            &[(1, 2), (3, 4)],
            Bar {
                first: 2,
                positions: 2,
            },
            &mut leds,
        );
        assert_eq!(leds, [0, 0, 1, 2, 3, 4, 0, 0]);
    }
}
