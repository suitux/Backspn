use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    Download(PathBuf),
    Separate {
        track: i64,
        file: PathBuf,
        target: PathBuf,
    },
}

#[derive(Debug)]
pub enum Finished {
    Downloaded(Result<(), String>),
    Separated {
        track: i64,
        result: Result<PathBuf, String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub label: String,
    pub done: f32,
    pub track: Option<i64>,
}

pub struct StemJobs {
    model: PathBuf,
    jobs: Arc<Mutex<VecDeque<Job>>>,
    progress: Arc<Mutex<Option<Progress>>>,
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    done_tx: Sender<Finished>,
    done: Receiver<Finished>,
}

impl std::fmt::Debug for StemJobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StemJobs")
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl StemJobs {
    pub fn new(model: PathBuf) -> Self {
        let (done_tx, done) = mpsc::channel();
        Self {
            model,
            jobs: Arc::default(),
            progress: Arc::default(),
            cancel: Arc::default(),
            running: Arc::default(),
            done_tx,
            done,
        }
    }

    pub fn model(&self) -> &Path {
        &self.model
    }

    pub fn has_model(&self) -> bool {
        self.model.is_file()
    }

    pub fn add(&mut self, jobs: impl IntoIterator<Item = Job>, wake: &eframe::egui::Context) {
        if let Ok(mut queue) = self.jobs.lock() {
            queue.extend(jobs);
        }
        if !self.running.swap(true, Ordering::AcqRel) {
            self.cancel.store(false, Ordering::Relaxed);
            self.spawn(wake.clone());
        }
    }

    pub fn has_track(&self, track: i64) -> bool {
        let queued = self.jobs.lock().is_ok_and(|q| {
            q.iter()
                .any(|j| matches!(j, Job::Separate { track: t, .. } if *t == track))
        });
        queued || self.progress().is_some_and(|p| p.track == Some(track))
    }

    pub fn is_idle(&self) -> bool {
        !self.running.load(Ordering::Acquire)
    }

    pub fn progress(&self) -> Option<Progress> {
        self.progress.lock().ok().and_then(|p| p.clone())
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Ok(mut queue) = self.jobs.lock() {
            queue.clear();
        }
    }

    pub fn poll(&self) -> Vec<Finished> {
        self.done.try_iter().collect()
    }

    fn spawn(&self, wake: eframe::egui::Context) {
        let model = self.model.clone();
        let (jobs, progress, cancel, running, done) = (
            Arc::clone(&self.jobs),
            Arc::clone(&self.progress),
            Arc::clone(&self.cancel),
            Arc::clone(&self.running),
            self.done_tx.clone(),
        );
        let spawned = std::thread::Builder::new()
            .name("stems".into())
            .spawn(move || {
                let mut separator = None;
                loop {
                    let job = jobs.lock().ok().and_then(|mut q| q.pop_front());
                    let Some(job) = job else { break };
                    cancel.store(false, Ordering::Relaxed);
                    let set = |label: &str, done: f32, track| {
                        if let Ok(mut p) = progress.lock() {
                            *p = Some(Progress {
                                label: label.to_string(),
                                done,
                                track,
                            });
                        }
                        wake.request_repaint();
                    };
                    let finished = match job {
                        Job::Download(path) => {
                            set("Downloading the stem model", 0.0, None);
                            let result = backspn_separate::download(
                                &path,
                                &mut |got, total| {
                                    set(
                                        "Downloading the stem model",
                                        got as f32 / total.max(1) as f32,
                                        None,
                                    );
                                },
                                &cancel,
                            );
                            Finished::Downloaded(result.map_err(|e| e.to_string()))
                        }
                        Job::Separate {
                            track,
                            file,
                            target,
                        } => {
                            set("Generating stems", 0.0, Some(track));
                            let result = separate(
                                &model,
                                &mut separator,
                                &file,
                                &target,
                                &mut |f| set("Generating stems", f, Some(track)),
                                &cancel,
                            );
                            Finished::Separated { track, result }
                        }
                    };
                    if let Ok(mut p) = progress.lock() {
                        *p = None;
                    }
                    if done.send(finished).is_err() {
                        break;
                    }
                    wake.request_repaint();
                }
                running.store(false, Ordering::Release);
                wake.request_repaint();
            });
        if let Err(e) = spawned {
            self.running.store(false, Ordering::Release);
            log::warn!("stems: cannot start the worker: {e}");
        }
    }
}

pub fn separate(
    model: &Path,
    separator: &mut Option<backspn_separate::Separator>,
    file: &Path,
    target: &Path,
    progress: &mut dyn FnMut(f32),
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let text = |e: &dyn std::fmt::Display| e.to_string();
    if separator.is_none() {
        if !backspn_separate::verify(model) {
            return Err("the stem model is missing or damaged".into());
        }
        *separator = Some(backspn_separate::Separator::load(model).map_err(|e| text(&e))?);
    }
    let sep = separator.as_mut().expect("loaded above");
    let track = backspn_decode::decode_file(file).map_err(|e| text(&e))?;
    let stems = sep
        .separate_track(&track, &mut |f| progress(0.9 * f), cancel)
        .map_err(|e| text(&e))?;

    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    let name = file.file_stem().unwrap_or_default().to_string_lossy();
    let temp = std::env::temp_dir()
        .join(format!("backspn-stems-{}", std::process::id()))
        .join(format!("{name}.{}", backspn_formats::bstem::EXTENSION));
    std::fs::create_dir_all(temp.parent().expect("has a folder")).map_err(|e| text(&e))?;
    backspn_encode::write(
        &temp,
        track.sample_rate(),
        std::array::from_fn(|k| stems[k].as_slice()),
        &mut |f| progress(0.9 + 0.1 * f),
    )
    .map_err(|e| text(&e))?;
    store(&temp, target).map_err(|e| text(&e))?;
    Ok(target.to_path_buf())
}

fn store(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if std::fs::rename(from, to).is_err() {
        std::fs::copy(from, to)?;
        std::fs::remove_file(from)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("backspn-stemjobs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn without_the_model_a_separation_fails_cleanly() {
        let target = scratch("none/ABC.bstem");
        let result = separate(
            &scratch("missing.onnx"),
            &mut None,
            &scratch("x.wav"),
            &target,
            &mut |_| {},
            &AtomicBool::new(false),
        );
        assert!(result.is_err());
        assert!(!target.exists());
    }

    #[test]
    fn the_queue_reports_each_job() {
        let ctx = eframe::egui::Context::default();
        let mut jobs = StemJobs::new(scratch("missing.onnx"));
        assert!(!jobs.has_model());
        jobs.add(
            [Job::Separate {
                track: 7,
                file: scratch("x.wav"),
                target: scratch("t/ABC.bstem"),
            }],
            &ctx,
        );
        let start = std::time::Instant::now();
        let mut done = Vec::new();
        while done.is_empty() && start.elapsed().as_secs() < 10 {
            done.extend(jobs.poll());
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(matches!(
            done.as_slice(),
            [Finished::Separated {
                track: 7,
                result: Err(_)
            }]
        ));
        assert!(!jobs.has_track(7));
    }

    #[test]
    fn a_track_becomes_a_stem_file() {
        let Some(model) = std::env::var_os("BACKSPN_STEM_MODEL") else {
            return;
        };
        let rate = 44_100;
        let samples: Vec<f32> = (0..rate * 3)
            .flat_map(|i| {
                let x = 0.3 * (std::f32::consts::TAU * 110.0 * i as f32 / rate as f32).sin();
                [x, x]
            })
            .collect();
        let wav = scratch("tone.wav");
        write_wav(&wav, rate, &samples);
        let target = scratch("store/001/ABC.bstem");
        let mut steps = Vec::new();
        let made = separate(
            Path::new(&model),
            &mut None,
            &wav,
            &target,
            &mut |f| steps.push(f),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(made, target);
        assert!(steps.windows(2).all(|w| w[0] <= w[1]));
        assert!((steps.last().unwrap() - 1.0).abs() < 1e-6);
        let header = backspn_formats::bstem::header(&std::fs::read(&target).unwrap()).unwrap();
        assert_eq!(header.stems, backspn_encode::generated_stems());
        assert_eq!(backspn_decode::decode_stems(&target).unwrap().len(), 4);
    }

    fn write_wav(path: &Path, rate: u32, stereo: &[f32]) {
        let data: Vec<u8> = stereo
            .iter()
            .flat_map(|s| ((s * 32_767.0) as i16).to_le_bytes())
            .collect();
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        std::fs::write(path, wav).unwrap();
    }
}
