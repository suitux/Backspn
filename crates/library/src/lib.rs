pub mod clock;
pub mod energy;
pub mod exchange;
pub mod external;
mod history;
pub mod library_file;
pub mod maintenance;
mod nml_io;
pub mod outside;
pub mod overview;
pub mod paths;
pub mod playlists;
pub mod sample_deck;
pub mod scan;
pub mod set_plan;
pub mod smart;
pub mod storage;
pub mod suggest;
pub mod tags;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use backspn_formats::transients;
pub use backspn_formats::transients::Transient;
use rusqlite::{Connection, OptionalExtension, params};

pub use history::{HistoryEntry, HistorySession};
pub use sample_deck::SampleSetRow;
pub use smart::Query;

const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS tracks (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL DEFAULT '',
    artist TEXT NOT NULL DEFAULT '',
    album TEXT NOT NULL DEFAULT '',
    genre TEXT NOT NULL DEFAULT '',
    label TEXT NOT NULL DEFAULT '',
    comment TEXT NOT NULL DEFAULT '',
    key_text TEXT NOT NULL DEFAULT '',
    musical_key INTEGER,
    bpm REAL,
    grid_anchor_s REAL,
    duration_s REAL,
    rating INTEGER NOT NULL DEFAULT 0,
    playcount INTEGER NOT NULL DEFAULT 0,
    import_date TEXT NOT NULL DEFAULT '',
    lufs REAL,
    analyzed INTEGER NOT NULL DEFAULT 0,
    locked INTEGER NOT NULL DEFAULT 0,
    remixer TEXT NOT NULL DEFAULT '',
    producer TEXT NOT NULL DEFAULT '',
    mix TEXT NOT NULL DEFAULT '',
    catalog_no TEXT NOT NULL DEFAULT '',
    release_date TEXT NOT NULL DEFAULT '',
    last_played TEXT NOT NULL DEFAULT '',
    comment2 TEXT NOT NULL DEFAULT '',
    bitrate INTEGER,
    track_number INTEGER,
    filesize_kib INTEGER,
    overview BLOB,
    color INTEGER NOT NULL DEFAULT 0,
    nml_raw TEXT,
    dirty INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS cues (
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    kind INTEGER NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    start_s REAL NOT NULL,
    len_s REAL NOT NULL DEFAULT 0,
    hotcue INTEGER NOT NULL DEFAULT -1,
    display_order INTEGER NOT NULL DEFAULT 0,
    repeats INTEGER NOT NULL DEFAULT -1,
    color TEXT
);
CREATE INDEX IF NOT EXISTS cues_track ON cues(track_id);
CREATE TABLE IF NOT EXISTS transients (
    track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
    data BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS nodes (
    id INTEGER PRIMARY KEY,
    parent INTEGER REFERENCES nodes(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    uuid TEXT,
    query TEXT
);
CREATE TABLE IF NOT EXISTS playlist_entries (
    node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    track_id INTEGER REFERENCES tracks(id) ON DELETE SET NULL,
    -- NML primary key, kept for entries whose track is not in the collection.
    key TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS entries_node ON playlist_entries(node_id, position);
CREATE INDEX IF NOT EXISTS entries_track ON playlist_entries(track_id);
";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("NML error: {0}")]
    Nml(#[from] backspn_formats::Error),
    #[error("cannot write {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackRow {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub comment: String,
    pub key_text: String,
    pub musical_key: Option<u8>,
    pub bpm: Option<f64>,
    pub grid_anchor_s: Option<f64>,
    pub duration_s: Option<f64>,
    pub rating: u8,
    pub playcount: u32,
    pub import_date: String,
    pub lufs: Option<f64>,
    pub analyzed: bool,
    pub locked: bool,
    pub remixer: String,
    pub producer: String,
    pub mix: String,
    pub catalog_no: String,
    pub release_date: String,
    pub last_played: String,
    pub comment2: String,
    pub bitrate: Option<u32>,
    pub track_number: Option<u32>,
    pub filesize_kib: Option<u64>,
    pub overview: Vec<u8>,
    pub color: u8,
    pub modified_date: String,
    pub modified_time: Option<u32>,
    pub cover: Option<String>,
    pub sample_type: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CuePoint {
    pub kind: i32,
    pub name: String,
    pub start_s: f64,
    pub len_s: f64,
    pub hotcue: Option<u8>,
    pub display_order: i32,
    pub repeats: i32,
    pub color: Option<String>,
}

impl CuePoint {
    pub const CUE: i32 = 0;
    pub const FADE_IN: i32 = 1;
    pub const FADE_OUT: i32 = 2;
    pub const LOAD: i32 = 3;
    pub const GRID: i32 = 4;
    pub const LOOP: i32 = 5;

    pub fn is_beatgrid(&self) -> bool {
        self.kind == Self::GRID && self.hotcue.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Folder,
    Playlist,
    Smartlist,
}

impl NodeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Playlist => "playlist",
            Self::Smartlist => "smartlist",
        }
    }

    pub(crate) fn parse(s: &str) -> Self {
        match s {
            "folder" => Self::Folder,
            "smartlist" => Self::Smartlist,
            _ => Self::Playlist,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: i64,
    pub parent: Option<i64>,
    pub position: i64,
    pub kind: NodeKind,
    pub name: String,
    pub uuid: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextField {
    Title,
    Artist,
    Album,
    Genre,
    Label,
    Comment,
    Comment2,
    Remixer,
    Producer,
    Mix,
    CatalogNo,
    ReleaseDate,
}

impl TextField {
    pub const ALL: [Self; 12] = [
        Self::Title,
        Self::Artist,
        Self::Album,
        Self::Mix,
        Self::Remixer,
        Self::Producer,
        Self::Label,
        Self::CatalogNo,
        Self::Genre,
        Self::Comment,
        Self::Comment2,
        Self::ReleaseDate,
    ];

    fn column(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::Genre => "genre",
            Self::Label => "label",
            Self::Comment => "comment",
            Self::Comment2 => "comment2",
            Self::Remixer => "remixer",
            Self::Producer => "producer",
            Self::Mix => "mix",
            Self::CatalogNo => "catalog_no",
            Self::ReleaseDate => "release_date",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Artist => "Artist",
            Self::Album => "Release",
            Self::Genre => "Genre",
            Self::Label => "Label",
            Self::Comment => "Comment",
            Self::Comment2 => "Comment 2",
            Self::Remixer => "Remixer",
            Self::Producer => "Producer",
            Self::Mix => "Mix",
            Self::CatalogNo => "Catalog No.",
            Self::ReleaseDate => "Release Date",
        }
    }

    pub fn get(self, t: &TrackRow) -> &str {
        match self {
            Self::Title => &t.title,
            Self::Artist => &t.artist,
            Self::Album => &t.album,
            Self::Genre => &t.genre,
            Self::Label => &t.label,
            Self::Comment => &t.comment,
            Self::Comment2 => &t.comment2,
            Self::Remixer => &t.remixer,
            Self::Producer => &t.producer,
            Self::Mix => &t.mix,
            Self::CatalogNo => &t.catalog_no,
            Self::ReleaseDate => &t.release_date,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnalysisUpdate {
    pub bpm: Option<f64>,
    pub grid_anchor_s: Option<f64>,
    pub musical_key: Option<u8>,
    pub key_text: Option<String>,
    pub lufs: Option<f64>,
    pub duration_s: Option<f64>,
    pub replace_locked: bool,
    pub transients: Vec<Transient>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportReport {
    pub tracks: usize,
    pub cues: usize,
    pub playlists: usize,
    pub smartlists: usize,
    pub folders: usize,
    pub missing_playlist_tracks: usize,
}

#[derive(Debug)]
pub struct Library {
    conn: Connection,
}

pub(crate) const TRACK_COLUMNS: &str = "id, path, title, artist, album, genre, label, comment, key_text, \
    musical_key, bpm, grid_anchor_s, duration_s, rating, playcount, import_date, lufs, analyzed, locked, \
    remixer, producer, mix, catalog_no, release_date, last_played, comment2, bitrate, track_number, \
    filesize_kib, overview, color, modified_date, modified_time, cover, sample_type";

const ADDED_COLUMNS: [(&str, &str); 18] = [
    ("remixer", "TEXT NOT NULL DEFAULT ''"),
    ("producer", "TEXT NOT NULL DEFAULT ''"),
    ("mix", "TEXT NOT NULL DEFAULT ''"),
    ("catalog_no", "TEXT NOT NULL DEFAULT ''"),
    ("release_date", "TEXT NOT NULL DEFAULT ''"),
    ("last_played", "TEXT NOT NULL DEFAULT ''"),
    ("comment2", "TEXT NOT NULL DEFAULT ''"),
    ("bitrate", "INTEGER"),
    ("track_number", "INTEGER"),
    ("filesize_kib", "INTEGER"),
    ("overview", "BLOB"),
    ("color", "INTEGER NOT NULL DEFAULT 0"),
    ("modified_date", "TEXT NOT NULL DEFAULT ''"),
    ("modified_time", "INTEGER"),
    ("cover", "TEXT"),
    ("stem_hash", "TEXT"),
    ("sample_type", "INTEGER"),
    ("scanned_mtime", "INTEGER"),
];

const NOT_FROM_NML: [&str; 2] = ["stem_hash", "scanned_mtime"];

pub(crate) fn track_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TrackRow> {
    Ok(TrackRow {
        id: r.get(0)?,
        path: r.get(1)?,
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        genre: r.get(5)?,
        label: r.get(6)?,
        comment: r.get(7)?,
        key_text: r.get(8)?,
        musical_key: r.get(9)?,
        bpm: r.get(10)?,
        grid_anchor_s: r.get(11)?,
        duration_s: r.get(12)?,
        rating: r.get(13)?,
        playcount: r.get(14)?,
        import_date: r.get(15)?,
        lufs: r.get(16)?,
        analyzed: r.get(17)?,
        locked: r.get(18)?,
        remixer: r.get(19)?,
        producer: r.get(20)?,
        mix: r.get(21)?,
        catalog_no: r.get(22)?,
        release_date: r.get(23)?,
        last_played: r.get(24)?,
        comment2: r.get(25)?,
        bitrate: r.get(26)?,
        track_number: r.get(27)?,
        filesize_kib: r
            .get::<_, Option<i64>>(28)?
            .and_then(|v| u64::try_from(v).ok()),
        overview: r.get::<_, Option<Vec<u8>>>(29)?.unwrap_or_default(),
        color: r.get(30)?,
        modified_date: r.get(31)?,
        modified_time: r.get(32)?,
        cover: r.get(33)?,
        sample_type: r.get(34)?,
    })
}

fn store_tags(tx: &rusqlite::Transaction<'_>, id: i64, t: &tags::FileTags) -> Result<()> {
    for (field, value) in [
        (TextField::Title, &t.title),
        (TextField::Artist, &t.artist),
        (TextField::Album, &t.album),
        (TextField::Genre, &t.genre),
        (TextField::Label, &t.label),
        (TextField::Comment, &t.comment),
        (TextField::Remixer, &t.remixer),
        (TextField::Producer, &t.producer),
        (TextField::CatalogNo, &t.catalog_no),
        (TextField::ReleaseDate, &t.release_date),
    ] {
        if !value.is_empty() {
            tx.execute(
                &format!("UPDATE tracks SET {} = ?2 WHERE id = ?1", field.column()),
                params![id, value],
            )?;
        }
    }
    if t.musical_key.is_some() {
        tx.execute(
            "UPDATE tracks SET musical_key = ?2, key_text = ?3 WHERE id = ?1",
            params![id, t.musical_key, t.key_text],
        )?;
    }
    if t.track_number.is_some() {
        tx.execute(
            "UPDATE tracks SET track_number = ?2 WHERE id = ?1",
            params![id, t.track_number],
        )?;
    }
    tx.execute("UPDATE tracks SET dirty = 1 WHERE id = ?1", [id])?;
    Ok(())
}

fn migrate(conn: &Connection) -> Result<()> {
    let existing: Vec<String> = {
        let mut stmt = conn.prepare("SELECT name FROM pragma_table_info('tracks')")?;
        stmt.query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut added = false;
    for (name, decl) in ADDED_COLUMNS {
        if !existing.iter().any(|c| c == name) {
            conn.execute_batch(&format!("ALTER TABLE tracks ADD COLUMN {name} {decl}"))?;
            added |= !NOT_FROM_NML.contains(&name);
        }
    }
    if added {
        nml_io::backfill_extras(conn)?;
    }
    canonical_paths(conn)
}

fn canonical_paths(conn: &Connection) -> Result<()> {
    let startup = paths::startup_volume();
    let prefix = format!("/Volumes/{startup}/");
    let rows: Vec<(i64, String)> = {
        let mut stmt =
            conn.prepare("SELECT id, path FROM tracks WHERE substr(path, 1, ?1) = ?2")?;
        let rows = stmt.query_map(params![prefix.len() as i64, prefix], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for (id, path) in rows {
        conn.execute(
            "UPDATE OR IGNORE tracks SET path = ?2 WHERE id = ?1",
            params![id, paths::canonical(&path, &startup)],
        )?;
    }
    Ok(())
}

pub(crate) fn cues_of(conn: &Connection, id: i64) -> Result<Vec<CuePoint>> {
    let mut stmt = conn.prepare(
        "SELECT kind, name, start_s, len_s, hotcue, display_order, repeats, color \
         FROM cues WHERE track_id = ?1 ORDER BY position, rowid",
    )?;
    let rows = stmt.query_map([id], |r| {
        let hotcue: i64 = r.get(4)?;
        Ok(CuePoint {
            kind: r.get(0)?,
            name: r.get(1)?,
            start_s: r.get(2)?,
            len_s: r.get(3)?,
            hotcue: u8::try_from(hotcue).ok(),
            display_order: r.get(5)?,
            repeats: r.get(6)?,
            color: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[derive(Debug, Clone, Default)]
pub struct FileFacts {
    tags: tags::FileTags,
    filesize_kib: Option<i64>,
    mtime: Option<i64>,
}

impl FileFacts {
    pub fn read(path: &Path) -> Self {
        Self {
            tags: tags::read(path).unwrap_or_default(),
            filesize_kib: std::fs::metadata(path)
                .ok()
                .and_then(|m| i64::try_from(m.len() / 1024).ok()),
            mtime: scan::file_mtime(path),
        }
    }
}

fn insert_file(conn: &Connection, path: &Path, path_text: &str, facts: FileFacts) -> Result<i64> {
    let mut t = facts.tags;
    if t.title.is_empty() {
        let (artist, title) = paths::names_from_file_name(path);
        t.title = title;
        if t.artist.is_empty() {
            t.artist = artist;
        }
    }
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, genre, label, comment, key_text, musical_key, \
         bpm, duration_s, rating, playcount, import_date, remixer, producer, catalog_no, release_date, \
         bitrate, track_number, filesize_kib, scanned_mtime, dirty) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, 1)",
        params![
            path_text,
            t.title,
            t.artist,
            t.album,
            t.genre,
            t.label,
            t.comment,
            t.key_text,
            t.musical_key,
            t.bpm,
            t.duration_s,
            t.rating,
            t.playcount,
            nml_io::today(),
            t.remixer,
            t.producer,
            t.catalog_no,
            t.release_date,
            t.bitrate,
            t.track_number,
            facts.filesize_kib,
            facts.mtime,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

impl Library {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch(history::SCHEMA)?;
        sample_deck::rename_old_tables(&conn)?;
        conn.execute_batch(sample_deck::SCHEMA)?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn tracks(&self) -> Result<Vec<TrackRow>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {TRACK_COLUMNS} FROM tracks ORDER BY id"))?;
        let rows = stmt.query_map([], track_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn track(&self, id: i64) -> Result<Option<TrackRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {TRACK_COLUMNS} FROM tracks WHERE id = ?1"),
                [id],
                track_from_row,
            )
            .optional()?)
    }

    pub fn track_by_path(&self, path: &str) -> Result<Option<TrackRow>> {
        let path = paths::canonical(path, &paths::startup_volume());
        let path = path.as_str();
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {TRACK_COLUMNS} FROM tracks WHERE path = ?1"),
                [path],
                track_from_row,
            )
            .optional()?)
    }

    pub fn search(&self, text: &str) -> Result<Vec<i64>> {
        let words: Vec<String> = text
            .split_whitespace()
            .map(|w| {
                format!(
                    "%{}%",
                    w.replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                )
            })
            .collect();
        let mut sql = String::from("SELECT id FROM tracks WHERE 1");
        for i in 1..=words.len() {
            let _ = write!(
                sql,
                " AND (title LIKE ?{i} ESCAPE '\\' OR artist LIKE ?{i} ESCAPE '\\' OR album LIKE ?{i} ESCAPE '\\' \
                 OR genre LIKE ?{i} ESCAPE '\\' OR label LIKE ?{i} ESCAPE '\\' OR comment LIKE ?{i} ESCAPE '\\' \
                 OR path LIKE ?{i} ESCAPE '\\')"
            );
        }
        sql.push_str(" ORDER BY id");
        let mut stmt = self.conn.prepare(&sql)?;
        let ids = stmt.query_map(rusqlite::params_from_iter(words.iter()), |r| r.get(0))?;
        Ok(ids.collect::<rusqlite::Result<_>>()?)
    }

    pub fn add_file(&mut self, path: &Path) -> Result<i64> {
        let path_text = paths::canonical(&path.to_string_lossy(), &paths::startup_volume());
        if let Some(t) = self.track_by_path(&path_text)? {
            return Ok(t.id);
        }
        let facts = FileFacts::read(path);
        insert_file(&self.conn, path, &path_text, facts)
    }

    pub fn add_read_files(&mut self, files: Vec<(PathBuf, FileFacts)>) -> Result<Vec<Option<i64>>> {
        let volume = paths::startup_volume();
        let tx = self.conn.transaction()?;
        let mut ids = Vec::with_capacity(files.len());
        for (path, facts) in files {
            let path_text = paths::canonical(&path.to_string_lossy(), &volume);
            let known: Option<i64> = tx
                .query_row(
                    "SELECT id FROM tracks WHERE path = ?1",
                    params![path_text],
                    |r| r.get(0),
                )
                .optional()?;
            ids.push(match known {
                Some(id) => Some(id),
                None => insert_file(&tx, &path, &path_text, facts).ok(),
            });
        }
        tx.commit()?;
        Ok(ids)
    }

    pub fn set_text(&mut self, id: i64, field: TextField, value: &str) -> Result<()> {
        self.conn.execute(
            &format!(
                "UPDATE tracks SET {} = ?2, dirty = 1 WHERE id = ?1",
                field.column()
            ),
            params![id, value],
        )?;
        Ok(())
    }

    pub fn set_color(&mut self, tracks: &[i64], color: u8) -> Result<()> {
        let tx = self.conn.transaction()?;
        for id in tracks {
            tx.execute(
                "UPDATE tracks SET color = ?2, dirty = 1 WHERE id = ?1",
                params![id, color],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_cover(&mut self, tracks: &[i64], cover: Option<&str>) -> Result<()> {
        let tx = self.conn.transaction()?;
        for id in tracks {
            tx.execute(
                "UPDATE tracks SET cover = ?2 WHERE id = ?1",
                params![id, cover],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn save_overview(&mut self, id: i64, overview: &[u8]) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET overview = ?2 WHERE id = ?1",
            params![id, overview],
        )?;
        Ok(())
    }

    pub fn set_locked(&mut self, tracks: &[i64], locked: bool) -> Result<()> {
        let tx = self.conn.transaction()?;
        for id in tracks {
            tx.execute(
                "UPDATE tracks SET locked = ?2, dirty = 1 WHERE id = ?1",
                params![id, locked],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_key(&mut self, id: i64, key: Option<u8>) -> Result<()> {
        let text = key
            .and_then(backspn_formats::nml::MusicalKey::from_value)
            .map(backspn_formats::nml::MusicalKey::open_key)
            .unwrap_or_default();
        self.conn.execute(
            "UPDATE tracks SET musical_key = ?2, key_text = ?3, dirty = 1 WHERE id = ?1",
            params![id, key, text],
        )?;
        Ok(())
    }

    pub fn set_play_count(&mut self, id: i64, count: u32) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET playcount = ?2, dirty = 1 WHERE id = ?1",
            params![id, count],
        )?;
        Ok(())
    }

    pub fn set_track_number(&mut self, id: i64, number: Option<u32>) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET track_number = ?2, dirty = 1 WHERE id = ?1",
            params![id, number],
        )?;
        Ok(())
    }

    pub fn prefer_file_tags(&mut self, known: &std::collections::HashSet<i64>) -> Result<usize> {
        let new: Vec<i64> = self
            .tracks()?
            .into_iter()
            .map(|t| t.id)
            .filter(|id| !known.contains(id))
            .collect();
        Ok(new
            .into_iter()
            .filter(|&id| self.import_tags(id).is_err())
            .count())
    }

    pub fn import_tags(&mut self, id: i64) -> Result<()> {
        let Some(track) = self.track(id)? else {
            return Ok(());
        };
        let file = track.path.split('#').next().unwrap_or(&track.path);
        let Some(t) = tags::read(Path::new(file)) else {
            return Err(Error::Invalid(format!("cannot read the tags of {file}")));
        };
        let tx = self.conn.transaction()?;
        store_tags(&tx, id, &t)?;
        tx.commit()?;
        Ok(())
    }

    pub fn scan_stamp(&self, id: i64) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT scanned_mtime FROM tracks WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }

    pub fn store_scan(&mut self, id: i64, scan: &scan::FileScan) -> Result<()> {
        let tx = self.conn.transaction()?;
        store_tags(&tx, id, &scan.tags)?;
        let modified = scan
            .mtime
            .and_then(|s| u64::try_from(s).ok())
            .map(clock::LocalTime::from_unix);
        tx.execute(
            "UPDATE tracks SET \
             bitrate = COALESCE(?2, bitrate), duration_s = COALESCE(?3, duration_s), \
             filesize_kib = COALESCE(?4, filesize_kib), bpm = COALESCE(bpm, ?5), \
             modified_date = COALESCE(?6, modified_date), \
             modified_time = COALESCE(?7, modified_time), scanned_mtime = ?8 WHERE id = ?1",
            params![
                id,
                scan.tags.bitrate,
                scan.tags.duration_s,
                scan.size_kib.and_then(|k| i64::try_from(k).ok()),
                scan.tags.bpm,
                modified.map(clock::LocalTime::date_text),
                modified.map(|t| t.seconds),
                scan.mtime,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn write_tags(
        &mut self,
        id: i64,
        notation: backspn_formats::nml::KeyNotation,
    ) -> Result<()> {
        let Some(track) = self.track(id)? else {
            return Ok(());
        };
        let file = Path::new(track.path.split('#').next().unwrap_or(&track.path));
        tags::write(file, &track, notation).map_err(|e| Error::Invalid(e.to_string()))?;
        self.update_from_file(id)
    }

    pub fn update_from_file(&mut self, id: i64) -> Result<()> {
        let Some(track) = self.track(id)? else {
            return Ok(());
        };
        let file = Path::new(track.path.split('#').next().unwrap_or(&track.path));
        let Ok(meta) = std::fs::metadata(file) else {
            return Ok(());
        };
        let kib = i64::try_from(meta.len() / 1024).ok();
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| clock::LocalTime::from_unix(d.as_secs()));
        self.conn.execute(
            "UPDATE tracks SET filesize_kib = ?2, \
             modified_date = COALESCE(?3, modified_date), \
             modified_time = COALESCE(?4, modified_time), \
             scanned_mtime = COALESCE(?5, scanned_mtime) WHERE id = ?1",
            params![
                id,
                kib,
                modified.map(clock::LocalTime::date_text),
                modified.map(|t| t.seconds),
                scan::file_mtime(file),
            ],
        )?;
        Ok(())
    }

    pub fn set_rating(&mut self, id: i64, rating: u8) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET rating = ?2, dirty = 1 WHERE id = ?1",
            params![id, rating],
        )?;
        Ok(())
    }

    pub fn save_analysis(&mut self, id: i64, update: &AnalysisUpdate) -> Result<()> {
        let locked: bool =
            self.conn
                .query_row("SELECT locked FROM tracks WHERE id = ?1", [id], |r| {
                    r.get(0)
                })?;
        if !update.transients.is_empty() {
            self.conn.execute(
                "INSERT OR REPLACE INTO transients (track_id, data) VALUES (?1, ?2)",
                params![id, transients::encode(&update.transients)],
            )?;
        }
        if locked && !update.replace_locked {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        if update.grid_anchor_s.is_none()
            && let Some(bpm) = update.bpm
        {
            tx.execute("UPDATE tracks SET bpm = ?2 WHERE id = ?1", params![id, bpm])?;
        }
        if let (Some(bpm), Some(anchor)) = (update.bpm, update.grid_anchor_s) {
            tx.execute(
                "UPDATE tracks SET bpm = ?2, grid_anchor_s = ?3 WHERE id = ?1",
                params![id, bpm, anchor],
            )?;
            let grid_cue: Option<i64> = tx
                .query_row(
                    "SELECT rowid FROM cues WHERE track_id = ?1 AND kind = 4 ORDER BY position LIMIT 1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?;
            match grid_cue {
                Some(rowid) => {
                    tx.execute(
                        "UPDATE cues SET start_s = ?2 WHERE rowid = ?1",
                        params![rowid, anchor],
                    )?;
                }
                None => {
                    tx.execute(
                        "INSERT INTO cues (track_id, position, kind, name, start_s) VALUES (?1, -1, 4, 'AutoGrid', ?2)",
                        params![id, anchor],
                    )?;
                }
            }
        }
        if let Some(key) = update.musical_key {
            tx.execute(
                "UPDATE tracks SET musical_key = ?2 WHERE id = ?1",
                params![id, key],
            )?;
        }
        if let Some(text) = &update.key_text {
            tx.execute(
                "UPDATE tracks SET key_text = ?2 WHERE id = ?1",
                params![id, text],
            )?;
        }
        if let Some(lufs) = update.lufs {
            tx.execute(
                "UPDATE tracks SET lufs = ?2 WHERE id = ?1",
                params![id, lufs],
            )?;
        }
        if let Some(d) = update.duration_s {
            tx.execute(
                "UPDATE tracks SET duration_s = ?2 WHERE id = ?1",
                params![id, d],
            )?;
        }
        tx.execute(
            "UPDATE tracks SET analyzed = 1, dirty = 1 WHERE id = ?1",
            [id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn transients(&self, id: i64) -> Result<Vec<Transient>> {
        let data: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT data FROM transients WHERE track_id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(match data {
            Some(bytes) => transients::parse(&bytes)?,
            None => Vec::new(),
        })
    }

    pub fn audio_id(&self, id: i64) -> Result<Option<String>> {
        let raw: Option<Option<String>> = self
            .conn
            .query_row("SELECT nml_raw FROM tracks WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(raw.flatten().and_then(|xml| {
            let start = xml.find("AUDIO_ID=\"")? + "AUDIO_ID=\"".len();
            let len = xml[start..].find('"')?;
            Some(xml[start..start + len].to_string()).filter(|id| !id.is_empty())
        }))
    }

    pub fn stem_hash(&self, id: i64) -> Result<Option<[u32; 4]>> {
        let text: Option<Option<String>> = self
            .conn
            .query_row("SELECT stem_hash FROM tracks WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(text.flatten().and_then(|t| storage::parse_hash(&t)))
    }

    pub fn set_stem_hash(&mut self, id: i64, hash: Option<[u32; 4]>) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET stem_hash = ?2 WHERE id = ?1",
            params![id, hash.map(storage::hash_text)],
        )?;
        Ok(())
    }

    pub fn cues(&self, id: i64) -> Result<Vec<CuePoint>> {
        cues_of(&self.conn, id)
    }

    pub fn save_cues(&mut self, id: i64, cues: &[CuePoint]) -> Result<()> {
        let existing: Vec<CuePoint> = self
            .cues(id)?
            .into_iter()
            .filter(|c| !c.is_beatgrid())
            .collect();
        let same = |a: &CuePoint, b: &CuePoint| {
            a.kind == b.kind
                && a.name == b.name
                && a.hotcue == b.hotcue
                && a.color == b.color
                && (a.start_s - b.start_s).abs() < 1e-6
                && (a.len_s - b.len_s).abs() < 1e-6
        };
        if existing.len() == cues.len() && existing.iter().zip(cues).all(|(a, b)| same(a, b)) {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM cues WHERE track_id = ?1 AND NOT (kind = ?2 AND hotcue < 0)",
            params![id, CuePoint::GRID],
        )?;
        for (position, c) in cues.iter().enumerate() {
            tx.execute(
                "INSERT INTO cues (track_id, position, kind, name, start_s, len_s, hotcue, \
                 display_order, repeats, color) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    id,
                    position as i64,
                    c.kind,
                    c.name,
                    c.start_s,
                    c.len_s,
                    c.hotcue.map_or(-1, i64::from),
                    c.display_order,
                    c.repeats,
                    c.color,
                ],
            )?;
        }
        tx.execute("UPDATE tracks SET dirty = 1 WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn nodes(&self) -> Result<Vec<Node>> {
        Self::nodes_of(&self.conn)
    }

    pub(crate) fn nodes_of(conn: &Connection) -> Result<Vec<Node>> {
        let mut stmt = conn
            .prepare("SELECT id, parent, position, kind, name, uuid, query FROM nodes ORDER BY parent IS NOT NULL, parent, position")?;
        let rows = stmt.query_map([], |r| {
            Ok(Node {
                id: r.get(0)?,
                parent: r.get(1)?,
                position: r.get(2)?,
                kind: NodeKind::parse(&r.get::<_, String>(3)?),
                name: r.get(4)?,
                uuid: r.get(5)?,
                query: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn create_node(
        &mut self,
        parent: Option<i64>,
        kind: NodeKind,
        name: &str,
        query: Option<&str>,
    ) -> Result<i64> {
        if let Some(q) = query {
            Query::parse(q).map_err(|e| Error::Invalid(e.to_string()))?;
        }
        let position: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM nodes WHERE parent IS ?1",
            [parent],
            |r| r.get(0),
        )?;
        let uuid = (kind != NodeKind::Folder).then(nml_io::new_uuid);
        self.conn.execute(
            "INSERT INTO nodes (parent, position, kind, name, uuid, query) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![parent, position, kind.as_str(), name, uuid, query],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn set_node_query(&mut self, id: i64, query: &str) -> Result<()> {
        Query::parse(query).map_err(|e| Error::Invalid(e.to_string()))?;
        self.conn.execute(
            "UPDATE nodes SET query = ?2 WHERE id = ?1",
            params![id, query],
        )?;
        Ok(())
    }

    pub fn rename_node(&mut self, id: i64, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE nodes SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    pub fn delete_node(&mut self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM nodes WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn node_tracks(&self, id: i64) -> Result<Vec<i64>> {
        self.node_tracks_in(id, &smart::Context::default())
    }

    pub fn node_tracks_in(&self, id: i64, cx: &smart::Context) -> Result<Vec<i64>> {
        let (kind, query): (String, Option<String>) =
            self.conn
                .query_row("SELECT kind, query FROM nodes WHERE id = ?1", [id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
        match NodeKind::parse(&kind) {
            NodeKind::Folder => Ok(Vec::new()),
            NodeKind::Smartlist => {
                let Some(query) = query else {
                    return Ok(Vec::new());
                };
                let query = Query::parse(&query).map_err(|e| Error::Invalid(e.to_string()))?;
                Ok(self
                    .tracks()?
                    .into_iter()
                    .filter(|t| query.matches_in(t, cx))
                    .map(|t| t.id)
                    .collect())
            }
            NodeKind::Playlist => {
                let mut stmt = self.conn.prepare(
                    "SELECT track_id FROM playlist_entries WHERE node_id = ?1 AND track_id IS NOT NULL ORDER BY position",
                )?;
                let ids = stmt.query_map([id], |r| r.get(0))?;
                Ok(ids.collect::<rusqlite::Result<_>>()?)
            }
        }
    }

    pub fn add_to_playlist(&mut self, node: i64, tracks: &[i64]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let first: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM playlist_entries WHERE node_id = ?1",
            [node],
            |r| r.get(0),
        )?;
        for (position, &track) in (first..).zip(tracks) {
            let key = nml_io::primary_key_of(&tx, track)?;
            tx.execute(
                "INSERT INTO playlist_entries (node_id, position, track_id, key) VALUES (?1, ?2, ?3, ?4)",
                params![node, position, track, key],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_from_playlist(&mut self, node: i64, index: usize) -> Result<()> {
        self.conn.execute(
            "DELETE FROM playlist_entries WHERE rowid = (SELECT rowid FROM playlist_entries \
             WHERE node_id = ?1 AND track_id IS NOT NULL ORDER BY position LIMIT 1 OFFSET ?2)",
            params![node, index as i64],
        )?;
        Ok(())
    }

    pub fn sorting(&self, path: &str) -> Result<Vec<(u32, bool)>> {
        nml_io::sorting(&self.conn, path)
    }

    pub fn set_sorting(&mut self, path: &str, sorting: &[(u32, bool)]) -> Result<()> {
        nml_io::set_sorting(&self.conn, path, sorting)
    }

    pub fn sorting_path(&self, node: i64) -> Result<String> {
        let nodes = self.nodes()?;
        let mut names = Vec::new();
        let mut at = Some(node);
        while let Some(id) = at {
            let Some(n) = nodes.iter().find(|n| n.id == id) else {
                break;
            };
            names.push(n.name.clone());
            at = n.parent;
        }
        names.reverse();
        Ok(names.join("\\"))
    }

    pub fn import_nml(&mut self, path: impl AsRef<Path>) -> Result<ImportReport> {
        let nml = backspn_formats::nml::Nml::from_path(path)?;
        nml_io::import(&mut self.conn, &nml)
    }

    pub fn stats(&self) -> Result<Stats> {
        let mut stats = Stats {
            playlists: self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM nodes WHERE kind = 'playlist'",
                    [],
                    |r| r.get::<_, i64>(0),
                )?
                .unsigned_abs(),
            ..Stats::default()
        };
        let mut stmt = self.conn.prepare("SELECT path, analyzed FROM tracks")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            stats.tracks += 1;
            stats.analyzed += u64::from(row.get::<_, bool>(1)?);
            let extension = Path::new(&path)
                .extension()
                .and_then(|e| e.to_str())
                .map_or_else(String::new, str::to_ascii_lowercase);
            *stats.extensions.entry(extension).or_default() += 1;
        }
        Ok(stats)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub tracks: u64,
    pub analyzed: u64,
    pub playlists: u64,
    pub extensions: std::collections::BTreeMap<String, u64>,
}

#[cfg(test)]
mod tests;
