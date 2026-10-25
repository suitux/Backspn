use std::path::{Path, PathBuf};

use backspn_formats::nml::SampleSet;
use backspn_library::clock::LocalTime;
use eframe::egui;

use crate::App;
use crate::i18n::tr;
use crate::set_folder;

pub(crate) struct SetImport {
    folder: PathBuf,
    set: SampleSet,
    title: String,
}

impl App {
    pub(crate) fn export_set(&mut self, id: i64) {
        let Some(row) = self.browser.library_set(id) else {
            return;
        };
        let Some(parent) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let files: Vec<Option<PathBuf>> = row
            .tracks
            .iter()
            .map(|t| {
                let path = self.browser.library_track((*t)?)?.path;
                Some(PathBuf::from(path.split('#').next().unwrap_or(&path)))
            })
            .collect();
        match set_folder::export(&row.set, &files, &parent, LocalTime::now()) {
            Ok(folder) => crate::toast::info(format!("Exported to {}", folder.display())),
            Err(e) => crate::toast::error(format!("Could not export the Sample Set: {e}")),
        }
    }

    pub(crate) fn import_set(&mut self) {
        let Some(folder) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let set = match set_folder::read(&folder) {
            Ok((set, _)) => set,
            Err(e) => {
                crate::toast::error(format!("Not a Sample Set folder: {e}"));
                return;
            }
        };
        if self.browser.library_set_by_title(&set.title).is_some() {
            let title = set.title.clone();
            self.set_import = Some(SetImport { folder, set, title });
        } else {
            let title = set.title.clone();
            self.finish_import(&folder, set, &title);
        }
    }

    fn finish_import(&mut self, folder: &Path, mut set: SampleSet, title: &str) {
        let kept =
            match set_folder::copy_in(folder, &self.settings.sets_dir, title, LocalTime::now()) {
                Ok(kept) => kept,
                Err(e) => {
                    crate::toast::error(format!("Could not import the Sample Set: {e}"));
                    return;
                }
            };
        let files = match set_folder::read(&kept) {
            Ok((_, files)) => files,
            Err(e) => {
                crate::toast::error(e);
                return;
            }
        };
        let present: Vec<PathBuf> = files.iter().flatten().cloned().collect();
        let mut ids = self.browser.add_files_keyed(&present).into_iter();
        let tracks: Vec<Option<i64>> = files
            .iter()
            .map(|f| f.as_ref().and_then(|_| ids.next().flatten()))
            .collect();
        title.clone_into(&mut set.title);
        set.location = None;
        set.import_date = Some(LocalTime::now().date_text());
        if self.browser.save_library_set(None, &set, &tracks).is_some() {
            crate::toast::info(format!("Imported Sample Set {title}"));
            self.refresh_set_menus();
        }
    }

    fn refresh_set_menus(&mut self) {
        let sets: Vec<(i64, String)> = self
            .browser
            .library_sets()
            .into_iter()
            .map(|s| (s.id, s.set.title))
            .collect();
        for deck in &mut self.decks {
            if let Some(view) = &mut deck.sample_deck {
                view.library_sets.clone_from(&sets);
            }
        }
    }

    pub(crate) fn set_import_dialog(&mut self, ctx: &egui::Context) {
        let Some(import) = &mut self.set_import else {
            return;
        };
        let mut answer = None;
        egui::Window::new(tr("Sample Set Name"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(tr(
                    "The folder you are trying to import contains a Sample Set that already \
                     exists in your Collection.",
                ));
                ui.label(tr(
                    "Please provide a new name for the imported Sample Set to continue \
                     importing or click Cancel to abort the import.",
                ));
                ui.text_edit_singleline(&mut import.title);
                ui.horizontal(|ui| {
                    if ui.button(tr("OK")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        answer = Some(false);
                    }
                });
            });
        match answer {
            Some(true) => {
                let title = import.title.trim().to_owned();
                if title.is_empty() || self.browser.library_set_by_title(&title).is_some() {
                    return;
                }
                if let Some(import) = self.set_import.take() {
                    self.finish_import(&import.folder, import.set, &title);
                }
            }
            Some(false) => self.set_import = None,
            None => {}
        }
    }
}
