use std::time::{Duration, Instant};

use backspn_engine::DeckCommand;

pub const LOCK: f64 = 0.5;
const FAST_BACK: f64 = -0.5;

pub fn tempo_ratio(fader: f32) -> f64 {
    1.0 + 0.08 * f64::from(fader.clamp(-1.0, 1.0))
}

pub fn seconds_per_turn(rpm: f32) -> f64 {
    60.0 / f64::from(rpm)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Ignore,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Copy, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Platter {
    playing: bool,
    motor_on: bool,
    run_down: bool,
    touched: bool,
    steady_s: f64,
    hold_s: f64,
    turns: f64,
    last: Option<Instant>,
    scratching: bool,
    motor_offs: u32,
}

impl Platter {
    pub fn motor_on(&self) -> bool {
        self.motor_on
    }

    pub fn motor_offs(&self) -> u32 {
        self.motor_offs
    }

    pub fn motor_off(&mut self) -> bool {
        if !(self.playing && self.motor_on) {
            return false;
        }
        self.motor_on = false;
        self.run_down = true;
        self.motor_offs += 1;
        true
    }

    pub fn touch(&mut self, down: bool) {
        self.touched = down;
    }

    pub fn turn(&mut self, turns: f64) {
        self.turns += turns;
    }

    pub fn update(&mut self, playing: bool, spt: f64, now: Instant) {
        if playing != self.playing {
            self.playing = playing;
            self.motor_on = playing;
            self.run_down = false;
        }
        let dt = self
            .last
            .map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f64());
        self.last = Some(now);
        let back = self.turns < 0.0;
        let speed = if dt > 0.0 { self.turns * spt / dt } else { 0.0 };
        self.turns = 0.0;
        if self.touched || back || !self.motor_on {
            self.steady_s = 0.0;
        } else {
            self.steady_s += dt;
        }
        self.hold_s = if self.motor_on {
            if self.touched {
                LOCK
            } else {
                (self.hold_s + dt).min(LOCK)
            }
        } else if self.run_down || speed < FAST_BACK {
            0.0
        } else {
            (self.hold_s - dt).max(0.0)
        };
    }

    pub fn route(&self) -> Route {
        let held = if self.motor_on {
            self.hold_s < LOCK
        } else {
            self.hold_s > 0.0
        };
        if held || self.steady_s >= LOCK {
            Route::Ignore
        } else if self.playing {
            Route::Playing
        } else {
            Route::Paused
        }
    }

    pub fn sync_scratch(&mut self) -> Option<DeckCommand> {
        let want = self.route() == Route::Playing;
        (want != self.scratching).then(|| {
            self.scratching = want;
            if want {
                DeckCommand::ScratchStart
            } else {
                DeckCommand::ScratchEnd
            }
        })
    }

    pub fn leave(&mut self) -> Option<DeckCommand> {
        std::mem::take(&mut self.scratching).then_some(DeckCommand::ScratchEnd)
    }

    pub fn pending(&self) -> bool {
        let holding = if self.motor_on {
            self.hold_s < LOCK
        } else {
            self.hold_s > 0.0
        };
        holding || (self.motor_on && !self.touched && self.steady_s < LOCK)
    }
}

pub const CHECK: Duration = Duration::from_millis(20);

#[cfg(test)]
mod tests {
    use super::*;

    const SPT: f64 = 1.8;

    fn run(p: &mut Platter, playing: bool, rps: f64, seconds: f64, t: &mut Instant) {
        let step = 0.01;
        for _ in 0..(seconds / step).round() as usize {
            *t += Duration::from_secs_f64(step);
            p.turn(rps * step);
            p.update(playing, SPT, *t);
        }
    }

    #[test]
    fn play_starts_at_tempo_and_stays_locked_while_the_platter_turns() {
        let mut t = Instant::now();
        let mut p = Platter::default();
        p.update(false, SPT, t);
        run(&mut p, true, 0.1, 0.2, &mut t);
        assert!(p.motor_on());
        assert_eq!(p.route(), Route::Ignore);
        run(&mut p, true, 0.55, 1.0, &mut t);
        assert_eq!(p.route(), Route::Ignore);
        assert!(!p.pending());
    }

    #[test]
    fn a_hand_on_the_platter_scratches_until_it_turns_steadily_again() {
        let mut t = Instant::now();
        let mut p = Platter::default();
        run(&mut p, true, 0.55, 1.0, &mut t);
        p.touch(true);
        p.update(true, SPT, t);
        assert_eq!(p.route(), Route::Playing);
        assert!(matches!(p.sync_scratch(), Some(DeckCommand::ScratchStart)));
        run(&mut p, true, -0.3, 0.3, &mut t);
        p.touch(false);
        run(&mut p, true, 0.4, 0.3, &mut t);
        assert_eq!(p.route(), Route::Playing);
        assert!(p.sync_scratch().is_none());
        run(&mut p, true, 0.55, 0.3, &mut t);
        assert_eq!(p.route(), Route::Ignore, "steady for half a second");
        assert!(matches!(p.sync_scratch(), Some(DeckCommand::ScratchEnd)));
    }

    #[test]
    fn a_backspin_is_heard() {
        let mut t = Instant::now();
        let mut p = Platter::default();
        run(&mut p, true, 0.55, 1.0, &mut t);
        run(&mut p, true, -3.0, 0.05, &mut t);
        assert_eq!(p.route(), Route::Playing);
    }

    #[test]
    fn pause_stands_still_then_the_platter_moves_the_track() {
        let mut t = Instant::now();
        let mut p = Platter::default();
        run(&mut p, true, 0.55, 1.0, &mut t);
        run(&mut p, false, 0.3, 0.3, &mut t);
        assert!(!p.motor_on());
        assert_eq!(p.route(), Route::Ignore, "held still after PAUSE");
        run(&mut p, false, 0.0, 0.3, &mut t);
        assert_eq!(p.route(), Route::Paused);
    }

    #[test]
    fn motor_off_lets_the_track_follow_the_run_down() {
        let mut t = Instant::now();
        let mut p = Platter::default();
        assert!(!p.motor_off(), "only while playing");
        run(&mut p, true, 0.55, 1.0, &mut t);
        assert!(p.motor_off());
        assert_eq!(p.motor_offs(), 1);
        run(&mut p, true, 0.5, 0.02, &mut t);
        assert!(!p.motor_on());
        assert_eq!(p.route(), Route::Playing, "no stop hold");
        run(&mut p, false, 0.0, 1.0, &mut t);
        run(&mut p, true, 0.1, 0.02, &mut t);
        assert!(p.motor_on());
        assert_eq!(p.route(), Route::Ignore);
    }

    #[test]
    fn the_tempo_fader_moves_eight_percent_either_way() {
        assert!((tempo_ratio(1.0) - 1.08).abs() < 1e-9);
        assert!((tempo_ratio(-1.0) - 0.92).abs() < 1e-9);
        assert!((tempo_ratio(0.0) - 1.0).abs() < 1e-9);
        assert!((seconds_per_turn(100.0 / 3.0) - 1.8).abs() < 1e-6);
        assert!((seconds_per_turn(45.0) - 4.0 / 3.0).abs() < 1e-9);
    }
}
