use std::collections::VecDeque;

use backspn_library::clock::LocalTime;
use eframe::egui::{self, RichText};

use crate::i18n::tr;
use crate::theme::{self, WindowExt};

const KEPT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Done,
    Error,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Done => "Done",
            Self::Error => "Error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub time: String,
    pub level: Level,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Activity {
    entries: VecDeque<Entry>,
    pub open: bool,
}

impl Activity {
    pub fn push(&mut self, level: Level, text: impl Into<String>) {
        self.push_at(clock_text(LocalTime::now()), level, text);
    }

    fn push_at(&mut self, time: String, level: Level, text: impl Into<String>) {
        if self.entries.len() == KEPT {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            time,
            level,
            text: text.into(),
        });
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn window(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let t = theme::get(ctx);
        let mut open = true;
        let mut clear = false;
        egui::Window::new(tr("Activity"))
            .default_size([520.0, 320.0])
            .collapsible(false)
            .show_closable(ctx, &mut open, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Analysis and stems, newest at the bottom.")).weak());
                    clear = ui.small_button(tr("Clear")).clicked();
                });
                ui.separator();
                if self.entries.is_empty() {
                    ui.label(RichText::new(tr("Nothing yet.")).weak());
                    return;
                }
                let rows: Vec<&Entry> = self.entries().collect();
                egui_extras::TableBuilder::new(ui)
                    .id_salt("activity-table")
                    .striped(true)
                    .resizable(true)
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(egui_extras::Column::auto().at_least(64.0))
                    .column(egui_extras::Column::auto().at_least(56.0))
                    .column(egui_extras::Column::remainder().at_least(120.0).clip(true))
                    .header(20.0, |mut header| {
                        for title in ["Time", "Status", "Job"] {
                            header.col(|ui| {
                                ui.label(RichText::new(tr(title)).strong().small());
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(20.0, rows.len(), |mut row| {
                            let e = rows[row.index()];
                            let colour = match e.level {
                                Level::Info => t.text_dim,
                                Level::Done => t.text,
                                Level::Error => t.danger,
                            };
                            row.col(|ui| {
                                ui.label(RichText::new(&e.time).monospace().weak());
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(tr(e.level.name())).color(colour));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&e.text).color(colour))
                                    .on_hover_text(&e.text);
                            });
                        });
                    });
            });
        if clear {
            self.clear();
        }
        self.open = open;
    }
}

fn clock_text(time: LocalTime) -> String {
    let s = time.seconds;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_diary_keeps_the_last_entries_in_order() {
        let mut a = Activity::default();
        for i in 0..KEPT + 2 {
            a.push_at(String::new(), Level::Info, i.to_string());
        }
        let texts: Vec<&str> = a.entries().map(|e| e.text.as_str()).collect();
        assert_eq!(texts.len(), KEPT);
        assert_eq!(texts[0], "2");
        assert_eq!(texts[KEPT - 1], (KEPT + 1).to_string());
        a.clear();
        assert_eq!(a.entries().count(), 0);
    }

    #[test]
    fn times_read_as_a_clock() {
        let t = LocalTime {
            year: 2026,
            month: 10,
            day: 23,
            seconds: 15 * 3600 + 4 * 60 + 9,
        };
        assert_eq!(clock_text(t), "15:04:09");
    }
}
