use backspn_formats::nml::{KeyNotation, MusicalKey};
use backspn_library::TrackRow;
use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Highlight {
    Off,
    Matching,
    #[default]
    Adjacent,
    All,
}

impl Highlight {
    pub const ALL: [Self; 4] = [Self::Off, Self::Matching, Self::Adjacent, Self::All];

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "Do not color any keys",
            Self::Matching => "Only color matching keys",
            Self::Adjacent => "Also color adjacent keys",
            Self::All => "Color all keys",
        }
    }
}

pub fn reference_deck(master: Option<usize>, focus: usize, use_focus: bool) -> Option<usize> {
    if use_focus { Some(focus) } else { master }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct KeyView {
    pub notation: KeyNotation,
    pub highlight: Highlight,
    pub reference: Option<MusicalKey>,
}

impl KeyView {
    pub fn text(self, track: &TrackRow) -> String {
        track
            .musical_key
            .and_then(MusicalKey::from_value)
            .map_or_else(|| track.key_text.clone(), |k| k.name(self.notation))
    }

    pub fn colour(self, track: &TrackRow) -> Option<Color32> {
        let key = track.musical_key.and_then(MusicalKey::from_value)?;
        let shown = match (self.highlight, self.reference) {
            (Highlight::All, _) => true,
            (Highlight::Matching, Some(r)) => r.matches(key),
            (Highlight::Adjacent, Some(r)) => r.adjacent(key),
            _ => false,
        };
        shown.then(|| key_colour(key))
    }
}

pub fn key_colour(key: MusicalKey) -> Color32 {
    let position = f32::from(
        key.open_key()
            .trim_end_matches(['d', 'm'])
            .parse::<u8>()
            .unwrap_or(1)
            - 1,
    );
    let hue = position / 12.0;
    let value = if key.is_minor() { 0.85 } else { 1.0 };
    let rgb = eframe::egui::ecolor::Hsva::new(hue, 0.65, value, 1.0).to_srgb();
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_deck_is_the_master_or_the_focused_deck() {
        assert_eq!(reference_deck(Some(1), 2, false), Some(1));
        assert_eq!(reference_deck(None, 2, false), None);
        assert_eq!(reference_deck(None, 2, true), Some(2));
    }

    fn track(key: u8) -> TrackRow {
        TrackRow {
            musical_key: Some(key),
            key_text: MusicalKey::from_value(key).unwrap().open_key(),
            ..TrackRow::default()
        }
    }

    #[test]
    fn notation_and_highlighting() {
        let am = MusicalKey::from_value(21);
        let mut view = KeyView {
            notation: KeyNotation::Flats,
            highlight: Highlight::Matching,
            reference: am,
        };
        assert_eq!(view.text(&track(13)), "Dbm");
        assert!(view.colour(&track(0)).is_some(), "C is Am's relative");
        assert!(
            view.colour(&track(22)).is_none(),
            "a semitone up only when adjacent"
        );
        view.highlight = Highlight::Adjacent;
        assert!(view.colour(&track(22)).is_some());
        view.reference = None;
        assert!(
            view.colour(&track(21)).is_none(),
            "no reference, nothing coloured"
        );
        view.highlight = Highlight::All;
        assert!(view.colour(&track(21)).is_some());
        let unknown = TrackRow {
            key_text: "?".into(),
            ..TrackRow::default()
        };
        assert_eq!(view.text(&unknown), "?");
        assert_ne!(
            key_colour(am.unwrap()),
            key_colour(MusicalKey::from_value(9).unwrap())
        );
    }
}
