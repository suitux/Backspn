use crate::{MAX_CLOCK_BPM, MIN_CLOCK_BPM};

pub const SPP: u8 = 0xF2;
pub const START: u8 = 0xFA;
pub const CONTINUE: u8 = 0xFB;
pub const TICK: u8 = 0xF8;
pub const STOP: u8 = 0xFC;

const WINDOW: usize = 96;
const RUN: i32 = 6;
const RUN_RESET: i32 = 12;
const MIN_LIMIT_US: i64 = 500;
const STALE_US: i64 = 500_001;
const TICKS_PER_BEAT: f64 = 24.0;

fn step_run(run: &mut i32, acc: &mut i64, value: i64, up: bool, down: bool) {
    if up {
        if *run < 0 {
            *run = 0;
            *acc = 0;
        }
        *run += 1;
    } else if down {
        if *run > 0 {
            *run = 0;
            *acc = 0;
        }
        *run -= 1;
    } else {
        return;
    }
    *acc += value;
}

#[derive(Debug, Clone)]
pub struct SyncStats {
    intervals: [i64; WINDOW],
    write: usize,
    count: usize,
    sum: i64,
    sum_sq: f64,
    prev_raw: Option<i64>,
    smoothed: i64,
    dev_acc: i64,
    dev_run: i32,
    res_acc: i64,
    res_run: i32,
}

impl Default for SyncStats {
    fn default() -> Self {
        Self {
            intervals: [0; WINDOW],
            write: 0,
            count: 0,
            sum: 0,
            sum_sq: 0.0,
            prev_raw: None,
            smoothed: 0,
            dev_acc: 0,
            dev_run: 0,
            res_acc: 0,
            res_run: 0,
        }
    }
}

impl SyncStats {
    fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum as f64 / self.count as f64
        }
    }

    pub fn tempo(&self) -> Option<f64> {
        let mean = self.mean();
        (mean > 0.0).then(|| 60e6 / (TICKS_PER_BEAT * mean))
    }

    fn sigma(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let mean = self.mean();
        (self.sum_sq / self.count as f64 - mean * mean)
            .max(0.0)
            .sqrt()
    }

    pub fn smoothed(&self) -> i64 {
        self.smoothed
    }

    pub fn last(&self) -> Option<i64> {
        self.prev_raw
    }

    fn add(&mut self, interval: i64) {
        if self.count == WINDOW {
            let old = self.intervals[self.write];
            self.sum -= old;
            self.sum_sq -= (old * old) as f64;
        } else {
            self.count += 1;
        }
        self.intervals[self.write] = interval;
        self.sum += interval;
        self.sum_sq += (interval * interval) as f64;
        self.write = (self.write + 1) % WINDOW;
    }

    fn cut(&mut self) {
        let keep = self.count.min(5);
        let last: Vec<i64> = (1..=keep)
            .rev()
            .map(|k| self.intervals[(self.write + WINDOW - k) % WINDOW])
            .collect();
        *self = Self {
            prev_raw: self.prev_raw,
            smoothed: self.smoothed,
            ..Self::default()
        };
        for i in last {
            self.add(i);
        }
    }

    pub fn push(&mut self, now: i64) {
        let Some(prev) = self.prev_raw else {
            self.prev_raw = Some(now);
            self.smoothed = now;
            return;
        };
        let interval = now - prev;
        let mean = self.mean();
        let dev = if mean > 0.0 {
            self.smoothed += mean.round() as i64;
            (mean - interval as f64).round() as i64
        } else {
            self.smoothed = now;
            0
        };
        step_run(&mut self.dev_run, &mut self.dev_acc, dev, dev >= 1, dev < 0);
        let mut cut = false;
        if self.dev_run.abs() == RUN {
            let n = f64::from(self.dev_run.abs());
            let threshold = self.tempo().map_or(f64::INFINITY, |t| mean / t);
            cut = (self.dev_acc as f64 / (2.0 * n)).abs() > threshold;
            self.dev_acc = 0;
            self.dev_run = 0;
        }
        if cut {
            self.cut();
        }
        self.add(interval);
        let residual = now - self.smoothed;
        step_run(
            &mut self.res_run,
            &mut self.res_acc,
            residual,
            residual >= 0,
            residual < 0,
        );
        if self.res_run.abs() == RUN {
            let avg = (self.res_acc as f64 / f64::from(self.res_run.abs())).round() as i64;
            let limit = ((2.0 * self.sigma()).round() as i64).max(MIN_LIMIT_US);
            if avg.abs() >= limit {
                self.smoothed += avg;
                self.res_acc = 0;
                self.res_run = 0;
            }
        }
        if self.res_run.abs() >= RUN_RESET {
            self.res_acc = 0;
            self.res_run = 0;
        }
        self.prev_raw = Some(now);
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClockReceiver {
    stats: SyncStats,
    held_tempo: Option<f64>,
    playing: bool,
    pending: bool,
    ticks: f64,
    at: i64,
    last_beat: f64,
}

impl ClockReceiver {
    pub fn handle(&mut self, status: u8, spp: u16, now: i64) {
        match status {
            SPP if !self.playing => self.ticks = f64::from(spp) * 6.0,
            START | CONTINUE => {
                if status == START {
                    self.ticks = 0.0;
                }
                self.pending = true;
                if self.stats.last().is_none_or(|t| now - t >= STALE_US) {
                    self.held_tempo = self.tempo();
                    self.stats = SyncStats::default();
                }
            }
            TICK => {
                self.stats.push(now);
                if self.pending {
                    self.pending = false;
                    self.playing = true;
                    self.last_beat = self.ticks / TICKS_PER_BEAT;
                } else if self.playing {
                    self.ticks += 1.0;
                }
                self.at = self.stats.smoothed();
            }
            STOP => {
                self.playing = false;
                self.pending = false;
            }
            _ => {}
        }
    }

    pub fn tempo(&self) -> Option<f64> {
        self.stats.tempo().or(self.held_tempo)
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn beat_at(&mut self, now: i64) -> Option<f64> {
        if !self.playing {
            return None;
        }
        let tempo = self.tempo()?;
        let beat = self.ticks / TICKS_PER_BEAT + (now - self.at) as f64 * tempo / 60e6;
        self.last_beat = self.last_beat.max(beat);
        Some(self.last_beat)
    }
}

#[derive(Debug, Clone, Default)]
struct PhaseSync {
    prev: f64,
    d: f64,
}

impl PhaseSync {
    const KP: f64 = -0.05;
    const KD: f64 = -10.0;
    const KI: f64 = -100.0;
    const ALPHA_D: f64 = 0.85;

    fn tempo_diff(&mut self, phase: f64) -> f64 {
        let d = (phase - self.prev).clamp(-0.5, 0.5);
        let d_new = Self::ALPHA_D * d + (1.0 - Self::ALPHA_D) * self.d;
        let i = d - self.d;
        self.prev = phase;
        self.d = d_new;
        Self::KP * phase + Self::KD * d_new + Self::KI * i
    }
}

#[derive(Debug, Clone, Default)]
pub struct Follower {
    error: f64,
    phase: PhaseSync,
}

impl Follower {
    pub fn step(&mut self, current: f64, target: f64, phase: f64) -> f64 {
        let target = target.clamp(MIN_CLOCK_BPM, MAX_CLOCK_BPM);
        let delta = (target - current) * 0.002 / target;
        let same = (self.error >= 0.0) == (delta >= 0.0);
        self.error = (self.error + delta) * if same { 0.99975 } else { 0.95 };
        let p = self.phase.tempo_diff(phase).clamp(-0.02, 0.02) * 0.01;
        let e = self.error;
        let next = if e.abs() > 0.075 || (current / target - 1.0).abs() > 0.1 {
            current * (1.0 + 0.002 * e)
        } else if e.abs() <= 1e-4 {
            current * (1.0 + p)
        } else {
            let w = (e.abs() - 1e-4) / 0.0749;
            current * (1.0 + 0.002 * e * w) * (1.0 + p * (1.0 - w))
        };
        next.clamp(MIN_CLOCK_BPM, MAX_CLOCK_BPM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticks(r: &mut ClockReceiver, from: i64, interval: i64, n: usize) -> i64 {
        let mut t = from;
        for _ in 0..n {
            r.handle(TICK, 0, t);
            t += interval;
        }
        t
    }

    #[test]
    fn the_tempo_is_the_mean_interval() {
        let mut r = ClockReceiver::default();
        ticks(&mut r, 0, 20_833, 50);
        assert!((r.tempo().unwrap() - 120.0).abs() < 0.01);
        assert!(!r.playing(), "ticks alone do not start");
    }

    #[test]
    fn a_tempo_change_is_followed_within_a_beat() {
        let mut r = ClockReceiver::default();
        let t = ticks(&mut r, 0, 20_833, 96);
        ticks(&mut r, t, 17_857, 24);
        assert!((r.tempo().unwrap() - 140.0).abs() < 1.0, "{:?}", r.tempo());
    }

    #[test]
    fn start_plays_from_the_next_tick_and_counts_beats() {
        let mut r = ClockReceiver::default();
        let t = ticks(&mut r, 0, 20_833, 48);
        r.handle(START, 0, t);
        assert!(!r.playing());
        let t = ticks(&mut r, t, 20_833, 25);
        assert!(r.playing());
        let beat = r.beat_at(t - 20_833).unwrap();
        assert!((beat - 1.0).abs() < 1e-3, "{beat}");
        let beat = r.beat_at(t - 20_833 + 250_000).unwrap();
        assert!((beat - 1.5).abs() < 1e-2, "{beat}");
        r.handle(STOP, 0, t);
        assert_eq!(r.beat_at(t), None);
    }

    #[test]
    fn continue_starts_at_the_song_position() {
        let mut r = ClockReceiver::default();
        let t = ticks(&mut r, 0, 20_833, 24);
        r.handle(SPP, 8, t);
        r.handle(CONTINUE, 0, t);
        r.handle(TICK, 0, t);
        assert!((r.beat_at(t).unwrap() - 2.0).abs() < 1e-3);
    }

    #[test]
    fn the_position_never_goes_back() {
        let mut r = ClockReceiver::default();
        let t = ticks(&mut r, 0, 20_833, 48);
        r.handle(START, 0, t);
        r.handle(TICK, 0, t);
        let ahead = r.beat_at(t + 40_000).unwrap();
        r.handle(TICK, 0, t + 20_833);
        assert!(r.beat_at(t + 20_833).unwrap() >= ahead);
    }

    #[test]
    fn a_late_start_restarts_the_statistics_and_keeps_the_tempo() {
        let mut r = ClockReceiver::default();
        let t = ticks(&mut r, 0, 20_833, 48);
        r.handle(START, 0, t + 1_000_000);
        assert!((r.tempo().unwrap() - 120.0).abs() < 0.01);
    }

    #[test]
    fn the_clock_follows_tempo_and_phase() {
        let mut f = Follower::default();
        let (mut bpm, mut beat, mut ext) = (120.0, 0.3, 0.0);
        let target = 126.0;
        let dt = 256.0 / 48_000.0;
        for _ in 0..(60.0 / dt) as usize {
            let phase = crate::wrap_phase(beat - ext);
            bpm = f.step(bpm, target, phase);
            beat += bpm / 60.0 * dt;
            ext += target / 60.0 * dt;
        }
        assert!((bpm - target).abs() < 0.05, "{bpm}");
        let phase = crate::wrap_phase(beat - ext);
        assert!(phase.abs() < 0.02, "{phase}");
    }
}
