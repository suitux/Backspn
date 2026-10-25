use backspn_formats::nml::MusicalKey;
use backspn_library::{TextField, TrackRow};
use eframe::egui::{self, ComboBox, RichText};

use crate::columns::stars;
use crate::i18n::tr;
use crate::theme::WindowExt;

#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    Text(TextField, String),
    Key(Option<u8>),
    Rating(u8),
    TrackNumber(Option<u32>),
    PlayCount(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Apply,
    Ok,
    Cancel,
    Restore,
    Step(i32),
    ImportTags,
    ExportTags,
}

#[derive(Debug, Clone)]
pub struct TrackEdit {
    pub tracks: Vec<i64>,
    pub current: usize,
    pub all: bool,
    text: Vec<(String, bool)>,
    key: (Option<u8>, bool),
    rating: (u8, bool),
    track_number: (String, bool),
    overview: Vec<Vec<String>>,
}

pub const OVERVIEW_FIELDS: [TextField; 4] = [
    TextField::Artist,
    TextField::Album,
    TextField::Genre,
    TextField::Label,
];

pub fn overview<'a>(rows: impl IntoIterator<Item = &'a TrackRow>, field: TextField) -> Vec<String> {
    let values: std::collections::BTreeSet<&str> = rows
        .into_iter()
        .map(|t| field.get(t).trim())
        .filter(|v| !v.is_empty())
        .collect();
    values.into_iter().map(str::to_owned).collect()
}

impl TrackEdit {
    pub fn new(tracks: Vec<i64>) -> Self {
        Self {
            all: tracks.len() > 1,
            tracks,
            current: 0,
            text: vec![(String::new(), false); TextField::ALL.len()],
            key: (None, false),
            rating: (0, false),
            track_number: (String::new(), false),
            overview: vec![Vec::new(); TextField::ALL.len()],
        }
    }

    pub fn set_overview(&mut self, rows: &[TrackRow]) {
        for (i, field) in TextField::ALL.into_iter().enumerate() {
            self.overview[i] = if OVERVIEW_FIELDS.contains(&field) {
                overview(rows, field)
            } else {
                Vec::new()
            };
        }
    }

    pub fn targets(&self) -> Vec<i64> {
        if self.all {
            self.tracks.clone()
        } else {
            self.tracks.get(self.current).copied().into_iter().collect()
        }
    }

    pub fn load(&mut self, rows: &[&TrackRow]) {
        fn common<T: PartialEq + Clone>(values: impl Iterator<Item = T>) -> Option<T> {
            let mut values = values.peekable();
            let first = values.peek()?.clone();
            values.all(|v| v == first).then_some(first)
        }
        for (i, field) in TextField::ALL.into_iter().enumerate() {
            let value = common(rows.iter().map(|t| field.get(t).to_owned())).unwrap_or_default();
            self.text[i] = (value, false);
        }
        self.key = (common(rows.iter().map(|t| t.musical_key)).flatten(), false);
        self.rating = (common(rows.iter().map(|t| t.rating)).unwrap_or(0), false);
        let number = common(rows.iter().map(|t| t.track_number)).flatten();
        self.track_number = (number.map_or_else(String::new, |n| n.to_string()), false);
    }

    pub fn edits(&self) -> Vec<Edit> {
        let mut out: Vec<Edit> = TextField::ALL
            .into_iter()
            .zip(&self.text)
            .filter(|(_, (_, checked))| *checked)
            .map(|(f, (v, _))| Edit::Text(f, v.trim().to_owned()))
            .collect();
        if self.key.1 {
            out.push(Edit::Key(self.key.0));
        }
        if self.rating.1 {
            out.push(Edit::Rating(self.rating.0));
        }
        if self.track_number.1 {
            out.push(Edit::TrackNumber(self.track_number.0.trim().parse().ok()));
        }
        out
    }

    pub fn ui(&mut self, ctx: &egui::Context, heading: &str) -> Option<Action> {
        let mut action = None;
        let mut open = true;
        egui::Window::new("Edit")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show_closable(ctx, &mut open, |ui| {
                ui.label(RichText::new(heading).strong());
                if self.tracks.len() > 1 {
                    ui.horizontal(|ui| {
                        let all = crate::widgets::check(ui, &mut self.all, "Edit All")
                            .on_hover_text(tr("Change the checked attributes of every selected track"));
                        if all.changed() {
                            action = Some(Action::Restore);
                        }
                        if !self.all {
                            if ui.button(tr("◀ Previous")).clicked() {
                                action = Some(Action::Step(-1));
                            }
                            ui.label(format!("{} / {}", self.current + 1, self.tracks.len()));
                            if ui.button(tr("Next ▶")).clicked() {
                                action = Some(Action::Step(1));
                            }
                        }
                    });
                }
                ui.separator();
                self.fields_ui(ui);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("Import Tags"))
                        .on_hover_text(
                            tr("Read the attributes from the music files into the collection"),
                        )
                        .clicked()
                    {
                        action = Some(Action::ImportTags);
                    }
                    if ui
                        .button(tr("Export Tags"))
                        .on_hover_text(
                            tr("Write the attributes stored in the collection into the music files' tags"),
                        )
                        .clicked()
                    {
                        action = Some(Action::ExportTags);
                    }
                    if ui
                        .button(tr("Restore"))
                        .on_hover_text(tr("Drop the changes and show the collection's values"))
                        .clicked()
                    {
                        action = Some(Action::Restore);
                    }
                    if ui.button(tr("Apply")).clicked() {
                        action = Some(Action::Apply);
                    }
                    if ui.button("OK").clicked() {
                        action = Some(Action::Ok);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        action = Some(Action::Cancel);
                    }
                });
            });
        if !open {
            action = Some(Action::Cancel);
        }
        action
    }

    fn fields_ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("track-edit")
            .num_columns(4)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for ((field, (value, checked)), values) in TextField::ALL
                    .into_iter()
                    .zip(&mut self.text)
                    .zip(&self.overview)
                {
                    crate::widgets::check(ui, checked, "")
                        .on_hover_text(tr("Tick to change this field in every track"));
                    ui.label(field.name());
                    if ui
                        .add(egui::TextEdit::singleline(value).desired_width(280.0))
                        .changed()
                    {
                        *checked = true;
                    }
                    if !values.is_empty() {
                        overview_menu(ui, field, values, value, checked);
                    }
                    ui.end_row();
                }
                crate::widgets::check(ui, &mut self.key.1, "");
                ui.label(tr("Key"));
                let shown = self
                    .key
                    .0
                    .and_then(MusicalKey::from_value)
                    .map_or_else(String::new, |k| format!("{k} ({})", k.open_key()));
                ComboBox::from_id_salt("edit-key")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.key.0.is_none(), "—").clicked() {
                            self.key = (None, true);
                        }
                        for v in 0..24u8 {
                            let k = MusicalKey::from_value(v).expect("in range");
                            if ui
                                .selectable_label(
                                    self.key.0 == Some(v),
                                    format!("{k} ({})", k.open_key()),
                                )
                                .clicked()
                            {
                                self.key = (Some(v), true);
                            }
                        }
                    });
                ui.end_row();
                crate::widgets::check(ui, &mut self.rating.1, "");
                ui.label(tr("Rating"));
                ComboBox::from_id_salt("edit-rating")
                    .selected_text(stars(self.rating.0))
                    .show_ui(ui, |ui| {
                        for n in 0..=5u8 {
                            if ui
                                .selectable_label(self.rating.0 == n * 51, stars(n * 51))
                                .clicked()
                            {
                                self.rating = (n * 51, true);
                            }
                        }
                    });
                ui.end_row();
                crate::widgets::check(ui, &mut self.track_number.1, "");
                ui.label(tr("Track"));
                if ui
                    .add(egui::TextEdit::singleline(&mut self.track_number.0).desired_width(60.0))
                    .changed()
                {
                    self.track_number.1 = true;
                }
                ui.end_row();
            });
    }
}

fn overview_menu(
    ui: &mut egui::Ui,
    field: TextField,
    values: &[String],
    value: &mut String,
    checked: &mut bool,
) {
    ui.menu_button("▾", |ui| {
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                for v in values {
                    if ui.selectable_label(v == value, v).clicked() {
                        v.clone_into(value);
                        *checked = true;
                        ui.close();
                    }
                }
            });
    })
    .response
    .on_hover_text(format!("{} in the collection", field.name()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, artist: &str, title: &str) -> TrackRow {
        TrackRow {
            id,
            artist: artist.into(),
            title: title.into(),
            rating: 153,
            ..TrackRow::default()
        }
    }

    #[test]
    fn group_editing_shows_common_values_and_writes_checked_ones() {
        let (a, b) = (track(1, "Ann", "One"), track(2, "Ann", "Two"));
        let mut e = TrackEdit::new(vec![1, 2]);
        assert!(e.all, "several tracks start in group mode");
        e.load(&[&a, &b]);
        assert_eq!(e.text[1].0, "Ann", "same artist shown");
        assert_eq!(e.text[0].0, "", "titles differ");
        assert_eq!(e.rating.0, 153);
        assert_eq!(e.edits(), [], "nothing checked yet");
        e.text[6] = ("Label X".into(), true);
        e.rating = (255, true);
        assert_eq!(
            e.edits(),
            [
                Edit::Text(TextField::Label, "Label X".into()),
                Edit::Rating(255)
            ]
        );
        assert_eq!(e.targets(), [1, 2]);
        e.all = false;
        e.current = 1;
        assert_eq!(e.targets(), [2]);
    }

    #[test]
    fn overview_lists_the_collection_values_once_sorted() {
        let rows: Vec<TrackRow> = ["B", "a", "", "B", " C "]
            .into_iter()
            .map(|artist| TrackRow {
                artist: artist.into(),
                ..TrackRow::default()
            })
            .collect();
        assert_eq!(overview(&rows, TextField::Artist), ["B", "C", "a"]);
        let mut edit = TrackEdit::new(vec![1]);
        edit.set_overview(&rows);
        let title = TextField::ALL
            .iter()
            .position(|&f| f == TextField::Title)
            .unwrap();
        assert!(edit.overview[title].is_empty(), "no overview for titles");
    }
}
