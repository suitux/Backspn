use backspn_engine::{DECKS, EffectKind, FX_SLOTS, FX_UNITS, FxMode, FxRouting};
use serde::{Deserialize, Serialize};

use crate::deck_view::DeckSize;
use crate::prefs::Settings;
use crate::status::DeckType;
use crate::waveform::{WaveMode, WavePalette};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ScreenLayout {
    pub deck_count: usize,
    pub deck_sizes: [DeckSize; DECKS],
    pub show_fx: bool,
    pub fx_units: usize,
    pub global_left_loop_recorder: bool,
    pub show_crossfader: bool,
    pub show_browser: bool,
    pub show_favorites: bool,
    pub show_preview_player: bool,
    pub fx_modes: [FxMode; FX_UNITS],
    pub fx_routing: [FxRouting; FX_UNITS],
    pub fx_kits: [String; FX_UNITS],
    pub wave_mode: WaveMode,
    pub wave_palette: WavePalette,
    pub wave_custom: [[u8; 3]; 3],
}

impl Default for ScreenLayout {
    fn default() -> Self {
        Self::of(&Settings::default())
    }
}

impl ScreenLayout {
    pub fn of(s: &Settings) -> Self {
        Self {
            deck_count: s.deck_count,
            deck_sizes: s.deck_sizes,
            show_fx: s.show_fx,
            fx_units: s.fx_units,
            global_left_loop_recorder: s.global_left_loop_recorder,
            show_crossfader: s.show_crossfader,
            show_browser: s.show_browser,
            show_favorites: s.show_favorites,
            show_preview_player: s.show_preview_player,
            fx_modes: s.fx_panel_mode,
            fx_routing: s.fx_routing,
            fx_kits: s.fx_pattern_kits.clone(),
            wave_mode: s.wave_mode,
            wave_palette: s.wave_palette,
            wave_custom: s.wave_custom,
        }
    }

    pub fn apply_to(&self, s: &mut Settings) {
        s.deck_count = self.deck_count;
        s.deck_sizes = self.deck_sizes;
        s.show_fx = self.show_fx;
        s.fx_units = self.fx_units;
        s.global_left_loop_recorder = self.global_left_loop_recorder;
        s.show_crossfader = self.show_crossfader;
        s.show_browser = self.show_browser;
        s.show_favorites = self.show_favorites;
        s.show_preview_player = self.show_preview_player;
        s.fx_panel_mode = self.fx_modes;
        s.fx_routing = self.fx_routing;
        s.fx_pattern_kits.clone_from(&self.fx_kits);
        s.wave_mode = self.wave_mode;
        s.wave_palette = self.wave_palette;
        s.wave_custom = self.wave_custom;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeckSetup {
    pub deck_types: [DeckType; DECKS],
    pub sample_sets: [Option<i64>; DECKS],
    pub effects: [[EffectKind; FX_SLOTS]; FX_UNITS],
}

impl Default for DeckSetup {
    fn default() -> Self {
        Self {
            deck_types: [DeckType::Track; DECKS],
            sample_sets: [None; DECKS],
            effects: [[EffectKind::None; FX_SLOTS]; FX_UNITS],
        }
    }
}

pub fn set_to_load(deck_type: DeckType, wanted: Option<i64>, loaded: Option<i64>) -> Option<i64> {
    wanted.filter(|&id| deck_type == DeckType::Sample && loaded != Some(id))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedLayout {
    pub name: String,
    pub layout: ScreenLayout,
    #[serde(default)]
    pub setup: DeckSetup,
}

pub fn deck_summary(saved: &SavedLayout, label: impl Fn(&str) -> &str) -> String {
    (0..saved.layout.deck_count.min(DECKS))
        .map(|d| {
            let letter = backspn_control::deck_letter(d);
            format!("{letter} {}", label(saved.setup.deck_types[d].label()))
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Empty,
    Taken,
}

impl NameError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Empty => "Type a name.",
            Self::Taken => "A layout with this name already exists.",
        }
    }
}

pub fn check_name(
    list: &[SavedLayout],
    name: &str,
    except: Option<usize>,
) -> Result<String, NameError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    let lower = name.to_lowercase();
    if list
        .iter()
        .enumerate()
        .any(|(i, l)| Some(i) != except && l.name.to_lowercase() == lower)
    {
        return Err(NameError::Taken);
    }
    Ok(name.to_owned())
}

pub fn add(
    list: &mut Vec<SavedLayout>,
    name: &str,
    layout: ScreenLayout,
    setup: DeckSetup,
) -> Result<usize, NameError> {
    let name = check_name(list, name, None)?;
    list.push(SavedLayout {
        name,
        layout,
        setup,
    });
    Ok(list.len() - 1)
}

pub fn rename(list: &mut [SavedLayout], index: usize, name: &str) -> Result<(), NameError> {
    let name = check_name(list, name, Some(index))?;
    if let Some(l) = list.get_mut(index) {
        l.name = name;
    }
    Ok(())
}

pub fn remove(list: &mut Vec<SavedLayout>, name: &str) {
    list.retain(|l| l.name != name);
}

pub fn move_by(list: &mut [SavedLayout], index: usize, delta: isize) -> usize {
    let Some(to) = index
        .checked_add_signed(delta)
        .filter(|&to| to < list.len() && index < list.len())
    else {
        return index;
    };
    list.swap(index, to);
    to
}

pub fn active(list: &[SavedLayout], screen: &ScreenLayout, setup: &DeckSetup) -> Option<usize> {
    list.iter()
        .position(|l| l.layout == *screen && l.setup == *setup)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn four_decks() -> ScreenLayout {
        ScreenLayout {
            deck_count: 4,
            show_browser: false,
            ..ScreenLayout::default()
        }
    }

    #[test]
    fn a_layout_round_trips_through_the_settings() {
        let mut s = Settings::default();
        let layout = ScreenLayout {
            deck_sizes: [DeckSize::Micro; DECKS],
            show_crossfader: false,
            ..four_decks()
        };
        layout.apply_to(&mut s);
        assert_eq!(ScreenLayout::of(&s), layout);
        assert!((s.headroom_db - Settings::default().headroom_db).abs() < f32::EPSILON);
    }

    #[test]
    fn names_must_be_new_and_not_blank() {
        let mut list = Vec::new();
        assert_eq!(
            add(&mut list, "  Club  ", four_decks(), DeckSetup::default()),
            Ok(0)
        );
        assert_eq!(list[0].name, "Club");
        assert_eq!(
            add(&mut list, "club", four_decks(), DeckSetup::default()),
            Err(NameError::Taken)
        );
        assert_eq!(
            add(&mut list, "   ", four_decks(), DeckSetup::default()),
            Err(NameError::Empty)
        );
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn renaming_may_keep_the_same_name() {
        let mut list = Vec::new();
        add(&mut list, "Club", four_decks(), DeckSetup::default()).unwrap();
        add(
            &mut list,
            "Home",
            ScreenLayout::default(),
            DeckSetup::default(),
        )
        .unwrap();
        assert_eq!(rename(&mut list, 0, "CLUB"), Ok(()));
        assert_eq!(list[0].name, "CLUB");
        assert_eq!(rename(&mut list, 0, "home"), Err(NameError::Taken));
    }

    #[test]
    fn deleting_goes_by_name() {
        let mut list = Vec::new();
        for name in ["A", "B"] {
            add(
                &mut list,
                name,
                ScreenLayout::default(),
                DeckSetup::default(),
            )
            .unwrap();
        }
        move_by(&mut list, 0, 1);
        remove(&mut list, "A");
        remove(&mut list, "missing");
        let names: Vec<&str> = list.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["B"]);
    }

    #[test]
    fn layouts_move_within_the_list() {
        let mut list = Vec::new();
        for name in ["A", "B", "C"] {
            add(
                &mut list,
                name,
                ScreenLayout::default(),
                DeckSetup::default(),
            )
            .unwrap();
        }
        assert_eq!(move_by(&mut list, 0, -1), 0);
        assert_eq!(move_by(&mut list, 0, 1), 1);
        assert_eq!(move_by(&mut list, 2, 1), 2);
        let names: Vec<&str> = list.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["B", "A", "C"]);
    }

    #[test]
    fn the_active_layout_is_the_one_on_screen() {
        let mut list = Vec::new();
        add(
            &mut list,
            "Two",
            ScreenLayout::default(),
            DeckSetup::default(),
        )
        .unwrap();
        add(&mut list, "Four", four_decks(), DeckSetup::default()).unwrap();
        let setup = DeckSetup::default();
        assert_eq!(active(&list, &four_decks(), &setup), Some(1));
        let other = ScreenLayout {
            show_fx: false,
            ..four_decks()
        };
        assert_eq!(active(&list, &other, &setup), None);
        let mut sample_d = setup.clone();
        sample_d.deck_types[3] = DeckType::Sample;
        assert_eq!(active(&list, &four_decks(), &sample_d), None);
    }

    #[test]
    fn the_summary_names_the_decks_shown() {
        let mut setup = DeckSetup::default();
        setup.deck_types[1] = DeckType::Sample;
        setup.deck_types[3] = DeckType::LiveInput;
        let two = SavedLayout {
            name: "Two".into(),
            layout: ScreenLayout::default(),
            setup: setup.clone(),
        };
        assert_eq!(deck_summary(&two, |t| t), "A Track · B Sample");
        let four = SavedLayout {
            layout: four_decks(),
            ..two
        };
        assert_eq!(
            deck_summary(&four, |t| t),
            "A Track · B Sample · C Track · D Live Input"
        );
    }

    #[test]
    fn a_sample_deck_loads_the_layouts_set_once() {
        assert_eq!(set_to_load(DeckType::Sample, Some(7), None), Some(7));
        assert_eq!(set_to_load(DeckType::Sample, Some(7), Some(3)), Some(7));
        assert_eq!(set_to_load(DeckType::Sample, Some(7), Some(7)), None);
        assert_eq!(set_to_load(DeckType::Track, Some(7), None), None);
        assert_eq!(set_to_load(DeckType::Sample, None, Some(3)), None);
    }
}
