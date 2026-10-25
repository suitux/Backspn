use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

pub mod core;
pub mod measure;
pub mod net;
pub mod timeline;
pub mod wire;

pub use timeline::{Beats, StartStop, Tempo, Timeline};

fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

pub fn host_micros() -> i64 {
    start().elapsed().as_micros() as i64
}

pub fn micros_at(at: Instant) -> i64 {
    at.saturating_duration_since(start()).as_micros() as i64
}

pub fn phase_at(timeline: &Timeline, time: i64, quantum: f64) -> f64 {
    let q = Beats::from_f64(quantum);
    timeline::phase(timeline::to_phase_encoded_beats(timeline, time, q), q).floating()
}

#[derive(Debug)]
pub struct Link {
    shared: Arc<Mutex<net::Shared>>,
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Link {
    pub fn new(bpm: f64) -> Self {
        Self {
            shared: Arc::new(Mutex::new(net::Shared {
                core: core::Core::new(bpm, host_micros()),
                pending: Vec::new(),
            })),
            running: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }

    pub fn enabled(&self) -> bool {
        self.thread.is_some()
    }

    pub fn enable(&mut self, on: bool) {
        if on == self.enabled() {
            return;
        }
        if on {
            if let Ok(mut s) = self.shared.lock() {
                s.core.reset(host_micros());
            }
            self.running.store(true, Ordering::Relaxed);
            let (shared, running) = (Arc::clone(&self.shared), Arc::clone(&self.running));
            self.thread = std::thread::Builder::new()
                .name("link".into())
                .spawn(move || net::run(&shared, &running))
                .ok();
        } else {
            self.running.store(false, Ordering::Relaxed);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    pub fn num_peers(&self) -> usize {
        self.shared.lock().map_or(0, |s| s.core.num_peers())
    }

    pub fn capture(&self) -> (Timeline, StartStop) {
        self.shared.lock().map_or_else(
            |_| (Timeline::default(), StartStop::default()),
            |s| s.core.client(),
        )
    }

    pub fn commit(&self, timeline: Timeline) {
        if let Ok(mut s) = self.shared.lock() {
            let fx = s.core.commit_timeline(timeline, host_micros());
            s.pending.extend(fx);
        }
    }

    pub fn enable_start_stop_sync(&self, on: bool) {
        if let Ok(mut s) = self.shared.lock() {
            s.core.start_stop_sync = on;
        }
    }

    pub fn set_playing(&self, playing: bool, time: i64) {
        if let Ok(mut s) = self.shared.lock() {
            let fx = s.core.commit_start_stop(playing, time, host_micros());
            s.pending.extend(fx);
        }
    }

    pub fn tempo(&self) -> f64 {
        self.capture().0.tempo.0
    }

    pub fn set_tempo(&self, bpm: f64, time: i64) {
        let (tl, _) = self.capture();
        let d = Timeline {
            tempo: Tempo(bpm),
            beat_origin: tl.to_beats(time),
            time_origin: time,
        }
        .clamped();
        self.commit(Timeline {
            tempo: d.tempo,
            time_origin: d.from_beats(tl.beat_origin),
            ..tl
        });
    }

    pub fn beat_at_time(&self, time: i64, quantum: f64) -> f64 {
        let (tl, _) = self.capture();
        timeline::to_phase_encoded_beats(&tl, time, Beats::from_f64(quantum)).floating()
    }

    pub fn phase_at_time(&self, time: i64, quantum: f64) -> f64 {
        phase_at(&self.capture().0, time, quantum)
    }

    pub fn force_beat_at_time(&self, beat: f64, time: i64, quantum: f64) {
        let (mut tl, _) = self.capture();
        let (b, q) = (Beats::from_f64(beat), Beats::from_f64(quantum));
        for t in [time, time + 1] {
            let cur = timeline::to_phase_encoded_beats(&tl, t, q);
            let c = timeline::closest_phase_match(cur, b, q);
            tl = timeline::shift_client(&tl, c - cur);
            tl.beat_origin = tl.beat_origin + (b - c);
            if timeline::to_phase_encoded_beats(&tl, t, q) <= b {
                break;
            }
        }
        self.commit(tl);
    }

    pub fn time_at_beat(&self, beat: f64, quantum: f64) -> i64 {
        let (tl, _) = self.capture();
        timeline::from_phase_encoded_beats(&tl, Beats::from_f64(beat), Beats::from_f64(quantum))
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.enable(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_nodes_on_this_machine_share_a_tempo() {
        let mut a = Link::new(120.0);
        let mut b = Link::new(90.0);
        a.enable(true);
        b.enable(true);
        let t0 = Instant::now();
        while (a.num_peers() == 0 || b.num_peers() == 0)
            && t0.elapsed() < std::time::Duration::from_secs(8)
        {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if a.num_peers() == 0 {
            return;
        }
        a.set_tempo(128.0, host_micros());
        let t0 = Instant::now();
        while (b.tempo() - 128.0).abs() > 0.01 && t0.elapsed() < std::time::Duration::from_secs(4) {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!((b.tempo() - 128.0).abs() < 0.01, "{}", b.tempo());
        a.enable(false);
        b.enable(false);
    }
}
