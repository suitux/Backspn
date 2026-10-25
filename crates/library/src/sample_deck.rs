use backspn_formats::nml::{Location, SET_CELLS, SET_SLOTS, SampleSet, SetCell, SetSlot};
use backspn_formats::xml::{self, Element};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{Library, Result, paths};

pub(crate) fn rename_old_tables(conn: &Connection) -> Result<()> {
    for (old, new) in [
        ("remix_sets", "sample_sets"),
        ("remix_slots", "sample_slots"),
        ("remix_cells", "sample_cells"),
    ] {
        let exists = |name: &str| -> rusqlite::Result<bool> {
            conn.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [name],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
        };
        if exists(old)? && !exists(new)? {
            conn.execute_batch(&format!("ALTER TABLE {old} RENAME TO {new}"))?;
        }
    }
    Ok(())
}

pub(crate) const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS sample_sets (
    id INTEGER PRIMARY KEY,
    position INTEGER NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    bpm REAL,
    quant_index INTEGER NOT NULL DEFAULT 2,
    quant_on INTEGER NOT NULL DEFAULT 1,
    location_dir TEXT,
    location_file TEXT,
    location_volume TEXT,
    location_volume_id TEXT,
    author_type TEXT,
    import_date TEXT,
    raw TEXT,
    dirty INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS sample_slots (
    set_id INTEGER NOT NULL REFERENCES sample_sets(id) ON DELETE CASCADE,
    slot INTEGER NOT NULL,
    keylock INTEGER NOT NULL DEFAULT 0,
    fx_enable INTEGER NOT NULL DEFAULT 1,
    punch INTEGER NOT NULL DEFAULT 0,
    active_cell INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (set_id, slot)
);
CREATE TABLE IF NOT EXISTS sample_cells (
    set_id INTEGER NOT NULL REFERENCES sample_sets(id) ON DELETE CASCADE,
    slot INTEGER NOT NULL,
    idx INTEGER NOT NULL,
    track_id INTEGER REFERENCES tracks(id) ON DELETE SET NULL,
    -- NML primary key of the sample, kept when its track is not in the collection.
    key TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    color INTEGER NOT NULL DEFAULT 0,
    sync INTEGER NOT NULL DEFAULT 1,
    reverse INTEGER NOT NULL DEFAULT 0,
    trigger_type INTEGER NOT NULL DEFAULT 1,
    play_type INTEGER NOT NULL DEFAULT 0,
    speed REAL NOT NULL DEFAULT 1,
    transpose REAL NOT NULL DEFAULT 0,
    offset REAL NOT NULL DEFAULT 0,
    nudge REAL,
    gain REAL NOT NULL DEFAULT 0.5,
    start_marker REAL NOT NULL DEFAULT 0,
    end_marker REAL NOT NULL DEFAULT 0,
    bpm REAL NOT NULL DEFAULT 0,
    PRIMARY KEY (set_id, slot, idx)
);
";

#[derive(Debug, Clone, PartialEq)]
pub struct SampleSetRow {
    pub id: i64,
    pub set: SampleSet,
    pub tracks: Vec<Option<i64>>,
}

impl SampleSetRow {
    pub fn track_of(&self, slot: usize, index: usize) -> Option<i64> {
        self.tracks.get(slot * SET_CELLS + index).copied().flatten()
    }

    pub fn distinct_tracks(&self) -> Vec<i64> {
        let mut seen = std::collections::HashSet::new();
        self.tracks
            .iter()
            .flatten()
            .copied()
            .filter(|t| seen.insert(*t))
            .collect()
    }
}

pub(crate) fn import(
    tx: &Transaction<'_>,
    sets: &[SampleSet],
    elements: &[&Element],
    ids_by_key: &std::collections::HashMap<String, i64>,
) -> Result<()> {
    for (position, (set, element)) in sets.iter().zip(elements).enumerate() {
        let raw = element_text(element);
        let keys = |slot: usize, cell: &SetCell| {
            let _ = slot;
            let key = cell.location.primary_key();
            let id = ids_by_key.get(&key).copied();
            (key, id)
        };
        insert(tx, position as i64, set, Some(&raw), false, &keys)?;
    }
    Ok(())
}

type KeyOf<'a> = &'a dyn Fn(usize, &SetCell) -> (String, Option<i64>);

fn insert(
    tx: &Transaction<'_>,
    position: i64,
    set: &SampleSet,
    raw: Option<&str>,
    dirty: bool,
    key_of: KeyOf<'_>,
) -> Result<i64> {
    let location = set.location.as_ref();
    tx.execute(
        "INSERT INTO sample_sets (position, title, bpm, quant_index, quant_on, location_dir, \
         location_file, location_volume, location_volume_id, author_type, import_date, raw, dirty) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            position,
            set.title,
            set.bpm,
            set.quant_index,
            set.quant_on,
            location.map(|l| l.dir.clone()),
            location.map(|l| l.file.clone()),
            location.map(|l| l.volume.clone()),
            location.and_then(|l| l.volume_id.clone()),
            set.author_type,
            set.import_date,
            raw,
            dirty,
        ],
    )?;
    let id = tx.last_insert_rowid();
    for (slot, s) in set.slots.iter().enumerate() {
        tx.execute(
            "INSERT INTO sample_slots (set_id, slot, keylock, fx_enable, punch, active_cell) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                slot as i64,
                s.keylock,
                s.fx_enable,
                s.punch,
                s.active_cell
            ],
        )?;
        for cell in &s.cells {
            let (key, track) = key_of(slot, cell);
            tx.execute(
                "INSERT OR REPLACE INTO sample_cells (set_id, slot, idx, track_id, key, name, color, \
                 sync, reverse, trigger_type, play_type, speed, transpose, offset, nudge, gain, \
                 start_marker, end_marker, bpm) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
                params![
                    id,
                    slot as i64,
                    cell.index,
                    track,
                    key,
                    cell.name,
                    cell.color,
                    cell.sync,
                    cell.reverse,
                    cell.trigger_type,
                    cell.play_type,
                    cell.speed,
                    cell.transpose,
                    cell.offset,
                    cell.nudge,
                    cell.gain,
                    cell.start_marker,
                    cell.end_marker,
                    cell.bpm,
                ],
            )?;
        }
    }
    Ok(id)
}

fn element_text(element: &Element) -> String {
    let mut out = String::new();
    xml::write_element(element, &mut out);
    out
}

pub(crate) fn rows(conn: &Connection) -> Result<Vec<SampleSetRow>> {
    let ids: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT id FROM sample_sets ORDER BY position, id")?;
        stmt.query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    ids.into_iter()
        .filter_map(|id| row(conn, id).transpose())
        .collect()
}

#[allow(clippy::too_many_lines)]
pub(crate) fn row(conn: &Connection, id: i64) -> Result<Option<SampleSetRow>> {
    let head = conn
        .query_row(
            "SELECT title, bpm, quant_index, quant_on, location_dir, location_file, \
             location_volume, location_volume_id, author_type, import_date FROM sample_sets WHERE id = ?1",
            [id],
            |r| {
                let dir: Option<String> = r.get(4)?;
                let location = dir.map(|dir| Location {
                    dir,
                    file: r.get::<_, Option<String>>(5).ok().flatten().unwrap_or_default(),
                    volume: r.get::<_, Option<String>>(6).ok().flatten().unwrap_or_default(),
                    volume_id: r.get(7).ok().flatten(),
                });
                Ok(SampleSet {
                    title: r.get(0)?,
                    bpm: r.get(1)?,
                    quant_index: r.get(2)?,
                    quant_on: r.get(3)?,
                    location,
                    author_type: r.get(8)?,
                    import_date: r.get(9)?,
                    slots: Vec::new(),
                })
            },
        )
        .optional()?;
    let Some(mut set) = head else {
        return Ok(None);
    };
    let mut tracks = vec![None; SET_SLOTS * SET_CELLS];
    let mut slots: Vec<SetSlot> = (0..SET_SLOTS).map(|_| SetSlot::default()).collect();
    {
        let mut stmt = conn.prepare(
            "SELECT slot, keylock, fx_enable, punch, active_cell FROM sample_slots WHERE set_id = ?1",
        )?;
        let rows = stmt.query_map([id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                SetSlot {
                    keylock: r.get(1)?,
                    fx_enable: r.get(2)?,
                    punch: r.get(3)?,
                    active_cell: r.get(4)?,
                    cells: Vec::new(),
                },
            ))
        })?;
        for row in rows {
            let (slot, s) = row?;
            if let Some(target) = usize::try_from(slot).ok().and_then(|i| slots.get_mut(i)) {
                *target = s;
            }
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT c.slot, c.idx, c.track_id, c.key, c.name, c.color, c.sync, c.reverse, \
             c.trigger_type, c.play_type, c.speed, c.transpose, c.offset, c.nudge, c.gain, \
             c.start_marker, c.end_marker, c.bpm, t.path FROM sample_cells c \
             LEFT JOIN tracks t ON t.id = c.track_id WHERE c.set_id = ?1 ORDER BY c.slot, c.idx",
        )?;
        let startup = paths::startup_volume();
        let rows = stmt.query_map([id], |r| {
            let key: String = r.get(3)?;
            let path: Option<String> = r.get(18)?;
            let location = path.map_or_else(
                || location_of_key(&key),
                |p| {
                    let p = p.split('#').next().unwrap_or(&p).to_owned();
                    let mut l = paths::location_of(std::path::Path::new(&p), &startup);
                    l.volume_id = None;
                    l
                },
            );
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(2)?,
                SetCell {
                    index: r.get(1)?,
                    name: r.get(4)?,
                    color: r.get(5)?,
                    sync: r.get(6)?,
                    reverse: r.get(7)?,
                    trigger_type: r.get(8)?,
                    play_type: r.get(9)?,
                    speed: r.get(10)?,
                    transpose: r.get(11)?,
                    offset: r.get(12)?,
                    nudge: r.get(13)?,
                    gain: r.get(14)?,
                    start_marker: r.get(15)?,
                    end_marker: r.get(16)?,
                    bpm: r.get(17)?,
                    location,
                },
            ))
        })?;
        for row in rows {
            let (slot, track, cell) = row?;
            let Some(slot) = usize::try_from(slot).ok().filter(|s| *s < SET_SLOTS) else {
                continue;
            };
            if let Some(t) = tracks.get_mut(slot * SET_CELLS + cell.index as usize) {
                *t = track;
            }
            slots[slot].cells.push(cell);
        }
    }
    set.slots = slots;
    Ok(Some(SampleSetRow { id, set, tracks }))
}

fn location_of_key(key: &str) -> Location {
    let Some(start) = key.find("/:") else {
        return Location {
            file: key.to_owned(),
            ..Location::default()
        };
    };
    let volume = key[..start].to_owned();
    let rest = &key[start..];
    let cut = rest.rfind("/:").map_or(0, |i| i + 2);
    Location {
        dir: rest[..cut].to_owned(),
        file: rest[cut..].to_owned(),
        volume,
        volume_id: None,
    }
}

impl Library {
    pub fn sample_sets(&self) -> Result<Vec<SampleSetRow>> {
        rows(&self.conn)
    }

    pub fn sample_set(&self, id: i64) -> Result<Option<SampleSetRow>> {
        row(&self.conn, id)
    }

    pub fn save_sample_set(
        &mut self,
        id: Option<i64>,
        set: &SampleSet,
        tracks: &[Option<i64>],
    ) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let position = match id {
            Some(id) => {
                let position: Option<i64> = tx
                    .query_row(
                        "SELECT position FROM sample_sets WHERE id = ?1",
                        [id],
                        |r| r.get(0),
                    )
                    .optional()?;
                tx.execute("DELETE FROM sample_sets WHERE id = ?1", [id])?;
                position
            }
            None => None,
        };
        let position = match position {
            Some(p) => p,
            None => tx.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM sample_sets",
                [],
                |r| r.get(0),
            )?,
        };
        let startup = paths::startup_volume();
        let key_of = |slot: usize, cell: &SetCell| {
            let track = tracks
                .get(slot * SET_CELLS + cell.index as usize)
                .copied()
                .flatten();
            let path: Option<String> = track.and_then(|t| {
                tx.query_row("SELECT path FROM tracks WHERE id = ?1", [t], |r| r.get(0))
                    .optional()
                    .ok()
                    .flatten()
            });
            match path {
                Some(p) => {
                    let p = p.split('#').next().unwrap_or(&p).to_owned();
                    let location = paths::location_of(std::path::Path::new(&p), &startup);
                    (location.primary_key(), track)
                }
                None => (cell.location.primary_key(), None),
            }
        };
        let new_id = insert(&tx, position, set, None, true, &key_of)?;
        tx.commit()?;
        Ok(new_id)
    }

    pub fn delete_sample_set(&mut self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM sample_sets WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn sample_set_by_title(&self, title: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM sample_sets WHERE title = ?1 ORDER BY position LIMIT 1",
                [title],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn set_sample_type(&mut self, id: i64, sample_type: Option<i32>) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET sample_type = ?2, dirty = 1 WHERE id = ?1",
            params![id, sample_type],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_tables_are_renamed_with_their_rows() {
        let conn = Connection::open_in_memory().unwrap();
        let old = SCHEMA.replace("sample_", "remix_");
        conn.execute_batch(&old).unwrap();
        conn.execute_batch(
            "INSERT INTO remix_sets (position, title, bpm, quant_index, quant_on) \
             VALUES (0, 'Kept', NULL, 2, 1)",
        )
        .unwrap();
        rename_old_tables(&conn).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        let title: String = conn
            .query_row("SELECT title FROM sample_sets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "Kept");
    }

    #[test]
    fn keys_split_into_locations() {
        let l = location_of_key("USB/:Samples/:Kit A/:KICK01.WAV");
        assert_eq!(
            (l.volume.as_str(), l.dir.as_str(), l.file.as_str()),
            ("USB", "/:Samples/:Kit A/:", "KICK01.WAV")
        );
        let l = location_of_key("x.wav");
        assert_eq!(l.file, "x.wav");
    }

    #[test]
    fn an_opened_set_lists_each_track_once() {
        let row = SampleSetRow {
            id: 1,
            set: SampleSet::default(),
            tracks: vec![Some(7), None, Some(3), Some(7), None, Some(9)],
        };
        assert_eq!(row.distinct_tracks(), [7, 3, 9]);
    }
}
