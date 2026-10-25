pub const STEPS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatternMode {
    #[default]
    Forward,
    Backward,
    PingPong,
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Record {
    #[default]
    Idle,
    Armed,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sequencer {
    pub on: bool,
    pub steps: u16,
    pub length: usize,
    pub speed: f64,
    pub mode: PatternMode,
    pub swing: f64,
    pub selected: usize,
    pub current: usize,
    last: i64,
    record: Record,
}

impl Default for Sequencer {
    fn default() -> Self {
        Self {
            on: false,
            steps: 0,
            length: STEPS,
            speed: 1.0,
            mode: PatternMode::Forward,
            swing: 0.0,
            selected: 0,
            current: 0,
            last: 0,
            record: Record::Idle,
        }
    }
}

fn sixteenth(beat: f64) -> i64 {
    ((beat * 4.0).floor() as i64).rem_euclid(STEPS as i64)
}

impl Sequencer {
    pub fn delete_pattern(&mut self) {
        self.steps = 0;
    }

    pub fn has_step(&self, step: usize) -> bool {
        step < STEPS && self.steps & (1 << step) != 0
    }

    fn set_step(&mut self, step: usize) {
        if step < STEPS {
            self.steps |= 1 << step;
        }
    }

    pub fn record(&mut self) {
        self.record = Record::Armed;
    }

    pub fn tick(&mut self, beat: f64, random: impl FnOnce() -> u32) -> Option<usize> {
        if beat < 0.0 {
            return None;
        }
        let swung = if sixteenth(beat) % 2 == 1 {
            beat - self.swing * 0.125
        } else {
            beat
        };
        let index = sixteenth(swung);
        let fired = if self.last == index {
            None
        } else {
            self.last = index;
            self.step(swung, random)
        };
        if self.record == Record::Armed {
            if sixteenth(beat + 0.125) == sixteenth(beat) {
                self.set_step(self.current);
                self.record = Record::Idle;
            } else {
                self.record = Record::Next;
            }
        }
        fired
    }

    fn step(&mut self, swung: f64, random: impl FnOnce() -> u32) -> Option<usize> {
        let length = self.length.clamp(1, STEPS);
        let raw = (swung * 4.0 * self.speed).floor() as i64;
        let step = raw.rem_euclid(length as i64) as usize;
        self.current = match self.mode {
            PatternMode::Forward => step,
            PatternMode::Backward => length - 1 - step,
            PatternMode::PingPong => {
                if raw.rem_euclid(2 * length as i64) as usize == step {
                    step
                } else {
                    length - 1 - step
                }
            }
            PatternMode::Random => random() as usize % length,
        };
        self.current_step()
    }

    fn current_step(&mut self) -> Option<usize> {
        if !self.on {
            return None;
        }
        let fired = self.has_step(self.current).then_some(self.selected);
        if self.record == Record::Next {
            self.set_step(self.current);
            self.record = Record::Idle;
        }
        fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn played(s: &mut Sequencer, sixteenths: usize) -> Vec<usize> {
        (1..=sixteenths)
            .map(|i| {
                s.tick(i as f64 * 0.25 + 0.01, || 0);
                s.current
            })
            .collect()
    }

    #[test]
    fn each_mode_walks_the_pattern_its_way() {
        let mut s = Sequencer {
            length: 4,
            ..Sequencer::default()
        };
        assert_eq!(played(&mut s, 6), [1, 2, 3, 0, 1, 2]);
        let mut s = Sequencer {
            length: 4,
            mode: PatternMode::Backward,
            ..Sequencer::default()
        };
        assert_eq!(played(&mut s, 5), [2, 1, 0, 3, 2]);
        let mut s = Sequencer {
            length: 4,
            mode: PatternMode::PingPong,
            ..Sequencer::default()
        };
        assert_eq!(played(&mut s, 10), [1, 2, 3, 3, 2, 1, 0, 0, 1, 2]);
        let mut s = Sequencer {
            length: 4,
            speed: 0.5,
            ..Sequencer::default()
        };
        assert_eq!(played(&mut s, 6), [0, 1, 1, 2, 2, 3]);
    }

    #[test]
    fn a_set_step_triggers_the_selected_cell_once_while_on() {
        let mut s = Sequencer {
            steps: 0b101,
            selected: 7,
            ..Sequencer::default()
        };
        assert_eq!(s.tick(0.0, || 0), None, "beat 0: no change yet");
        s.on = true;
        assert_eq!(s.tick(0.5, || 0), Some(7), "step 2");
        assert_eq!(s.tick(0.6, || 0), None, "the same sixteenth again");
        assert_eq!(s.tick(0.75, || 0), None, "step 3 is not set");
        assert_eq!(s.tick(-1.0, || 0), None, "before the start");
        assert_eq!(s.tick(4.0, || 0), Some(7), "step 0, next bar");
    }

    #[test]
    fn swing_delays_the_off_beat_sixteenths() {
        let mut s = Sequencer {
            on: true,
            steps: 0b10,
            swing: 1.0,
            ..Sequencer::default()
        };
        s.tick(0.0, || 0);
        assert_eq!(s.tick(0.3, || 0), None, "step 1 waits 1/32");
        assert_eq!(s.current, 0);
        assert_eq!(s.tick(0.4, || 0), Some(0));
        assert_eq!(s.current, 1);
    }

    #[test]
    fn a_recorded_press_takes_the_nearest_step() {
        let mut s = Sequencer {
            on: true,
            ..Sequencer::default()
        };
        s.tick(1.0, || 0);
        s.record();
        s.tick(1.05, || 0);
        assert!(s.has_step(4), "early in step 4: step 4");
        s.tick(1.25, || 0);
        s.record();
        s.tick(1.4, || 0);
        assert!(!s.has_step(5));
        assert_eq!(
            s.tick(1.5, || 0),
            None,
            "late in 5: step 6, not played again (the press played)"
        );
        assert!(s.has_step(6));
        for i in 7..22 {
            s.tick(f64::from(i) * 0.25, || 0);
        }
        assert_eq!(s.tick(5.5, || 0), Some(0), "step 6 on the next pass");
        s.delete_pattern();
        assert_eq!(s.steps, 0);
    }

    #[test]
    fn random_picks_within_the_length() {
        let mut s = Sequencer {
            length: 3,
            mode: PatternMode::Random,
            ..Sequencer::default()
        };
        s.tick(0.0, || 7);
        assert_eq!(s.current, 0, "beat 0 is no change");
        s.tick(0.25, || 7);
        assert_eq!(s.current, 1);
    }
}
