use eframe::egui::{self, ComboBox, Key, Response, RichText, Ui};

use crate::i18n::tr;

pub enum Entry<T> {
    Item {
        value: T,
        label: String,
        selected: bool,
        hint: Option<String>,
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
        }
    }

    #[must_use]
    pub fn with_hint(self, text: impl Into<String>) -> Self {
        match self {
            Self::Item {
                value,
                label,
                selected,
                ..
            } => Self::Item {
                value,
                label,
                selected,
                hint: Some(text.into()).filter(|t: &String| !t.is_empty()),
            },
            other => other,
        }
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
            Entry::Item { label, .. } => {
                if matches(query, label) {
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
fn list_height(lines: usize, row: f32) -> f32 {
    (lines as f32 * row).min(MAX_HEIGHT)
}

pub fn show<T: Clone>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    selected_text: impl Into<egui::WidgetText>,
    width: f32,
    entries: &[Entry<T>],
) -> (Response, Option<T>) {
    let id = ui.make_persistent_id(id_salt);
    let query_id = id.with("query");
    let mut chosen = None;
    let inner = ComboBox::from_id_salt(id)
        .width(width)
        .height(MAX_HEIGHT)
        .selected_text(selected_text)
        .show_ui(ui, |ui| {
            ui.set_min_width(width);
            let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
            ui.set_min_height(list_height(entries.len() + 2, row));
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
        assert!((list_height(5, 20.0) - 100.0).abs() < f32::EPSILON);
        assert!((list_height(100, 20.0) - MAX_HEIGHT).abs() < f32::EPSILON);
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
