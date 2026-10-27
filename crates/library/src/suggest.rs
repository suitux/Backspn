use std::collections::HashSet;
use std::hash::BuildHasher;

use serde::{Deserialize, Serialize};

use crate::{TrackRow, energy};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum KeyRule {
    #[default]
    Compatible,
    Same,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EnergyRule {
    #[default]
    Similar,
    Up,
    Down,
    Any,
}

impl EnergyRule {
    /// The change in energy this rule prefers.
    fn target(self) -> f64 {
        match self {
            Self::Similar | Self::Any => 0.0,
            Self::Up => 1.0,
            Self::Down => -1.0,
        }
    }

    fn allows(self, delta: f64, range: f64) -> bool {
        match self {
            Self::Similar => delta.abs() <= range,
            Self::Up => (0.0..=range).contains(&delta),
            Self::Down => (-range..=0.0).contains(&delta),
            Self::Any => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Criteria {
    pub bpm_tolerance: f64,
    pub key: KeyRule,
    pub energy: EnergyRule,
    pub energy_range: f64,
    pub same_genre: bool,
    pub skip_played: bool,
    pub count: usize,
}

impl Default for Criteria {
    fn default() -> Self {
        Self {
            bpm_tolerance: 0.06,
            key: KeyRule::Compatible,
            energy: EnergyRule::Similar,
            energy_range: 2.0,
            same_genre: false,
            skip_played: true,
            count: 50,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub track_id: Option<i64>,
    pub bpm: Option<f64>,
    pub key: Option<u8>,
    pub energy: Option<f64>,
    pub genre: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Suggestion {
    pub track_id: i64,
    pub bpm_offset: Option<f64>,
    pub key_distance: Option<u8>,
    pub energy: Option<f64>,
    pub energy_delta: Option<f64>,
    pub score: f64,
}

fn wheel(key: u8) -> (u8, bool) {
    let minor = key >= 12;
    let tonic = key % 12;
    let major_tonic = if minor { (tonic + 3) % 12 } else { tonic };
    ((major_tonic * 7) % 12, minor)
}

pub fn key_distance(a: u8, b: u8) -> Option<u8> {
    let ((pa, ma), (pb, mb)) = (wheel(a), wheel(b));
    let steps = pa.abs_diff(pb).min(12 - pa.abs_diff(pb));
    match (steps, ma == mb) {
        (0, _) => Some(0),
        (1, true) => Some(1),
        _ => None,
    }
}

pub fn bpm_offset(reference: f64, candidate: f64) -> f64 {
    [1.0, 2.0, 0.5]
        .iter()
        .map(|factor| (candidate * factor - reference).abs() / reference)
        .fold(f64::INFINITY, f64::min)
}

pub fn suggest<S: BuildHasher>(
    reference: &Reference,
    candidates: &[TrackRow],
    played: &HashSet<i64, S>,
    criteria: &Criteria,
) -> Vec<Suggestion> {
    let genre = reference.genre.trim().to_lowercase();
    let mut out: Vec<Suggestion> = candidates
        .iter()
        .filter(|t| Some(t.id) != reference.track_id)
        .filter(|t| !(criteria.skip_played && played.contains(&t.id)))
        .filter_map(|t| {
            let bpm_offset = reference
                .bpm
                .zip(t.bpm)
                .filter(|(r, c)| *r > 0.0 && *c > 0.0)
                .map(|(r, c)| bpm_offset(r, c));
            if bpm_offset.is_some_and(|o| o > criteria.bpm_tolerance) {
                return None;
            }
            let key_distance = reference
                .key
                .zip(t.musical_key)
                .map(|(r, c)| key_distance(r, c));
            let key_ok = match criteria.key {
                KeyRule::Any => true,
                KeyRule::Same => key_distance == Some(Some(0)) && reference.key == t.musical_key,
                KeyRule::Compatible => key_distance != Some(None),
            };
            if !key_ok {
                return None;
            }
            let energy = energy::estimate(t);
            let energy_delta = reference.energy.zip(energy).map(|(r, c)| c - r);
            if energy_delta.is_some_and(|d| !criteria.energy.allows(d, criteria.energy_range)) {
                return None;
            }
            let same_genre = !genre.is_empty() && t.genre.trim().to_lowercase() == genre;
            if criteria.same_genre && !same_genre {
                return None;
            }
            let tempo = bpm_offset.map_or(1.0, |o| o / criteria.bpm_tolerance.max(1e-6));
            let key = match key_distance {
                Some(Some(d)) => f64::from(d) * 0.5,
                Some(None) => 1.5,
                None => 1.0,
            };
            let energy_score = match (criteria.energy, energy_delta) {
                (EnergyRule::Any, _) => 0.0,
                (rule, Some(d)) => (d - rule.target()).abs() / criteria.energy_range.max(1e-6),
                (_, None) if reference.energy.is_some() => 0.5,
                (_, None) => 0.0,
            };
            let genre_bonus = if same_genre { -0.5 } else { 0.0 };
            Some(Suggestion {
                track_id: t.id,
                bpm_offset,
                key_distance: key_distance.flatten(),
                energy,
                energy_delta,
                score: tempo + key + energy_score + genre_bonus,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.score
            .total_cmp(&b.score)
            .then(a.track_id.cmp(&b.track_id))
    });
    out.truncate(criteria.count);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: u8 = 0;
    const A_MINOR: u8 = 21;
    const G: u8 = 7;
    const E_MINOR: u8 = 16;
    const D: u8 = 2;

    fn track(id: i64, bpm: f64, key: u8, genre: &str) -> TrackRow {
        TrackRow {
            id,
            bpm: Some(bpm),
            musical_key: Some(key),
            genre: genre.into(),
            ..TrackRow::default()
        }
    }

    fn reference() -> Reference {
        Reference {
            track_id: Some(1),
            bpm: Some(128.0),
            key: Some(C),
            energy: None,
            genre: "Techno".into(),
        }
    }

    fn with_energy(id: i64, energy: u8) -> TrackRow {
        TrackRow {
            id,
            comment: format!("Energy {energy}"),
            ..TrackRow::default()
        }
    }

    #[test]
    fn energy_rules_filter_and_rank() {
        let tracks = [
            with_energy(2, 6),
            with_energy(3, 5),
            with_energy(4, 7),
            with_energy(5, 9),
            with_energy(6, 2),
            TrackRow {
                id: 7,
                ..TrackRow::default()
            },
        ];
        let reference = Reference {
            energy: Some(5.0),
            ..reference()
        };
        let ids = |energy: EnergyRule, energy_range: f64| -> Vec<i64> {
            let criteria = Criteria {
                energy,
                energy_range,
                ..Criteria::default()
            };
            suggest(&reference, &tracks, &HashSet::new(), &criteria)
                .iter()
                .map(|s| s.track_id)
                .collect()
        };
        assert_eq!(ids(EnergyRule::Similar, 2.0), [3, 2, 7, 4]);
        assert_eq!(ids(EnergyRule::Up, 2.0), [2, 3, 4, 7]);
        assert_eq!(ids(EnergyRule::Down, 3.0), [3, 7, 6]);
        assert_eq!(ids(EnergyRule::Any, 1.0), [2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn suggestions_report_energy_change() {
        let reference = Reference {
            energy: Some(5.0),
            ..reference()
        };
        let out = suggest(
            &reference,
            &[with_energy(2, 6)],
            &HashSet::new(),
            &Criteria::default(),
        );
        assert_eq!(out[0].energy, Some(6.0));
        assert_eq!(out[0].energy_delta, Some(1.0));
    }

    #[test]
    fn keys_on_the_wheel() {
        assert_eq!(key_distance(C, C), Some(0));
        assert_eq!(key_distance(C, A_MINOR), Some(0), "relative minor");
        assert_eq!(key_distance(C, G), Some(1), "a fifth up");
        assert_eq!(key_distance(A_MINOR, E_MINOR), Some(1));
        assert_eq!(key_distance(C, D), None, "two steps");
        assert_eq!(
            key_distance(C, E_MINOR),
            None,
            "neighbour of the other mode"
        );
    }

    #[test]
    fn tempo_allows_half_and_double_time() {
        assert!((bpm_offset(128.0, 64.0)).abs() < 1e-9);
        assert!((bpm_offset(128.0, 132.0) - 4.0 / 128.0).abs() < 1e-9);
    }

    #[test]
    fn suggestions_filter_rank_and_skip() {
        let tracks = [
            track(1, 128.0, C, "Techno"),
            track(2, 129.0, G, "Techno"),
            track(3, 128.0, A_MINOR, "House"),
            track(4, 150.0, C, "Techno"),
            track(5, 127.0, D, "Techno"),
            track(6, 128.5, C, "Techno"),
        ];
        let played = HashSet::from([6]);
        let ids = |c: &Criteria| -> Vec<i64> {
            suggest(&reference(), &tracks, &played, c)
                .iter()
                .map(|s| s.track_id)
                .collect()
        };
        let criteria = Criteria::default();
        assert_eq!(ids(&criteria), [3, 2]);
        let genre = Criteria {
            same_genre: true,
            ..criteria
        };
        assert_eq!(ids(&genre), [2]);
        let any_key = Criteria {
            key: KeyRule::Any,
            skip_played: false,
            ..criteria
        };
        assert_eq!(ids(&any_key), [6, 3, 2, 5]);
        let same_key = Criteria {
            key: KeyRule::Same,
            skip_played: false,
            ..criteria
        };
        assert_eq!(ids(&same_key), [6]);
    }

    #[test]
    fn unknown_tempo_or_key_still_suggests_after_known_matches() {
        let mut unknown = track(7, 0.0, C, "Techno");
        unknown.bpm = None;
        unknown.musical_key = None;
        let tracks = [unknown, track(8, 128.0, C, "Techno")];
        let out = suggest(&reference(), &tracks, &HashSet::new(), &Criteria::default());
        assert_eq!(out.iter().map(|s| s.track_id).collect::<Vec<_>>(), [8, 7]);
    }
}
