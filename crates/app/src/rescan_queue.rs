use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use backspn_library::scan::{self, FileScan};

#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub track: i64,
    pub file: PathBuf,
    pub stamp: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Read(Box<FileScan>),
    Unchanged,
    Failed,
}

type Finished = (i64, Outcome);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub read: usize,
    pub unchanged: usize,
    pub failed: usize,
}

pub fn process(job: &Job, quick: bool) -> Outcome {
    if quick && !scan::changed(job.stamp, scan::file_mtime(&job.file)) {
        return Outcome::Unchanged;
    }
    scan::read(&job.file).map_or(Outcome::Failed, |s| Outcome::Read(Box::new(s)))
}

#[derive(Default)]
pub struct RescanQueue {
    jobs: Arc<Mutex<Vec<(Job, bool)>>>,
    channel: Option<(Sender<Finished>, Receiver<Finished>)>,
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    pub total: usize,
    pub finished: usize,
    pub tally: Tally,
}

impl RescanQueue {
    pub fn add(&mut self, jobs: Vec<Job>, quick: bool, wake: &eframe::egui::Context) {
        self.total += jobs.len();
        if let Ok(mut queued) = self.jobs.lock() {
            queued.extend(jobs.into_iter().map(|j| (j, quick)));
        }
        if self.running.swap(true, Ordering::AcqRel) {
            return;
        }
        self.cancel.store(false, Ordering::Relaxed);
        let tx = self.channel.get_or_insert_with(mpsc::channel).0.clone();
        let (jobs, cancel, running) = (
            Arc::clone(&self.jobs),
            Arc::clone(&self.cancel),
            Arc::clone(&self.running),
        );
        let wake = wake.clone();
        std::thread::spawn(move || {
            let next = || {
                jobs.lock()
                    .ok()
                    .and_then(|mut q| (!q.is_empty()).then(|| q.remove(0)))
            };
            loop {
                let Some((job, quick)) = next().filter(|_| !cancel.load(Ordering::Relaxed)) else {
                    running.store(false, Ordering::Release);
                    let more = jobs.lock().is_ok_and(|q| !q.is_empty());
                    if more
                        && !cancel.load(Ordering::Relaxed)
                        && !running.swap(true, Ordering::AcqRel)
                    {
                        continue;
                    }
                    break;
                };
                let outcome = process(&job, quick);
                if tx.send((job.track, outcome)).is_err() {
                    running.store(false, Ordering::Release);
                    break;
                }
                wake.request_repaint();
            }
            wake.request_repaint();
        });
    }

    pub fn poll(&mut self) -> Vec<(i64, Outcome)> {
        let done: Vec<_> = self
            .channel
            .as_ref()
            .map(|(_, rx)| rx.try_iter().collect())
            .unwrap_or_default();
        for (_, outcome) in &done {
            self.finished += 1;
            match outcome {
                Outcome::Read(_) => self.tally.read += 1,
                Outcome::Unchanged => self.tally.unchanged += 1,
                Outcome::Failed => self.tally.failed += 1,
            }
        }
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
        self.finished >= self.total && !self.running.load(Ordering::Acquire)
    }

    pub fn take_summary(&mut self) -> Option<Tally> {
        if self.total == 0 || !self.is_idle() {
            return None;
        }
        self.total = 0;
        self.finished = 0;
        Some(std::mem::take(&mut self.tally))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quick_scan_skips_files_read_since_their_last_change() {
        let file = std::env::temp_dir().join(format!("backspn-rescan-{}.txt", std::process::id()));
        std::fs::write(&file, b"not audio").unwrap();
        let stamp = scan::file_mtime(&file);
        let job = Job {
            track: 1,
            file: file.clone(),
            stamp,
        };
        assert_eq!(process(&job, true), Outcome::Unchanged);
        assert_eq!(process(&job, false), Outcome::Failed);
        let never = Job { stamp: None, ..job };
        assert_eq!(process(&never, true), Outcome::Failed);
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn the_queue_counts_and_sums_up_once() {
        let ctx = eframe::egui::Context::default();
        let mut q = RescanQueue::default();
        let jobs = (0..3)
            .map(|i| Job {
                track: i,
                file: PathBuf::from(format!("/no/such/file-{i}.mp3")),
                stamp: None,
            })
            .collect();
        q.add(jobs, false, &ctx);
        let start = std::time::Instant::now();
        while !q.is_idle() {
            q.poll();
            assert!(start.elapsed().as_secs() < 10, "timed out");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(q.take_summary().map(|t| t.failed), Some(3));
        assert_eq!(q.take_summary(), None);
    }
}
