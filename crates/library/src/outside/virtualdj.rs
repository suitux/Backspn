use std::collections::HashMap;
use std::path::{Path, PathBuf};

use backspn_formats::nml::MusicalKey;
use backspn_formats::xml::{self, Element};

use crate::clock::LocalTime;
use crate::external::{ExternalLibrary, ExternalNode, ExternalTrack, Grid, natural_cmp};
use crate::{CuePoint, Error, Result};

const DATABASE: &str = "database.xml";

const LIST_FOLDERS: [(&str, &str); 4] = [
    ("MyLists", "My Lists"),
    ("My Lists", "My Lists"),
    ("Playlists", "Playlists"),
    ("Folders", "Folders"),
];

const HOT_CUES: i64 = 8;

pub fn find() -> Vec<PathBuf> {
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    candidates(home.as_deref(), &super::drives())
        .into_iter()
        .filter(|root| root.join(DATABASE).is_file())
        .collect()
}

fn candidates(home: Option<&Path>, drives: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = home {
        out.push(home.join("Documents").join("VirtualDJ"));
        if cfg!(target_os = "macos") {
            out.push(
                home.join("Library")
                    .join("Application Support")
                    .join("VirtualDJ"),
            );
        }
    }
    out.extend(drives.iter().map(|d| d.join("VirtualDJ")));
    out
}

pub fn read(root: &Path) -> Result<ExternalLibrary> {
    let file = root.join(DATABASE);
    let text = read_text(&file)?;
    let base = root.parent().unwrap_or(root);
    let mut lib = Builder::default();
    lib.database(&text, base)?;
    for (dir, shown) in LIST_FOLDERS {
        let dir = root.join(dir);
        if !dir.is_dir() {
            continue;
        }
        let children = lib.folder(&dir, base);
        if !children.is_empty() {
            lib.playlists.push(ExternalNode::Folder {
                name: shown.to_owned(),
                children,
            });
        }
    }
    Ok(ExternalLibrary {
        tracks: lib.tracks,
        playlists: lib.playlists,
    })
}

fn read_text(file: &Path) -> Result<String> {
    let bytes = std::fs::read(file).map_err(|source| Error::Io {
        path: file.to_path_buf(),
        source,
    })?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.trim_start_matches('\u{feff}').to_owned())
}

#[derive(Default)]
struct Builder {
    tracks: Vec<ExternalTrack>,
    playlists: Vec<ExternalNode>,
    by_path: HashMap<String, usize>,
    by_lower: HashMap<String, usize>,
    from_database: usize,
}

impl Builder {
    fn add(&mut self, track: ExternalTrack) -> usize {
        let at = self.tracks.len();
        self.by_path.entry(track.path.clone()).or_insert(at);
        self.by_lower.entry(track.path.to_lowercase()).or_insert(at);
        self.tracks.push(track);
        at
    }

    fn find(&self, path: &str) -> Option<usize> {
        self.by_path
            .get(path)
            .or_else(|| self.by_lower.get(&path.to_lowercase()))
            .copied()
    }

    fn database(&mut self, text: &str, base: &Path) -> Result<()> {
        let doc = xml::parse(text)?;
        if doc.root.name != "VirtualDJ_Database" {
            return Err(Error::Invalid(format!(
                "not a VirtualDJ database: <{}>",
                doc.root.name
            )));
        }
        for song in doc.root.children_named("Song") {
            if let Some(track) = song_track(song, base) {
                self.add(track);
            }
        }
        self.from_database = self.tracks.len();
        Ok(())
    }

    fn folder(&mut self, dir: &Path, base: &Path) -> Vec<ExternalNode> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort_by(|a, b| natural_cmp(&file_name(a), &file_name(b)));
        let mut out = Vec::new();
        for path in paths {
            let name = file_name(&path);
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                let children = self.folder(&path, base);
                out.push(ExternalNode::Folder {
                    name: name.strip_suffix(".subfolder").unwrap_or(&name).to_owned(),
                    children,
                });
                continue;
            }
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let entries = match ext.as_str() {
                "vdjfolder" | "xml" => read_text(&path)
                    .ok()
                    .and_then(|text| virtual_folder(&text, base)),
                "m3u" | "m3u8" => read_text(&path)
                    .ok()
                    .map(|text| m3u(&text, path.parent().unwrap_or(base))),
                _ => None,
            };
            if let Some(entries) = entries {
                let tracks = entries.into_iter().map(|e| self.entry(e)).collect();
                out.push(ExternalNode::Playlist { name: stem, tracks });
            }
        }
        out
    }

    fn entry(&mut self, entry: ExternalTrack) -> usize {
        let Some(at) = self.find(&entry.path) else {
            return self.add(entry);
        };
        if at >= self.from_database {
            fill_missing(&mut self.tracks[at], entry);
        }
        at
    }
}

fn fill_missing(track: &mut ExternalTrack, entry: ExternalTrack) {
    let text = |mine: &mut String, theirs: String| {
        if mine.is_empty() {
            *mine = theirs;
        }
    };
    text(&mut track.title, entry.title);
    text(&mut track.artist, entry.artist);
    text(&mut track.mix, entry.mix);
    if track.key_text.is_empty() {
        track.key_text = entry.key_text;
        track.musical_key = entry.musical_key;
    }
    track.bpm = track.bpm.or(entry.bpm);
    track.duration_s = track.duration_s.or(entry.duration_s);
    track.filesize_kib = track.filesize_kib.or(entry.filesize_kib);
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn song_track(song: &Element, base: &Path) -> Option<ExternalTrack> {
    let raw = song.attr("FilePath").unwrap_or_default();
    if raw.is_empty() || raw.contains("://") {
        return None;
    }
    let empty = Element::default();
    let tags = song.child("Tags").unwrap_or(&empty);
    let infos = song.child("Infos").unwrap_or(&empty);
    let scan = song.child("Scan").unwrap_or(&empty);
    let text = |e: &Element, k: &str| e.attr(k).unwrap_or_default().trim().to_owned();

    let bpm = scan
        .attr("Bpm")
        .and_then(period_bpm)
        .or_else(|| tags.attr("Bpm").and_then(any_bpm));
    let key_raw = Some(text(tags, "Key"))
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| text(scan, "Key"));
    let (key_text, musical_key) = key(&key_raw);
    let pois: Vec<&Element> = song.children_named("Poi").collect();
    let grid_bpm = bpm.filter(|_| scan.attr("Bpm").and_then(period_bpm).is_some());
    let anchor = scan.attr("Phase").and_then(number).or_else(|| {
        pois.iter()
            .filter(|p| p.attr("Type") == Some("beatgrid"))
            .filter_map(|p| p.attr("Pos").and_then(number))
            .reduce(f64::min)
    });
    Some(ExternalTrack {
        path: resolve(raw, base),
        title: text(tags, "Title"),
        artist: text(tags, "Author"),
        album: text(tags, "Album"),
        genre: text(tags, "Genre"),
        label: text(tags, "Label"),
        comment: song
            .child("Comment")
            .and_then(|c| c.text.as_deref())
            .unwrap_or_default()
            .trim()
            .to_owned(),
        remixer: text(tags, "Remixer"),
        producer: text(tags, "Composer"),
        mix: text(tags, "Remix"),
        key_text,
        musical_key,
        bpm,
        duration_s: infos.attr("SongLength").and_then(number),
        rating: tags
            .attr("Stars")
            .and_then(|s| s.trim().parse::<i64>().ok())
            .map_or(0, |s| s.clamp(0, 5) * 51),
        playcount: infos
            .attr("PlayCount")
            .and_then(|p| p.trim().parse::<i64>().ok())
            .unwrap_or(0)
            .max(0),
        import_date: infos.attr("FirstSeen").map(date).unwrap_or_default(),
        last_played: infos.attr("LastPlay").map(date).unwrap_or_default(),
        bitrate: infos
            .attr("Bitrate")
            .and_then(|b| b.trim().parse::<i64>().ok())
            .filter(|&b| b > 0)
            .map(|b| b * 1000),
        track_number: tags.attr("TrackNumber").and_then(leading_number),
        filesize_kib: song.attr("FileSize").and_then(kib),
        color: 0,
        grid: grid_bpm.zip(anchor).map(|(bpm, anchor_s)| Grid {
            bpm,
            anchor_s,
            markers_s: Vec::new(),
        }),
        cues: pois.iter().filter_map(|p| cue(p, bpm)).collect(),
    })
}

fn cue(poi: &Element, bpm: Option<f64>) -> Option<CuePoint> {
    let start_s = poi.attr("Pos").and_then(number)?;
    let kind = poi.attr("Type").unwrap_or("cue");
    let len_s = match kind {
        "cue" => 0.0,
        "loop" => {
            let beats = poi.attr("Size").and_then(number).unwrap_or(0.0);
            bpm.map_or(0.0, |bpm| beats * 60.0 / bpm)
        }
        _ => return None,
    };
    let hotcue = poi
        .attr("Num")
        .and_then(|n| n.trim().parse::<i64>().ok())
        .filter(|n| (1..=HOT_CUES).contains(n))
        .and_then(|n| u8::try_from(n - 1).ok());
    Some(CuePoint {
        kind: if len_s > 0.0 {
            CuePoint::LOOP
        } else {
            CuePoint::CUE
        },
        name: poi.attr("Name").unwrap_or_default().to_owned(),
        start_s,
        len_s,
        hotcue,
        display_order: 0,
        repeats: -1,
        color: poi.attr("Color").and_then(color),
    })
}

fn virtual_folder(text: &str, base: &Path) -> Option<Vec<ExternalTrack>> {
    let doc = xml::parse(text).ok()?;
    if doc.root.name != "VirtualFolder" {
        return None;
    }
    let mut songs: Vec<(Option<i64>, ExternalTrack)> = doc
        .root
        .children
        .iter()
        .filter(|c| c.name.eq_ignore_ascii_case("song"))
        .filter_map(|s| {
            let path = s.attr("path").filter(|p| !p.is_empty())?;
            let attr = |k: &str| s.attr(k).unwrap_or_default().trim().to_owned();
            let (key_text, musical_key) = key(&attr("key"));
            let idx = s.attr("idx").and_then(|i| i.trim().parse().ok());
            Some((
                idx,
                ExternalTrack {
                    path: resolve(path, base),
                    title: attr("title"),
                    artist: attr("artist"),
                    mix: attr("remix"),
                    key_text,
                    musical_key,
                    bpm: s.attr("bpm").and_then(any_bpm),
                    duration_s: s.attr("songlength").and_then(number),
                    filesize_kib: s.attr("size").and_then(kib),
                    ..ExternalTrack::default()
                },
            ))
        })
        .collect();
    songs.sort_by_key(|(idx, _)| idx.unwrap_or(i64::MAX));
    Some(songs.into_iter().map(|(_, t)| t).collect())
}

fn m3u(text: &str, dir: &Path) -> Vec<ExternalTrack> {
    let mut out = Vec::new();
    let mut extra: Option<&str> = None;
    for line in text.lines().map(str::trim) {
        if let Some(tags) = line.strip_prefix("#EXTVDJ:") {
            extra = Some(tags);
        } else if !line.is_empty() && !line.starts_with('#') {
            let tag = |name: &str| extra.map(|e| m3u_tag(e, name)).unwrap_or_default();
            out.push(ExternalTrack {
                path: resolve(line, dir),
                artist: tag("artist"),
                title: tag("title"),
                mix: tag("remix"),
                duration_s: number(&tag("songlength")),
                filesize_kib: kib(&tag("filesize")),
                ..ExternalTrack::default()
            });
            extra = None;
        }
    }
    out
}

fn m3u_tag(line: &str, name: &str) -> String {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    line.find(&open)
        .map(|at| &line[at + open.len()..])
        .and_then(|rest| rest.find(&close).map(|end| &rest[..end]))
        .map(|v| {
            v.replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&apos;", "'")
                .replace("&amp;", "&")
        })
        .unwrap_or_default()
}

fn resolve(path: &str, base: &Path) -> String {
    let bytes = path.as_bytes();
    let drive_letter = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    let unc = path.starts_with("\\\\");
    if Path::new(path).is_absolute() || path.starts_with('/') || drive_letter || unc {
        return path.to_owned();
    }
    base.join(path.trim_start_matches('\\'))
        .to_string_lossy()
        .into_owned()
}

fn number(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
}

fn period_bpm(text: &str) -> Option<f64> {
    number(text).filter(|&v| v > 0.0).map(|v| 60.0 / v)
}

fn any_bpm(text: &str) -> Option<f64> {
    let v = number(text).filter(|&v| v > 0.0)?;
    Some(if v < 10.0 { 60.0 / v } else { v })
}

fn key(text: &str) -> (String, Option<u8>) {
    match MusicalKey::parse(text) {
        Some(k) => (k.open_key(), Some(k.value())),
        None => (text.to_owned(), None),
    }
}

fn date(text: &str) -> String {
    text.trim()
        .parse::<u64>()
        .ok()
        .filter(|&s| s > 0)
        .map(|s| LocalTime::from_unix(s).date_text())
        .unwrap_or_default()
}

fn leading_number(text: &str) -> Option<i64> {
    let digits: String = text
        .trim()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok().filter(|&n| n > 0)
}

fn kib(text: &str) -> Option<i64> {
    let bytes: u64 = text.trim().parse().ok().filter(|&b| b > 0)?;
    i64::try_from(bytes / 1024).ok()
}

fn color(text: &str) -> Option<String> {
    let text = text.trim();
    let hex = text
        .strip_prefix('#')
        .or_else(|| text.strip_prefix("0x"))
        .or_else(|| text.strip_prefix("0X"));
    let argb = if let Some(h) = hex {
        u32::from_str_radix(h, 16).ok()?
    } else {
        let v: i64 = text.parse().ok()?;
        u32::try_from(v.rem_euclid(1 << 32)).ok()?
    };
    Some(format!("#{:06X}", argb & 0x00FF_FFFF))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATABASE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<VirtualDJ_Database Version="2024">
 <Song FilePath="/music/one.mp3" FileSize="2048000" Flag="0">
  <Tags Author="A &amp; B" Title="One" Genre="House" Album="LP" Composer="Prod" Label="Lbl"
    Remix="Extended Mix" Remixer="Rmx" TrackNumber="3/12" Year="2020" Stars="4" Key="Am" Bpm="0.5" />
  <Infos SongLength="300.5" FirstSeen="1700000000" LastPlay="1710000000" PlayCount="7" Bitrate="320" />
  <Comment>Nice one</Comment>
  <Scan Version="801" Bpm="0.468750" AltBpm="0.703125" Volume="1.0" Key="C#m" Flag="32768" />
  <Poi Pos="0.512" Type="beatgrid" />
  <Poi Pos="3.7" Type="automix" Point="realStart" />
  <Poi Name="Drop" Pos="30.25" Num="3" Color="-65536" />
  <Poi Name="Memo" Pos="12.0" Type="cue" />
  <Poi Pos="60.0" Type="loop" Size="4" Num="5" Color="4278255360" />
  <Poi Pos="90.0" Type="remix" Num="1" />
 </Song>
 <Song FilePath="Music\two.mp3" FileSize="1024">
  <Tags Title="Two" Bpm="124" />
  <Scan Bpm="0.5" Phase="0.25" />
  <Poi Pos="1.0" Type="beatgrid" />
 </Song>
 <Song FilePath="netsearch://dz123">
  <Tags Title="Online" />
 </Song>
</VirtualDJ_Database>
"#;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("backspn-vdj-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn songs_become_tracks_with_cues_and_grid() {
        let drive = temp("songs");
        let root = drive.join("VirtualDJ");
        write(&root.join(DATABASE), DATABASE_XML);
        let lib = read(&root).unwrap();
        assert_eq!(lib.tracks.len(), 2, "the online entry is left out");
        let t = &lib.tracks[0];
        assert_eq!(t.path, "/music/one.mp3");
        assert_eq!(
            (t.title.as_str(), t.artist.as_str(), t.album.as_str()),
            ("One", "A & B", "LP")
        );
        assert_eq!((t.producer.as_str(), t.label.as_str()), ("Prod", "Lbl"));
        assert_eq!(
            (t.mix.as_str(), t.remixer.as_str()),
            ("Extended Mix", "Rmx")
        );
        assert_eq!(t.comment, "Nice one");
        assert_eq!(t.key_text, MusicalKey::parse("Am").unwrap().open_key());
        assert_eq!(t.bpm, Some(128.0), "Scan wins over Tags");
        assert_eq!(t.duration_s, Some(300.5));
        assert_eq!((t.rating, t.playcount), (204, 7));
        assert_eq!(
            t.import_date,
            LocalTime::from_unix(1_700_000_000).date_text()
        );
        assert_eq!(
            t.last_played,
            LocalTime::from_unix(1_710_000_000).date_text()
        );
        assert_eq!(t.bitrate, Some(320_000));
        assert_eq!(t.track_number, Some(3));
        assert_eq!(t.filesize_kib, Some(2000));
        let g = t.grid.as_ref().unwrap();
        assert_eq!((g.bpm, g.anchor_s, g.markers_s.len()), (128.0, 0.512, 0));

        assert_eq!(t.cues.len(), 3, "automix and remix points are not cues");
        let drop = &t.cues[0];
        assert_eq!(
            (drop.kind, drop.hotcue, drop.name.as_str()),
            (CuePoint::CUE, Some(2), "Drop")
        );
        assert_eq!(drop.color.as_deref(), Some("#FF0000"));
        assert_eq!((t.cues[1].hotcue, t.cues[1].start_s), (None, 12.0));
        let lp = &t.cues[2];
        assert_eq!((lp.kind, lp.hotcue), (CuePoint::LOOP, Some(4)));
        assert!((lp.len_s - 1.875).abs() < 1e-9, "4 beats at 128 BPM");
        assert_eq!(lp.color.as_deref(), Some("#00FF00"));

        let two = &lib.tracks[1];
        assert_eq!(two.path, drive.join("Music\\two.mp3").to_string_lossy());
        assert_eq!(two.bpm, Some(120.0));
        assert_eq!(
            two.grid.as_ref().unwrap().anchor_s,
            0.25,
            "Phase wins over the Poi"
        );
        let _ = std::fs::remove_dir_all(&drive);
    }

    #[test]
    fn list_folders_nest_and_add_unknown_tracks() {
        let root = temp("lists").join("VirtualDJ");
        write(&root.join(DATABASE), DATABASE_XML);
        write(
            &root.join("Folders/Gigs.subfolder/Friday.vdjfolder"),
            r#"<VirtualFolder noDuplicates="no" ordered="yes">
  <song path="/music/new.mp3" size="4096" artist="N" title="New" songlength="200.0" bpm="0.5" key="8A" idx="1" />
  <song path="/MUSIC/ONE.MP3" idx="0" />
</VirtualFolder>"#,
        );
        write(
            &root.join("Folders/Smart.vdjfolder"),
            r#"<FilterFolder query="bpm > 120" />"#,
        );
        write(
            &root.join("Playlists/Warmup.m3u"),
            "#EXTVDJ:<filesize>2048</filesize><artist>X</artist><title>Old</title>\n/music/old.mp3\n/music/one.mp3\n",
        );
        write(&root.join("MyLists/Set 10.vdjfolder"), "<VirtualFolder />");
        write(
            &root.join("MyLists/Set 9.vdjfolder"),
            r#"<VirtualFolder><song path="/music/new.mp3" /></VirtualFolder>"#,
        );
        let lib = read(&root).unwrap();
        let names: Vec<&str> = lib.playlists.iter().map(ExternalNode::name).collect();
        assert_eq!(names, ["My Lists", "Playlists", "Folders"]);

        let ExternalNode::Folder { children, .. } = &lib.playlists[2] else {
            panic!()
        };
        assert_eq!(children.len(), 1, "the filter folder is left out");
        assert_eq!(children[0].name(), "Gigs");
        let ExternalNode::Folder { children: gigs, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(gigs[0].name(), "Friday");
        assert_eq!(gigs[0].all_tracks(), [0, 2]);
        let new = &lib.tracks[2];
        assert_eq!((new.title.as_str(), new.artist.as_str()), ("New", "N"));
        assert_eq!(
            (new.bpm, new.duration_s, new.filesize_kib),
            (Some(120.0), Some(200.0), Some(4))
        );
        assert_eq!(
            new.musical_key,
            MusicalKey::parse("8A").map(MusicalKey::value)
        );

        let ExternalNode::Folder { children: mine, .. } = &lib.playlists[0] else {
            panic!()
        };
        let mine: Vec<&str> = mine.iter().map(ExternalNode::name).collect();
        assert_eq!(mine, ["Set 9", "Set 10"]);
        assert_eq!(lib.playlists[0].all_tracks(), [2]);

        let old = &lib.tracks[3];
        assert_eq!(
            (old.path.as_str(), old.title.as_str(), old.filesize_kib),
            ("/music/old.mp3", "Old", Some(2))
        );
        assert_eq!(lib.playlists[1].all_tracks(), [3, 0]);
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn a_missing_or_foreign_database_is_an_error() {
        let root = temp("bad");
        assert!(read(&root).is_err());
        write(&root.join(DATABASE), "<DJ_PLAYLISTS/>");
        assert!(matches!(read(&root), Err(Error::Invalid(_))));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn values_convert() {
        assert_eq!(period_bpm("0.46875"), Some(128.0));
        assert_eq!(period_bpm("0"), None);
        assert_eq!(any_bpm("124.5"), Some(124.5));
        assert_eq!(any_bpm("0.5"), Some(120.0));
        assert_eq!(color("#00ff10").as_deref(), Some("#00FF10"));
        assert_eq!(color("4294901760").as_deref(), Some("#FF0000"));
        assert_eq!(color("x"), None);
        assert_eq!(date("0"), "");
        assert_eq!(leading_number("07"), Some(7));
        assert_eq!(leading_number(""), None);
        assert_eq!(
            resolve("D:\\Music\\a.mp3", Path::new("/x")),
            "D:\\Music\\a.mp3"
        );
        assert_eq!(m3u_tag("<title>A &amp; B</title>", "title"), "A & B");
        let roots = candidates(Some(Path::new("/home/u")), &[PathBuf::from("/Volumes/USB")]);
        assert_eq!(roots[0], Path::new("/home/u/Documents/VirtualDJ"));
        assert_eq!(roots.last().unwrap(), Path::new("/Volumes/USB/VirtualDJ"));
    }
}
