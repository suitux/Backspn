use std::path::Path;

use crate::Error;
use crate::xml::{self, Element};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MusicLibrary {
    pub tracks: Vec<MusicTrack>,
    pub playlists: Vec<MusicPlaylist>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MusicTrack {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub comments: String,
    pub size_kib: u64,
    pub bitrate: u32,
    pub seconds: u32,
    pub track_number: u32,
    pub bpm: u32,
    pub rating: u8,
    pub date_added: String,
    pub last_played: String,
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MusicPlaylist {
    pub name: String,
    pub id: String,
    pub parent: Option<String>,
    pub tracks: Vec<i64>,
    pub folder: bool,
    pub smart: bool,
    pub master: bool,
}

pub fn is_music_kind(kind: &str) -> bool {
    !matches!(
        kind,
        "Book" | "Purchased book" | "Protected book" | "iPhone/iPod touch/iPad app"
    )
}

pub fn rating(value: i64) -> u8 {
    u8::try_from(value.clamp(0, 100) * 255 / 100).unwrap_or(0)
}

pub fn seconds(ms: u64) -> u32 {
    u32::try_from((ms + 500) / 1000).unwrap_or(u32::MAX)
}

pub fn url_path(url: &str) -> String {
    let rest = url
        .strip_prefix("file://localhost")
        .or_else(|| url.strip_prefix("file://"))
        .unwrap_or(url);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(&h), Some(&l)) = (bytes.get(i + 1), bytes.get(i + 2))
            && let (Some(h), Some(l)) = (hex(h), hex(l))
        {
            out.push(u8::try_from(h * 16 + l).unwrap_or(0));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn read(path: &Path) -> Result<MusicLibrary, Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&text)
}

enum Value<'a> {
    Dict(Vec<(&'a str, &'a Element)>),
    Text(&'a str),
    Bool(bool),
    Other,
}

fn value(e: &Element) -> Value<'_> {
    match e.name.as_str() {
        "dict" => {
            let mut pairs = Vec::new();
            let mut children = e.children.iter();
            while let Some(k) = children.next() {
                if k.name == "key"
                    && let Some(v) = children.next()
                {
                    pairs.push((k.text.as_deref().unwrap_or_default(), v));
                }
            }
            Value::Dict(pairs)
        }
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "string" | "integer" | "date" | "real" => {
            Value::Text(e.text.as_deref().unwrap_or_default())
        }
        _ => Value::Other,
    }
}

fn dict(e: &Element) -> Vec<(&str, &Element)> {
    match value(e) {
        Value::Dict(pairs) => pairs,
        _ => Vec::new(),
    }
}

fn get<'a>(pairs: &[(&str, &'a Element)], key: &str) -> Option<&'a Element> {
    pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

fn text(pairs: &[(&str, &Element)], key: &str) -> String {
    match get(pairs, key).map(value) {
        Some(Value::Text(t)) => t.to_owned(),
        _ => String::new(),
    }
}

fn int(pairs: &[(&str, &Element)], key: &str) -> i64 {
    text(pairs, key).trim().parse().unwrap_or(0)
}

fn flag(pairs: &[(&str, &Element)], key: &str) -> bool {
    matches!(get(pairs, key).map(value), Some(Value::Bool(true)))
}

fn date(text: &str) -> String {
    let day = text.split('T').next().unwrap_or_default();
    let mut parts = day.split('-').map(|p| p.parse::<u32>().ok());
    match (parts.next(), parts.next(), parts.next()) {
        (Some(Some(y)), Some(Some(m)), Some(Some(d))) => format!("{y}/{m}/{d}"),
        _ => String::new(),
    }
}

pub fn parse(input: &str) -> Result<MusicLibrary, Error> {
    let root = xml::parse(input)?.root;
    let top = root
        .children
        .iter()
        .find(|c| c.name == "dict")
        .ok_or_else(|| Error::Invalid {
            element: root.name.clone(),
            message: "not a music library".into(),
        })?;
    let top = dict(top);
    let mut tracks = Vec::new();
    for (key, t) in get(&top, "Tracks").map(dict).unwrap_or_default() {
        let t = dict(t);
        if flag(&t, "Movie") || !is_music_kind(&text(&t, "Kind")) {
            continue;
        }
        tracks.push(MusicTrack {
            id: key.parse().unwrap_or(0),
            title: text(&t, "Name"),
            artist: text(&t, "Artist"),
            album: text(&t, "Album"),
            genre: text(&t, "Genre"),
            comments: text(&t, "Comments"),
            size_kib: u64::try_from(int(&t, "Size")).unwrap_or(0) / 1024,
            bitrate: u32::try_from(int(&t, "Bit Rate")).unwrap_or(0) * 1000,
            seconds: seconds(u64::try_from(int(&t, "Total Time")).unwrap_or(0)),
            track_number: u32::try_from(int(&t, "Track Number")).unwrap_or(0),
            bpm: u32::try_from(int(&t, "BPM")).unwrap_or(0),
            rating: rating(int(&t, "Rating")),
            date_added: date(&text(&t, "Date Added")),
            last_played: date(&text(&t, "Play Date UTC")),
            path: url_path(&text(&t, "Location")),
        });
    }
    let mut playlists = Vec::new();
    if let Some(list) = get(&top, "Playlists") {
        for p in list.children.iter().filter(|c| c.name == "dict") {
            let p = dict(p);
            if int(&p, "Distinguished Kind") != 0 {
                continue;
            }
            let ids = get(&p, "Playlist Items")
                .map(|items| {
                    items
                        .children
                        .iter()
                        .filter(|c| c.name == "dict")
                        .map(|i| int(&dict(i), "Track ID"))
                        .collect()
                })
                .unwrap_or_default();
            let parent = text(&p, "Parent Persistent ID");
            playlists.push(MusicPlaylist {
                name: text(&p, "Name"),
                id: text(&p, "Playlist Persistent ID"),
                parent: (!parent.is_empty()).then_some(parent),
                tracks: ids,
                folder: flag(&p, "Folder"),
                smart: get(&p, "Smart Info")
                    .is_some_and(|s| s.text.as_deref().is_some_and(|t| !t.trim().is_empty())),
                master: flag(&p, "Master"),
            });
        }
    }
    Ok(MusicLibrary { tracks, playlists })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Major Version</key><integer>1</integer>
  <key>Tracks</key>
  <dict>
    <key>101</key>
    <dict>
      <key>Track ID</key><integer>101</integer>
      <key>Name</key><string>Song &amp; Dance</string>
      <key>Artist</key><string>Band</string>
      <key>Kind</key><string>MPEG audio file</string>
      <key>Size</key><integer>4096000</integer>
      <key>Total Time</key><integer>201500</integer>
      <key>Bit Rate</key><integer>320</integer>
      <key>BPM</key><integer>125</integer>
      <key>Rating</key><integer>80</integer>
      <key>Date Added</key><date>2024-05-06T10:00:00Z</date>
      <key>Location</key><string>file:///Users/me/Music/Song%20%26%20Dance.mp3</string>
    </dict>
    <key>102</key>
    <dict><key>Name</key><string>A Film</string><key>Movie</key><true/></dict>
    <key>103</key>
    <dict><key>Name</key><string>Novel</string><key>Kind</key><string>Book</string></dict>
  </dict>
  <key>Playlists</key>
  <array>
    <dict><key>Name</key><string>Library</string><key>Master</key><true/>
      <key>Playlist Persistent ID</key><string>A1</string>
      <key>Playlist Items</key><array><dict><key>Track ID</key><integer>101</integer></dict></array></dict>
    <dict><key>Name</key><string>Music</string><key>Distinguished Kind</key><integer>4</integer>
      <key>Playlist Persistent ID</key><string>A2</string></dict>
    <dict><key>Name</key><string>Sets</string><key>Folder</key><true/>
      <key>Playlist Persistent ID</key><string>F1</string></dict>
    <dict><key>Name</key><string>Friday</string>
      <key>Playlist Persistent ID</key><string>P1</string>
      <key>Parent Persistent ID</key><string>F1</string>
      <key>Playlist Items</key><array><dict><key>Track ID</key><integer>101</integer></dict></array></dict>
  </array>
</dict>
</plist>"#;

    #[test]
    fn tracks_read_with_every_field() {
        let lib = parse(FILE).unwrap();
        assert_eq!(lib.tracks.len(), 1, "no movies or books");
        let t = &lib.tracks[0];
        assert_eq!((t.id, t.title.as_str()), (101, "Song & Dance"));
        assert_eq!((t.size_kib, t.bitrate, t.seconds), (4000, 320_000, 202));
        assert_eq!((t.bpm, t.rating), (125, 204));
        assert_eq!(t.date_added, "2024/5/6");
        assert_eq!(t.path, "/Users/me/Music/Song & Dance.mp3");
    }

    #[test]
    fn playlists_keep_folders_and_drop_system_lists() {
        let lib = parse(FILE).unwrap();
        let names: Vec<&str> = lib.playlists.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Library", "Sets", "Friday"]);
        assert!(lib.playlists[0].master);
        assert!(lib.playlists[1].folder);
        assert_eq!(lib.playlists[2].parent.as_deref(), Some("F1"));
        assert_eq!(lib.playlists[2].tracks, [101]);
    }

    #[test]
    fn conversions() {
        assert_eq!(rating(100), 255);
        assert_eq!(rating(20), 51);
        assert_eq!(seconds(499), 0);
        assert_eq!(seconds(500), 1);
        assert_eq!(url_path("file://localhost/a%20b.mp3"), "/a b.mp3");
        assert!(!is_music_kind("Purchased book"));
        assert!(is_music_kind("AAC audio file"));
    }
}
