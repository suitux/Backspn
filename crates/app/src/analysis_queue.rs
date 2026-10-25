use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use backspn_analysis::{Analysis, BpmRange, Options};

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Parts {
    pub bpm: bool,
    pub grid: bool,
    pub key: bool,
    pub gain: bool,
    pub replace_locked: bool,
    pub range: BpmRange,
    pub parallel: bool,
}

impl Default for Parts {
    fn default() -> Self {
        Self {
            bpm: true,
            grid: true,
            key: true,
            gain: true,
            replace_locked: false,
            range: BpmRange::AUTOMATIC,
            parallel: false,
        }
    }
}

pub struct Done {
    pub track: i64,
    pub parts: Parts,
    pub result: Result<Analysis, String>,
    pub overview: Vec<u8>,
}

type Job = (i64, PathBuf, Parts);

pub struct AnalysisQueue {
    jobs: Arc<Mutex<Vec<Job>>>,
    done_tx: Sender<Done>,
    done: Receiver<Done>,
    cancel: Arc<AtomicBool>,
    running: Arc<Mutex<usize>>,
    pub total: usize,
    pub finished: usize,
}

impl std::fmt::Debug for AnalysisQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalysisQueue")
            .field("total", &self.total)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

impl Default for AnalysisQueue {
    fn default() -> Self {
        let (done_tx, done) = mpsc::channel();
        Self {
            jobs: Arc::default(),
            done_tx,
            done,
            cancel: Arc::default(),
            running: Arc::default(),
            total: 0,
            finished: 0,
        }
    }
}

impl AnalysisQueue {
    pub fn add(&mut self, tracks: Vec<(i64, PathBuf)>, parts: Parts, wake: &eframe::egui::Context) {
        if self.is_idle() {
            self.total = 0;
            self.finished = 0;
        }
        self.cancel.store(false, Ordering::Relaxed);
        self.total += tracks.len();
        if let Ok(mut jobs) = self.jobs.lock() {
            for (id, path) in tracks.into_iter().rev() {
                jobs.insert(0, (id, path, parts));
            }
        }
        let workers = if parts.parallel {
            std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(2))
        } else {
            1
        };
        let running = self.running.lock().map_or(0, |r| *r);
        for _ in running..workers {
            self.spawn(wake.clone());
        }
    }

    fn spawn(&self, wake: eframe::egui::Context) {
        let (jobs, done, cancel, running) = (
            Arc::clone(&self.jobs),
            self.done_tx.clone(),
            Arc::clone(&self.cancel),
            Arc::clone(&self.running),
        );
        if let Ok(mut r) = running.lock() {
            *r += 1;
        }
        let spawned = std::thread::Builder::new()
            .name("analysis".into())
            .spawn(move || {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let job = jobs.lock().ok().and_then(|mut j| j.pop());
                    let Some((track, path, parts)) = job else {
                        break;
                    };
                    let mut overview = Vec::new();
                    let result = backspn_decode::decode_file(&path)
                        .map(|t| {
                            let options = Options {
                                bpm_range: parts.range,
                                ..Options::default()
                            };
                            overview = crate::waveform::Peaks::compute(&t)
                                .overview(crate::waveform::OVERVIEW_COLUMNS);
                            backspn_analysis::analyze(&t, &options)
                        })
                        .map_err(|e| e.to_string());
                    if done
                        .send(Done {
                            track,
                            parts,
                            result,
                            overview,
                        })
                        .is_err()
                    {
                        break;
                    }
                    wake.request_repaint();
                }
                if let Ok(mut r) = running.lock() {
                    *r -= 1;
                }
                wake.request_repaint();
            });
        if let Err(e) = spawned {
            log::warn!("analysis: cannot start a worker: {e}");
        }
    }

    pub fn poll(&mut self) -> Vec<Done> {
        let done: Vec<Done> = self.done.try_iter().collect();
        self.finished += done.len();
        done
    }

    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Ok(mut jobs) = self.jobs.lock() {
            self.total -= jobs.len();
            jobs.clear();
        }
    }

    pub fn is_idle(&self) -> bool {
        self.jobs.lock().is_ok_and(|j| j.is_empty()) && self.running.lock().is_ok_and(|r| *r == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click_wav(name: &str) -> PathBuf {
        let rate = 44_100u32;
        let mut samples = vec![0i16; rate as usize * 4];
        for beat in 0..8 {
            let at = beat * rate as usize / 2;
            for (i, s) in samples[at..at + 400].iter_mut().enumerate() {
                *s = if i % 2 == 0 { 20_000 } else { -20_000 };
            }
        }
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        let path =
            std::env::temp_dir().join(format!("backspn-queue-{name}-{}.wav", std::process::id()));
        std::fs::write(&path, wav).unwrap();
        path
    }

    fn wait(queue: &mut AnalysisQueue, n: usize) -> Vec<Done> {
        let mut done = Vec::new();
        let start = std::time::Instant::now();
        while done.len() < n && start.elapsed().as_secs() < 60 {
            done.extend(queue.poll());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        done
    }

    #[test]
    fn queued_tracks_come_back_analysed_or_failed() {
        let ctx = eframe::egui::Context::default();
        let wav = click_wav("ok");
        let mut queue = AnalysisQueue::default();
        queue.add(
            vec![(1, wav.clone()), (2, PathBuf::from("/nonexistent.wav"))],
            Parts::default(),
            &ctx,
        );
        let done = wait(&mut queue, 2);
        assert_eq!(done.len(), 2);
        assert_eq!((queue.finished, queue.total), (2, 2));
        let ok = done.iter().find(|d| d.track == 1).unwrap();
        assert!(ok.result.as_ref().is_ok_and(|a| a.loudness.is_some()));
        assert!(done.iter().find(|d| d.track == 2).unwrap().result.is_err());
        for _ in 0..100 {
            if queue.is_idle() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(queue.is_idle());
        std::fs::remove_file(wav).unwrap();
    }

    #[test]
    fn cancel_drops_what_has_not_started() {
        let ctx = eframe::egui::Context::default();
        let mut queue = AnalysisQueue::default();
        let jobs = (0..50)
            .map(|i| (i, PathBuf::from("/nonexistent.wav")))
            .collect();
        queue.add(jobs, Parts::default(), &ctx);
        queue.cancel();
        let left = queue.total;
        wait(&mut queue, left);
        assert!(left <= 50);
        assert_eq!(
            queue.finished, queue.total,
            "what was dropped is not counted"
        );
        for _ in 0..100 {
            if queue.is_idle() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(queue.is_idle());
    }
}
