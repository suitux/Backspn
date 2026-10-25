use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use backspn_formats::nml::{Nml, Node as NmlNode};
use backspn_formats::xml::{self, Element};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{ImportReport, Result, paths};

pub fn import(conn: &mut Connection, nml: &Nml) -> Result<ImportReport> {
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DELETE FROM playlist_entries; DELETE FROM nodes; DELETE FROM cues; DELETE FROM transients; \
         DELETE FROM tracks; DELETE FROM meta;",
    )?;

    let root = &nml.document.root;
    let layout: Vec<&str> = root.children.iter().map(|c| c.name.as_str()).collect();
    set_meta(&tx, "layout", &layout.join("\n"))?;
    for (i, child) in root.children.iter().enumerate() {
        let raw = match child.name.as_str() {
            "COLLECTION" | "PLAYLISTS" | "SETS" => shell(child),
            _ => element_text(child),
        };
        set_meta(&tx, &format!("child:{i}"), &raw)?;
    }
    tx.execute_batch("DELETE FROM sample_sets;")?;

    let mut report = ImportReport::default();
    let mut ids_by_key = HashMap::new();
    let elements = root
        .child("COLLECTION")
        .map(|c| c.children_named("ENTRY").collect::<Vec<_>>())
        .unwrap_or_default();
    for (entry, element) in nml.entries.iter().zip(elements) {
        let mut path = paths::local_path(&entry.location)
            .to_string_lossy()
            .into_owned();
        let taken: bool = tx
            .query_row("SELECT 1 FROM tracks WHERE path = ?1", [&path], |_| {
                Ok(true)
            })
            .optional()?
            .is_some();
        if taken {
            path = format!("{path}#{}", report.tracks);
        }
        let (id, cues) = insert_entry(&tx, entry, element, &path)?;
        ids_by_key.entry(entry.location.primary_key()).or_insert(id);
        report.cues += cues;
        report.tracks += 1;
    }

    if let Some(NmlNode::Folder { children, .. }) = &nml.playlists {
        insert_nodes(&tx, None, children, &ids_by_key, &mut report)?;
    }
    let set_elements: Vec<&Element> = root
        .child("SETS")
        .map(|c| c.children_named("SET").collect())
        .unwrap_or_default();
    crate::sample_deck::import(&tx, &nml.sets, &set_elements, &ids_by_key)?;
    tx.commit()?;
    Ok(report)
}

fn insert_entry(
    tx: &Transaction<'_>,
    entry: &backspn_formats::nml::Entry,
    element: &Element,
    path: &str,
) -> Result<(i64, usize)> {
    let grid = entry.grid_cue();
    tx.execute(
        "INSERT INTO tracks (path, title, artist, album, genre, label, comment, key_text, musical_key, \
         bpm, grid_anchor_s, duration_s, rating, playcount, import_date, lufs, analyzed, locked, nml_raw, dirty) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, NULL, ?16, ?17, ?18, 0)",
        params![
            path,
            entry.title.as_deref().unwrap_or(""),
            entry.artist.as_deref().unwrap_or(""),
            entry.album.as_ref().and_then(|a| a.title.as_deref()).unwrap_or(""),
            entry.info.genre.as_deref().unwrap_or(""),
            entry.info.label.as_deref().unwrap_or(""),
            entry.info.comment.as_deref().unwrap_or(""),
            entry.info.key_text.as_deref().unwrap_or(""),
            entry.musical_key.map(backspn_formats::nml::MusicalKey::value),
            entry.tempo.and_then(|t| t.bpm),
            grid.map(|g| g.start_ms / 1000.0),
            entry.info.playtime_float.or(entry.info.playtime.map(f64::from)),
            entry.info.ranking.unwrap_or(0),
            entry.info.playcount.unwrap_or(0),
            entry.info.import_date.as_deref().unwrap_or(""),
            grid.is_some() && entry.tempo.and_then(|t| t.bpm).is_some(),
            entry.locked,
            element_text(element),
        ],
    )?;
    let id = tx.last_insert_rowid();
    store_extras(tx, id, element)?;
    for (position, cue) in entry.cues.iter().enumerate() {
        tx.execute(
            "INSERT INTO cues (track_id, position, kind, name, start_s, len_s, hotcue, display_order, repeats, color) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                position as i64,
                i32::from(cue.kind),
                cue.name,
                cue.start_ms / 1000.0,
                cue.len_ms / 1000.0,
                cue.hotcue.map_or(-1, i64::from),
                cue.display_order,
                cue.repeats,
                cue.color,
            ],
        )?;
    }
    Ok((id, entry.cues.len()))
}

pub fn merge(conn: &mut Connection, nml: &Nml, parent: Option<i64>) -> Result<ImportReport> {
    let tx = conn.transaction()?;
    let mut report = ImportReport::default();
    let mut ids_by_key = HashMap::new();
    let elements = nml
        .document
        .root
        .child("COLLECTION")
        .map(|c| c.children_named("ENTRY").collect::<Vec<_>>())
        .unwrap_or_default();
    for (entry, element) in nml.entries.iter().zip(elements) {
        let path = paths::local_path(&entry.location)
            .to_string_lossy()
            .into_owned();
        let existing: Option<i64> = tx
            .query_row("SELECT id FROM tracks WHERE path = ?1", [&path], |r| {
                r.get(0)
            })
            .optional()?;
        let id = if let Some(id) = existing {
            id
        } else {
            let (id, cues) = insert_entry(&tx, entry, element, &path)?;
            report.tracks += 1;
            report.cues += cues;
            id
        };
        ids_by_key.entry(entry.location.primary_key()).or_insert(id);
    }
    if let Some(NmlNode::Folder { children, .. }) = &nml.playlists {
        let first: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM nodes WHERE parent IS ?1",
            [parent],
            |r| r.get(0),
        )?;
        let start: i64 =
            tx.query_row("SELECT COALESCE(MAX(id), 0) FROM nodes", [], |r| r.get(0))?;
        insert_nodes(&tx, parent, children, &ids_by_key, &mut report)?;
        let added: Vec<(i64, String)> = {
            let mut stmt = tx.prepare(
                "SELECT id, name FROM nodes WHERE parent IS ?1 AND id > ?2 ORDER BY position, id",
            )?;
            let rows = stmt.query_map(params![parent, start], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for (i, (id, name)) in added.into_iter().enumerate() {
            let taken: Vec<String> = {
                let mut stmt =
                    tx.prepare("SELECT name FROM nodes WHERE parent IS ?1 AND id <> ?2")?;
                let rows = stmt.query_map(params![parent, id], |r| r.get(0))?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            let name = crate::playlists::unique_name(
                &name,
                &taken.iter().map(String::as_str).collect::<Vec<_>>(),
            );
            tx.execute(
                "UPDATE nodes SET name = ?2, position = ?3 WHERE id = ?1",
                params![id, name, first + i as i64],
            )?;
        }
    }
    tx.commit()?;
    Ok(report)
}

type HistoryRow = (String, i64, i64, i64, f64, bool);

fn history_rows(e: &Element, rows: &mut Vec<HistoryRow>) {
    if e.name == "PLAYLIST" && e.attr("TYPE") == Some("PROTOCOL") {
        for entry in e.children_named("ENTRY") {
            let key = entry
                .child("PRIMARYKEY")
                .and_then(|k| k.attr("KEY"))
                .unwrap_or_default()
                .to_owned();
            let data = entry.child("EXTENDEDDATA");
            let num = |k: &str| {
                data.and_then(|d| d.attr(k))
                    .and_then(|v| v.parse::<f64>().ok())
            };
            rows.push((
                key,
                num("DECK").unwrap_or(0.0) as i64,
                num("STARTDATE").unwrap_or(0.0) as i64,
                num("STARTTIME").unwrap_or(0.0) as i64,
                num("DURATION").unwrap_or(0.0),
                data.and_then(|d| d.attr("PLAYEDPUBLIC")) != Some("0"),
            ));
        }
    }
    for c in &e.children {
        history_rows(c, rows);
    }
}

pub fn import_history(conn: &mut Connection, nml: &Nml, name: &str) -> Result<(i64, usize)> {
    let tx = conn.transaction()?;
    let elements = nml
        .document
        .root
        .child("COLLECTION")
        .map(|c| c.children_named("ENTRY").collect::<Vec<_>>())
        .unwrap_or_default();
    let mut ids_by_key = HashMap::new();
    for (entry, element) in nml.entries.iter().zip(elements) {
        if entry.location.volume.is_empty() {
            continue;
        }
        let path = paths::local_path(&entry.location)
            .to_string_lossy()
            .into_owned();
        let existing: Option<i64> = tx
            .query_row("SELECT id FROM tracks WHERE path = ?1", [&path], |r| {
                r.get(0)
            })
            .optional()?;
        let id = match existing {
            Some(id) => id,
            None => insert_entry(&tx, entry, element, &path)?.0,
        };
        ids_by_key.insert(entry.location.primary_key(), id);
    }
    let mut rows: Vec<HistoryRow> = Vec::new();
    history_rows(&nml.document.root, &mut rows);
    tx.execute("INSERT INTO history_sessions (name) VALUES (?1)", [name])?;
    let session = tx.last_insert_rowid();
    for (position, (key, deck, date, time, duration, public)) in rows.iter().enumerate() {
        tx.execute(
            "INSERT INTO history_entries (session_id, position, track_id, deck, start_date, start_time, \
             duration, played_public) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![session, position as i64, ids_by_key.get(key), deck, date, time, duration, public],
        )?;
    }
    tx.commit()?;
    Ok((session, rows.len()))
}

fn insert_nodes(
    tx: &Transaction<'_>,
    parent: Option<i64>,
    nodes: &[NmlNode],
    ids_by_key: &HashMap<String, i64>,
    report: &mut ImportReport,
) -> Result<()> {
    for (position, node) in nodes.iter().enumerate() {
        let (kind, name, uuid, query) = match node {
            NmlNode::Folder { name, .. } | NmlNode::Other { name, .. } => {
                ("folder", name, None, None)
            }
            NmlNode::Playlist { name, uuid, .. } => ("playlist", name, uuid.as_deref(), None),
            NmlNode::Smartlist { name, uuid, query } => {
                ("smartlist", name, uuid.as_deref(), query.as_deref())
            }
        };
        tx.execute(
            "INSERT INTO nodes (parent, position, kind, name, uuid, query) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![parent, position as i64, kind, name, uuid, query],
        )?;
        let id = tx.last_insert_rowid();
        match node {
            NmlNode::Folder { children, .. } => {
                report.folders += 1;
                insert_nodes(tx, Some(id), children, ids_by_key, report)?;
            }
            NmlNode::Playlist { entries, .. } => {
                report.playlists += 1;
                for (i, key) in entries.iter().enumerate() {
                    let track = ids_by_key.get(key);
                    if track.is_none() {
                        report.missing_playlist_tracks += 1;
                    }
                    tx.execute(
                        "INSERT INTO playlist_entries (node_id, position, track_id, key) VALUES (?1, ?2, ?3, ?4)",
                        params![id, i as i64, track, key],
                    )?;
                }
            }
            NmlNode::Smartlist { .. } => report.smartlists += 1,
            NmlNode::Other { .. } => report.folders += 1,
        }
    }
    Ok(())
}

fn set_meta(tx: &Transaction<'_>, key: &str, value: &str) -> Result<()> {
    tx.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

fn indexing_key(conn: &Connection) -> Result<String> {
    let layout =
        get_meta(conn, "layout")?.unwrap_or_else(|| "HEAD\nCOLLECTION\nPLAYLISTS".to_owned());
    let mut names: Vec<&str> = layout.lines().collect();
    let at = names
        .iter()
        .position(|n| *n == "INDEXING")
        .unwrap_or_else(|| {
            names.push("INDEXING");
            names.len() - 1
        });
    let tx = conn.unchecked_transaction()?;
    set_meta(&tx, "layout", &names.join("\n"))?;
    let key = format!("child:{at}");
    if get_meta(&tx, &key)?.is_none() {
        set_meta(&tx, &key, &element_text(&Element::new("INDEXING", &[])))?;
    }
    tx.commit()?;
    Ok(key)
}

pub fn sorting(conn: &Connection, path: &str) -> Result<Vec<(u32, bool)>> {
    let layout = get_meta(conn, "layout")?.unwrap_or_default();
    let Some(at) = layout.lines().position(|n| n == "INDEXING") else {
        return Ok(Vec::new());
    };
    let Some(raw) = get_meta(conn, &format!("child:{at}"))? else {
        return Ok(Vec::new());
    };
    let indexing = parse_element(&raw)?;
    Ok(indexing
        .children_named("SORTING_INFO")
        .find(|i| i.attr("PATH") == Some(path))
        .map(|i| {
            i.children_named("CRITERIA")
                .filter_map(|c| {
                    let attribute = c.attr("ATTRIBUTE")?.parse().ok()?;
                    Some((attribute, c.attr("DIRECTION") == Some("1")))
                })
                .collect()
        })
        .unwrap_or_default())
}

pub fn set_sorting(conn: &Connection, path: &str, sorting: &[(u32, bool)]) -> Result<()> {
    let key = indexing_key(conn)?;
    let raw = get_meta(conn, &key)?.unwrap_or_default();
    let mut indexing = parse_element(&raw)?;
    let mut info = Element::new("SORTING_INFO", &[("PATH", path.to_owned())]);
    for &(attribute, descending) in sorting {
        info.children.push(Element::new(
            "CRITERIA",
            &[
                ("ATTRIBUTE", attribute.to_string()),
                ("DIRECTION", if descending { "1" } else { "0" }.to_owned()),
            ],
        ));
    }
    match indexing
        .children
        .iter_mut()
        .find(|c| c.name == "SORTING_INFO" && c.attr("PATH") == Some(path))
    {
        Some(existing) => *existing = info,
        None => indexing.children.push(info),
    }
    let tx = conn.unchecked_transaction()?;
    set_meta(&tx, &key, &element_text(&indexing))?;
    tx.commit()?;
    Ok(())
}

fn shell(element: &Element) -> String {
    let mut empty = element.clone();
    empty.children.clear();
    empty.text = None;
    element_text(&empty)
}

fn store_extras(conn: &Connection, id: i64, entry: &Element) -> Result<()> {
    let info = entry.child("INFO");
    let text = |key: &str| info.and_then(|i| i.attr(key)).unwrap_or("").to_owned();
    let number = |key: &str| {
        info.and_then(|i| i.attr(key))
            .and_then(|v| v.parse::<i64>().ok())
    };
    let track = entry
        .child("ALBUM")
        .and_then(|a| a.attr("TRACK"))
        .and_then(|v| v.parse::<i64>().ok());
    let sample_type = entry
        .child("LOOPINFO")
        .and_then(|l| l.attr("SAMPLE_TYPE_INFO"))
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|t| *t >= 0);
    conn.execute(
        "UPDATE tracks SET remixer = ?2, producer = ?3, mix = ?4, catalog_no = ?5, release_date = ?6, \
         last_played = ?7, comment2 = ?8, bitrate = ?9, track_number = ?10, filesize_kib = ?11, \
         color = ?12, modified_date = ?13, modified_time = ?14, sample_type = ?15 WHERE id = ?1",
        params![
            id,
            text("REMIXER"),
            text("PRODUCER"),
            text("MIX"),
            text("CATALOG_NO"),
            text("RELEASE_DATE"),
            text("LAST_PLAYED"),
            text("RATING"),
            number("BITRATE"),
            track,
            number("FILESIZE"),
            number("COLOR").unwrap_or(0),
            entry.attr("MODIFIED_DATE").unwrap_or(""),
            entry
                .attr("MODIFIED_TIME")
                .and_then(|v| v.parse::<i64>().ok()),
            sample_type,
        ],
    )?;
    Ok(())
}

pub fn backfill_extras(conn: &Connection) -> Result<()> {
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn.prepare("SELECT id, nml_raw FROM tracks WHERE nml_raw IS NOT NULL")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let tx = conn.unchecked_transaction()?;
    for (id, raw) in rows {
        if let Ok(entry) = parse_element(&raw) {
            store_extras(&tx, id, &entry)?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn element_text(element: &Element) -> String {
    let mut out = String::new();
    xml::write_element(element, &mut out);
    out
}

fn parse_element(text: &str) -> Result<Element> {
    Ok(xml::parse(text)?.root)
}

pub fn primary_key_of(tx: &Transaction<'_>, track: i64) -> Result<String> {
    let path: String = tx.query_row("SELECT path FROM tracks WHERE id = ?1", [track], |r| {
        r.get(0)
    })?;
    let path = path.split('#').next().unwrap_or(&path).to_owned();
    Ok(paths::location_of(Path::new(&path), &paths::startup_volume()).primary_key())
}

pub fn new_uuid() -> String {
    let a = RandomState::new().hash_one(SystemTime::now());
    let b = RandomState::new().hash_one(a);
    format!("{a:016x}{b:016x}")
}

pub fn today() -> String {
    now_date_time().0
}

fn now_date_time() -> (String, u64) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    (format!("{y}/{m}/{d}"), secs % 86_400)
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_531), (2026, 3, 19));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn uuids_are_hex_and_differ() {
        let (a, b) = (new_uuid(), new_uuid());
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
