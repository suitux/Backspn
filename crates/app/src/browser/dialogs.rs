use std::path::PathBuf;

use backspn_library::{Library, TrackRow};
use eframe::egui::{self, RichText, ScrollArea};

use crate::i18n::tr;
use crate::modal::{self, Ask};
use crate::theme::WindowExt;
use crate::track_edit::{self, TrackEdit};

use super::{Browser, Source};

#[derive(Debug, Clone)]
pub(super) enum BrowserAsk {
    PurgeRecordings,
    DeleteFromDisk(Vec<i64>),
    RemoveMissing(Vec<i64>),
    ClearCollection,
    DeleteNode(i64),
    DeleteSets(Vec<i64>),
    NmlReplace(PathBuf, bool),
    ImportNml {
        path: PathBuf,
        replace: bool,
        file_tags: bool,
    },
    Analyse(Vec<i64>),
    Rescan(Vec<i64>, bool),
    SyncSongInfo,
}

impl Browser {
    pub(super) fn ask_dialog(&mut self, ctx: &egui::Context) {
        if let Some(answer) = modal::show(ctx, &mut self.ask) {
            self.answer(answer);
        }
    }

    pub(super) fn answer(&mut self, answer: BrowserAsk) {
        match answer {
            BrowserAsk::PurgeRecordings => self.delete_recordings(),
            BrowserAsk::DeleteFromDisk(tracks) => self.delete_tracks(&tracks, true),
            BrowserAsk::RemoveMissing(missing) => {
                self.delete_tracks(&missing, false);
                if let Some(deletion) = &mut self.deletion {
                    deletion.check_after = true;
                }
            }
            BrowserAsk::ClearCollection => {
                let result = self.with_lib(Library::clear_collection);
                Self::report(result);
                self.refresh();
            }
            BrowserAsk::DeleteNode(id) => {
                let result = self.with_lib_value(|lib| lib.delete_node(id));
                Self::report(result);
                self.refresh();
            }
            BrowserAsk::DeleteSets(sets) => {
                let result =
                    self.with_lib(|lib| sets.iter().try_for_each(|&id| lib.delete_sample_set(id)));
                Self::report(result);
                self.refresh();
            }
            BrowserAsk::NmlReplace(path, replace) => self.ask_nml_tags(&path, replace),
            BrowserAsk::SyncSongInfo => self.sync_song_info(),
            BrowserAsk::ImportNml {
                path,
                replace,
                file_tags,
            } => self.import_nml_file(&path, replace, file_tags),
            BrowserAsk::Analyse(tracks) => {
                let parts = self.default_parts();
                self.analyse(&tracks, parts);
            }
            BrowserAsk::Rescan(tracks, quick) => self.rescan(&tracks, quick),
        }
    }

    pub(super) fn ask_analyse(&mut self, tracks: Vec<i64>) {
        if tracks.is_empty() {
            return;
        }
        let text = tr("Analyze {n}? Tempo, beatgrid, key and gain are calculated again.")
            .replace("{n}", &super::analysis::count(tracks.len()));
        self.ask = Some(Ask::yes_no("Analyze", text, BrowserAsk::Analyse(tracks)));
    }

    pub(super) fn ask_rescan(&mut self, tracks: Vec<i64>, quick: bool) {
        let tracks: Vec<i64> = tracks.into_iter().filter(|&id| id > 0).collect();
        if tracks.is_empty() {
            return;
        }
        let (title, text) = rescan_question(quick);
        let text = tr(text).replace("{n}", &super::analysis::count(tracks.len()));
        self.ask = Some(Ask::yes_no(title, text, BrowserAsk::Rescan(tracks, quick)));
    }

    pub(super) fn found_in_dialog(&mut self, ctx: &egui::Context) {
        let Some((track, lists)) = self.found_in.clone() else {
            return;
        };
        let mut open = true;
        let mut chosen = None;
        egui::Window::new("Search in Playlists")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show_closable(ctx, &mut open, |ui| {
                if lists.is_empty() {
                    ui.label(RichText::new(tr("The track is in no playlist.")).weak());
                }
                for id in &lists {
                    let path = crate::favorites::node_path(&self.nodes, *id)
                        .map(|p| p.trim_start_matches("$PLAYLIST/t").replace("/t", " / "))
                        .unwrap_or_default();
                    if ui.selectable_label(false, path).clicked() {
                        chosen = Some(*id);
                    }
                }
            });
        if let Some(id) = chosen {
            self.found_in = None;
            self.show(Source::Node(id));
            self.select_track(track);
        } else if !open {
            self.found_in = None;
        }
    }

    pub(super) fn edit_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut edit) = self.editing.take() else {
            return;
        };
        let heading = match (
            edit.all,
            edit.targets().first().and_then(|&id| self.row(id)),
        ) {
            (false, Some(t)) => format!("{} - {}", t.artist, t.title),
            _ => format!("{} tracks", edit.tracks.len()),
        };
        match edit.ui(ctx, &heading) {
            Some(track_edit::Action::Cancel) => return,
            Some(track_edit::Action::Ok) => {
                self.write_edit(&edit);
                self.recompute();
                return;
            }
            Some(track_edit::Action::Apply) => {
                self.write_edit(&edit);
                self.load_edit(&mut edit);
                self.recompute();
            }
            Some(track_edit::Action::Restore) => self.load_edit(&mut edit),
            Some(track_edit::Action::Step(delta)) => {
                self.write_edit(&edit);
                let last = edit.tracks.len() as i64 - 1;
                edit.current = (edit.current as i64 + i64::from(delta)).clamp(0, last) as usize;
                self.load_edit(&mut edit);
            }
            Some(track_edit::Action::ImportTags) => {
                for id in edit.targets() {
                    let result = self.with_lib(|lib| lib.import_tags(id));
                    Self::report(result);
                    self.refresh_track(id);
                }
                self.load_edit(&mut edit);
            }
            Some(track_edit::Action::ExportTags) => self.export_tags(&edit.targets()),
            None => {}
        }
        self.editing = Some(edit);
    }

    pub(super) fn load_edit(&self, edit: &mut TrackEdit) {
        let rows: Vec<&TrackRow> = edit
            .targets()
            .iter()
            .filter_map(|&id| self.row(id))
            .collect();
        edit.load(&rows);
    }

    pub(super) fn write_edit(&mut self, edit: &TrackEdit) {
        self.write_changes(&edit.targets(), &edit.edits());
    }

    pub(super) fn write_changes(&mut self, targets: &[i64], changes: &[track_edit::Edit]) {
        let result = self.with_lib(|lib| {
            for &id in targets {
                for change in changes {
                    match change {
                        track_edit::Edit::Text(field, value) => lib.set_text(id, *field, value)?,
                        track_edit::Edit::Key(key) => lib.set_key(id, *key)?,
                        track_edit::Edit::Rating(r) => lib.set_rating(id, *r)?,
                        track_edit::Edit::TrackNumber(n) => lib.set_track_number(id, *n)?,
                        track_edit::Edit::PlayCount(n) => lib.set_play_count(id, *n)?,
                    }
                }
            }
            Ok(())
        });
        Self::report(result);
        for &id in targets {
            self.refresh_track(id);
        }
        if !changes.is_empty() {
            self.tags_changed(targets);
        }
    }

    pub(super) fn check_consistency(&mut self) {
        match self.lib.as_ref().map(Library::consistency_report) {
            Ok(Ok(report)) => self.consistency = Some(report),
            Ok(Err(e)) => crate::toast::error(e.to_string()),
            Err(_) => {}
        }
    }

    pub(super) fn delete_dialog(&mut self, ctx: &egui::Context) {
        let Some((tracks, mut from_disk)) = self.deleting.clone() else {
            return;
        };
        let mut open = true;
        let (mut delete, mut cancel) = (false, false);
        egui::Window::new("Delete")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show_closable(ctx, &mut open, |ui| {
                ui.label(tr("Delete the selected tracks from the collection?"));
                crate::widgets::check(ui, &mut from_disk, "Also delete their files from the disk")
                    .on_hover_text(tr("The files are deleted for good, not moved to the Trash"));
                ui.horizontal(|ui| {
                    delete = ui.button(tr("Delete")).clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if delete {
            self.deleting = None;
            if from_disk {
                let count = tracks.len();
                self.ask = Some(Ask::yes_no(
                    "Delete from hard drive",
                    format!(
                        "Really delete {count} track file(s) from the hard drive? \
                         This cannot be undone."
                    ),
                    BrowserAsk::DeleteFromDisk(tracks),
                ));
                return;
            }
            self.delete_tracks(&tracks, false);
        } else if open && !cancel {
            self.deleting = Some((tracks, from_disk));
        } else {
            self.deleting = None;
        }
    }

    pub(super) fn consistency_dialog(&mut self, ctx: &egui::Context) {
        let Some(report) = self.consistency.clone() else {
            return;
        };
        let mut open = true;
        let mut action = None;
        egui::Window::new("Consistency Check Report")
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show_closable(ctx, &mut open, |ui| {
                egui::Grid::new("consistency")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (label, n) in [
                            ("Total Tracks", report.total),
                            ("Tracks Missing", report.missing.len()),
                            ("Tracks Not Analyzed", report.not_analyzed.len()),
                        ] {
                            ui.label(label);
                            ui.label(RichText::new(n.to_string()).monospace());
                            ui.end_row();
                        }
                    });
                if !report.missing.is_empty() {
                    ui.separator();
                    ui.label(RichText::new(tr("Missing Tracks")).strong());
                    ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        egui::Grid::new("missing")
                            .striped(true)
                            .num_columns(3)
                            .show(ui, |ui| {
                                for h in ["Artist", "Track", "File"] {
                                    ui.label(RichText::new(h).weak());
                                }
                                ui.end_row();
                                for id in &report.missing {
                                    if let Some(t) = self.row(*id) {
                                        ui.label(&t.artist);
                                        ui.label(&t.title);
                                        ui.label(RichText::new(&t.path).size(11.0));
                                        ui.end_row();
                                    }
                                }
                            });
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let missing = !report.missing.is_empty();
                    if ui
                        .add_enabled(missing, egui::Button::new(tr("Relocate…")))
                        .on_hover_text(tr("Look for the missing files in a folder"))
                        .clicked()
                    {
                        action = Some(true);
                    }
                    if ui
                        .add_enabled(missing, egui::Button::new(tr("Remove Missing")))
                        .clicked()
                    {
                        action = Some(false);
                    }
                });
            });
        match action {
            Some(true) => {
                if let Some(root) = rfd::FileDialog::new().pick_folder() {
                    let result = self.with_lib_value(|lib| lib.relocate(&report.missing, &root));
                    match result {
                        Ok(r) => {
                            self.message = Some(format!(
                                "Relocated {} track(s); {} still missing, {} without a unique path, {} already in the collection",
                                r.found,
                                r.missing.len(),
                                r.ambiguous.len(),
                                r.duplicates.len()
                            ));
                        }
                        Err(e) => crate::toast::error(e.to_string()),
                    }
                    self.refresh();
                    self.check_consistency();
                }
            }
            Some(false) => {
                self.ask = Some(Ask::yes_no(
                    "Remove Missing",
                    "Really remove missing track(s) from collection?",
                    BrowserAsk::RemoveMissing(report.missing.clone()),
                ));
            }
            None if !open => self.consistency = None,
            None => {}
        }
    }
}

fn rescan_question(quick: bool) -> (&'static str, &'static str) {
    if quick {
        (
            "Quick Scan",
            "Read again the tags of the files modified among {n}? The collection takes what \
             the files say.",
        )
    } else {
        (
            "Full Rescan",
            "Read again the tags of {n}? The collection takes what the files say.",
        )
    }
}
