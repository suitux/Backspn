use std::path::PathBuf;

use eframe::egui::{self, RichText, Ui};

use crate::activity::Level;
use crate::analysis_queue::Parts;
use crate::i18n::tr;
use crate::theme;
use crate::theme::WindowExt;

use super::Browser;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalysedGrid {
    pub track: i64,
    pub bpm: Option<f64>,
    pub anchor_s: Option<f64>,
}

impl Browser {
    pub fn set_analysis_defaults(
        &mut self,
        range: backspn_analysis::BpmRange,
        analyze_imports: bool,
    ) {
        self.analysis_range = range;
        self.analyze_imports = analyze_imports;
    }

    pub(super) fn analyse(&mut self, tracks: &[i64], parts: Parts) {
        let Some(ctx) = self.ctx.clone() else { return };
        let jobs: Vec<(i64, PathBuf)> = tracks
            .iter()
            .filter_map(|&id| {
                backspn_library::paths::local_file(&self.row(id)?.path).map(|file| (id, file))
            })
            .collect();
        if !jobs.is_empty() {
            self.activity.push(
                Level::Info,
                format!("{} {}", tr("Analysis queued:"), count(jobs.len())),
            );
            self.analysis.add(jobs, parts, &ctx);
        }
    }

    pub(super) fn default_parts(&self) -> Parts {
        Parts {
            range: self.analysis_range,
            ..Parts::default()
        }
    }

    pub fn store_overview(&mut self, track: i64, overview: &[u8]) {
        if overview.is_empty() || self.row(track).is_some_and(|t| t.overview == overview) {
            return;
        }
        let result = self.with_lib(|lib| lib.save_overview(track, overview));
        Self::report(result);
        self.refresh_track(track);
    }

    pub fn reanalyse_grid(&mut self, track: i64, grid: bool) {
        let parts = Parts {
            bpm: true,
            grid,
            key: false,
            gain: false,
            ..self.default_parts()
        };
        self.analyse(&[track], parts);
    }

    pub fn collect_analysis(&mut self) -> Vec<AnalysedGrid> {
        self.collect_rescan();
        let mut grids = Vec::new();
        for done in self.analysis.poll() {
            self.store_overview(done.track, &done.overview);
            let name = self.track_label(done.track);
            let result = match done.result {
                Ok(analysis) => {
                    self.activity
                        .push(Level::Done, analysed_text(&name, &analysis));
                    let update = crate::track_store::batch_update(&analysis, &done.parts);
                    let applies =
                        update.replace_locked || !self.row(done.track).is_some_and(|t| t.locked);
                    let saved = self.with_lib(|lib| lib.save_analysis(done.track, &update));
                    if applies && saved.is_ok() && update.bpm.is_some() {
                        grids.push(AnalysedGrid {
                            track: done.track,
                            bpm: update.bpm,
                            anchor_s: update.grid_anchor_s,
                        });
                    }
                    self.refresh_track(done.track);
                    self.tags_changed(&[done.track]);
                    saved
                }
                Err(e) => {
                    log::warn!("analysis of track {} failed: {e}", done.track);
                    self.activity.push(
                        Level::Error,
                        format!("{} {name}: {e}", tr("Analysis failed:")),
                    );
                    Ok(())
                }
            };
            Self::report(result);
            self.refresh_track(done.track);
        }
        grids
    }

    pub(super) fn analysis_status(&mut self, ui: &mut Ui) {
        if self.analysis.is_idle() {
            return;
        }
        let (n, total) = (self.analysis.finished, self.analysis.total.max(1));
        if ui.small_button(tr("Cancel")).clicked() {
            self.analysis.cancel();
            self.activity.push(Level::Info, tr("Analysis cancelled"));
        }
        let bar = ui.add(
            egui::ProgressBar::new(n as f32 / total as f32)
                .desired_width(140.0)
                .text(format!("Analyzing {n}/{total}")),
        );
        self.activity_toggle(&bar);
    }

    pub fn progress(&mut self, ui: &mut Ui) {
        self.stems_status(ui);
        self.analysis_status(ui);
        self.rescan_status(ui);
        self.activity.window(ui.ctx());
    }

    pub(super) fn activity_toggle(&mut self, bar: &egui::Response) {
        let bar = bar
            .interact(egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(tr("Click: what the background jobs are doing"));
        if bar.clicked() {
            self.activity.open = !self.activity.open;
        }
    }

    pub(super) fn track_label(&self, id: i64) -> String {
        let Some(row) = self.row(id) else {
            return format!("#{id}");
        };
        match (row.artist.is_empty(), row.title.is_empty()) {
            (_, true) => std::path::Path::new(row.path.split('#').next().unwrap_or(&row.path))
                .file_name()
                .map_or_else(|| row.path.clone(), |n| n.to_string_lossy().into_owned()),
            (true, false) => row.title.clone(),
            (false, false) => format!("{} - {}", row.artist, row.title),
        }
    }

    pub(super) fn analysis_options_dialog(&mut self, ctx: &egui::Context) {
        let Some((tracks, mut parts)) = self.analysis_dialog.take() else {
            return;
        };
        let mut open = true;
        let mut start = false;
        let mut cancel = false;
        egui::Window::new("Analysis Options")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show_closable(ctx, &mut open, |ui| {
                ui.label(format!("{} track(s)", tracks.len()));
                crate::widgets::check(ui, &mut parts.grid, "Set Beatgrid");
                crate::widgets::check(ui, &mut parts.bpm, "BPM");
                crate::widgets::check(ui, &mut parts.key, "Key");
                crate::widgets::check(ui, &mut parts.gain, "Gain");
                crate::widgets::check(ui, &mut parts.replace_locked, "Replace Locked Values");
                crate::widgets::check(ui, &mut parts.parallel, "Parallel Processing");
                if parts.parallel {
                    ui.label(
                        RichText::new(tr("This uses a lot of CPU: avoid it while playing live."))
                            .color(theme::get(ctx).danger),
                    );
                }
                ui.horizontal(|ui| {
                    start = ui.button("OK").clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if start {
            self.analyse(&tracks, parts);
        } else if open && !cancel {
            self.analysis_dialog = Some((tracks, parts));
        }
    }

    pub(super) fn analyse_imported(&mut self, ids: &[i64]) {
        if !self.analyze_imports {
            return;
        }
        let fresh: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|id| self.row(*id).is_some_and(|t| !t.analyzed))
            .collect();
        let parts = self.default_parts();
        self.analyse(&fresh, parts);
    }
}

pub(super) fn count(n: usize) -> String {
    if n == 1 {
        tr("1 track").to_owned()
    } else {
        format!("{n} {}", tr("tracks"))
    }
}

fn analysed_text(name: &str, analysis: &backspn_analysis::Analysis) -> String {
    let mut found = Vec::new();
    if let Some(g) = analysis.beatgrid {
        found.push(format!("{:.2} BPM", g.bpm));
    }
    if let Some(k) = analysis.key
        && let Some(key) = backspn_formats::nml::MusicalKey::from_value(k.nml_value())
    {
        found.push(key.open_key());
    }
    if found.is_empty() {
        format!("{} {name}", tr("Analysed:"))
    } else {
        format!("{} {name} ({})", tr("Analysed:"), found.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_analysis_reads_with_its_tempo() {
        let analysis = backspn_analysis::Analysis {
            beatgrid: Some(backspn_analysis::Beatgrid {
                bpm: 128.0,
                anchor_s: 0.1,
            }),
            key: None,
            loudness: None,
            transients: Vec::new(),
        };
        assert_eq!(
            analysed_text("A - B", &analysis),
            "Analysed: A - B (128.00 BPM)"
        );
        let none = backspn_analysis::Analysis {
            beatgrid: None,
            ..analysis
        };
        assert_eq!(analysed_text("A - B", &none), "Analysed: A - B");
        assert_eq!(count(1), "1 track");
        assert_eq!(count(3), "3 tracks");
    }
}
