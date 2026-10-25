use backspn_engine::{MAX_CLOCK_BPM, MIN_CLOCK_BPM};

const RESTART_GAP_S: f64 = 1.5;
const FIRST_TAP_S: f64 = 8.0;
const DECK_TAPS: usize = 4;
const MIN_RATE: f64 = 1e-8;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BeatTapIn {
    taps: Vec<f64>,
}

impl BeatTapIn {
    pub fn tap(&mut self, t_s: f64) {
        if let Some(&last) = self.taps.last()
            && !(0.0..=RESTART_GAP_S).contains(&(t_s - last))
        {
            self.taps.clear();
        }
        self.taps.push(t_s);
    }

    pub fn bpm(&self) -> Option<f64> {
        let (&first, &last) = (self.taps.first()?, self.taps.last()?);
        let n = self.taps.len();
        (n >= 2).then(|| 60.0 / ((last - first) / (n - 1) as f64))
    }

    pub fn taps(&self) -> &[f64] {
        &self.taps
    }

    pub fn clear(&mut self) {
        self.taps.clear();
    }
}

pub fn deck_tap(taps: &mut BeatTapIn, position_s: f64, rate: f64) -> Option<f64> {
    taps.tap(position_s);
    let n = taps.taps().len();
    if rate.abs() > MIN_RATE && n >= DECK_TAPS && n.is_multiple_of(DECK_TAPS) {
        taps.bpm().map(|bpm| bpm / rate)
    } else {
        None
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TapTempo {
    taps: BeatTapIn,
    engaged_until: Option<f64>,
}

impl TapTempo {
    pub fn tap(&mut self, now_s: f64) -> Option<f64> {
        self.expire(now_s);
        self.taps.tap(now_s);
        let Some(bpm) = self.taps.bpm() else {
            self.engaged_until = Some(now_s + FIRST_TAP_S);
            return None;
        };
        let bpm = bpm.clamp(MIN_CLOCK_BPM, MAX_CLOCK_BPM);
        self.engaged_until = Some(now_s + 60.0 / bpm);
        Some(bpm)
    }

    pub fn expire(&mut self, now_s: f64) {
        if self.engaged_until.is_some_and(|until| now_s >= until) {
            self.engaged_until = None;
            self.taps.clear();
        }
    }

    pub fn engaged(&self) -> bool {
        self.engaged_until.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(bpm: Option<f64>, expected: f64) -> bool {
        bpm.is_some_and(|b| (b - expected).abs() < 1e-9)
    }

    #[test]
    fn tempo_from_the_average_interval() {
        let mut tap = TapTempo::default();
        assert_eq!(tap.tap(10.0), None);
        assert!(tap.engaged());
        assert!(near(tap.tap(10.5), 120.0));
        assert!(near(tap.tap(10.9), 60.0 / 0.45));
    }

    #[test]
    fn a_long_gap_starts_over() {
        let mut tap = TapTempo::default();
        tap.tap(1.0);
        assert!(near(tap.tap(1.4), 150.0));
        tap.expire(2.0);
        assert!(!tap.engaged());
        assert_eq!(tap.tap(2.0), None);
        assert_eq!(tap.tap(3.6), None, "more than 1.5 s later");
        assert!(near(tap.tap(4.1), 120.0));
    }

    #[test]
    fn deck_taps_set_the_grid_every_fourth_tap() {
        let mut taps = BeatTapIn::default();
        assert_eq!(deck_tap(&mut taps, 10.0, 1.0), None);
        assert_eq!(deck_tap(&mut taps, 10.5, 1.0), None);
        assert_eq!(deck_tap(&mut taps, 11.0, 1.0), None);
        assert!(near(deck_tap(&mut taps, 11.5, 1.0), 120.0));
        assert_eq!(deck_tap(&mut taps, 12.0, 1.0), None);
        taps.clear();
        for t in [1.0, 1.5, 2.0] {
            deck_tap(&mut taps, t, 1.25);
        }
        assert!(near(deck_tap(&mut taps, 2.5, 1.25), 96.0));
        taps.clear();
        for t in [1.0, 1.5, 2.0] {
            deck_tap(&mut taps, t, 0.0);
        }
        assert_eq!(deck_tap(&mut taps, 2.5, 0.0), None, "stopped");
    }

    #[test]
    fn first_tap_waits_eight_seconds_and_tempo_is_clamped() {
        let mut tap = TapTempo::default();
        tap.tap(0.0);
        tap.expire(7.9);
        assert!(tap.engaged());
        tap.expire(8.0);
        assert!(!tap.engaged());
        tap.tap(20.0);
        assert!(near(tap.tap(20.1), MAX_CLOCK_BPM));
    }
}
