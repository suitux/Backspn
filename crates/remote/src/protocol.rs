//! Requests and answers between an AI client and the app, one JSON object
//! per line over a local socket.

use std::collections::BTreeMap;

use backspn_library::TrackRow;
use backspn_library::energy;
use backspn_library::set_plan::{SessionBrief, Transition};
use serde::{Deserialize, Serialize};

use crate::transition::{PatternSpec, Recipe};

/// A track in a set as the client sends it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetTrack {
    pub id: i64,
    /// Minutes to play before mixing out; none plays the whole track.
    #[serde(default)]
    pub play_minutes: Option<f64>,
    /// Where the track starts, in percent of its length (50 is the middle);
    /// none starts at its cue.
    #[serde(default)]
    pub start_at_pct: Option<f64>,
    /// How this track comes in, as a named style.
    #[serde(default)]
    pub transition: Option<Transition>,
    /// How this track comes in, written beat by beat. Wins over `transition`.
    #[serde(default)]
    pub recipe: Option<Recipe>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MixerControl {
    Volume,
    Gain,
    EqLow,
    EqMid,
    EqHigh,
    /// -1 low-pass … 0 off … 1 high-pass.
    Filter,
}

/// How a control is moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Press {
    /// Press and release, like a button tap.
    #[default]
    Tap,
    Press,
    Release,
}

/// Track fields to change; the rest stay as they are.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackChanges {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub label: Option<String>,
    pub comment: Option<String>,
    pub comment2: Option<String>,
    pub remixer: Option<String>,
    pub producer: Option<String>,
    pub mix: Option<String>,
    pub catalog_no: Option<String>,
    pub release_date: Option<String>,
    /// Key in any notation ("8A", "Am"); empty clears it.
    pub key: Option<String>,
    /// Stars, 0–5.
    pub rating: Option<u8>,
    pub play_count: Option<u32>,
    pub track_number: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListKind {
    Playlist,
    Smartlist,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Request {
    LibraryOverview,
    SearchTracks {
        #[serde(default)]
        filters: SessionBrief,
        #[serde(default)]
        limit: Option<usize>,
    },
    PlanSet {
        #[serde(default)]
        brief: SessionBrief,
    },
    StartSet {
        tracks: Vec<SetTrack>,
        #[serde(default)]
        transition: Option<Transition>,
        #[serde(default)]
        keylock: Option<bool>,
    },
    GetSetStatus,
    ReplaceUpcoming {
        tracks: Vec<SetTrack>,
    },
    /// Fields of the brief to change; the rest stay as planned.
    AdjustSet {
        #[serde(default)]
        brief: serde_json::Value,
    },
    StopSet,
    ResumeSet,
    GetState,
    LoadTrack {
        deck: usize,
        id: i64,
    },
    Play {
        deck: usize,
        on: bool,
    },
    Sync {
        deck: usize,
        on: bool,
    },
    SetMixer {
        deck: usize,
        control: MixerControl,
        value: f32,
    },
    Crossfader {
        value: f32,
    },
    SetPattern {
        #[serde(flatten)]
        pattern: PatternSpec,
        /// Decks whose channels the pattern plays through.
        #[serde(default)]
        decks: Option<Vec<usize>>,
    },
    ListControls {
        #[serde(default)]
        filter: String,
    },
    /// Moves any control as a controller would.
    Control {
        action: serde_json::Value,
        /// For knobs and faders: the position, 0–1.
        #[serde(default)]
        value: Option<f32>,
        /// For endless encoders: steps to turn.
        #[serde(default)]
        delta: Option<f32>,
        #[serde(default)]
        press: Option<Press>,
    },
    GetSettings {
        #[serde(default)]
        fields: Vec<String>,
    },
    SetSettings {
        changes: serde_json::Value,
    },
    SearchLibrary {
        query: String,
        #[serde(default)]
        limit: Option<usize>,
    },
    GetTrack {
        id: i64,
    },
    EditTracks {
        ids: Vec<i64>,
        changes: TrackChanges,
    },
    AnalyzeTracks {
        ids: Vec<i64>,
    },
    ListLists,
    GetList {
        id: i64,
        #[serde(default)]
        limit: Option<usize>,
    },
    CreateList {
        kind: ListKind,
        name: String,
        #[serde(default)]
        parent: Option<i64>,
        #[serde(default)]
        query: Option<String>,
        #[serde(default)]
        tracks: Vec<i64>,
    },
    EditList {
        id: i64,
        #[serde(default)]
        name: Option<String>,
        /// Folder to move the list into.
        #[serde(default)]
        parent: Option<i64>,
        /// Moves the list to the top level.
        #[serde(default)]
        to_top: bool,
        #[serde(default)]
        query: Option<String>,
        #[serde(default)]
        add: Vec<i64>,
        /// Track ids to take out (every entry of each).
        #[serde(default)]
        remove: Vec<i64>,
    },
    DeleteList {
        id: i64,
    },
    ShowList {
        id: i64,
    },
    ListHistory,
    GetHistory {
        id: i64,
    },
    SetFx {
        unit: usize,
        #[serde(default)]
        effect: Option<String>,
        #[serde(default)]
        on: Option<bool>,
        #[serde(default)]
        wet: Option<f32>,
        #[serde(default)]
        decks: Option<Vec<usize>>,
    },
}

/// What travels over the socket: a request with the token from the
/// connection file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub token: String,
    pub request: Request,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl From<Result<serde_json::Value, String>> for Reply {
    fn from(result: Result<serde_json::Value, String>) -> Self {
        match result {
            Ok(value) => Self {
                ok: true,
                result: Some(value),
                error: None,
            },
            Err(error) => Self {
                ok: false,
                result: None,
                error: Some(error),
            },
        }
    }
}

/// The brief with the fields of `patch` changed.
pub fn patch_brief(
    brief: &SessionBrief,
    patch: &serde_json::Value,
) -> Result<SessionBrief, String> {
    let mut merged = serde_json::to_value(brief).map_err(|e| e.to_string())?;
    match (merged.as_object_mut(), patch) {
        (Some(fields), serde_json::Value::Object(changes)) => {
            for (key, value) in changes {
                fields.insert(key.clone(), value.clone());
            }
        }
        (_, serde_json::Value::Null) => {}
        _ => return Err("brief must be an object".to_owned()),
    }
    serde_json::from_value(merged).map_err(|e| format!("brief: {e}"))
}

/// `current` with the fields of `changes` replaced, objects merged field
/// by field. Every field changed must already exist, so a misspelt name is
/// an error instead of being ignored.
pub fn merge_known(
    current: &serde_json::Value,
    changes: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    fn merge(
        into: &mut serde_json::Value,
        changes: &serde_json::Value,
        path: &str,
    ) -> Result<(), String> {
        let (Some(fields), Some(changes)) = (into.as_object_mut(), changes.as_object()) else {
            *into = changes.clone();
            return Ok(());
        };
        for (key, value) in changes {
            let at = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            let field = fields
                .get_mut(key)
                .ok_or_else(|| format!("unknown setting {at}"))?;
            if field.is_object() && value.is_object() {
                merge(field, value, &at)?;
            } else {
                *field = value.clone();
            }
        }
        Ok(())
    }
    if !changes.is_object() {
        return Err("changes must be an object".to_owned());
    }
    let mut merged = current.clone();
    merge(&mut merged, changes, "")?;
    Ok(merged)
}

/// The settings named in `fields`, whole; with no names, every setting with
/// plain values shown and nested ones summarised.
pub fn settings_view(
    all: &serde_json::Value,
    fields: &[String],
) -> Result<serde_json::Value, String> {
    let map = all.as_object().ok_or("settings are not an object")?;
    if fields.is_empty() {
        let view = map
            .iter()
            .map(|(k, v)| {
                let shown = match v {
                    serde_json::Value::Array(a) => {
                        serde_json::json!(format!("<list of {}>", a.len()))
                    }
                    serde_json::Value::Object(_) => serde_json::json!("<object>"),
                    plain => plain.clone(),
                };
                (k.clone(), shown)
            })
            .collect();
        return Ok(serde_json::Value::Object(view));
    }
    fields
        .iter()
        .map(|f| {
            map.get(f)
                .map(|v| (f.clone(), v.clone()))
                .ok_or_else(|| format!("unknown setting {f}"))
        })
        .collect::<Result<serde_json::Map<_, _>, _>>()
        .map(serde_json::Value::Object)
}

/// A track as a client sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackInfo {
    pub id: i64,
    pub artist: String,
    pub title: String,
    pub genre: String,
    pub bpm: Option<f64>,
    pub key: String,
    pub energy: Option<f64>,
    pub minutes: Option<f64>,
    pub rating: u8,
    pub year: String,
    pub plays: u32,
}

impl From<&TrackRow> for TrackInfo {
    fn from(t: &TrackRow) -> Self {
        Self {
            id: t.id,
            artist: t.artist.clone(),
            title: t.title.clone(),
            genre: t.genre.clone(),
            bpm: t.bpm.map(|b| (b * 10.0).round() / 10.0),
            key: t
                .musical_key
                .and_then(backspn_formats::nml::MusicalKey::from_value)
                .map(backspn_formats::nml::MusicalKey::camelot)
                .unwrap_or_default(),
            energy: energy::estimate(t).map(|e| (e * 10.0).round() / 10.0),
            minutes: t.duration_s.map(|d| (d / 6.0).round() / 10.0),
            rating: t.rating / 51,
            year: t.release_date.chars().take(4).collect(),
            plays: t.playcount,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Overview {
    pub tracks: usize,
    pub analyzed: usize,
    pub bpm_min: Option<f64>,
    pub bpm_max: Option<f64>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    /// The most common genres with their track counts.
    pub genres: Vec<(String, usize)>,
    pub playlists: Vec<String>,
    pub smartlists: Vec<String>,
    /// Effect names a transition recipe can use.
    pub effects: Vec<String>,
    /// Drum kits for the pattern player, with their sounds.
    pub kits: Vec<KitInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KitInfo {
    pub name: String,
    pub sounds: Vec<String>,
}

const TOP_GENRES: usize = 40;

/// What the library holds, for a client to know before planning.
pub fn overview(rows: &[TrackRow]) -> Overview {
    let songs: Vec<&TrackRow> = rows.iter().filter(|t| t.sample_type.is_none()).collect();
    let bpms = songs.iter().filter_map(|t| t.bpm.filter(|b| *b > 0.0));
    let years = songs.iter().filter_map(|t| {
        let y: String = t.release_date.chars().take(4).collect();
        y.parse::<i32>().ok().filter(|y| *y > 1900)
    });
    let mut genres: BTreeMap<String, usize> = BTreeMap::new();
    for t in &songs {
        let genre = t.genre.trim();
        if !genre.is_empty() {
            *genres.entry(genre.to_owned()).or_default() += 1;
        }
    }
    let mut genres: Vec<(String, usize)> = genres.into_iter().collect();
    genres.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    genres.truncate(TOP_GENRES);
    let round = |b: f64| (b * 10.0).round() / 10.0;
    Overview {
        tracks: songs.len(),
        analyzed: songs.iter().filter(|t| t.analyzed).count(),
        bpm_min: bpms.clone().reduce(f64::min).map(round),
        bpm_max: bpms.reduce(f64::max).map(round),
        year_min: years.clone().min(),
        year_max: years.max(),
        genres,
        ..Overview::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_read_from_tool_input() {
        let r: Request = serde_json::from_str(
            r#"{"type": "start_set", "tracks": [{"id": 4}, {"id": 9, "transition": {"style": "echo_out"}}],
                "transition": {"style": "blend", "bars": 32}}"#,
        )
        .unwrap();
        let Request::StartSet {
            tracks, transition, ..
        } = r
        else {
            panic!("{r:?}");
        };
        assert_eq!(tracks[0].id, 4);
        assert_eq!(transition.unwrap().bars, 32);
        let r: Request = serde_json::from_str(r#"{"type": "plan_set"}"#).unwrap();
        assert_eq!(
            r,
            Request::PlanSet {
                brief: SessionBrief::default()
            }
        );
        let r: Request = serde_json::from_str(
            r#"{"type": "set_mixer", "deck": 1, "control": "eq_low", "value": 0}"#,
        )
        .unwrap();
        assert!(matches!(
            r,
            Request::SetMixer {
                control: MixerControl::EqLow,
                ..
            }
        ));
        let r: Request = serde_json::from_str(
            r#"{"type": "set_pattern", "unit": 3, "kit": "Tight", "decks": [0],
                "steps": {"Hat": "..x...x...x...x."}, "on": true}"#,
        )
        .unwrap();
        let Request::SetPattern { pattern, decks } = r else {
            panic!("{r:?}");
        };
        assert_eq!((pattern.unit, decks), (3, Some(vec![0])));
        assert_eq!(pattern.on, Some(true));
    }

    #[test]
    fn a_patch_changes_only_its_fields() {
        let brief = SessionBrief {
            duration_min: Some(60.0),
            energy_end: Some(5.0),
            ..SessionBrief::default()
        };
        let patched = patch_brief(&brief, &serde_json::json!({"energy_end": 9})).unwrap();
        assert_eq!(patched.duration_min, Some(60.0));
        assert_eq!(patched.energy_end, Some(9.0));
        assert_eq!(
            patch_brief(&brief, &serde_json::Value::Null),
            Ok(brief.clone())
        );
        assert!(patch_brief(&brief, &serde_json::json!([1])).is_err());
        assert!(patch_brief(&brief, &serde_json::json!({"curve": "zigzag"})).is_err());
    }

    #[test]
    fn settings_merge_only_known_fields() {
        let current =
            serde_json::json!({"deck_count": 2, "mixer": {"limiter": true, "headroom": 3}});
        let merged =
            merge_known(&current, &serde_json::json!({"mixer": {"limiter": false}})).unwrap();
        assert_eq!(
            merged["mixer"],
            serde_json::json!({"limiter": false, "headroom": 3})
        );
        assert_eq!(merged["deck_count"], 2);
        assert_eq!(
            merge_known(&current, &serde_json::json!({"mixer": {"limitr": false}})),
            Err("unknown setting mixer.limitr".to_owned())
        );
        assert!(merge_known(&current, &serde_json::json!(4)).is_err());
        let view = settings_view(&current, &[]).unwrap();
        assert_eq!(
            view,
            serde_json::json!({"deck_count": 2, "mixer": "<object>"})
        );
        let one = settings_view(&current, &["mixer".to_owned()]).unwrap();
        assert_eq!(one["mixer"]["headroom"], 3);
        assert!(settings_view(&current, &["nope".to_owned()]).is_err());
    }

    #[test]
    fn replies_carry_a_result_or_an_error() {
        let ok = serde_json::to_string(&Reply::from(Ok(serde_json::json!(3)))).unwrap();
        assert_eq!(ok, r#"{"ok":true,"result":3}"#);
        let err = serde_json::to_string(&Reply::from(Err("no".to_owned()))).unwrap();
        assert_eq!(err, r#"{"ok":false,"error":"no"}"#);
    }

    #[test]
    fn overview_sums_up_the_library() {
        let row = |id, genre: &str, bpm, year: &str| TrackRow {
            id,
            genre: genre.into(),
            bpm: Some(bpm),
            release_date: year.into(),
            analyzed: true,
            ..TrackRow::default()
        };
        let mut sample = row(9, "Tools", 90.0, "");
        sample.sample_type = Some(1);
        let rows = [
            row(1, "House", 122.0, "1996/1/1"),
            row(2, "House", 126.5, "2021"),
            row(3, "Techno", 132.0, ""),
            sample,
        ];
        let o = overview(&rows);
        assert_eq!(o.tracks, 3);
        assert_eq!((o.bpm_min, o.bpm_max), (Some(122.0), Some(132.0)));
        assert_eq!((o.year_min, o.year_max), (Some(1996), Some(2021)));
        assert_eq!(
            o.genres,
            [("House".to_owned(), 2), ("Techno".to_owned(), 1)]
        );
    }

    #[test]
    fn track_info_rounds_for_reading() {
        let t = TrackRow {
            id: 5,
            bpm: Some(123.456),
            musical_key: Some(21),
            duration_s: Some(405.0),
            rating: 204,
            release_date: "2019/5/1".into(),
            ..TrackRow::default()
        };
        let info = TrackInfo::from(&t);
        assert_eq!(info.bpm, Some(123.5));
        assert_eq!(info.key, "8A");
        assert_eq!(info.minutes, Some(6.8));
        assert_eq!((info.rating, info.year.as_str()), (4, "2019"));
    }
}
