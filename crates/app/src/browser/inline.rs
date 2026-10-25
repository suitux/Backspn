use backspn_formats::nml::MusicalKey;
use backspn_library::{TextField, TrackRow};
use eframe::egui::{self, Ui};

use crate::columns::BrowserColumn;
use crate::track_edit::Edit;

use super::Browser;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct InlineEdit {
    pub id: i64,
    pub column: BrowserColumn,
    pub text: String,
    pub focused: bool,
}

fn text_field(column: BrowserColumn) -> Option<TextField> {
    Some(match column {
        BrowserColumn::Title => TextField::Title,
        BrowserColumn::Artist => TextField::Artist,
        BrowserColumn::Album => TextField::Album,
        BrowserColumn::Genre => TextField::Genre,
        BrowserColumn::Label => TextField::Label,
        BrowserColumn::Comment => TextField::Comment,
        BrowserColumn::Comment2 => TextField::Comment2,
        BrowserColumn::Remixer => TextField::Remixer,
        BrowserColumn::Producer => TextField::Producer,
        BrowserColumn::Mix => TextField::Mix,
        BrowserColumn::CatalogNo => TextField::CatalogNo,
        BrowserColumn::ReleaseDate => TextField::ReleaseDate,
        _ => return None,
    })
}

pub(super) fn editable(column: BrowserColumn, track: &TrackRow) -> bool {
    match column {
        BrowserColumn::Key => !track.locked,
        BrowserColumn::TrackNo | BrowserColumn::Plays => true,
        c => text_field(c).is_some(),
    }
}

pub(super) fn commit(column: BrowserColumn, text: &str, track: &TrackRow) -> Option<Edit> {
    if !editable(column, track) {
        return None;
    }
    if let Some(field) = text_field(column) {
        return (field != TextField::Title || !text.is_empty())
            .then(|| Edit::Text(field, text.to_owned()));
    }
    match column {
        BrowserColumn::TrackNo => text.parse().ok().map(|n| Edit::TrackNumber(Some(n))),
        BrowserColumn::Plays => text.parse().ok().map(Edit::PlayCount),
        BrowserColumn::Key => MusicalKey::parse(text).map(|k| Edit::Key(Some(k.value()))),
        _ => None,
    }
}

pub(super) fn edit_box(ui: &mut Ui, edit: &mut InlineEdit) -> Option<bool> {
    let output = egui::TextEdit::singleline(&mut edit.text)
        .desired_width(f32::INFINITY)
        .show(ui);
    let response = output.response;
    if !edit.focused {
        edit.focused = true;
        response.request_focus();
        let mut state = output.state;
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(edit.text.chars().count()),
            )));
        state.store(ui.ctx(), response.id);
        return None;
    }
    response
        .lost_focus()
        .then(|| !ui.input(|i| i.key_pressed(egui::Key::Escape)))
}

impl Browser {
    pub(super) fn start_edit(&mut self, id: i64, column: BrowserColumn) {
        if !self.inplace_edit
            || self
                .inline
                .as_ref()
                .is_some_and(|e| e.id == id && e.column == column)
        {
            return;
        }
        let Some(track) = self.row(id) else { return };
        if !editable(column, track) {
            return;
        }
        let text = column.text(0, track, self.keys);
        self.inline = Some(InlineEdit {
            id,
            column,
            text,
            focused: false,
        });
    }

    pub(super) fn edit_selected(&mut self) -> bool {
        let Some(id) = self.selected().filter(|_| self.selection.len() == 1) else {
            return false;
        };
        let Some(track) = self.row(id) else {
            return false;
        };
        let columns = self.columns.visible();
        let column = columns
            .iter()
            .copied()
            .find(|&c| c == BrowserColumn::Title)
            .or_else(|| columns.iter().copied().find(|&c| editable(c, track)));
        let Some(column) = column else { return false };
        self.start_edit(id, column);
        self.inline.is_some()
    }

    pub(super) fn commit_edit(&mut self, edit: &InlineEdit) {
        let Some(track) = self.row(edit.id) else {
            return;
        };
        if edit.text == edit.column.text(0, track, self.keys) {
            return;
        }
        if edit.column == BrowserColumn::Title && edit.text.is_empty() {
            crate::toast::warning("A track cannot be left without a title.");
            return;
        }
        if let Some(change) = commit(edit.column, &edit.text, track) {
            self.write_changes(&[edit.id], &[change]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> TrackRow {
        TrackRow {
            title: "Song".into(),
            ..TrackRow::default()
        }
    }

    #[test]
    fn only_attribute_columns_are_editable() {
        let t = track();
        for c in [
            BrowserColumn::Title,
            BrowserColumn::Artist,
            BrowserColumn::Album,
            BrowserColumn::ReleaseDate,
            BrowserColumn::TrackNo,
            BrowserColumn::Plays,
            BrowserColumn::Key,
        ] {
            assert!(editable(c, &t), "{c:?}");
        }
        for c in [
            BrowserColumn::Number,
            BrowserColumn::Time,
            BrowserColumn::Rating,
            BrowserColumn::Location,
            BrowserColumn::FileName,
            BrowserColumn::Bitrate,
            BrowserColumn::Analyzed,
            BrowserColumn::Cover,
            BrowserColumn::Color,
        ] {
            assert!(!editable(c, &t), "{c:?}");
        }
        let locked = TrackRow {
            locked: true,
            ..track()
        };
        assert!(!editable(BrowserColumn::Key, &locked));
        assert_eq!(commit(BrowserColumn::Key, "8A", &locked), None);
    }

    #[test]
    fn commits_check_what_was_typed() {
        let t = track();
        assert_eq!(commit(BrowserColumn::Title, "", &t), None, "no empty title");
        assert_eq!(
            commit(BrowserColumn::Artist, "", &t),
            Some(Edit::Text(TextField::Artist, String::new()))
        );
        assert_eq!(
            commit(BrowserColumn::TrackNo, "7", &t),
            Some(Edit::TrackNumber(Some(7)))
        );
        assert_eq!(commit(BrowserColumn::TrackNo, "-1", &t), None);
        assert_eq!(commit(BrowserColumn::TrackNo, "x", &t), None);
        assert_eq!(
            commit(BrowserColumn::Plays, "3", &t),
            Some(Edit::PlayCount(3))
        );
        assert_eq!(commit(BrowserColumn::Plays, "-3", &t), None);
        assert_eq!(
            commit(BrowserColumn::Key, "8A", &t),
            Some(Edit::Key(MusicalKey::parse("8A").map(MusicalKey::value)))
        );
        assert_eq!(commit(BrowserColumn::Key, "nope", &t), None);
        assert_eq!(commit(BrowserColumn::Time, "1:00", &t), None);
    }
}
