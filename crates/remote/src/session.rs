//! A session: a whole set written ahead of time (tracks, how each one comes
//! in, what it does while it plays) that the app plays on its own, saved
//! as a `.bsession` file so it can be played again without an assistant.

use std::path::{Path, PathBuf};

use backspn_library::set_plan::Transition;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::protocol::SetTrack;

pub const EXTENSION: &str = "bsession";
/// Folder of the data directory that holds saved sessions.
pub const FOLDER: &str = "sessions";
const FORMAT: &str = "backspn-session";
const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Session {
    pub name: String,
    /// What the session is for, in the author's words.
    #[serde(default)]
    pub notes: String,
    /// How tracks without their own transition come in.
    #[serde(default)]
    pub transition: Option<Transition>,
    #[serde(default)]
    pub keylock: Option<bool>,
    pub tracks: Vec<SetTrack>,
}

impl Session {
    pub fn to_json(&self) -> String {
        let mut value = serde_json::to_value(self).unwrap_or_default();
        if let Value::Object(map) = &mut value {
            map.insert("format".into(), FORMAT.into());
            map.insert("version".into(), VERSION.into());
        }
        serde_json::to_string_pretty(&value).unwrap_or_default()
    }

    /// Reads a session field by field: a field that does not read is left
    /// at its default and a track that does not read is skipped, each with
    /// a warning.
    pub fn from_json(text: &str) -> Result<(Self, Vec<String>), String> {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let Value::Object(map) = value else {
            return Err("a session is a JSON object".to_owned());
        };
        if map.get("format").and_then(Value::as_str) != Some(FORMAT) {
            return Err("not a Backspn session".to_owned());
        }
        let mut warnings = Vec::new();
        let mut session = Self::default();
        if let Some(name) = field(&map, "name", &mut warnings) {
            session.name = name;
        }
        if let Some(notes) = field(&map, "notes", &mut warnings) {
            session.notes = notes;
        }
        session.transition = field(&map, "transition", &mut warnings);
        session.keylock = field(&map, "keylock", &mut warnings);
        if let Some(Value::Array(tracks)) = map.get("tracks") {
            for (i, track) in tracks.iter().enumerate() {
                match serde_json::from_value(track.clone()) {
                    Ok(t) => session.tracks.push(t),
                    Err(e) => warnings.push(format!("track {}: {e}", i + 1)),
                }
            }
        }
        Ok((session, warnings))
    }
}

/// One field of a saved object; a field that does not read is left out
/// with a warning.
fn field<T: serde::de::DeserializeOwned>(
    map: &serde_json::Map<String, Value>,
    name: &str,
    warnings: &mut Vec<String>,
) -> Option<T> {
    let value = map.get(name).filter(|v| !v.is_null())?;
    serde_json::from_value(value.clone())
        .map_err(|e| warnings.push(format!("{name}: {e}")))
        .ok()
}

/// The file name a session is saved under: its name with only letters,
/// digits, spaces, `-` and `_`.
pub fn file_name(name: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let stem = stem.trim_matches(|c: char| c == '-' || c.is_whitespace());
    let stem = if stem.is_empty() { "session" } else { stem };
    format!("{stem}.{EXTENSION}")
}

pub fn folder(data_dir: &Path) -> PathBuf {
    data_dir.join(FOLDER)
}

/// Writes a session into `folder`, replacing one of the same name.
pub fn save(folder: &Path, session: &Session) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let path = folder.join(file_name(&session.name));
    let partial = path.with_extension("partial");
    std::fs::write(&partial, session.to_json())?;
    std::fs::rename(&partial, &path)?;
    Ok(path)
}

pub fn load(path: &Path) -> Result<(Session, Vec<String>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let (mut session, warnings) = Session::from_json(&text)?;
    if session.name.trim().is_empty() {
        session.name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    Ok((session, warnings))
}

/// The sessions saved in `folder`, by name; files that do not read are
/// left out.
pub fn list(folder: &Path) -> Vec<(PathBuf, Session)> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, Session)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == EXTENSION))
        .filter_map(|p| load(&p).ok().map(|(s, _)| (p, s)))
        .collect();
    found.sort_by_key(|(_, s)| s.name.to_lowercase());
    found
}

/// The saved session called `name`, by its name or its file name.
pub fn find(folder: &Path, name: &str) -> Option<PathBuf> {
    let wanted = name.trim().to_lowercase();
    list(folder)
        .into_iter()
        .find(|(p, s)| {
            s.name.trim().to_lowercase() == wanted
                || p.file_name().is_some_and(|f| *f == *file_name(name))
        })
        .map(|(p, _)| p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transition::Script;

    fn track(id: i64) -> SetTrack {
        SetTrack {
            id,
            play_minutes: None,
            start_at_pct: None,
            transition: None,
            recipe: None,
            script: None,
        }
    }

    #[test]
    fn a_session_survives_a_round_trip_through_its_file() {
        let folder = std::env::temp_dir().join(format!("backspn-sessions-{}", std::process::id()));
        let mut scripted = track(2);
        scripted.script = Some(Script::default());
        scripted.play_minutes = Some(4.0);
        let session = Session {
            name: "Friday: warm-up".into(),
            notes: "Deep to driving".into(),
            transition: None,
            keylock: Some(true),
            tracks: vec![track(1), scripted],
        };
        let path = save(&folder, &session).unwrap();
        assert_eq!(path.file_name().unwrap(), "Friday- warm-up.bsession");
        assert_eq!(load(&path).unwrap(), (session.clone(), Vec::new()));
        assert_eq!(list(&folder).len(), 1);
        assert_eq!(find(&folder, "friday: WARM-UP"), Some(path.clone()));
        assert_eq!(find(&folder, "nope"), None);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_session_reads_what_it_can() {
        let (session, warnings) = Session::from_json(
            r#"{"format": "backspn-session", "version": 7, "name": "Old", "keylock": "yes",
                "tracks": [{"id": 1}, {"title": "no id"}, {"id": 3, "play_minutes": 2}],
                "added_later": true}"#,
        )
        .unwrap();
        assert_eq!(session.name, "Old");
        assert_eq!(session.keylock, None);
        assert_eq!(
            session.tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
            [1, 3]
        );
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(Session::from_json(r#"{"name": "x"}"#).is_err());
        assert!(Session::from_json("[]").is_err());
    }

    #[test]
    fn file_names_keep_only_safe_characters() {
        assert_eq!(file_name("../../etc/passwd"), "etc-passwd.bsession");
        assert_eq!(file_name("  "), "session.bsession");
        assert_eq!(file_name("Sábado 2"), "Sábado 2.bsession");
    }
}
