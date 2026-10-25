use std::time::{Duration, Instant};

use backspn_engine::DeckCommand;

pub fn bend_factor(sensitivity: f64) -> f64 {
    0.1 + 1.8 * sensitivity.clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bend {
    pub spin: f64,
    pub nudge: f64,
}

pub const IDLE: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, Default)]
pub struct Jog {
    pub turntable: bool,
    touched: bool,
    idle_scratch: Option<Instant>,
    spinning: Option<Instant>,
}

impl Jog {
    pub fn new(turntable: bool) -> Self {
        Self {
            turntable,
            ..Self::default()
        }
    }

    pub fn touch(&mut self, down: bool) -> DeckCommand {
        self.touched = down;
        self.spinning = (!down).then(Instant::now);
        if down {
            self.idle_scratch = None;
            DeckCommand::ScratchStart
        } else {
            DeckCommand::ScratchEnd
        }
    }

    pub fn turn(
        &mut self,
        seconds: f64,
        bend: Bend,
        playing: bool,
        scratching: bool,
        now: Instant,
    ) -> Vec<DeckCommand> {
        if self.touched {
            vec![DeckCommand::ScratchMove(seconds)]
        } else if playing && !scratching {
            let spin = !self.touched
                && self
                    .spinning
                    .is_some_and(|last| now.saturating_duration_since(last) < IDLE);
            self.spinning = spin.then_some(now);
            let factor = if spin { bend.spin } else { bend.nudge };
            vec![DeckCommand::Nudge(seconds * factor)]
        } else if !playing {
            let mut out = Vec::new();
            if self.idle_scratch.is_none() && !self.touched {
                out.push(DeckCommand::ScratchStart);
            }
            self.idle_scratch = Some(now);
            out.push(DeckCommand::ScratchMove(seconds));
            out
        } else {
            Vec::new()
        }
    }

    pub fn tick(&mut self, now: Instant) -> (Option<DeckCommand>, bool) {
        match self.idle_scratch {
            Some(since) if now.duration_since(since) >= IDLE => {
                self.idle_scratch = None;
                (Some(DeckCommand::ScratchEnd), false)
            }
            Some(_) => (None, true),
            None => (None, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(non_snake_case)]
    fn B(f: f64) -> Bend {
        Bend { spin: f, nudge: f }
    }

    fn is(cmds: &[DeckCommand], names: &[&str]) -> bool {
        let got: Vec<String> = cmds
            .iter()
            .map(|c| format!("{c:?}").split('(').next().unwrap().to_owned())
            .collect();
        got == names
    }

    #[test]
    fn jog_mode_holds_a_playing_track_under_the_hand() {
        let now = Instant::now();
        let mut jog = Jog::new(false);
        assert!(matches!(jog.touch(true), DeckCommand::ScratchStart));
        assert!(is(
            &jog.turn(0.1, B(0.5), true, true, now),
            &["ScratchMove"]
        ));
        assert!(matches!(jog.touch(false), DeckCommand::ScratchEnd));
        assert!(matches!(jog.touch(true), DeckCommand::ScratchStart));
        assert!(is(
            &jog.turn(0.1, B(0.5), false, true, now),
            &["ScratchMove"]
        ));
    }

    #[test]
    fn turntable_mode_scratches_while_playing() {
        let now = Instant::now();
        let mut jog = Jog::new(true);
        assert!(matches!(jog.touch(true), DeckCommand::ScratchStart));
        assert!(is(
            &jog.turn(0.1, B(0.5), true, true, now),
            &["ScratchMove"]
        ));
        jog.touch(false);
        assert!(is(&jog.turn(0.1, B(0.5), true, false, now), &["Nudge"]));
    }

    #[test]
    fn sensitivity_maps_to_the_bend_factor() {
        assert!(
            (bend_factor(0.5) - 1.0).abs() < 1e-9,
            "the default follows the wheel"
        );
        assert!((bend_factor(0.0) - 0.1).abs() < 1e-9);
        assert!((bend_factor(1.0) - 1.9).abs() < 1e-9);
    }

    #[test]
    fn a_wheel_let_go_spins_and_the_ring_nudges() {
        let bend = Bend {
            spin: 1.0,
            nudge: 0.2,
        };
        let factor = |cmds: Vec<DeckCommand>| match cmds[..] {
            [DeckCommand::Nudge(s)] => s,
            _ => panic!("{cmds:?}"),
        };
        let mut jog = Jog::new(false);
        let t0 = Instant::now();
        assert!((factor(jog.turn(1.0, bend, true, false, t0)) - 0.2).abs() < 1e-9);
        jog.touch(true);
        jog.touch(false);
        let t1 = Instant::now();
        assert!((factor(jog.turn(1.0, bend, true, false, t1)) - 1.0).abs() < 1e-9);
        let t2 = t1 + IDLE / 2;
        assert!((factor(jog.turn(1.0, bend, true, false, t2)) - 1.0).abs() < 1e-9);
        let later = t2 + IDLE;
        assert!(
            (factor(jog.turn(1.0, bend, true, false, later)) - 0.2).abs() < 1e-9,
            "the wheel stopped: the next turn is a nudge"
        );
    }

    #[test]
    fn nudge_is_scaled() {
        let mut jog = Jog::new(false);
        let cmds = jog.turn(1.0, B(0.25), true, false, Instant::now());
        assert!(matches!(cmds[..], [DeckCommand::Nudge(s)] if (s - 0.25).abs() < 1e-9));
    }

    #[test]
    fn play_while_touching_a_paused_deck_keeps_scratching() {
        let now = Instant::now();
        let mut jog = Jog::new(false);
        jog.touch(true);
        assert!(is(
            &jog.turn(0.1, B(0.5), true, true, now),
            &["ScratchMove"]
        ));
    }

    #[test]
    fn untouched_scrub_starts_once_and_ends_when_idle() {
        let t0 = Instant::now();
        let mut jog = Jog::new(false);
        assert!(is(
            &jog.turn(0.1, B(0.5), false, false, t0),
            &["ScratchStart", "ScratchMove"]
        ));
        assert!(is(
            &jog.turn(0.1, B(0.5), false, true, t0),
            &["ScratchMove"]
        ));
        assert!(matches!(jog.tick(t0), (None, true)));
        let later = t0 + IDLE;
        assert!(matches!(
            jog.tick(later),
            (Some(DeckCommand::ScratchEnd), false)
        ));
        assert!(matches!(jog.tick(later), (None, false)));
    }
}
