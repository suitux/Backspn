use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use backspn_formats::nml::MusicalKey;
use rusqlite::{Connection, OpenFlags, types::Value};

use crate::CuePoint;
use crate::external::{ExternalLibrary, ExternalNode, ExternalTrack, Grid};

const FOLDER: &str = "Engine Library";
const DATABASE: &str = "Database2/m.db";
const EMPTY_OFFSET: f64 = -1.0;
const SLOTS: usize = 8;
const MAX_EXTRA_MARKERS: usize = 7;
const MAX_BLOB: u64 = 64 << 20;

pub fn find() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let mut out: Vec<PathBuf> = home
        .map(|h| h.join("Music").join(FOLDER))
        .into_iter()
        .chain(super::drives().into_iter().map(|d| d.join(FOLDER)))
        .filter(|root| root.join(DATABASE).is_file())
        .collect();
    out.dedup();
    out
}

pub fn read(root: &Path) -> crate::Result<ExternalLibrary> {
    let conn = Connection::open_with_flags(
        root.join(DATABASE),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    read_db(&conn, root)
}

fn read_db(conn: &Connection, root: &Path) -> crate::Result<ExternalLibrary> {
    let (ids, mut tracks) = read_tracks(conn, root)?;
    let index: HashMap<i64, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    if has_table(conn, "PerformanceData")? {
        read_performance(conn, &index, &mut tracks)?;
    }
    let playlists = read_playlists(conn, &index)?;
    Ok(ExternalLibrary { tracks, playlists })
}

fn has_table(conn: &Connection, name: &str) -> crate::Result<bool> {
    Ok(conn
        .prepare("SELECT 1 FROM sqlite_master WHERE name = ?1 AND type IN ('table', 'view')")?
        .exists([name])?)
}

fn int(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i),
        #[allow(clippy::cast_possible_truncation)]
        Value::Real(r) => Some(*r as i64),
        Value::Text(t) => t.trim().parse().ok(),
        _ => None,
    }
}

fn real(v: &Value) -> Option<f64> {
    match v {
        #[allow(clippy::cast_precision_loss)]
        Value::Integer(i) => Some(*i as f64),
        Value::Real(r) => Some(*r),
        Value::Text(t) => t.trim().parse().ok(),
        _ => None,
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::Text(t) => t.clone(),
        _ => String::new(),
    }
}

fn blob(v: &Value) -> &[u8] {
    match v {
        Value::Blob(b) => b,
        _ => &[],
    }
}

fn read_tracks(conn: &Connection, root: &Path) -> crate::Result<(Vec<i64>, Vec<ExternalTrack>)> {
    let mut stmt = conn.prepare(
        "SELECT id, path, title, artist, album, genre, comment, label, composer, remixer, \
         bpm, bpmAnalyzed, length, bitrate, fileBytes, rating, dateAdded, timeLastPlayed, key \
         FROM Track ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        (0..19)
            .map(|i| r.get::<_, Value>(i))
            .collect::<rusqlite::Result<Vec<Value>>>()
    })?;
    let (mut ids, mut tracks) = (Vec::new(), Vec::new());
    for row in rows {
        let v = row?;
        let (Some(id), path) = (int(&v[0]), text(&v[1])) else {
            continue;
        };
        if path.trim().is_empty() {
            continue;
        }
        let key = int(&v[18])
            .and_then(key_value)
            .and_then(MusicalKey::from_value);
        let bpm = real(&v[11])
            .filter(|b| *b > 0.0)
            .or_else(|| real(&v[10]).filter(|b| *b > 0.0));
        ids.push(id);
        tracks.push(ExternalTrack {
            path: absolute(root, &path),
            title: text(&v[2]),
            artist: text(&v[3]),
            album: text(&v[4]),
            genre: text(&v[5]),
            comment: text(&v[6]),
            label: text(&v[7]),
            producer: text(&v[8]),
            remixer: text(&v[9]),
            key_text: key.map(MusicalKey::open_key).unwrap_or_default(),
            musical_key: key.map(MusicalKey::value),
            bpm,
            duration_s: real(&v[12]).filter(|l| *l > 0.0),
            bitrate: int(&v[13]).filter(|b| *b > 0).map(|b| b * 1000),
            filesize_kib: int(&v[14]).filter(|b| *b > 0).map(|b| b / 1024),
            rating: rating(int(&v[15]).unwrap_or(0)),
            import_date: date(int(&v[16])),
            last_played: date(int(&v[17])),
            ..ExternalTrack::default()
        });
    }
    Ok((ids, tracks))
}

fn absolute(root: &Path, path: &str) -> String {
    let p = Path::new(path);
    let is_drive = path.len() >= 2 && path.as_bytes()[1] == b':';
    if p.is_absolute() || path.starts_with('/') || is_drive {
        return path.to_owned();
    }
    let mut out = PathBuf::new();
    for c in root.join(p).components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out.to_string_lossy().into_owned()
}

pub fn key_value(engine: i64) -> Option<u8> {
    let k = u8::try_from(engine).ok().filter(|k| *k < 24)?;
    let major = (k / 2) * 7 % 12;
    Some(if k % 2 == 0 {
        major
    } else {
        12 + (major + 9) % 12
    })
}

fn rating(engine: i64) -> i64 {
    (engine.clamp(0, 100) * 255 + 50) / 100
}

fn date(secs: Option<i64>) -> String {
    secs.and_then(|s| u64::try_from(s).ok())
        .filter(|s| *s > 0)
        .map(|s| crate::clock::LocalTime::from_unix(s).date_text())
        .unwrap_or_default()
}

fn read_performance(
    conn: &Connection,
    index: &HashMap<i64, usize>,
    tracks: &mut [ExternalTrack],
) -> crate::Result<()> {
    let mut stmt =
        conn.prepare("SELECT trackId, trackData, beatData, quickCues, loops FROM PerformanceData")?;
    let rows = stmt.query_map([], |r| {
        (0..5)
            .map(|i| r.get::<_, Value>(i))
            .collect::<rusqlite::Result<Vec<Value>>>()
    })?;
    for row in rows {
        let v = row?;
        let Some(&t) = int(&v[0]).and_then(|id| index.get(&id)) else {
            continue;
        };
        let beats = uncompress(blob(&v[2])).and_then(|d| beat_data(&d));
        let rate = beats
            .as_ref()
            .map(|b| b.sample_rate)
            .filter(|r| *r > 0.0)
            .or_else(|| uncompress(blob(&v[1])).and_then(|d| track_data_rate(&d)));
        let Some(rate) = rate.filter(|r| *r > 0.0) else {
            continue;
        };
        let track = &mut tracks[t];
        track.grid = beats.and_then(|b| grid(rate, b.markers()));
        let quick = uncompress(blob(&v[3])).and_then(|d| quick_cues(&d));
        let loops = loops(blob(&v[4])).unwrap_or_default();
        track.cues = cues(rate, quick.as_ref(), &loops);
    }
    Ok(())
}

pub fn uncompress(data: &[u8]) -> Option<Vec<u8>> {
    let size = u32::from_be_bytes(data.get(..4)?.try_into().ok()?);
    if size == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(usize::try_from(size).ok()?.min(1 << 20));
    flate2::read::ZlibDecoder::new(&data[4..])
        .take(MAX_BLOB)
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

struct Cursor<'a>(&'a [u8]);

impl Cursor<'_> {
    fn bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_at_checked(N)?;
        self.0 = rest;
        head.try_into().ok()
    }
    fn u8(&mut self) -> Option<u8> {
        self.bytes::<1>().map(|[b]| b)
    }
    fn f64_be(&mut self) -> Option<f64> {
        self.bytes().map(f64::from_be_bytes)
    }
    fn f64_le(&mut self) -> Option<f64> {
        self.bytes().map(f64::from_le_bytes)
    }
    fn i64_be(&mut self) -> Option<i64> {
        self.bytes().map(i64::from_be_bytes)
    }
    fn i64_le(&mut self) -> Option<i64> {
        self.bytes().map(i64::from_le_bytes)
    }
    fn i32_le(&mut self) -> Option<i32> {
        self.bytes().map(i32::from_le_bytes)
    }
    fn label(&mut self) -> Option<String> {
        let len = usize::from(self.u8()?);
        let (head, rest) = self.0.split_at_checked(len)?;
        self.0 = rest;
        Some(String::from_utf8_lossy(head).into_owned())
    }
    fn argb(&mut self) -> Option<[u8; 4]> {
        self.bytes()
    }
}

fn track_data_rate(data: &[u8]) -> Option<f64> {
    Cursor(data).f64_be()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Marker {
    pub sample: f64,
    pub beat: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BeatData {
    pub sample_rate: f64,
    pub default_grid: Vec<Marker>,
    pub adjusted_grid: Vec<Marker>,
}

impl BeatData {
    fn markers(&self) -> &[Marker] {
        if self.adjusted_grid.is_empty() {
            &self.default_grid
        } else {
            &self.adjusted_grid
        }
    }
}

pub fn beat_data(data: &[u8]) -> Option<BeatData> {
    let mut c = Cursor(data);
    let sample_rate = c.f64_be()?;
    c.f64_be()?;
    c.u8()?;
    let mut grid = || -> Option<Vec<Marker>> {
        let count = usize::try_from(c.i64_be()?).ok()?;
        if count > c.0.len() / 24 {
            return None;
        }
        (0..count)
            .map(|_| {
                let sample = c.f64_le()?;
                let beat = c.i64_le()?;
                c.i32_le()?;
                c.i32_le()?;
                Some(Marker { sample, beat })
            })
            .collect()
    };
    let default_grid = grid()?;
    let adjusted_grid = grid()?;
    Some(BeatData {
        sample_rate,
        default_grid,
        adjusted_grid,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuickCue {
    pub label: String,
    pub sample: f64,
    pub argb: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuickCues {
    pub cues: Vec<QuickCue>,
    pub main_cue: f64,
}

pub fn quick_cues(data: &[u8]) -> Option<QuickCues> {
    let mut c = Cursor(data);
    let count = usize::try_from(c.i64_be()?).ok()?;
    if count > c.0.len() / 13 {
        return None;
    }
    let cues = (0..count)
        .map(|_| {
            Some(QuickCue {
                label: c.label()?,
                sample: c.f64_be()?,
                argb: c.argb()?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let adjusted = c.f64_be()?;
    let is_adjusted = c.u8()? != 0;
    let default = c.f64_be()?;
    Some(QuickCues {
        cues,
        main_cue: if is_adjusted { adjusted } else { default },
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Loop {
    pub label: String,
    pub start: f64,
    pub end: f64,
    pub set: bool,
    pub argb: [u8; 4],
}

pub fn loops(data: &[u8]) -> Option<Vec<Loop>> {
    if data.is_empty() {
        return Some(Vec::new());
    }
    let mut c = Cursor(data);
    let count = usize::try_from(c.i64_le()?).ok()?;
    if count > c.0.len() / 23 {
        return None;
    }
    (0..count)
        .map(|_| {
            let label = c.label()?;
            let start = c.f64_le()?;
            let end = c.f64_le()?;
            let set = c.u8()? != 0 && c.u8()? != 0;
            Some(Loop {
                label,
                start,
                end,
                set,
                argb: c.argb()?,
            })
        })
        .collect()
}

fn rgb([_, r, g, b]: [u8; 4]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

pub fn cues(rate: f64, quick: Option<&QuickCues>, loops: &[Loop]) -> Vec<CuePoint> {
    let mut out = Vec::new();
    let cue = |kind, name: &str, start_s, len_s, hotcue, color| CuePoint {
        kind,
        name: name.to_owned(),
        start_s,
        len_s,
        hotcue,
        display_order: 0,
        repeats: -1,
        color,
    };
    if let Some(q) = quick {
        for (slot, c) in q.cues.iter().take(SLOTS).enumerate() {
            if c.sample >= 0.0 && c.sample != EMPTY_OFFSET {
                let slot = u8::try_from(slot).ok();
                out.push(cue(
                    CuePoint::CUE,
                    &c.label,
                    c.sample / rate,
                    0.0,
                    slot,
                    Some(rgb(c.argb)),
                ));
            }
        }
        if q.main_cue > 0.0 {
            out.push(cue(CuePoint::CUE, "", q.main_cue / rate, 0.0, None, None));
        }
    }
    for l in loops
        .iter()
        .filter(|l| l.set && l.start >= 0.0 && l.end > l.start)
    {
        out.push(cue(
            CuePoint::LOOP,
            &l.label,
            l.start / rate,
            (l.end - l.start) / rate,
            None,
            Some(rgb(l.argb)),
        ));
    }
    out
}

pub fn grid(rate: f64, markers: &[Marker]) -> Option<Grid> {
    #[allow(clippy::cast_precision_loss)]
    let stretches: Vec<(f64, f64)> = markers
        .windows(2)
        .filter_map(|w| {
            let beats = (w[1].beat - w[0].beat) as f64;
            let secs = (w[1].sample - w[0].sample) / rate;
            (beats > 0.0 && secs > 0.0).then(|| (w[0].sample / rate, 60.0 * beats / secs))
        })
        .collect();
    let &(start, bpm) = stretches.first()?;
    let beat = 60.0 / bpm;
    let anchor_s = if start < 0.0 {
        start + (-start / beat).ceil() * beat
    } else {
        start
    };
    let mut current = bpm;
    let mut markers_s = Vec::new();
    for &(at, b) in &stretches[1..] {
        if (b - current).abs() >= 0.005 && markers_s.len() < MAX_EXTRA_MARKERS {
            markers_s.push(at);
            current = b;
        }
    }
    Some(Grid {
        bpm,
        anchor_s,
        markers_s,
    })
}

pub fn linked_order(items: &[(i64, i64)]) -> Vec<i64> {
    let next: HashMap<i64, i64> = items.iter().copied().collect();
    let pointed: HashSet<i64> = items.iter().map(|(_, n)| *n).collect();
    let mut ids: Vec<i64> = items.iter().map(|(i, _)| *i).collect();
    ids.sort_unstable();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let heads: Vec<i64> = ids
        .iter()
        .copied()
        .filter(|i| !pointed.contains(i))
        .collect();
    for start in heads.into_iter().chain(ids.iter().copied()) {
        let mut at = start;
        while next.contains_key(&at) && seen.insert(at) {
            out.push(at);
            at = next[&at];
        }
    }
    out
}

struct List {
    title: String,
    parent: i64,
    next: i64,
    persisted: bool,
}

fn read_playlists(
    conn: &Connection,
    index: &HashMap<i64, usize>,
) -> crate::Result<Vec<ExternalNode>> {
    let local_uuid: Option<String> = if has_table(conn, "Information")? {
        conn.query_row("SELECT uuid FROM Information LIMIT 1", [], |r| {
            r.get::<_, Value>(0)
        })
        .ok()
        .map(|v| text(&v))
        .filter(|u| !u.is_empty())
    } else {
        None
    };
    let mut lists: HashMap<i64, List> = HashMap::new();
    let mut stmt =
        conn.prepare("SELECT id, title, parentListId, nextListId, isPersisted FROM Playlist")?;
    let rows = stmt.query_map([], |r| {
        (0..5)
            .map(|i| r.get::<_, Value>(i))
            .collect::<rusqlite::Result<Vec<Value>>>()
    })?;
    for row in rows {
        let v = row?;
        if let Some(id) = int(&v[0]) {
            lists.insert(
                id,
                List {
                    title: text(&v[1]),
                    parent: int(&v[2]).unwrap_or(0),
                    next: int(&v[3]).unwrap_or(0),
                    persisted: int(&v[4]).is_none_or(|p| p != 0),
                },
            );
        }
    }
    let mut entities: HashMap<i64, Vec<(i64, i64, i64)>> = HashMap::new();
    let mut stmt =
        conn.prepare("SELECT id, listId, trackId, nextEntityId, databaseUuid FROM PlaylistEntity")?;
    let rows = stmt.query_map([], |r| {
        (0..5)
            .map(|i| r.get::<_, Value>(i))
            .collect::<rusqlite::Result<Vec<Value>>>()
    })?;
    for row in rows {
        let v = row?;
        let uuid = text(&v[4]);
        let local = uuid.is_empty() || local_uuid.as_ref().is_none_or(|l| *l == uuid);
        if let (Some(id), Some(list), Some(track), true) =
            (int(&v[0]), int(&v[1]), int(&v[2]), local)
        {
            entities
                .entry(list)
                .or_default()
                .push((id, int(&v[3]).unwrap_or(0), track));
        }
    }
    Ok(build_tree(&lists, &entities, index))
}

fn build_tree(
    lists: &HashMap<i64, List>,
    entities: &HashMap<i64, Vec<(i64, i64, i64)>>,
    index: &HashMap<i64, usize>,
) -> Vec<ExternalNode> {
    let mut children: HashMap<i64, Vec<(i64, i64)>> = HashMap::new();
    for (id, l) in lists {
        let parent = if lists.contains_key(&l.parent) && l.parent != *id {
            l.parent
        } else {
            0
        };
        children.entry(parent).or_default().push((*id, l.next));
    }
    nodes(0, lists, &children, entities, index, &mut HashSet::new())
}

fn nodes(
    parent: i64,
    lists: &HashMap<i64, List>,
    children: &HashMap<i64, Vec<(i64, i64)>>,
    entities: &HashMap<i64, Vec<(i64, i64, i64)>>,
    index: &HashMap<i64, usize>,
    seen: &mut HashSet<i64>,
) -> Vec<ExternalNode> {
    let Some(kids) = children.get(&parent) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for id in linked_order(kids) {
        let list = &lists[&id];
        if !list.persisted || !seen.insert(id) {
            continue;
        }
        let tracks: Vec<usize> = entities.get(&id).map_or_else(Vec::new, |e| {
            let track: HashMap<i64, i64> = e.iter().map(|(i, _, t)| (*i, *t)).collect();
            let links: Vec<(i64, i64)> = e.iter().map(|(i, n, _)| (*i, *n)).collect();
            linked_order(&links)
                .into_iter()
                .filter_map(|i| index.get(&track[&i]).copied())
                .collect()
        });
        let sub = nodes(id, lists, children, entities, index, seen);
        if sub.is_empty() {
            out.push(ExternalNode::Playlist {
                name: list.title.clone(),
                tracks,
            });
        } else {
            let mut folder = Vec::new();
            if !tracks.is_empty() {
                folder.push(ExternalNode::Playlist {
                    name: list.title.clone(),
                    tracks,
                });
            }
            folder.extend(sub);
            out.push(ExternalNode::Folder {
                name: list.title.clone(),
                children: folder,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn compress(raw: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(raw.len()).unwrap().to_be_bytes().to_vec();
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(raw).unwrap();
        out.extend(z.finish().unwrap());
        out
    }

    fn beat_blob(rate: f64, grid: &[(f64, i64)], adjusted: &[(f64, i64)]) -> Vec<u8> {
        let mut b = rate.to_be_bytes().to_vec();
        b.extend(1000.0f64.to_be_bytes());
        b.push(1);
        for g in [grid, adjusted] {
            b.extend(i64::try_from(g.len()).unwrap().to_be_bytes());
            for (s, n) in g {
                b.extend(s.to_le_bytes());
                b.extend(n.to_le_bytes());
                b.extend(0i32.to_le_bytes());
                b.extend(0i32.to_le_bytes());
            }
        }
        b
    }

    fn quick_blob(cues: &[(&str, f64, [u8; 4])], main: f64) -> Vec<u8> {
        let mut b = i64::try_from(cues.len()).unwrap().to_be_bytes().to_vec();
        for (label, s, c) in cues {
            b.push(u8::try_from(label.len()).unwrap());
            b.extend(label.as_bytes());
            b.extend(s.to_be_bytes());
            b.extend(c);
        }
        b.extend(main.to_be_bytes());
        b.push(1);
        b.extend(0.0f64.to_be_bytes());
        b
    }

    fn loop_blob(loops: &[(&str, f64, f64, bool)]) -> Vec<u8> {
        let mut b = i64::try_from(loops.len()).unwrap().to_le_bytes().to_vec();
        for (label, s, e, set) in loops {
            b.push(u8::try_from(label.len()).unwrap());
            b.extend(label.as_bytes());
            b.extend(s.to_le_bytes());
            b.extend(e.to_le_bytes());
            b.extend([u8::from(*set), u8::from(*set)]);
            b.extend([255, 0, 0x80, 0xFF]);
        }
        b
    }

    #[test]
    fn keys_follow_the_circle_of_fifths() {
        let name = |k| {
            MusicalKey::from_value(key_value(k).unwrap())
                .unwrap()
                .to_string()
        };
        assert_eq!(key_value(0), Some(0));
        assert_eq!(key_value(1), Some(12 + 9));
        assert_eq!(key_value(2), Some(7));
        assert_eq!(key_value(3), Some(12 + 4));
        assert_eq!(key_value(9), Some(12 + 1));
        assert_eq!(key_value(23), Some(12 + 2));
        assert_eq!(key_value(22), Some(5));
        assert_eq!(key_value(24), None);
        assert_eq!(key_value(-1), None);
        assert_eq!(
            MusicalKey::from_value(key_value(1).unwrap()).unwrap(),
            MusicalKey::parse("Am").unwrap()
        );
        assert_eq!(name(12), MusicalKey::parse("F#").unwrap().to_string());
    }

    #[test]
    fn ratings_and_paths() {
        assert_eq!(rating(0), 0);
        assert_eq!(rating(20), 51);
        assert_eq!(rating(100), 255);
        assert_eq!(rating(150), 255);
        let root = Path::new("/Users/me/Music/Engine Library");
        assert_eq!(absolute(root, "../Pop/a.mp3"), "/Users/me/Music/Pop/a.mp3");
        assert_eq!(absolute(root, "/abs/b.mp3"), "/abs/b.mp3");
        assert_eq!(absolute(root, "C:/x/c.mp3"), "C:/x/c.mp3");
    }

    #[test]
    fn linked_lists_order() {
        assert_eq!(linked_order(&[(3, 0), (1, 5), (5, 3)]), vec![1, 5, 3]);
        assert_eq!(
            linked_order(&[(1, 9), (2, 1), (7, 8), (8, 7)]),
            vec![2, 1, 7, 8]
        );
        assert_eq!(linked_order(&[]), Vec::<i64>::new());
    }

    #[test]
    fn blobs_decode() {
        assert_eq!(uncompress(&[]), None);
        assert_eq!(uncompress(&[0, 0, 0, 0]), None);
        let raw = beat_blob(44_100.0, &[(0.0, 0)], &[]);
        assert_eq!(uncompress(&compress(&raw)), Some(raw.clone()));
        let b = beat_data(&raw).unwrap();
        assert!((b.sample_rate - 44_100.0).abs() < 1e-9);
        assert_eq!(
            b.default_grid,
            vec![Marker {
                sample: 0.0,
                beat: 0
            }]
        );
        assert!(beat_data(&raw[..20]).is_none());

        let q = quick_cues(&quick_blob(
            &[
                ("Drop", 88_200.0, [255, 0x11, 0x22, 0x33]),
                ("", -1.0, [0; 4]),
            ],
            44_100.0,
        ))
        .unwrap();
        assert_eq!(q.cues.len(), 2);
        assert_eq!(q.cues[0].label, "Drop");
        assert!((q.main_cue - 44_100.0).abs() < 1e-9);

        let l = loops(&loop_blob(&[
            ("L", 0.0, 44_100.0, true),
            ("", -1.0, -1.0, false),
        ]))
        .unwrap();
        assert_eq!(l.len(), 2);
        assert!(l[0].set && !l[1].set);
        assert_eq!(loops(&[]), Some(Vec::new()));
    }

    #[test]
    fn cues_from_slots_main_cue_and_loops() {
        let q = QuickCues {
            cues: vec![
                QuickCue {
                    label: "A".into(),
                    sample: 44_100.0,
                    argb: [255, 1, 2, 3],
                },
                QuickCue {
                    label: String::new(),
                    sample: -1.0,
                    argb: [0; 4],
                },
                QuickCue {
                    label: "C".into(),
                    sample: 0.0,
                    argb: [255, 0xAA, 0xBB, 0xCC],
                },
            ],
            main_cue: 22_050.0,
        };
        let l = [Loop {
            label: "Loop".into(),
            start: 44_100.0,
            end: 88_200.0,
            set: true,
            argb: [255, 0, 0, 0],
        }];
        let c = cues(44_100.0, Some(&q), &l);
        assert_eq!(c.len(), 4);
        assert_eq!((c[0].hotcue, c[0].start_s), (Some(0), 1.0));
        assert_eq!(c[0].color.as_deref(), Some("#010203"));
        assert_eq!(
            (c[1].hotcue, c[1].start_s, c[1].name.as_str()),
            (Some(2), 0.0, "C")
        );
        assert_eq!(
            (c[2].hotcue, c[2].start_s, c[2].kind),
            (None, 0.5, CuePoint::CUE)
        );
        assert_eq!(
            (c[3].kind, c[3].start_s, c[3].len_s),
            (CuePoint::LOOP, 1.0, 1.0)
        );
    }

    #[test]
    fn grid_from_markers() {
        let m = |sample, beat| Marker { sample, beat };
        let g = grid(44_100.0, &[m(-88_200.0, -4), m(44_100.0 * 98.0, 196)]).unwrap();
        assert!((g.bpm - 120.0).abs() < 1e-9);
        assert!(g.anchor_s.abs() < 1e-9);
        assert_eq!(g.markers_s, Vec::<f64>::new());
        let g = grid(
            1000.0,
            &[m(500.0, 0), m(10_000.0, 19), m(10_000.0 + 20_000.0, 69)],
        )
        .unwrap();
        assert!((g.anchor_s - 0.5).abs() < 1e-9);
        assert_eq!(g.markers_s, vec![10.0]);
        assert!(grid(44_100.0, &[m(0.0, 0)]).is_none());
    }

    fn sample_db(dir: &Path) {
        std::fs::create_dir_all(dir.join("Database2")).unwrap();
        let conn = Connection::open(dir.join(DATABASE)).unwrap();
        conn.execute_batch(
            "CREATE TABLE Information (id INTEGER PRIMARY KEY, uuid TEXT);
             INSERT INTO Information VALUES (1, 'local');
             CREATE TABLE Track (id INTEGER PRIMARY KEY, playOrder INTEGER, length INTEGER,
               bpm INTEGER, year INTEGER, path TEXT, filename TEXT, bitrate INTEGER,
               bpmAnalyzed REAL, fileBytes INTEGER, title TEXT, artist TEXT, album TEXT,
               genre TEXT, comment TEXT, label TEXT, composer TEXT, remixer TEXT,
               key INTEGER, rating INTEGER, timeLastPlayed DATETIME, dateAdded DATETIME,
               isAvailable BOOLEAN);
             CREATE TABLE PerformanceData (trackId INTEGER PRIMARY KEY, trackData BLOB,
               overviewWaveFormData BLOB, beatData BLOB, quickCues BLOB, loops BLOB);
             CREATE TABLE Playlist (id INTEGER PRIMARY KEY, title TEXT, parentListId INTEGER,
               isPersisted BOOLEAN, nextListId INTEGER);
             CREATE TABLE PlaylistEntity (id INTEGER PRIMARY KEY, listId INTEGER,
               trackId INTEGER, databaseUuid TEXT, nextEntityId INTEGER);
             INSERT INTO Track (id, length, bpm, path, filename, bitrate, bpmAnalyzed,
               fileBytes, title, artist, key, rating, dateAdded)
               VALUES (1, 300, 120, '../Music/a.mp3', 'a.mp3', 320, 120.5, 4096,
               'A', 'Artist', 1, 80, 1700000000);
             INSERT INTO Track (id, path, title) VALUES (2, '../Music/b.mp3', 'B');
             INSERT INTO Track (id, path, title) VALUES (3, NULL, 'Streamed');
             INSERT INTO Track (id, path, title) VALUES (4, '../Music/d.mp3', 'D');
             -- Root: Crates (folder with own track) then Solo; Hidden not persisted.
             INSERT INTO Playlist VALUES (10, 'Solo', 0, 1, 0);
             INSERT INTO Playlist VALUES (11, 'Crates', 0, 1, 10);
             INSERT INTO Playlist VALUES (12, 'Inner', 11, 1, 0);
             INSERT INTO Playlist VALUES (13, 'Hidden', 0, 0, 11);
             INSERT INTO PlaylistEntity VALUES (1, 10, 4, 'local', 0);
             INSERT INTO PlaylistEntity VALUES (2, 10, 1, 'local', 3);
             INSERT INTO PlaylistEntity VALUES (3, 10, 2, 'local', 1);
             INSERT INTO PlaylistEntity VALUES (4, 10, 2, 'other', 0);
             INSERT INTO PlaylistEntity VALUES (5, 11, 2, 'local', 0);
             INSERT INTO PlaylistEntity VALUES (6, 12, 1, 'local', 0);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO PerformanceData (trackId, beatData, quickCues, loops) \
             VALUES (1, ?1, ?2, ?3)",
            rusqlite::params![
                compress(&beat_blob(
                    44_100.0,
                    &[(0.0, 0), (44_100.0 * 100.0, 200)],
                    &[]
                )),
                compress(&quick_blob(&[("Intro", 44_100.0, [255, 0xFF, 0, 0])], 0.0)),
                loop_blob(&[("", 0.0, 88_200.0, true)]),
            ],
        )
        .unwrap();
    }

    #[test]
    fn reads_a_library_read_only() {
        let dir = std::env::temp_dir()
            .join(format!("backspn-engine-{}", std::process::id()))
            .join(FOLDER);
        let _ = std::fs::remove_dir_all(&dir);
        sample_db(&dir);
        let before = std::fs::read(dir.join(DATABASE)).unwrap();
        let lib = read(&dir).unwrap();
        assert_eq!(std::fs::read(dir.join(DATABASE)).unwrap(), before);

        let titles: Vec<&str> = lib.tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["A", "B", "D"]);
        let a = &lib.tracks[0];
        let music = dir.parent().unwrap().join("Music");
        assert_eq!(a.path, music.join("a.mp3").to_string_lossy());
        assert_eq!(a.bpm, Some(120.5));
        assert_eq!(a.duration_s, Some(300.0));
        assert_eq!(a.bitrate, Some(320_000));
        assert_eq!(a.filesize_kib, Some(4));
        assert_eq!(a.rating, 204);
        assert_eq!(a.musical_key, Some(21));
        assert_eq!(a.key_text, MusicalKey::parse("Am").unwrap().open_key());
        assert_ne!(a.import_date, "");
        let g = a.grid.as_ref().unwrap();
        assert!((g.bpm - 120.0).abs() < 1e-9);
        assert_eq!(a.cues.len(), 2);
        assert_eq!(a.cues[0].hotcue, Some(0));
        assert_eq!(a.cues[0].color.as_deref(), Some("#FF0000"));
        assert_eq!((a.cues[1].kind, a.cues[1].len_s), (CuePoint::LOOP, 2.0));
        assert!(lib.tracks[1].cues.is_empty() && lib.tracks[1].grid.is_none());

        assert_eq!(
            lib.playlists,
            vec![
                ExternalNode::Folder {
                    name: "Crates".into(),
                    children: vec![
                        ExternalNode::Playlist {
                            name: "Crates".into(),
                            tracks: vec![1],
                        },
                        ExternalNode::Playlist {
                            name: "Inner".into(),
                            tracks: vec![0],
                        },
                    ],
                },
                ExternalNode::Playlist {
                    name: "Solo".into(),
                    tracks: vec![0, 1, 2],
                },
            ]
        );
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
