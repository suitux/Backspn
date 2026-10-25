use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use backspn_formats::nml::KeyNotation;
use backspn_library::TrackRow;

struct Job {
    path: PathBuf,
    track: TrackRow,
    notation: KeyNotation,
}

pub type Done = (i64, Result<(), String>);

pub struct TagWriter {
    jobs: Sender<Job>,
    done: Receiver<Done>,
    pending: usize,
    pub after_changes: bool,
    pub notation: KeyNotation,
}

impl Default for TagWriter {
    fn default() -> Self {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (report, done) = mpsc::channel();
        std::thread::Builder::new()
            .name("tag-writer".into())
            .spawn(move || {
                for job in inbox {
                    let result = backspn_library::tags::write(&job.path, &job.track, job.notation)
                        .map_err(|e| e.to_string());
                    if report.send((job.track.id, result)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn the tag writer");
        Self {
            jobs,
            done,
            pending: 0,
            after_changes: false,
            notation: KeyNotation::OpenKey,
        }
    }
}

impl std::fmt::Debug for TagWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TagWriter")
            .field("pending", &self.pending)
            .finish_non_exhaustive()
    }
}

impl TagWriter {
    pub fn schedule(&mut self, track: &TrackRow) {
        let path = PathBuf::from(track.path.split('#').next().unwrap_or(&track.path));
        let job = Job {
            path,
            track: track.clone(),
            notation: self.notation,
        };
        if self.jobs.send(job).is_ok() {
            self.pending += 1;
        }
    }

    pub fn poll(&mut self) -> Vec<Done> {
        let done: Vec<Done> = self.done.try_iter().collect();
        self.pending = self.pending.saturating_sub(done.len());
        done
    }

    pub fn busy(&self) -> bool {
        self.pending > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_reports_the_failure() {
        let mut w = TagWriter::default();
        let track = TrackRow {
            id: 7,
            path: "/nonexistent/x.mp3".into(),
            ..TrackRow::default()
        };
        w.schedule(&track);
        assert!(w.busy());
        let mut done = Vec::new();
        for _ in 0..200 {
            done.extend(w.poll());
            if !done.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(matches!(done.as_slice(), [(7, Err(_))]), "{done:?}");
        assert!(!w.busy());
    }
}
