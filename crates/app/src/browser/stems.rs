use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText, Ui};

use crate::i18n::tr;
use crate::stem_jobs::{Finished, Job};
use crate::stems::StemDirs;
use crate::theme::WindowExt;

use super::Browser;

const FAILED: &str = "Could not generate the stems of: ";
const LOADED: &str = "Unload the track from its deck before removing its stems.";

impl Browser {
    pub fn set_stem_dirs(&mut self, dirs: StemDirs) {
        if self.stem_dirs != dirs {
            self.stem_dirs = dirs;
            self.generated.borrow_mut().clear();
        }
    }

    fn local_file(&self, id: i64) -> Option<PathBuf> {
        if id < 0 {
            return None;
        }
        backspn_library::paths::local_file(&self.row(id)?.path)
    }

    fn stem_hash(&self, id: i64) -> Option<[u32; 4]> {
        let lib = self.lib.as_ref().ok()?;
        match lib.audio_id(id).ok().flatten() {
            Some(audio_id) => {
                backspn_library::storage::track_hash(Some(&audio_id), Path::new("")).ok()
            }
            None => lib.stem_hash(id).ok().flatten(),
        }
    }

    fn new_stem_hash(&mut self, id: i64, file: &Path) -> Option<[u32; 4]> {
        if let Some(hash) = self.stem_hash(id) {
            return Some(hash);
        }
        let hash = backspn_library::storage::track_hash(None, file).ok()?;
        let result = self.with_lib(|lib| lib.set_stem_hash(id, Some(hash)));
        Self::report(result);
        Some(hash)
    }

    pub fn generated_stems(&self, id: i64) -> Option<PathBuf> {
        if let Some(found) = self.generated.borrow().get(&id) {
            return found.clone();
        }
        let found = self
            .stem_hash(id)
            .and_then(|hash| self.stem_dirs.find(hash));
        self.generated.borrow_mut().insert(id, found.clone());
        found
    }

    pub(super) fn stems_path(&self, id: i64) -> Option<PathBuf> {
        self.row(id)
            .map(|t| PathBuf::from(&t.path))
            .filter(|p| crate::stems::is_stem_file(p))
            .or_else(|| self.generated_stems(id))
    }

    pub(super) fn can_generate_stems(&self, id: i64) -> bool {
        self.local_file(id)
            .is_some_and(|f| !crate::stems::is_stem_file(&f))
            && self.generated_stems(id).is_none()
            && !self.stem_jobs.has_track(id)
    }

    pub(super) fn has_own_stems(&self, id: i64) -> bool {
        self.generated_stems(id)
            .is_some_and(|f| self.stem_dirs.is_own(&f))
    }

    pub(super) fn generate_stems(&mut self, id: i64) {
        if !self.can_generate_stems(id) {
            return;
        }
        if self.stem_jobs.has_model() {
            self.queue_stems(id, false);
        } else {
            self.stem_model_ask = Some(id);
        }
    }

    fn queue_stems(&mut self, id: i64, download: bool) {
        let (Some(file), Some(ctx)) = (self.local_file(id), self.ctx.clone()) else {
            return;
        };
        let Some(hash) = self.new_stem_hash(id, &file) else {
            crate::toast::error(format!("{FAILED}{}", file.display()));
            return;
        };
        let target = self.stem_dirs.target(hash);
        log::info!("stems: {} -> {}", file.display(), target.display());
        let model = self.stem_jobs.model().to_path_buf();
        let jobs = download
            .then_some(Job::Download(model))
            .into_iter()
            .chain([Job::Separate {
                track: id,
                file,
                target,
            }]);
        self.activity.push(
            crate::activity::Level::Info,
            format!("{} {}", tr("Stems queued:"), self.track_label(id)),
        );
        self.stem_jobs.add(jobs, &ctx);
    }

    pub(super) fn delete_stems(&mut self, id: i64) {
        if self.loaded.contains(&id) {
            crate::toast::warning(LOADED);
            return;
        }
        let Some(file) = self
            .generated_stems(id)
            .filter(|f| self.stem_dirs.is_own(f))
        else {
            return;
        };
        if let Err(e) = std::fs::remove_file(&file) {
            crate::toast::error(format!("{}: {e}", file.display()));
        }
        self.generated.borrow_mut().remove(&id);
    }

    pub fn collect_stems(&mut self) {
        for finished in self.stem_jobs.poll() {
            match finished {
                Finished::Downloaded(Err(e)) => {
                    crate::toast::error(format!("Stem model download failed: {e}"));
                    self.activity.push(
                        crate::activity::Level::Error,
                        format!("{} {e}", tr("Stem model download failed:")),
                    );
                }
                Finished::Downloaded(Ok(())) => {
                    self.activity
                        .push(crate::activity::Level::Done, tr("Stem model downloaded"));
                }
                Finished::Separated { track, result } => {
                    self.generated.borrow_mut().remove(&track);
                    let name = self.track_label(track);
                    match &result {
                        Ok(_) => {
                            self.usage.stems_separated += 1;
                            self.activity.push(
                                crate::activity::Level::Done,
                                format!("{} {name}", tr("Stems ready:")),
                            );
                        }
                        Err(e) if e == "cancelled" => self.activity.push(
                            crate::activity::Level::Info,
                            format!("{} {name}", tr("Stems cancelled:")),
                        ),
                        Err(e) => self.activity.push(
                            crate::activity::Level::Error,
                            format!("{} {name}: {e}", tr("Stems failed:")),
                        ),
                    }
                    if let Err(e) = result {
                        let file = self.local_file(track).unwrap_or_default();
                        log::warn!("stems of {}: {e}", file.display());
                        if e != "cancelled" {
                            crate::toast::error(format!("{FAILED}{}", file.display()));
                        }
                    }
                }
            }
        }
    }

    pub(super) fn stems_status(&mut self, ui: &mut Ui) {
        let Some(p) = self.stem_jobs.progress() else {
            return;
        };
        if ui.small_button(tr("Cancel")).clicked() {
            self.stem_jobs.cancel();
        }
        let bar = ui.add(
            egui::ProgressBar::new(p.done)
                .desired_width(160.0)
                .text(format!("{} {:.0}%", p.label, 100.0 * p.done)),
        );
        self.activity_toggle(&bar);
    }

    pub(super) fn stem_model_dialog(&mut self, ctx: &egui::Context) {
        let Some(track) = self.stem_model_ask else {
            return;
        };
        let mut open = true;
        let (mut yes, mut no) = (false, false);
        egui::Window::new("Generate Stems")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show_closable(ctx, &mut open, |ui| {
                ui.label(format!(
                    "Generating stems needs the separation model (htdemucs, {} MB), \
                     downloaded once from Hugging Face.",
                    backspn_separate::MODEL_SIZE / 1_000_000
                ));
                ui.label(
                    RichText::new(model_note(self.stem_jobs.model()))
                        .weak()
                        .small(),
                );
                ui.horizontal(|ui| {
                    yes = ui.button(tr("Download")).clicked();
                    no = ui.button(tr("Cancel")).clicked();
                });
            });
        if yes {
            self.stem_model_ask = None;
            self.queue_stems(track, true);
        } else if no || !open {
            self.stem_model_ask = None;
        }
    }
}

fn model_note(path: &Path) -> String {
    format!("Saved to {}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser_with_track(dir: &Path) -> (Browser, i64, PathBuf) {
        let mut b = Browser::in_memory();
        let file = dir.join("song.wav");
        std::fs::write(&file, b"RIFF....WAVE").unwrap();
        let id = b.add_files(std::slice::from_ref(&file))[0];
        b.set_stem_dirs(StemDirs {
            own: dir.join("own"),
            theirs: Some(dir.join("theirs")),
        });
        (b, id, file)
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("backspn-bstems-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn generated_stems_are_found_and_only_own_ones_deleted() {
        let dir = scratch("find");
        let (mut b, id, file) = browser_with_track(&dir);
        assert!(b.can_generate_stems(id));
        assert_eq!(b.generated_stems(id), None);
        assert_eq!(b.stems_path(id), None);
        assert_eq!(b.stem_hash(id), None);
        let hash = b.new_stem_hash(id, &file).unwrap();
        assert_eq!(
            hash,
            backspn_library::storage::track_hash(None, &file).unwrap()
        );
        assert_eq!(b.stem_hash(id), Some(hash), "kept");

        let theirs = backspn_library::storage::stem_file(&dir.join("theirs"), hash);
        std::fs::create_dir_all(theirs.parent().unwrap()).unwrap();
        std::fs::write(&theirs, b"x").unwrap();
        b.generated.borrow_mut().clear();
        assert_eq!(b.generated_stems(id), Some(theirs.clone()));
        assert_eq!(
            b.stems_path(id),
            Some(theirs.clone()),
            "the Stems column shows it"
        );
        assert!(!b.can_generate_stems(id));
        assert!(!b.has_own_stems(id));
        b.delete_stems(id);
        assert!(theirs.exists());

        std::fs::remove_file(&theirs).unwrap();
        let own = backspn_library::storage::stem_file(&dir.join("own"), hash);
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        std::fs::write(&own, b"x").unwrap();
        b.generated.borrow_mut().clear();
        assert!(b.has_own_stems(id));
        b.set_loaded(vec![id]);
        b.delete_stems(id);
        assert!(own.exists());
        assert!(crate::toast::showing(crate::toast::Kind::Warning, |t| {
            t == LOADED
        }));
        b.set_loaded(Vec::new());
        b.delete_stems(id);
        assert!(!own.exists());
        assert_eq!(b.generated_stems(id), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_file_says_so() {
        let dir = scratch("missing");
        let (mut b, id, file) = browser_with_track(&dir);
        b.set_context(&egui::Context::default());
        std::fs::remove_file(&file).unwrap();
        b.queue_stems(id, false);
        assert!(!b.stem_jobs.has_track(id));
        let failed = format!("{FAILED}{}", file.display());
        assert!(crate::toast::showing(crate::toast::Kind::Error, |t| {
            t == failed
        }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_the_model_generating_asks_first() {
        let dir = scratch("ask");
        let (mut b, id, _) = browser_with_track(&dir);
        b.stem_jobs = crate::stem_jobs::StemJobs::new(dir.join("missing.onnx"));
        b.generate_stems(id);
        assert_eq!(b.stem_model_ask, Some(id));
        let _ = std::fs::remove_dir_all(dir);
    }
}
