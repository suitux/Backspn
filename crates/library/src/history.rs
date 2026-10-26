use rusqlite::{OptionalExtension, params};

use crate::clock::LocalTime;
use crate::{Library, Result};

pub(crate) const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS history_sessions (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS history_entries (
    session_id INTEGER NOT NULL REFERENCES history_sessions(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    track_id INTEGER REFERENCES tracks(id) ON DELETE SET NULL,
    deck INTEGER NOT NULL,
    start_date INTEGER NOT NULL,
    start_time INTEGER NOT NULL,
    duration REAL NOT NULL DEFAULT 0,
    played_public INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS history_entries_session ON history_entries(session_id, position);
CREATE INDEX IF NOT EXISTS history_entries_track ON history_entries(track_id);
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySession {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub position: i64,
    pub track_id: Option<i64>,
    pub deck: usize,
    pub start_date: u32,
    pub start_time: u32,
    pub duration: f64,
    pub played_public: bool,
}

impl Library {
    pub fn mark_played(&mut self, id: i64, now: LocalTime) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET playcount = playcount + 1, last_played = ?2, dirty = 1 WHERE id = ?1",
            params![id, now.date_text()],
        )?;
        Ok(())
    }

    pub fn start_history_session(&mut self, now: LocalTime) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO history_sessions (name) VALUES (?1)",
            [now.history_name()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn history_sessions(&self) -> Result<Vec<HistorySession>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name FROM history_sessions ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(HistorySession {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn delete_history_session(&mut self, session: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM history_sessions WHERE id = ?1", [session])?;
        Ok(())
    }

    pub fn add_history_entry(
        &mut self,
        session: i64,
        track: i64,
        deck: usize,
        now: LocalTime,
    ) -> Result<i64> {
        let position: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM history_entries WHERE session_id = ?1",
            [session],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO history_entries (session_id, position, track_id, deck, start_date, start_time) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![session, position, track, deck as i64, now.packed_date(), now.seconds],
        )?;
        Ok(position)
    }

    pub fn set_last_history_entry_private(&mut self, session: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE history_entries SET played_public = 0 WHERE session_id = ?1 AND position = \
             (SELECT MAX(position) FROM history_entries WHERE session_id = ?1)",
            [session],
        )?;
        Ok(())
    }

    pub fn set_history_duration(
        &mut self,
        session: i64,
        position: i64,
        seconds: f64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE history_entries SET duration = ?3 WHERE session_id = ?1 AND position = ?2",
            params![session, position, seconds],
        )?;
        Ok(())
    }

    pub fn history_entries(&self, session: i64) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT position, track_id, deck, start_date, start_time, duration, played_public \
             FROM history_entries WHERE session_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map([session], |r| {
            Ok(HistoryEntry {
                position: r.get(0)?,
                track_id: r.get(1)?,
                deck: r
                    .get::<_, i64>(2)
                    .map(|d| usize::try_from(d).unwrap_or(0))?,
                start_date: r.get(3)?,
                start_time: r.get(4)?,
                duration: r.get(5)?,
                played_public: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn remove_history_entries(&mut self, session: i64, positions: &[i64]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for p in positions {
            tx.execute(
                "DELETE FROM history_entries WHERE session_id = ?1 AND position = ?2",
                params![session, p],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn import_history(&mut self, file: &std::path::Path) -> Result<(i64, usize)> {
        let nml = backspn_formats::nml::Nml::from_path(file)?;
        let name = file.file_stem().map_or_else(
            || "history".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        crate::nml_io::import_history(&mut self.conn, &nml, &name)
    }

    pub fn history_session_name(&self, session: i64) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT name FROM history_sessions WHERE id = ?1",
                [session],
                |r| r.get(0),
            )
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u32) -> LocalTime {
        LocalTime {
            year: 2025,
            month: 3,
            day: 8,
            seconds,
        }
    }

    #[test]
    fn played_tracks_count_and_join_the_session_history() {
        let mut lib = Library::open_in_memory().unwrap();
        let a = lib.add_file(std::path::Path::new("/m/A.mp3")).unwrap();
        let b = lib.add_file(std::path::Path::new("/m/B.mp3")).unwrap();
        lib.mark_played(a, at(70_687)).unwrap();
        lib.mark_played(a, at(70_700)).unwrap();
        let t = lib.track(a).unwrap().unwrap();
        assert_eq!((t.playcount, t.last_played.as_str()), (2, "2025/3/8"));

        let s = lib.start_history_session(at(70_000)).unwrap();
        assert_eq!(
            lib.history_session_name(s).unwrap().unwrap(),
            "history_2025y03m08d_19h26m40s"
        );
        let first = lib.add_history_entry(s, a, 0, at(70_687)).unwrap();
        lib.add_history_entry(s, b, 1, at(71_059)).unwrap();
        lib.set_last_history_entry_private(s).unwrap();
        lib.set_history_duration(s, first, 421.5).unwrap();
        let entries = lib.history_entries(s).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!((entries[0].track_id, entries[0].deck), (Some(a), 0));
        assert_eq!(entries[0].start_date, 132_711_176);
        assert!((entries[0].duration - 421.5).abs() < 1e-9);
        assert!(entries[0].played_public && !entries[1].played_public);

        lib.remove_history_entries(s, &[first]).unwrap();
        assert_eq!(lib.history_entries(s).unwrap().len(), 1);
        lib.delete_history_session(s).unwrap();
        assert_eq!(lib.history_sessions().unwrap(), []);
        assert!(lib.track(a).unwrap().is_some(), "tracks stay");
    }

    #[test]
    fn an_exported_session_imports_back_as_it_was() {
        let mut lib = Library::open_in_memory().unwrap();
        let a = lib.add_file(std::path::Path::new("/m/A.mp3")).unwrap();
        let b = lib.add_file(std::path::Path::new("/m/B.mp3")).unwrap();
        let s = lib.start_history_session(at(70_000)).unwrap();
        let first = lib.add_history_entry(s, a, 0, at(70_687)).unwrap();
        lib.add_history_entry(s, b, 1, at(71_059)).unwrap();
        lib.add_history_entry(s, a, 1, at(71_479)).unwrap();
        lib.set_last_history_entry_private(s).unwrap();
        lib.set_history_duration(s, first, 421.469_91).unwrap();
        let file =
            std::env::temp_dir().join(format!("backspn-history-{}.backspn", std::process::id()));
        std::fs::write(&file, lib.export_history(s).unwrap()).unwrap();
        let mut other = Library::open_in_memory().unwrap();
        let count = other.import_history_file(&file).unwrap();
        std::fs::remove_file(&file).unwrap();
        assert_eq!(count, 3);
        assert_eq!(other.tracks().unwrap().len(), 2, "each track once");
        let back = other.history_sessions().unwrap()[0].id;
        let shape = |lib: &Library, s: i64| {
            lib.history_entries(s)
                .unwrap()
                .into_iter()
                .map(|e| {
                    let path = lib.track(e.track_id.unwrap()).unwrap().unwrap().path;
                    (path, e.deck, e.start_date, e.start_time, e.played_public)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(shape(&other, back), shape(&lib, s));
    }
}
