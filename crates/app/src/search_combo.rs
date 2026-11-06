use eframe::egui::{self, AsIdSalt, Key, Response, RichText, Ui};

use crate::i18n::tr;
use crate::select::select;

pub enum Entry<T> {
    Item {
        value: T,
        label: String,
        selected: bool,
        hint: Option<String>,
        /// Shown only while the search field is empty.
        idle_only: bool,
    },
    Heading(String),
    Separator,
}

impl<T> Entry<T> {
    pub fn item(value: T, label: impl Into<String>, selected: bool) -> Self {
        Self::Item {
            value,
            label: label.into(),
            selected,
            hint: None,
            idle_only: false,
        }
    }

    #[must_use]
    pub fn with_hint(self, text: impl Into<String>) -> Self {
        match self {
            Self::Item {
                value,
                label,
                selected,
                idle_only,
                ..
            } => Self::Item {
                value,
                label,
                selected,
                hint: Some(text.into()).filter(|t: &String| !t.is_empty()),
                idle_only,
            },
            other => other,
        }
    }

    /// Leaves the item out of search results (e.g. an "empty" choice).
    #[must_use]
    pub fn hidden_while_searching(mut self) -> Self {
        if let Self::Item { idle_only, .. } = &mut self {
            *idle_only = true;
        }
        self
    }
}

pub fn matches(query: &str, label: &str) -> bool {
    let label = label.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| label.contains(word))
}

pub fn visible<T>(entries: &[Entry<T>], query: &str) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..entries.len()).collect();
    }
    let mut shown = Vec::new();
    let mut heading = None;
    for (i, e) in entries.iter().enumerate() {
        match e {
            Entry::Heading(_) => heading = Some(i),
            Entry::Separator => {}
            Entry::Item {
                label, idle_only, ..
            } => {
                if !idle_only && matches(query, label) {
                    if let Some(h) = heading.take() {
                        shown.push(h);
                    }
                    shown.push(i);
                }
            }
        }
    }
    shown
}

const MAX_HEIGHT: f32 = 400.0;

#[allow(clippy::cast_precision_loss)]
fn list_height(lines: usize, row: f32, max: f32) -> f32 {
    (lines as f32 * row).min(max)
}

/// A select whose list starts with a search field.
#[must_use]
pub struct SearchSelect<'e, T> {
    id: egui::IdSalt,
    text: String,
    width: f32,
    entries: &'e [Entry<T>],
    compact: bool,
    truncate: bool,
    max_height: f32,
}

pub fn search_select<T: Clone>(
    id_salt: impl AsIdSalt,
    selected_text: impl Into<String>,
    width: f32,
    entries: &[Entry<T>],
) -> SearchSelect<'_, T> {
    SearchSelect {
        id: egui::IdSalt::new(id_salt),
        text: selected_text.into(),
        width,
        entries,
        compact: false,
        truncate: false,
        max_height: MAX_HEIGHT,
    }
}

/// Shows a search select with the default look and returns the picked value.
pub fn show<T: Clone>(
    ui: &mut Ui,
    id_salt: impl AsIdSalt,
    selected_text: impl Into<String>,
    width: f32,
    entries: &[Entry<T>],
) -> (Response, Option<T>) {
    search_select(id_salt, selected_text, width, entries).show(ui)
}

impl<T: Clone> SearchSelect<'_, T> {
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub fn truncate(mut self) -> Self {
        self.truncate = true;
        self
    }

    /// Maximum height of the open list.
    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = height;
        self
    }

    pub fn show(self, ui: &mut Ui) -> (Response, Option<T>) {
        let Self {
            id,
            text,
            width,
            entries,
            compact,
            truncate,
            max_height,
        } = self;
        let id = ui.make_persistent_id(id);
        let mut combo = select(id, text)
            .width(width)
            .height(f32::INFINITY)
            .close_on_click_outside();
        if compact {
            combo = combo.compact();
        }
        if truncate {
            combo = combo.truncate();
        }
        list(ui, id, combo, width, max_height, entries)
    }
}

fn list<T: Clone>(
    ui: &mut Ui,
    id: egui::Id,
    combo: crate::select::Select<'_>,
    width: f32,
    max_height: f32,
    entries: &[Entry<T>],
) -> (Response, Option<T>) {
    let query_id = id.with("query");
    let mut chosen = None;
    let inner = combo.show(ui, |ui| {
        ui.set_min_width(width);
        let opened = ui.data(|d| d.get_temp::<String>(query_id).is_none());
        let mut query = ui
            .data(|d| d.get_temp::<String>(query_id))
            .unwrap_or_default();
        let field = ui.add(
            egui::TextEdit::singleline(&mut query)
                .hint_text(tr("Search"))
                .desired_width(f32::INFINITY),
        );
        if opened {
            field.request_focus();
        }
        ui.separator();
        let shown = visible(entries, &query);
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        if enter {
            chosen = shown.iter().find_map(|&i| match &entries[i] {
                Entry::Item { value, .. } => Some(value.clone()),
                _ => None,
            });
        }
        let mut any = false;
        let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .show(ui, |ui| {
                ui.set_min_height(list_height(entries.len(), row, max_height));
                for &i in &shown {
                    match &entries[i] {
                        Entry::Heading(text) => {
                            ui.label(RichText::new(text).weak());
                        }
                        Entry::Separator => {
                            ui.separator();
                        }
                        Entry::Item {
                            value,
                            label,
                            selected,
                            hint,
                            ..
                        } => {
                            any = true;
                            let mut r = ui.selectable_label(*selected, label);
                            if let Some(hint) = hint {
                                r = r.on_hover_text(hint);
                            }
                            if r.clicked() {
                                chosen = Some(value.clone());
                            }
                        }
                    }
                }
                if !any {
                    ui.label(RichText::new(tr("No matches")).weak());
                }
            });
        ui.data_mut(|d| d.insert_temp(query_id, query));
        if chosen.is_some() {
            ui.close();
        }
    });
    if inner.inner.is_none() {
        ui.data_mut(|d| d.remove::<String>(query_id));
    }
    (inner.response, chosen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Vec<Entry<u8>> {
        vec![
            Entry::item(0, "Kontrol S4 MK3", true),
            Entry::Separator,
            Entry::Heading("Add".into()),
            Entry::item(1, "AlphaTheta DDJ-FLX4", false),
            Entry::item(2, "Pioneer DJ DDJ-FLX6", false),
            Entry::Heading("Other".into()),
            Entry::item(3, "Generic MIDI", false),
        ]
    }

    #[test]
    fn every_word_must_appear_in_any_case() {
        assert!(matches("flx4", "AlphaTheta DDJ-FLX4"));
        assert!(matches("pioneer flx", "Pioneer DJ DDJ-FLX6"));
        assert!(!matches("pioneer flx4", "Pioneer DJ DDJ-FLX6"));
        assert!(matches("  ", "anything"));
    }

    #[test]
    fn searching_keeps_matches_under_their_headings() {
        let l = list();
        assert_eq!(visible(&l, ""), (0..7).collect::<Vec<_>>());
        assert_eq!(visible(&l, "ddj"), vec![2, 3, 4]);
        assert_eq!(visible(&l, "generic"), vec![5, 6]);
        assert_eq!(visible(&l, "s4"), vec![0], "no separator while searching");
        assert_eq!(visible(&l, "nothing"), Vec::<usize>::new());
    }

    #[test]
    fn the_list_keeps_its_full_height_up_to_the_cap() {
        assert!((list_height(5, 20.0, MAX_HEIGHT) - 100.0).abs() < f32::EPSILON);
        assert!((list_height(100, 20.0, MAX_HEIGHT) - MAX_HEIGHT).abs() < f32::EPSILON);
        assert!((list_height(100, 20.0, 260.0) - 260.0).abs() < f32::EPSILON);
    }

    #[test]
    fn idle_only_items_leave_while_searching() {
        let l = vec![
            Entry::item(0, "None", true).hidden_while_searching(),
            Entry::item(1, "Delay", false),
            Entry::item(2, "Noise", false),
        ];
        assert_eq!(visible(&l, ""), vec![0, 1, 2]);
        assert_eq!(visible(&l, "no"), vec![2]);
        let Entry::Item { idle_only, .. } = Entry::item(0, "None", true)
            .hidden_while_searching()
            .with_hint("tip")
        else {
            panic!()
        };
        assert!(idle_only, "a hint keeps the flag");
    }

    #[test]
    fn a_hint_goes_on_items_only() {
        let Entry::Item { hint, .. } = Entry::item(1, "a", false).with_hint("tip") else {
            panic!()
        };
        assert_eq!(hint.as_deref(), Some("tip"));
        let Entry::Item { hint, .. } = Entry::item(1, "a", false).with_hint("") else {
            panic!()
        };
        assert_eq!(hint, None);
        assert!(matches!(
            Entry::<u8>::Separator.with_hint("x"),
            Entry::Separator
        ));
    }
}
