use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use backspn_formats::dj_playlists::Colour;
use backspn_formats::nml::MusicalKey;
use rusqlite::{Connection, MAIN_DB, types::Value};

use super::anlz;
use super::engine::{date as unix_date, has_table, int, real, text};
use crate::external::{self, ExternalLibrary, ExternalNode, ExternalTrack};

const DATABASE: &str = "PIONEER/rekordbox/exportLibrary.db";
const PASSPHRASE: &[u8] = b"r8gddnr4k847830ar6cqzbkk0el6qytmb3trbbx805jm74vez64i5o8fnrqryqls";
const MAX_DATABASE: u64 = 1 << 30;
const MAX_ANALYSIS: u64 = 32 << 20;
const FOLDER: i64 = 1;
const COLOURS: [Colour; 8] = [
    Colour::Pink,
    Colour::Red,
    Colour::Orange,
    Colour::Yellow,
    Colour::Green,
    Colour::Aqua,
    Colour::Blue,
    Colour::Purple,
];

type Row = HashMap<String, Value>;

pub fn find() -> Vec<PathBuf> {
    super::drives()
        .into_iter()
        .filter(|root| root.join(DATABASE).is_file())
        .collect()
}

fn read_capped(path: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    std::fs::File::open(path)?.take(max).read_to_end(&mut out)?;
    Ok(out)
}

pub fn read(root: &Path) -> crate::Result<ExternalLibrary> {
    let path = root.join(DATABASE);
    let data = read_capped(&path, MAX_DATABASE).map_err(|source| crate::Error::Io {
        path: path.clone(),
        source,
    })?;
    let plain = super::sqlcipher::decrypt(&data, PASSPHRASE)?;
    let mut conn = Connection::open_in_memory()?;
    conn.deserialize_read_exact(MAIN_DB, plain.as_slice(), plain.len(), true)?;
    read_db(&conn, root)
}

fn rows(conn: &Connection, table: &str) -> crate::Result<Vec<Row>> {
    if !has_table(conn, table)? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\""))?;
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|n| (*n).to_owned())
        .collect();
    let found = stmt.query_map([], |r| {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| Ok((n.clone(), r.get::<_, Value>(i)?)))
            .collect::<rusqlite::Result<Row>>()
    })?;
    Ok(found.collect::<rusqlite::Result<Vec<Row>>>()?)
}

fn get<'a>(row: &'a Row, name: &str) -> &'a Value {
    row.get(name).unwrap_or(&Value::Null)
}

fn id(row: &Row, table: &str) -> Option<i64> {
    int(get(row, &format!("{table}_id"))).or_else(|| int(get(row, "id")))
}

fn names(conn: &Connection, table: &str) -> crate::Result<HashMap<i64, String>> {
    Ok(rows(conn, table)?
        .iter()
        .filter_map(|r| Some((id(r, table)?, text(get(r, "name")))))
        .collect())
}

struct Names {
    artists: HashMap<i64, String>,
    albums: HashMap<i64, String>,
    genres: HashMap<i64, String>,
    labels: HashMap<i64, String>,
    keys: HashMap<i64, String>,
}

impl Names {
    fn read(conn: &Connection) -> crate::Result<Self> {
        Ok(Self {
            artists: names(conn, "artist")?,
            albums: names(conn, "album")?,
            genres: names(conn, "genre")?,
            labels: names(conn, "label")?,
            keys: names(conn, "key")?,
        })
    }
}

fn name(row: &Row, column: &str, names: &HashMap<i64, String>) -> String {
    int(get(row, column))
        .and_then(|i| names.get(&i))
        .cloned()
        .unwrap_or_default()
}

fn on_drive(root: &Path, path: &str) -> PathBuf {
    root.join(path.trim_start_matches(['/', '\\']))
}

fn added(v: &Value) -> String {
    match v {
        Value::Text(t) => external::date(t.get(..10).unwrap_or(t)),
        other => unix_date(int(other)),
    }
}

pub fn colour(id: i64) -> u8 {
    usize::try_from(id)
        .ok()
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| COLOURS.get(i))
        .map_or(0, |c| external::color(*c))
}

fn track(row: &Row, names: &Names, root: &Path) -> Option<ExternalTrack> {
    let path = text(get(row, "path"));
    if path.trim().is_empty() {
        return None;
    }
    let key_name = name(row, "key_id", &names.keys);
    let key = MusicalKey::parse(key_name.trim());
    let mut track = ExternalTrack {
        path: on_drive(root, &path).to_string_lossy().into_owned(),
        title: text(get(row, "title")),
        artist: name(row, "artist_id_artist", &names.artists),
        album: name(row, "album_id", &names.albums),
        genre: name(row, "genre_id", &names.genres),
        label: name(row, "label_id", &names.labels),
        comment: text(get(row, "djComment")),
        remixer: name(row, "artist_id_remixer", &names.artists),
        producer: name(row, "artist_id_composer", &names.artists),
        mix: text(get(row, "subtitle")),
        key_text: key.map_or(key_name, MusicalKey::open_key),
        musical_key: key.map(MusicalKey::value),
        bpm: real(get(row, "bpmx100"))
            .filter(|b| *b > 0.0)
            .map(|b| b / 100.0),
        duration_s: real(get(row, "length")).filter(|l| *l > 0.0),
        rating: int(get(row, "rating")).unwrap_or(0).clamp(0, 5) * 51,
        playcount: int(get(row, "djPlayCount")).unwrap_or(0).max(0),
        import_date: added(get(row, "dateAdded")),
        last_played: String::new(),
        bitrate: int(get(row, "bitrate"))
            .filter(|b| *b > 0)
            .map(|b| b * 1000),
        track_number: int(get(row, "trackNo")).filter(|n| *n > 0),
        filesize_kib: int(get(row, "fileSize"))
            .filter(|b| *b > 0)
            .map(|b| b / 1024),
        color: int(get(row, "color_id")).map_or(0, colour),
        grid: None,
        cues: Vec::new(),
    };
    let analysis = text(get(row, "analysisDataFilePath"));
    if !analysis.trim().is_empty() {
        let basic_path = on_drive(root, &analysis);
        let basic = read_capped(&basic_path, MAX_ANALYSIS).unwrap_or_default();
        let extended =
            read_capped(&basic_path.with_extension("EXT"), MAX_ANALYSIS).unwrap_or_default();
        track.grid = anlz::grid(&anlz::beats(&basic));
        track.cues = anlz::cues(&basic, &extended)
            .iter()
            .map(anlz::cue_point)
            .collect();
    }
    Some(track)
}

fn read_db(conn: &Connection, root: &Path) -> crate::Result<ExternalLibrary> {
    let names = Names::read(conn)?;
    let mut index = HashMap::new();
    let mut tracks = Vec::new();
    for row in rows(conn, "content")? {
        let (Some(id), Some(t)) = (id(&row, "content"), track(&row, &names, root)) else {
            continue;
        };
        index.insert(id, tracks.len());
        tracks.push(t);
    }
    let playlists = read_playlists(conn, &index)?;
    Ok(ExternalLibrary { tracks, playlists })
}

struct List {
    id: i64,
    parent: i64,
    order: i64,
    name: String,
    folder: bool,
}

fn read_playlists(
    conn: &Connection,
    index: &HashMap<i64, usize>,
) -> crate::Result<Vec<ExternalNode>> {
    let lists: Vec<List> = rows(conn, "playlist")?
        .iter()
        .filter_map(|r| {
            Some(List {
                id: id(r, "playlist")?,
                parent: int(get(r, "playlist_id_parent")).unwrap_or(0),
                order: int(get(r, "sequenceNo")).unwrap_or(0),
                name: text(get(r, "name")),
                folder: int(get(r, "attribute")) == Some(FOLDER),
            })
        })
        .collect();
    let mut entries: HashMap<i64, Vec<(i64, usize)>> = HashMap::new();
    for r in rows(conn, "playlist_content")? {
        let (Some(list), Some(&track)) = (
            int(get(&r, "playlist_id")),
            int(get(&r, "content_id")).and_then(|c| index.get(&c)),
        ) else {
            continue;
        };
        let order = int(get(&r, "sequenceNo")).unwrap_or(0);
        entries.entry(list).or_default().push((order, track));
    }
    for e in entries.values_mut() {
        e.sort_by_key(|(order, _)| *order);
    }
    let ids: HashSet<i64> = lists.iter().map(|l| l.id).collect();
    let mut children: HashMap<i64, Vec<&List>> = HashMap::new();
    for l in &lists {
        let parent = if ids.contains(&l.parent) && l.parent != l.id {
            l.parent
        } else {
            0
        };
        children.entry(parent).or_default().push(l);
    }
    for c in children.values_mut() {
        c.sort_by_key(|l| (l.order, l.id));
    }
    let mut seen = HashSet::new();
    Ok(nodes(0, &children, &entries, &mut seen))
}

fn nodes(
    parent: i64,
    children: &HashMap<i64, Vec<&List>>,
    entries: &HashMap<i64, Vec<(i64, usize)>>,
    seen: &mut HashSet<i64>,
) -> Vec<ExternalNode> {
    let Some(lists) = children.get(&parent) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for l in lists {
        if !seen.insert(l.id) {
            continue;
        }
        if l.folder || children.contains_key(&l.id) {
            out.push(ExternalNode::Folder {
                name: l.name.clone(),
                children: nodes(l.id, children, entries, seen),
            });
        } else {
            out.push(ExternalNode::Playlist {
                name: l.name.clone(),
                tracks: entries
                    .get(&l.id)
                    .map(|e| e.iter().map(|(_, t)| *t).collect())
                    .unwrap_or_default(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::anlz::tests::{basic_cues, beat_section, extended_cues, file};
    use super::*;

    const SCHEMA: &str = "
        CREATE TABLE content (content_id INTEGER PRIMARY KEY, title TEXT, subtitle TEXT,
            bpmx100 INTEGER, length INTEGER, trackNo INTEGER, artist_id_artist INTEGER,
            artist_id_remixer INTEGER, artist_id_composer INTEGER, album_id INTEGER,
            genre_id INTEGER, label_id INTEGER, key_id INTEGER, color_id INTEGER,
            djComment TEXT, rating INTEGER, dateAdded TEXT, path TEXT, fileSize INTEGER,
            bitrate INTEGER, djPlayCount INTEGER, analysisDataFilePath TEXT);
        CREATE TABLE artist (artist_id INTEGER PRIMARY KEY, name TEXT);
        CREATE TABLE album (album_id INTEGER PRIMARY KEY, name TEXT);
        CREATE TABLE genre (genre_id INTEGER PRIMARY KEY, name TEXT);
        CREATE TABLE label (label_id INTEGER PRIMARY KEY, name TEXT);
        CREATE TABLE key (key_id INTEGER PRIMARY KEY, name TEXT);
        CREATE TABLE playlist (playlist_id INTEGER PRIMARY KEY, sequenceNo INTEGER, name TEXT,
            attribute INTEGER, playlist_id_parent INTEGER);
        CREATE TABLE playlist_content (playlist_id INTEGER, content_id INTEGER,
            sequenceNo INTEGER);
        INSERT INTO artist VALUES (1, 'Artist'), (2, 'Remixer'), (3, 'Composer');
        INSERT INTO album VALUES (1, 'Album');
        INSERT INTO genre VALUES (1, 'Techno');
        INSERT INTO label VALUES (1, 'Label');
        INSERT INTO key VALUES (1, 'Am'), (2, '8B');
        INSERT INTO content VALUES
            (10, 'One', 'Extended Mix', 12800, 300, 4, 1, 2, 3, 1, 1, 1, 1, 2, 'Nice', 4,
             '2024-05-01', '/Contents/one.mp3', 10240, 320, 7,
             '/PIONEER/USBANLZ/P001/0001/ANLZ0000.DAT'),
            (11, 'Two', '', 0, 0, 0, NULL, NULL, NULL, NULL, NULL, NULL, 2, NULL, '', 0,
             NULL, '/Contents/two.flac', 0, 0, 0, ''),
            (12, 'No path', '', 0, 0, 0, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, '',
             0, NULL, '', 0, 0, 0, '');
        INSERT INTO playlist VALUES
            (1, 2, 'Sets', 1, 0), (2, 1, 'Warm-up', 0, 0), (3, 1, 'Peak', 0, 1),
            (4, 1, 'Empty', 1, 1);
        INSERT INTO playlist_content VALUES (2, 11, 2), (2, 10, 1), (2, 99, 3), (3, 10, 1);
    ";

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("backspn-ol-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn tracks_and_playlists_are_read() {
        let root = temp_root("lib");
        let anlz = root.join("PIONEER/USBANLZ/P001/0001");
        std::fs::create_dir_all(&anlz).unwrap();
        std::fs::write(
            anlz.join("ANLZ0000.DAT"),
            file(&[
                beat_section(&[(12800, 50), (12800, 519)]),
                basic_cues(&[(1, 1, 1000, u32::MAX)]),
            ]),
        )
        .unwrap();
        std::fs::write(
            anlz.join("ANLZ0000.EXT"),
            file(&[extended_cues(&[(
                1,
                1,
                1000,
                u32::MAX,
                "Intro",
                [1, 1, 2, 3],
            )])]),
        )
        .unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        let lib = read_db(&conn, &root).unwrap();
        std::fs::remove_dir_all(&root).ok();

        assert_eq!(lib.tracks.len(), 2);
        let one = &lib.tracks[0];
        assert_eq!(one.path, root.join("Contents/one.mp3").to_string_lossy());
        assert_eq!(
            (
                one.title.as_str(),
                one.artist.as_str(),
                one.remixer.as_str()
            ),
            ("One", "Artist", "Remixer")
        );
        assert_eq!(
            (one.producer.as_str(), one.album.as_str(), one.mix.as_str()),
            ("Composer", "Album", "Extended Mix")
        );
        assert_eq!(
            (one.genre.as_str(), one.label.as_str()),
            ("Techno", "Label")
        );
        assert_eq!(one.comment, "Nice");
        assert_eq!(one.bpm, Some(128.0));
        assert_eq!(one.duration_s, Some(300.0));
        assert_eq!((one.rating, one.playcount, one.color), (204, 7, 1));
        assert_eq!(one.import_date, "2024/5/1");
        assert_eq!(
            (one.bitrate, one.filesize_kib, one.track_number),
            (Some(320_000), Some(10), Some(4))
        );
        assert_eq!(
            one.musical_key,
            MusicalKey::parse("Am").map(MusicalKey::value)
        );
        let grid = one.grid.as_ref().unwrap();
        assert!((grid.bpm - 128.0).abs() < 1e-9 && (grid.anchor_s - 0.05).abs() < 1e-9);
        assert_eq!(one.cues.len(), 1);
        assert_eq!(one.cues[0].name, "Intro");
        assert_eq!(one.cues[0].hotcue, Some(0));
        assert_eq!(one.cues[0].color.as_deref(), Some("#010203"));

        let two = &lib.tracks[1];
        assert_eq!((two.bpm, two.grid.as_ref(), two.color), (None, None, 0));
        assert_eq!(
            two.musical_key,
            MusicalKey::parse("8B").map(MusicalKey::value)
        );

        assert_eq!(
            lib.playlists,
            vec![
                ExternalNode::Playlist {
                    name: "Warm-up".into(),
                    tracks: vec![0, 1],
                },
                ExternalNode::Folder {
                    name: "Sets".into(),
                    children: vec![
                        ExternalNode::Playlist {
                            name: "Peak".into(),
                            tracks: vec![0],
                        },
                        ExternalNode::Folder {
                            name: "Empty".into(),
                            children: vec![],
                        },
                    ],
                },
            ]
        );
    }

    #[test]
    fn missing_tables_give_an_empty_library() {
        let conn = Connection::open_in_memory().unwrap();
        let lib = read_db(&conn, Path::new("/x")).unwrap();
        assert_eq!(lib, ExternalLibrary::default());
    }

    #[test]
    fn colours_follow_the_device_order() {
        assert_eq!(colour(1), external::color(Colour::Pink));
        assert_eq!(colour(8), external::color(Colour::Purple));
        assert_eq!((colour(0), colour(9), colour(-1)), (0, 0, 0));
    }

    #[test]
    #[ignore = "needs BACKSPN_ONELIBRARY pointing to a copy of a drive root"]
    fn real_onelibrary_reads() {
        let root = std::env::var("BACKSPN_ONELIBRARY").expect("BACKSPN_ONELIBRARY");
        let lib = read(Path::new(&root)).unwrap();
        assert_ne!(lib.tracks.len(), 0);
        let with_grid = lib.tracks.iter().filter(|t| t.grid.is_some()).count();
        let with_cues = lib.tracks.iter().filter(|t| !t.cues.is_empty()).count();
        eprintln!(
            "{} tracks, {with_grid} with grid, {with_cues} with cues, {} top-level lists",
            lib.tracks.len(),
            lib.playlists.len()
        );
    }
}
