//! AI control of the rest of the app: every controller command, the
//! settings and the library (search, tracks, lists, history).

use backspn_control::{Action, Trigger};
use backspn_engine::EngineState;
use backspn_formats::nml::MusicalKey;
use backspn_library::{NodeKind, TextField, TrackRow};
use backspn_remote::protocol::{self, ListKind, Press, Request, TrackChanges, TrackInfo};
use serde_json::{Value, json};

use crate::App;
use crate::track_edit::Edit;

const SEARCH_LIMIT: usize = 50;
const CONTROLS_LIMIT: usize = 80;

/// The track edits a change asks for, or what is wrong with it.
fn edits(changes: &TrackChanges) -> Result<Vec<Edit>, String> {
    let texts = [
        (TextField::Title, &changes.title),
        (TextField::Artist, &changes.artist),
        (TextField::Album, &changes.album),
        (TextField::Genre, &changes.genre),
        (TextField::Label, &changes.label),
        (TextField::Comment, &changes.comment),
        (TextField::Comment2, &changes.comment2),
        (TextField::Remixer, &changes.remixer),
        (TextField::Producer, &changes.producer),
        (TextField::Mix, &changes.mix),
        (TextField::CatalogNo, &changes.catalog_no),
        (TextField::ReleaseDate, &changes.release_date),
    ];
    let mut out: Vec<Edit> = texts
        .into_iter()
        .filter_map(|(field, value)| value.as_ref().map(|v| Edit::Text(field, v.clone())))
        .collect();
    if let Some(key) = &changes.key {
        let key = key.trim();
        let value = if key.is_empty() {
            None
        } else {
            let parsed = MusicalKey::parse(key).ok_or_else(|| format!("unknown key \"{key}\""))?;
            Some(parsed.value())
        };
        out.push(Edit::Key(value));
    }
    if let Some(stars) = changes.rating {
        if stars > 5 {
            return Err("rating must be 0–5 stars".to_owned());
        }
        out.push(Edit::Rating(stars * 51));
    }
    if let Some(n) = changes.play_count {
        out.push(Edit::PlayCount(n));
    }
    if let Some(n) = changes.track_number {
        out.push(Edit::TrackNumber(Some(n)));
    }
    if out.is_empty() {
        return Err("nothing to change".to_owned());
    }
    Ok(out)
}

/// The trigger a control call asks for.
fn triggers(value: Option<f32>, delta: Option<f32>, press: Option<Press>) -> Vec<Trigger> {
    match (value, delta, press.unwrap_or_default()) {
        (Some(v), _, _) => vec![Trigger::Set(v.clamp(0.0, 1.0))],
        (None, Some(d), _) => vec![Trigger::Delta(d)],
        (None, None, Press::Tap) => vec![Trigger::Press, Trigger::Release],
        (None, None, Press::Press) => vec![Trigger::Press],
        (None, None, Press::Release) => vec![Trigger::Release],
    }
}

/// Every command whose label holds all the words of `filter`.
fn controls(filter: &str) -> Vec<(String, Value)> {
    let words: Vec<String> = filter.split_whitespace().map(str::to_lowercase).collect();
    crate::controls::all_commands()
        .into_iter()
        .map(|a| (a.label(), a))
        .filter(|(label, _)| {
            let label = label.to_lowercase();
            words.iter().all(|w| label.contains(w.as_str()))
        })
        .filter_map(|(label, a)| serde_json::to_value(a).ok().map(|v| (label, v)))
        .collect()
}

fn track_json(t: &TrackRow) -> Value {
    let mut info = serde_json::to_value(TrackInfo::from(t)).unwrap_or_default();
    let extra = json!({
        "album": t.album, "label": t.label, "comment": t.comment, "comment2": t.comment2,
        "remixer": t.remixer, "producer": t.producer, "mix": t.mix,
        "catalog_no": t.catalog_no, "release_date": t.release_date,
        "track_number": t.track_number, "key_text": t.key_text,
        "grid_anchor_s": t.grid_anchor_s, "lufs": t.lufs, "analyzed": t.analyzed,
        "locked": t.locked, "color": t.color, "bitrate": t.bitrate,
        "import_date": t.import_date, "last_played": t.last_played, "path": t.path,
    });
    if let (Some(info), Some(extra)) = (info.as_object_mut(), extra.as_object()) {
        info.extend(extra.clone());
    }
    info
}

impl App {
    fn infos(&self, ids: impl IntoIterator<Item = i64>, limit: usize) -> Vec<TrackInfo> {
        ids.into_iter()
            .filter_map(|id| self.browser.row(id).map(TrackInfo::from))
            .take(limit)
            .collect()
    }

    fn known_tracks(&self, ids: &[i64]) -> Result<(), String> {
        let missing: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|&id| id <= 0 || self.browser.row(id).is_none())
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!("tracks not in the library: {missing:?}"))
        }
    }

    /// Answers the requests for controls, settings and the library.
    pub(super) fn answer_app(
        &mut self,
        request: Request,
        state: &EngineState,
    ) -> Result<Value, String> {
        match request {
            Request::ListControls { filter } => {
                let found = controls(&filter);
                let shown: Vec<Value> = found
                    .iter()
                    .take(CONTROLS_LIMIT)
                    .map(|(label, action)| json!({"label": label, "action": action}))
                    .collect();
                Ok(json!({"total": found.len(), "controls": shown}))
            }
            Request::Control {
                action,
                value,
                delta,
                press,
            } => {
                let action: Action = serde_json::from_value(action)
                    .map_err(|e| format!("not an action from list_controls: {e}"))?;
                for trigger in triggers(value, delta, press) {
                    self.remote.hand_moved(action);
                    self.execute(action, trigger, state);
                }
                self.flush();
                Ok(json!({"done": action.label()}))
            }
            Request::GetSettings { fields } => {
                let all = serde_json::to_value(&self.settings).map_err(|e| e.to_string())?;
                protocol::settings_view(&all, &fields)
            }
            Request::SetSettings { changes } => {
                let current = serde_json::to_value(&self.settings).map_err(|e| e.to_string())?;
                let merged = protocol::merge_known(&current, &changes)?;
                let asked: crate::prefs::Settings =
                    serde_json::from_value(merged).map_err(|e| format!("settings: {e}"))?;
                self.remote.ask_settings(asked);
                Ok(json!({"applied": "on the next frame"}))
            }
            other => self.answer_library(other),
        }
    }

    fn answer_library(&mut self, request: Request) -> Result<Value, String> {
        match request {
            Request::SearchLibrary { query, limit } => {
                let found: Vec<i64> = self
                    .browser
                    .search_rows(&query)?
                    .iter()
                    .map(|t| t.id)
                    .collect();
                let tracks = self.infos(found.iter().copied(), limit.unwrap_or(SEARCH_LIMIT));
                Ok(json!({"total": found.len(), "tracks": tracks}))
            }
            Request::GetTrack { id } => {
                let row = self
                    .browser
                    .row(id)
                    .filter(|_| id > 0)
                    .ok_or_else(|| format!("track {id} is not in the library"))?;
                let mut track = track_json(row);
                let cues: Vec<Value> = self
                    .browser
                    .track_cues(id)
                    .iter()
                    .map(|c| {
                        json!({"kind": c.kind, "name": c.name, "start_s": c.start_s,
                               "len_s": c.len_s, "hotcue": c.hotcue})
                    })
                    .collect();
                track["cues"] = Value::Array(cues);
                Ok(track)
            }
            Request::EditTracks { ids, changes } => {
                self.known_tracks(&ids)?;
                let edits = edits(&changes)?;
                self.browser.edit_tracks(&ids, &edits);
                Ok(json!({"tracks": self.infos(ids.iter().copied(), ids.len())}))
            }
            Request::AnalyzeTracks { ids } => {
                self.known_tracks(&ids)?;
                self.browser.analyze_tracks(&ids);
                Ok(json!({"queued": ids.len()}))
            }
            other => self.answer_lists(other),
        }
    }

    fn answer_lists(&mut self, request: Request) -> Result<Value, String> {
        match request {
            Request::ListLists => {
                let lists: Vec<Value> = self
                    .browser
                    .lists()
                    .into_iter()
                    .map(|l| {
                        let kind = match l.node.kind {
                            NodeKind::Folder => "folder",
                            NodeKind::Playlist => "playlist",
                            NodeKind::Smartlist => "smartlist",
                        };
                        json!({"id": l.node.id, "parent": l.node.parent, "kind": kind,
                               "name": l.node.name, "rule": l.node.query, "tracks": l.tracks})
                    })
                    .collect();
                Ok(json!({"lists": lists}))
            }
            Request::GetList { id, limit } => {
                let ids = self.browser.list_contents(id)?;
                let tracks = self.infos(ids.iter().copied(), limit.unwrap_or(usize::MAX));
                Ok(json!({"total": ids.len(), "tracks": tracks}))
            }
            Request::CreateList {
                kind,
                name,
                parent,
                query,
                tracks,
            } => {
                self.known_tracks(&tracks)?;
                let kind = match kind {
                    ListKind::Playlist => NodeKind::Playlist,
                    ListKind::Smartlist => NodeKind::Smartlist,
                    ListKind::Folder => NodeKind::Folder,
                };
                let id =
                    self.browser
                        .create_list(kind, &name, parent, query.as_deref(), &tracks)?;
                Ok(json!({"id": id}))
            }
            Request::EditList {
                id,
                name,
                parent,
                to_top,
                query,
                add,
                remove,
            } => {
                self.known_tracks(&add)?;
                self.browser.edit_list(
                    id,
                    name.as_deref(),
                    parent,
                    to_top,
                    query.as_deref(),
                    &add,
                    &remove,
                )?;
                Ok(json!({"done": true}))
            }
            Request::DeleteList { id } => {
                self.browser.delete_list(id)?;
                Ok(json!({"deleted": id}))
            }
            Request::ShowList { id } => {
                self.browser.show_list(id)?;
                self.settings.show_browser = true;
                Ok(json!({"shown": id}))
            }
            Request::ListHistory => {
                let sessions: Vec<Value> = self
                    .browser
                    .history_sessions()?
                    .into_iter()
                    .map(|(s, n)| json!({"id": s.id, "name": s.name, "tracks": n}))
                    .collect();
                Ok(json!({"sessions": sessions}))
            }
            Request::GetHistory { id } => {
                let entries: Vec<Value> = self
                    .browser
                    .history_entries(id)?
                    .into_iter()
                    .map(|e| {
                        json!({
                            "position": e.position,
                            "track": e.track_id.and_then(|t| self.browser.row(t)).map(TrackInfo::from),
                            "deck": e.deck,
                            "seconds": e.duration.round(),
                        })
                    })
                    .collect();
                Ok(json!({"entries": entries}))
            }
            _ => Err("not a library request".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_changes_become_edits() {
        let changes = TrackChanges {
            genre: Some("Techno".into()),
            key: Some("8A".into()),
            rating: Some(4),
            ..TrackChanges::default()
        };
        assert_eq!(
            edits(&changes).unwrap(),
            [
                Edit::Text(TextField::Genre, "Techno".into()),
                Edit::Key(Some(21)),
                Edit::Rating(204)
            ]
        );
        let clear = TrackChanges {
            key: Some(String::new()),
            ..TrackChanges::default()
        };
        assert_eq!(edits(&clear).unwrap(), [Edit::Key(None)]);
        assert!(edits(&TrackChanges::default()).is_err());
        let bad = TrackChanges {
            rating: Some(9),
            ..TrackChanges::default()
        };
        assert!(edits(&bad).is_err());
    }

    #[test]
    fn controls_are_found_by_label_words() {
        let all = controls("");
        assert!(all.len() > 100, "{}", all.len());
        let found = controls("deck a play");
        assert!(
            found.iter().any(|(l, a)| l == "Deck A Play/Pause"
                && a == &json!({"action": "PlayPause", "deck": 0})),
            "{found:?}"
        );
        let action: Action = serde_json::from_value(found[0].1.clone()).unwrap();
        assert_eq!(action.label(), found[0].0);
    }

    #[test]
    fn a_control_is_tapped_set_or_turned() {
        assert_eq!(
            triggers(None, None, None),
            [Trigger::Press, Trigger::Release]
        );
        assert_eq!(triggers(Some(1.4), None, None), [Trigger::Set(1.0)]);
        assert_eq!(triggers(None, Some(-2.0), None), [Trigger::Delta(-2.0)]);
        assert_eq!(
            triggers(None, None, Some(Press::Release)),
            [Trigger::Release]
        );
    }
}
