use backspn_formats::nml::MusicalKey;
use backspn_library::TrackRow;
use eframe::egui::{self, ComboBox, DragValue, RichText, Ui};

use crate::columns::{stars, track_colour};
use crate::i18n::tr;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackFilter {
    pub open: bool,
    pub bpm: Option<(f64, f64)>,
    pub key: Option<u8>,
    pub ratings: Vec<u8>,
    pub colors: Vec<u8>,
}

impl TrackFilter {
    pub fn active(&self) -> bool {
        self.bpm.is_some()
            || self.key.is_some()
            || !self.ratings.is_empty()
            || !self.colors.is_empty()
    }

    pub fn matches(&self, t: &TrackRow) -> bool {
        let bpm = self
            .bpm
            .is_none_or(|(lo, hi)| t.bpm.is_some_and(|b| b >= lo && b <= hi));
        let key = self.key.is_none_or(|k| t.musical_key == Some(k));
        let rating = self.ratings.is_empty() || self.ratings.contains(&(t.rating / 51));
        let color = self.colors.is_empty() || self.colors.contains(&t.color);
        bpm && key && rating && color
    }

    pub fn ui(&mut self, ui: &mut Ui) -> bool {
        let before = self.clone();
        ui.horizontal_wrapped(|ui| {
            let mut on = self.bpm.is_some();
            if crate::widgets::check(ui, &mut on, "BPM").changed() {
                self.bpm = on.then_some((120.0, 130.0));
            }
            if let Some((lo, hi)) = &mut self.bpm {
                ui.add(DragValue::new(lo).range(1.0..=300.0).speed(0.5));
                ui.label("–");
                ui.add(DragValue::new(hi).range(1.0..=300.0).speed(0.5));
            }
            ui.separator();
            ui.label(tr("Key"));
            let shown = self
                .key
                .and_then(MusicalKey::from_value)
                .map_or_else(|| "Any".to_owned(), |k| format!("{k} ({})", k.open_key()));
            ComboBox::from_id_salt("filter-key")
                .width(90.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.key, None, tr("Any"));
                    for v in 0..24u8 {
                        let k = MusicalKey::from_value(v).expect("in range");
                        ui.selectable_value(
                            &mut self.key,
                            Some(v),
                            format!("{k} ({})", k.open_key()),
                        );
                    }
                });
            ui.separator();
            ui.label(tr("Rating"));
            for n in (0..=5u8).rev() {
                toggle(
                    ui,
                    &mut self.ratings,
                    n,
                    RichText::new(stars(n * 51)).size(10.0),
                );
            }
            ui.separator();
            ui.label(tr("Color"));
            for c in 1..=7u8 {
                let swatch = RichText::new("■").color(track_colour(c).unwrap_or_default());
                toggle(ui, &mut self.colors, c, swatch);
            }
            ui.separator();
            if ui.small_button(tr("Reset")).clicked() {
                *self = Self {
                    open: true,
                    ..Self::default()
                };
            }
        });
        *self != before
    }
}

fn toggle(ui: &mut Ui, set: &mut Vec<u8>, value: u8, label: RichText) {
    let on = set.contains(&value);
    if ui.add(egui::Button::selectable(on, label)).clicked() {
        if on {
            set.retain(|v| *v != value);
        } else {
            set.push(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(bpm: f64, key: u8, rating: u8, color: u8) -> TrackRow {
        TrackRow {
            bpm: Some(bpm),
            musical_key: Some(key),
            rating,
            color,
            ..TrackRow::default()
        }
    }

    #[test]
    fn every_part_narrows_and_empty_parts_pass() {
        let t = track(128.0, 21, 204, 3);
        let mut f = TrackFilter::default();
        assert!(!f.active() && f.matches(&t));
        f.bpm = Some((125.0, 130.0));
        assert!(f.matches(&t));
        f.bpm = Some((129.0, 135.0));
        assert!(!f.matches(&t));
        f.bpm = None;
        f.key = Some(21);
        assert!(f.matches(&t));
        f.key = Some(0);
        assert!(!f.matches(&t), "the same key only");
        f.key = None;
        f.ratings = vec![4, 5];
        assert!(f.matches(&t));
        f.ratings = vec![5];
        assert!(!f.matches(&t));
        f.ratings.clear();
        f.colors = vec![1, 3];
        assert!(f.matches(&t) && f.active());
    }
}
