pub const TICK: u8 = 0xF8;
pub const START: u8 = 0xFA;
pub const STOP: u8 = 0xFC;

const TICKS_PER_BEAT: f64 = 24.0;

#[derive(Debug, Clone, Default)]
pub struct ClockSender {
    send: bool,
    running: bool,
    pending: bool,
    offset_ms: f64,
    frame: f64,
    start_at: f64,
}

impl ClockSender {
    pub fn set_offset_ms(&mut self, ms: f64) {
        self.offset_ms = ms;
    }

    pub fn set_send(&mut self, on: bool, emit: impl FnMut(f64, u8)) {
        self.send = on;
        if on {
            self.pending = !self.running;
        } else {
            self.stop(emit);
        }
    }

    pub fn resync(&mut self, emit: impl FnMut(f64, u8)) {
        self.stop(emit);
        self.pending = self.send;
    }

    fn stop(&mut self, mut emit: impl FnMut(f64, u8)) {
        if self.running {
            emit(0.0, STOP);
        }
        self.running = false;
        self.pending = false;
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn block(
        &mut self,
        bpm: f64,
        beat: f64,
        frames: usize,
        rate: u32,
        mut emit: impl FnMut(f64, u8),
    ) {
        let len = frames as f64;
        if bpm > 0.0 && rate > 0 {
            let rate = f64::from(rate);
            let beat_len = 60.0 * rate / bpm;
            let tick = beat_len / TICKS_PER_BEAT;
            let offset = self.offset_ms * rate / 1000.0;
            let into = beat * beat_len;
            if self.pending {
                let s = (-offset - into).rem_euclid(beat_len);
                if s > len && s < 3.0 * len {
                    emit(s - len, START);
                    self.running = true;
                    self.pending = false;
                    self.start_at = self.frame + s;
                }
            }
            if self.running {
                let mut at = (-offset - into).rem_euclid(tick);
                while at < len {
                    if self.frame + at >= self.start_at - 0.5 {
                        emit(at, TICK);
                    }
                    at += tick;
                }
            }
        }
        self.frame += len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const BPM: f64 = 120.0;
    const NONE: [(f64, u8); 0] = [];

    fn run(s: &mut ClockSender, beat: f64, frames: usize) -> Vec<(f64, u8)> {
        let mut out = Vec::new();
        s.block(BPM, beat, frames, RATE, |at, b| {
            out.push(((at * 1e3).round() / 1e3, b));
        });
        out
    }

    fn running() -> ClockSender {
        let mut s = ClockSender::default();
        s.set_send(true, |_, _| {});
        let beat = -1_500.0 / 24_000.0;
        assert_eq!(run(&mut s, beat, 1_000), [(500.0, START)]);
        assert_eq!(
            run(&mut s, beat + 1_000.0 / 24_000.0, 1_000),
            [(500.0, TICK)]
        );
        s
    }

    #[test]
    fn start_goes_one_block_ahead_of_the_next_beat() {
        let mut s = ClockSender::default();
        s.set_send(true, |_, _| {});
        let beat = -1_500.0 / 24_000.0;
        assert_eq!(run(&mut s, beat, 1_000), [(500.0, START)]);
        assert!(s.running());
        let next = beat + 1_000.0 / 24_000.0;
        assert_eq!(run(&mut s, next, 1_000), [(500.0, TICK)]);
    }

    #[test]
    fn no_start_when_the_beat_is_too_close_or_far() {
        let mut s = ClockSender::default();
        s.set_send(true, |_, _| {});
        assert_eq!(run(&mut s, -500.0 / 24_000.0, 1_000), NONE);
        assert_eq!(run(&mut s, -5_000.0 / 24_000.0, 1_000), NONE);
        assert!(!s.running());
    }

    #[test]
    fn every_tick_of_the_block_goes_out() {
        let mut s = running();
        let out = run(&mut s, 3.0, 2_500);
        assert_eq!(out, [(0.0, TICK), (1_000.0, TICK), (2_000.0, TICK)]);
        let out = run(&mut s, 3.0 + 0.5 / 24.0, 1_000);
        assert_eq!(out, [(500.0, TICK)]);
    }

    #[test]
    fn a_positive_offset_sends_earlier() {
        let mut s = running();
        s.set_offset_ms(5.0);
        let out = run(&mut s, 3.0, 1_000);
        assert_eq!(out, [(760.0, TICK)]);
    }

    #[test]
    fn off_stops_at_once_and_resync_starts_again() {
        let mut s = running();
        let mut out = Vec::new();
        s.resync(|at, b| out.push((at, b)));
        assert_eq!(out, [(0.0, STOP)]);
        assert_eq!(run(&mut s, 3.0, 1_000), NONE, "waits for a beat");
        assert_eq!(
            run(&mut s, 4.0 - 1_500.0 / 24_000.0, 1_000),
            [(500.0, START)]
        );
        let mut out = Vec::new();
        s.set_send(false, |at, b| out.push((at, b)));
        assert_eq!(out, [(0.0, STOP)]);
        assert_eq!(run(&mut s, 5.0, 1_000), NONE);
    }

    #[test]
    fn nothing_without_a_tempo() {
        let mut s = running();
        let mut out = Vec::new();
        s.block(0.0, 3.0, 1_000, RATE, |at, b| out.push((at, b)));
        assert_eq!(out, NONE);
    }
}
