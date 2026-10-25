pub(crate) mod wav;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use backspn_engine::{Command, RecordFeed, record_tap};
use backspn_library::clock::LocalTime;

pub use wav::Format;
use wav::WavWriter;

const RING_SECONDS: f64 = 4.0;
const POLL: Duration = Duration::from_millis(50);
const MB: u64 = 1024 * 1024;
const WAV_LIMIT: u64 = u32::MAX as u64 - 64 * MB;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub dir: PathBuf,
    pub prefix: String,
    pub split_mb: u32,
    pub format: Format,
}

pub fn gain(knob: f32) -> f32 {
    (2.0 * (2.0 * knob.clamp(0.0, 1.0) - 1.0)).exp2()
}

fn file_safe(text: &str) -> String {
    text.chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

fn hms(seconds: u64) -> String {
    format!(
        "{:02}h{:02}m{:02}s",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

pub fn first_name(prefix: &str, now: LocalTime) -> String {
    let prefix = if prefix.trim().is_empty() {
        "Recording".to_owned()
    } else {
        file_safe(prefix.trim())
    };
    format!(
        "{prefix}_{}-{:02}-{:02}_{}",
        now.year,
        now.month,
        now.day,
        hms(u64::from(now.seconds))
    )
}

pub fn split_name(first: &str, recorded_s: u64) -> String {
    format!("{first}_{}", hms(recorded_s))
}

fn free_path(dir: &Path, name: &str) -> PathBuf {
    let mut path = dir.join(format!("{name}.wav"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{name} {n}.wav"));
        n += 1;
    }
    path
}

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub recording: bool,
    pub seconds: f64,
    pub file: Option<PathBuf>,
    pub peak: [f32; 2],
    pub error: Option<String>,
}

#[derive(Default)]
struct Shared {
    status: Mutex<Status>,
    finished: Mutex<Vec<PathBuf>>,
    stop: AtomicBool,
    cut: AtomicBool,
    gain: AtomicU32,
}

impl Shared {
    fn status(&self) -> std::sync::MutexGuard<'_, Status> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct Recorder {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    pub last: Option<PathBuf>,
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder")
            .field("recording", &self.recording())
            .finish_non_exhaustive()
    }
}

impl Default for Recorder {
    fn default() -> Self {
        let shared = Shared::default();
        shared.gain.store(1f32.to_bits(), Ordering::Relaxed);
        Self {
            shared: Arc::new(shared),
            worker: None,
            last: None,
        }
    }
}

impl Recorder {
    pub fn recording(&self) -> bool {
        self.worker.is_some()
    }

    pub fn set_gain(&self, knob: f32) {
        self.shared
            .gain
            .store(gain(knob).to_bits(), Ordering::Relaxed);
    }

    pub fn start(&mut self, rate: u32, options: &Options) -> anyhow::Result<Command> {
        if self.recording() {
            anyhow::bail!("already recording");
        }
        std::fs::create_dir_all(&options.dir)?;
        let first = first_name(&options.prefix, LocalTime::now());
        let path = free_path(&options.dir, &first);
        let writer = WavWriter::create(&path, options.format, rate)?;
        let (tap, feed) = record_tap(rate, RING_SECONDS);
        self.shared.stop.store(false, Ordering::Relaxed);
        self.shared.cut.store(false, Ordering::Relaxed);
        *self.shared.status() = Status {
            recording: true,
            file: Some(path.clone()),
            ..Status::default()
        };
        let job = Job {
            shared: Arc::clone(&self.shared),
            feed,
            rate,
            dir: options.dir.clone(),
            format: options.format,
            first,
            split_bytes: (u64::from(options.split_mb.max(1)) * MB).min(WAV_LIMIT),
            writer: Some((writer, path)),
            frames: 0,
        };
        self.worker = Some(
            std::thread::Builder::new()
                .name("mix-recorder".into())
                .spawn(move || job.run())?,
        );
        Ok(Command::RecordTap(Some(tap)))
    }

    pub fn stop(&mut self) -> Command {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            drop(worker.join());
        }
        Command::RecordTap(None)
    }

    pub fn cut(&self) {
        if self.recording() {
            self.shared.cut.store(true, Ordering::Relaxed);
        }
    }

    pub fn set_error(&self, error: String) {
        self.shared.status().error = Some(error);
    }

    pub fn status(&self) -> Status {
        let mut s = self.shared.status();
        let out = s.clone();
        s.peak = [0.0; 2];
        out
    }

    pub fn take_finished(&mut self) -> Vec<PathBuf> {
        let files: Vec<PathBuf> = std::mem::take(
            &mut *self
                .shared
                .finished
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        if let Some(f) = files.last() {
            self.last = Some(f.clone());
        }
        files
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if self.recording() {
            drop(self.stop());
        }
    }
}

struct Job {
    shared: Arc<Shared>,
    feed: RecordFeed,
    rate: u32,
    dir: PathBuf,
    format: Format,
    first: String,
    split_bytes: u64,
    writer: Option<(WavWriter, PathBuf)>,
    frames: u64,
}

impl Job {
    fn run(mut self) {
        let mut buf = Vec::new();
        loop {
            let stopping = self.shared.stop.load(Ordering::Relaxed);
            buf.clear();
            self.feed.drain(&mut buf);
            if let Err(e) = self.write(&buf) {
                self.shared.status().error = Some(e.to_string());
                break;
            }
            if stopping {
                break;
            }
            std::thread::sleep(POLL);
        }
        self.close();
        let mut s = self.shared.status();
        s.recording = false;
        s.file = None;
    }

    fn write(&mut self, samples: &[f32]) -> std::io::Result<()> {
        let gain = f32::from_bits(self.shared.gain.load(Ordering::Relaxed));
        let cut = self.shared.cut.swap(false, Ordering::Relaxed);
        let full = self
            .writer
            .as_ref()
            .is_some_and(|(w, _)| w.data_bytes() >= self.split_bytes);
        if cut || full {
            self.next_file()?;
        }
        if let Some((w, _)) = &mut self.writer {
            w.write(samples, gain)?;
        }
        self.frames += samples.len() as u64 / 2;
        let mut peak = [0.0f32; 2];
        for [l, r] in samples.as_chunks::<2>().0 {
            peak[0] = peak[0].max((l * gain).abs());
            peak[1] = peak[1].max((r * gain).abs());
        }
        let mut s = self.shared.status();
        s.seconds = self.frames as f64 / f64::from(self.rate);
        s.peak = [s.peak[0].max(peak[0]), s.peak[1].max(peak[1])];
        Ok(())
    }

    fn next_file(&mut self) -> std::io::Result<()> {
        self.close();
        let name = split_name(&self.first, self.frames / u64::from(self.rate));
        let path = free_path(&self.dir, &name);
        self.writer = Some((
            WavWriter::create(&path, self.format, self.rate)?,
            path.clone(),
        ));
        self.shared.status().file = Some(path);
        Ok(())
    }

    fn close(&mut self) {
        if let Some((w, path)) = self.writer.take() {
            match w.finish() {
                Ok(()) => self
                    .shared
                    .finished
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(path),
                Err(e) => self.shared.status().error = Some(e.to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(h: u32, m: u32, s: u32) -> LocalTime {
        LocalTime {
            year: 2026,
            month: 10,
            day: 2,
            seconds: h * 3600 + m * 60 + s,
        }
    }

    #[test]
    fn names_are_right() {
        assert_eq!(
            first_name("", at(14, 5, 33)),
            "Recording_2026-10-02_14h05m33s"
        );
        assert_eq!(
            first_name("My: set", at(0, 0, 0)),
            "My_ set_2026-10-02_00h00m00s"
        );
        assert_eq!(
            split_name("Recording_2026-10-02_14h05m33s", 3725),
            "Recording_2026-10-02_14h05m33s_01h02m05s"
        );
    }

    #[test]
    fn gain_is_twelve_db_each_way() {
        assert!((gain(0.5) - 1.0).abs() < 1e-6);
        assert!((gain(1.0) - 4.0).abs() < 1e-6);
        assert!((gain(0.0) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn a_recording_is_cut_and_closed() {
        let dir = std::env::temp_dir().join(format!("backspn-rec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut rec = Recorder::default();
        let options = Options {
            dir: dir.clone(),
            prefix: "t".into(),
            split_mb: 650,
            format: Format::Pcm16,
        };
        let Command::RecordTap(Some(mut tap)) = rec.start(1000, &options).unwrap() else {
            panic!("no tap");
        };
        assert!(tap.push(&[0.5; 2000]));
        std::thread::sleep(POLL * 3);
        rec.cut();
        std::thread::sleep(POLL * 3);
        assert!(tap.push(&[0.25; 1000]));
        std::thread::sleep(POLL * 3);
        assert!(matches!(rec.stop(), Command::RecordTap(None)));
        let files = rec.take_finished();
        assert_eq!(files.len(), 2, "{files:?}");
        assert!(files[1].to_string_lossy().ends_with("_00h00m01s.wav"));
        assert_eq!(rec.last.as_ref(), files.last());
        let first = backspn_decode::decode_file(&files[0]).unwrap();
        assert_eq!(first.frames(), 1000);
        assert!(!rec.status().recording);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_files_are_kept() {
        let dir = std::env::temp_dir().join(format!("backspn-rec-free-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.wav"), b"x").unwrap();
        assert_eq!(free_path(&dir, "a"), dir.join("a 2.wav"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
