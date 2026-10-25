use std::path::Path;

use eframe::egui::{self, Ui};

use crate::activity::Level;
use crate::i18n::tr;
use crate::rescan_queue::{Job, Outcome, Tally};

use super::Browser;
use super::files::audio_files;

impl Browser {
    pub(super) fn rescan(&mut self, tracks: &[i64], quick: bool) {
        let Some(ctx) = self.ctx.clone() else { return };
        let jobs: Vec<Job> = tracks
            .iter()
            .filter(|&&id| id > 0)
            .filter_map(|&id| {
                let file = backspn_library::paths::local_file(&self.row(id)?.path)?;
                let stamp = self
                    .lib
                    .as_ref()
                    .ok()
                    .and_then(|lib| lib.scan_stamp(id).ok().flatten());
                Some(Job {
                    track: id,
                    file,
                    stamp,
                })
            })
            .collect();
        if jobs.is_empty() {
            return;
        }
        let what = if quick {
            tr("Quick Scan queued:")
        } else {
            tr("Full Rescan queued:")
        };
        self.activity.push(
            Level::Info,
            format!("{what} {}", super::analysis::count(jobs.len())),
        );
        self.rescan.add(jobs, quick, &ctx);
    }

    pub(super) fn folder_tracks(&self, dir: &Path) -> Vec<i64> {
        audio_files(dir)
            .iter()
            .filter_map(|p| self.by_path.get(&*p.to_string_lossy()).copied())
            .collect()
    }

    pub(super) fn collect_rescan(&mut self) {
        for (track, outcome) in self.rescan.poll() {
            match outcome {
                Outcome::Read(scan) => {
                    let result = self.with_lib(|lib| lib.store_scan(track, &scan));
                    Self::report(result);
                    self.refresh_track(track);
                }
                Outcome::Unchanged => {}
                Outcome::Failed => {
                    let name = self.track_label(track);
                    self.activity.push(
                        Level::Error,
                        format!("{} {name}", tr("Cannot read the tags of")),
                    );
                }
            }
        }
        if let Some(tally) = self.rescan.take_summary() {
            self.activity.push(Level::Done, summary(tally));
        }
    }

    pub(super) fn rescan_status(&mut self, ui: &mut Ui) {
        if self.rescan.is_idle() {
            return;
        }
        let (n, total) = (self.rescan.finished, self.rescan.total.max(1));
        if ui.small_button(tr("Cancel")).clicked() {
            self.rescan.cancel();
            self.activity.push(Level::Info, tr("Rescan cancelled"));
        }
        let bar = ui.add(
            egui::ProgressBar::new(n as f32 / total as f32)
                .desired_width(140.0)
                .text(format!("{} {n}/{total}", tr("Scanning"))),
        );
        self.activity_toggle(&bar);
    }
}

fn summary(t: Tally) -> String {
    format!(
        "{} {} {}, {} {}, {} {}",
        tr("Rescan done:"),
        t.read,
        tr("read"),
        t.unchanged,
        tr("unchanged"),
        t.failed,
        tr("failed"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_counts_each_outcome() {
        let t = Tally {
            read: 3,
            unchanged: 10,
            failed: 1,
        };
        assert_eq!(summary(t), "Rescan done: 3 read, 10 unchanged, 1 failed");
    }
}
