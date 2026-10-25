use std::collections::HashMap;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Error, ImportReport, Library, NodeKind, Result};

pub const EXTENSION: &str = "backspn";
const FORMAT: &str = "backspn.library";
const VERSION: u32 = 1;

const SKIPPED_TRACK_COLUMNS: [&str; 3] = ["id", "nml_raw", "dirty"];
const SKIPPED_CUE_COLUMNS: [&str; 1] = ["track_id"];

#[derive(Debug, Default, Serialize, Deserialize)]
struct LibraryFile {
    format: String,
    version: u32,
    #[serde(default)]
    tracks: Vec<TrackFile>,
    #[serde(default)]
    nodes: Vec<NodeFile>,
    #[serde(default)]
    history: Vec<SessionFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrackFile {
    fields: Map<String, Value>,
    #[serde(default)]
    cues: Vec<Map<String, Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NodeFile {
    parent: Option<usize>,
    kind: String,
    name: String,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    entries: Vec<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionFile {
    name: String,
    entries: Vec<PlayFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlayFile {
    track: usize,
    deck: usize,
    start_date: u32,
    start_time: u32,
    duration: f64,
    played_public: bool,
}

fn columns(conn: &Connection, table: &str) -> Result<Vec<(String, bool)>> {
    let mut stmt = conn.prepare("SELECT name, type FROM pragma_table_info(?1)")?;
    let rows = stmt.query_map([table], |r| {
        let kind: String = r.get(1)?;
        Ok((r.get::<_, String>(0)?, kind.eq_ignore_ascii_case("BLOB")))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => i.into(),
        ValueRef::Real(f) => f.into(),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned().into(),
        ValueRef::Blob(b) => BASE64.encode(b).into(),
    }
}

fn to_sql(value: &Value, blob: bool) -> Result<rusqlite::types::Value> {
    use rusqlite::types::Value as Sql;
    Ok(match value {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(i64::from(*b)),
        Value::Number(n) => n
            .as_i64()
            .map_or_else(|| Sql::Real(n.as_f64().unwrap_or(0.0)), Sql::Integer),
        Value::String(s) if blob => Sql::Blob(
            BASE64
                .decode(s)
                .map_err(|e| Error::Invalid(format!("bad blob: {e}")))?,
        ),
        Value::String(s) => Sql::Text(s.clone()),
        Value::Array(_) | Value::Object(_) => {
            return Err(Error::Invalid("unexpected value in a column".into()));
        }
    })
}

fn rows(
    conn: &Connection,
    sql: &str,
    id: i64,
    skipped: &[&str],
) -> Result<Vec<Map<String, Value>>> {
    let mut stmt = conn.prepare(sql)?;
    let names: Vec<String> = stmt.column_names().iter().map(|&n| n.to_owned()).collect();
    let mut rows = stmt.query([id])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut map = Map::new();
        for (i, name) in names.iter().enumerate() {
            if skipped.contains(&name.as_str()) {
                continue;
            }
            let value = to_json(row.get_ref(i)?);
            if !value.is_null() {
                map.insert(name.clone(), value);
            }
        }
        out.push(map);
    }
    Ok(out)
}

fn insert(
    conn: &Connection,
    table: &str,
    known: &[(String, bool)],
    skipped: &[&str],
    fields: &Map<String, Value>,
    extra: &[(&str, rusqlite::types::Value)],
) -> Result<()> {
    let mut names: Vec<&str> = Vec::new();
    let mut values: Vec<rusqlite::types::Value> = Vec::new();
    for (name, blob) in known {
        if skipped.contains(&name.as_str()) {
            continue;
        }
        if let Some(value) = fields.get(name) {
            names.push(name);
            values.push(to_sql(value, *blob)?);
        }
    }
    for (name, value) in extra {
        names.push(name);
        values.push(value.clone());
    }
    let marks = vec!["?"; names.len()].join(", ");
    conn.execute(
        &format!(
            "INSERT INTO {table} ({}) VALUES ({marks})",
            names.join(", ")
        ),
        rusqlite::params_from_iter(values),
    )?;
    Ok(())
}

impl LibraryFile {
    fn new() -> Self {
        Self {
            format: FORMAT.to_owned(),
            version: VERSION,
            ..Self::default()
        }
    }

    fn to_text(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| Error::Invalid(e.to_string()))
    }
}

struct Written<'a> {
    conn: &'a Connection,
    file: LibraryFile,
    index: HashMap<i64, usize>,
}

impl<'a> Written<'a> {
    fn new(conn: &'a Connection) -> Self {
        Self {
            conn,
            file: LibraryFile::new(),
            index: HashMap::new(),
        }
    }

    fn track(&mut self, id: i64) -> Result<Option<usize>> {
        if let Some(&i) = self.index.get(&id) {
            return Ok(Some(i));
        }
        let Some(fields) = rows(
            self.conn,
            "SELECT * FROM tracks WHERE id = ?1",
            id,
            &SKIPPED_TRACK_COLUMNS,
        )?
        .pop() else {
            return Ok(None);
        };
        let cues = rows(
            self.conn,
            "SELECT * FROM cues WHERE track_id = ?1 ORDER BY position",
            id,
            &SKIPPED_CUE_COLUMNS,
        )?;
        let i = self.file.tracks.len();
        self.file.tracks.push(TrackFile { fields, cues });
        self.index.insert(id, i);
        Ok(Some(i))
    }

    fn tracks(&mut self, ids: &[i64]) -> Result<Vec<usize>> {
        let mut out = Vec::new();
        for &id in ids {
            out.extend(self.track(id)?);
        }
        Ok(out)
    }
}

impl Library {
    pub fn export_collection(&self) -> Result<String> {
        let mut w = Written::new(&self.conn);
        for t in self.tracks()? {
            w.track(t.id)?;
        }
        let mut index: HashMap<i64, usize> = HashMap::new();
        for node in self.nodes()? {
            let entries = if node.kind == NodeKind::Playlist {
                w.tracks(&self.node_tracks(node.id)?)?
            } else {
                Vec::new()
            };
            index.insert(node.id, w.file.nodes.len());
            w.file.nodes.push(NodeFile {
                parent: node.parent.and_then(|p| index.get(&p).copied()),
                kind: node.kind.as_str().to_owned(),
                name: node.name,
                query: node.query,
                entries,
            });
        }
        w.file.to_text()
    }

    pub fn export_list(&self, node: Option<i64>, title: &str) -> Result<String> {
        let (_, tracks) = self.list(node)?;
        let mut w = Written::new(&self.conn);
        let entries = w.tracks(&tracks)?;
        w.file.nodes.push(NodeFile {
            parent: None,
            kind: NodeKind::Playlist.as_str().to_owned(),
            name: title.to_owned(),
            query: None,
            entries,
        });
        w.file.to_text()
    }

    pub fn export_history(&self, session: i64) -> Result<String> {
        let name = self.history_session_name(session)?.unwrap_or_default();
        let mut w = Written::new(&self.conn);
        let mut entries = Vec::new();
        for e in self.history_entries(session)? {
            let Some(track) = e.track_id else { continue };
            let Some(track) = w.track(track)? else {
                continue;
            };
            entries.push(PlayFile {
                track,
                deck: e.deck,
                start_date: e.start_date,
                start_time: e.start_time,
                duration: e.duration,
                played_public: e.played_public,
            });
        }
        w.file.history.push(SessionFile { name, entries });
        w.file.to_text()
    }

    pub fn import_file(&mut self, file: &Path, parent: Option<i64>) -> Result<ImportReport> {
        let data = read(file)?;
        let mut report = ImportReport::default();
        let tracks = self.import_tracks(&data.tracks, &mut report)?;
        let mut nodes: Vec<i64> = Vec::new();
        for n in &data.nodes {
            let under = match n.parent {
                Some(p) => Some(
                    *nodes
                        .get(p)
                        .ok_or_else(|| Error::Invalid("a node's parent comes after it".into()))?,
                ),
                None => parent,
            };
            let name = if n.parent.is_none() {
                self.unique_node_name(under, &n.name)?
            } else {
                n.name.clone()
            };
            let kind = NodeKind::parse(&n.kind);
            let id = self.create_node(under, kind, &name, n.query.as_deref())?;
            match kind {
                NodeKind::Folder => report.folders += 1,
                NodeKind::Playlist => report.playlists += 1,
                NodeKind::Smartlist => report.smartlists += 1,
            }
            let entries: Vec<i64> = n
                .entries
                .iter()
                .filter_map(|&i| tracks.get(i).copied())
                .collect();
            self.add_to_playlist(id, &entries)?;
            nodes.push(id);
        }
        for s in &data.history {
            self.import_session(s, &tracks)?;
        }
        Ok(report)
    }

    pub fn import_history_file(&mut self, file: &Path) -> Result<usize> {
        let data = read(file)?;
        if data.history.is_empty() {
            return Err(Error::Invalid(format!(
                "{}: no history in it",
                file.display()
            )));
        }
        let tracks = self.import_tracks(&data.tracks, &mut ImportReport::default())?;
        let mut played = 0;
        for s in &data.history {
            played += self.import_session(s, &tracks)?;
        }
        Ok(played)
    }

    fn import_tracks(
        &mut self,
        files: &[TrackFile],
        report: &mut ImportReport,
    ) -> Result<Vec<i64>> {
        let track_columns = columns(&self.conn, "tracks")?;
        let cue_columns = columns(&self.conn, "cues")?;
        let tx = self.conn.transaction()?;
        let mut ids = Vec::with_capacity(files.len());
        for t in files {
            let path = t
                .fields
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Invalid("a track without a path".into()))?;
            let existing: Option<i64> = tx
                .query_row("SELECT id FROM tracks WHERE path = ?1", [path], |r| {
                    r.get(0)
                })
                .ok();
            if let Some(id) = existing {
                ids.push(id);
                continue;
            }
            insert(
                &tx,
                "tracks",
                &track_columns,
                &SKIPPED_TRACK_COLUMNS,
                &t.fields,
                &[],
            )?;
            let id = tx.last_insert_rowid();
            for cue in &t.cues {
                insert(
                    &tx,
                    "cues",
                    &cue_columns,
                    &SKIPPED_CUE_COLUMNS,
                    cue,
                    &[("track_id", id.into())],
                )?;
            }
            report.tracks += 1;
            report.cues += t.cues.len();
            ids.push(id);
        }
        tx.commit()?;
        Ok(ids)
    }

    fn import_session(&mut self, s: &SessionFile, tracks: &[i64]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        tx.execute("INSERT INTO history_sessions (name) VALUES (?1)", [&s.name])?;
        let session = tx.last_insert_rowid();
        let mut played = 0;
        for (position, e) in s.entries.iter().enumerate() {
            let Some(&track) = tracks.get(e.track) else {
                continue;
            };
            played += 1;
            tx.execute(
                "INSERT INTO history_entries (session_id, position, track_id, deck, start_date, \
                 start_time, duration, played_public) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    session,
                    position as i64,
                    track,
                    e.deck as i64,
                    e.start_date,
                    e.start_time,
                    e.duration,
                    e.played_public,
                ],
            )?;
        }
        tx.commit()?;
        Ok(played)
    }
}

fn read(file: &Path) -> Result<LibraryFile> {
    let text = std::fs::read_to_string(file).map_err(|source| Error::Io {
        path: file.to_owned(),
        source,
    })?;
    let data: LibraryFile =
        serde_json::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
    if data.format != FORMAT {
        return Err(Error::Invalid(format!(
            "{}: not a library file",
            file.display()
        )));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CuePoint;
    use crate::clock::LocalTime;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("backspn-libfile-{}-{name}", std::process::id()))
    }

    fn cue(start_s: f64) -> CuePoint {
        CuePoint {
            kind: CuePoint::CUE,
            name: "Drop".to_owned(),
            start_s,
            len_s: 0.0,
            hotcue: Some(2),
            display_order: 0,
            repeats: -1,
            color: None,
        }
    }

    #[test]
    fn a_collection_goes_out_and_into_another_library() {
        let mut lib = Library::open_in_memory().unwrap();
        let a = lib.add_file(Path::new("/m/Ann - A.mp3")).unwrap();
        let b = lib.add_file(Path::new("/m/Bob - B.mp3")).unwrap();
        lib.set_rating(a, 4).unwrap();
        lib.save_overview(a, &[1, 2, 3]).unwrap();
        lib.save_cues(a, &[cue(12.5)]).unwrap();
        let folder = lib
            .create_node(None, NodeKind::Folder, "Gigs", None)
            .unwrap();
        let list = lib
            .create_node(Some(folder), NodeKind::Playlist, "Friday", None)
            .unwrap();
        lib.add_to_playlist(list, &[b, a, b]).unwrap();
        lib.create_node(None, NodeKind::Smartlist, "Good", Some("$RATING >= 4"))
            .unwrap();

        let text = lib.export_collection().unwrap();
        assert!(text.contains("\"format\": \"backspn.library\""));
        assert!(!text.contains("NML"), "nothing of a foreign format");
        let file = temp("collection.backspn");
        std::fs::write(&file, text).unwrap();

        let mut other = Library::open_in_memory().unwrap();
        let report = other.import_file(&file, None).unwrap();
        assert_eq!(
            (
                report.tracks,
                report.cues,
                report.folders,
                report.playlists,
                report.smartlists
            ),
            (2, 1, 1, 1, 1)
        );
        let a2 = other.track_by_path("/m/Ann - A.mp3").unwrap().unwrap();
        assert_eq!(a2.rating, 4);
        assert_eq!(other.cues(a2.id).unwrap(), [cue(12.5)]);
        let nodes = other.nodes().unwrap();
        let friday = nodes.iter().find(|n| n.name == "Friday").unwrap();
        let gigs = nodes.iter().find(|n| n.name == "Gigs").unwrap();
        assert_eq!(friday.parent, Some(gigs.id));
        let names: Vec<String> = other
            .node_tracks(friday.id)
            .unwrap()
            .iter()
            .map(|&t| other.track(t).unwrap().unwrap().path)
            .collect();
        assert_eq!(
            names,
            ["/m/Bob - B.mp3", "/m/Ann - A.mp3", "/m/Bob - B.mp3"]
        );
        let good = nodes.iter().find(|n| n.name == "Good").unwrap();
        assert_eq!(good.query.as_deref(), Some("$RATING >= 4"));

        let again = other.import_file(&file, None).unwrap();
        assert_eq!(again.tracks, 0);
        assert!(other.nodes().unwrap().iter().any(|n| n.name == "Gigs (2)"));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn a_history_session_goes_out_and_back() {
        let mut lib = Library::open_in_memory().unwrap();
        let a = lib.add_file(Path::new("/m/Ann - A.mp3")).unwrap();
        let now = LocalTime {
            year: 2026,
            month: 10,
            day: 23,
            seconds: 3600,
        };
        let session = lib.start_history_session(now).unwrap();
        lib.add_history_entry(session, a, 1, now).unwrap();
        let file = temp("history.backspn");
        std::fs::write(&file, lib.export_history(session).unwrap()).unwrap();

        let mut other = Library::open_in_memory().unwrap();
        assert_eq!(other.import_history_file(&file).unwrap(), 1);
        let sessions = other.history_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        let entries = other.history_entries(sessions[0].id).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!((entries[0].deck, entries[0].start_time), (1, 3600));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn other_files_are_refused() {
        let file = temp("other.backspn");
        std::fs::write(&file, r#"{"format": "something", "version": 1}"#).unwrap();
        let mut lib = Library::open_in_memory().unwrap();
        assert!(lib.import_file(&file, None).is_err());
        std::fs::write(&file, r#"{"format": "backspn.library", "version": 1}"#).unwrap();
        assert!(lib.import_history_file(&file).is_err(), "no history in it");
        let _ = std::fs::remove_file(&file);
    }
}
