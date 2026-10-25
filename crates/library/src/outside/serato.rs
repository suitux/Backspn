use std::collections::{BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};

use backspn_formats::nml::MusicalKey;
use base64::Engine as _;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType};
use lofty::flac::FlacFile;
use lofty::id3::v2::{Frame, GeneralEncapsulatedObject, Id3v2Tag};
use lofty::iff::aiff::AiffFile;
use lofty::iff::wav::WavFile;
use lofty::mp4::{AtomData, AtomIdent, Mp4File};
use lofty::mpeg::MpegFile;

use crate::CuePoint;
use crate::external::{ExternalLibrary, ExternalNode, ExternalTrack, Grid, natural_cmp};

const FOLDER: &str = "_Serato_";
const DATABASE: &str = "database V2";
const NESTING: &str = "%%";
const MAX_EXTRA_MARKERS: usize = 7;
const HOT_CUES: u8 = 8;

pub fn find() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = home()
        .map(|h| h.join("Music").join(FOLDER))
        .into_iter()
        .collect();
    candidates.extend(super::drives().into_iter().map(|d| d.join(FOLDER)));
    let mut out: Vec<PathBuf> = Vec::new();
    for c in candidates {
        if c.join(DATABASE).is_file() && !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

pub fn read(root: &Path) -> crate::Result<ExternalLibrary> {
    let serato = if root.file_name().is_some_and(|n| n == FOLDER) {
        root.to_path_buf()
    } else {
        root.join(FOLDER)
    };
    let database = serato.join(DATABASE);
    let bytes = std::fs::read(&database).map_err(|source| crate::Error::Io {
        path: database.clone(),
        source,
    })?;
    let base = base_dir(&serato, home().as_deref());
    let mut lib = ExternalLibrary {
        tracks: tracks(&bytes, &base),
        playlists: Vec::new(),
    };
    let mut crates = Vec::new();
    if let Ok(entries) = std::fs::read_dir(serato.join("Subcrates")) {
        for path in entries.flatten().map(|e| e.path()) {
            if path.extension().is_none_or(|e| e != "crate") {
                continue;
            }
            let (Some(stem), Ok(bytes)) = (
                path.file_stem().and_then(|s| s.to_str()),
                std::fs::read(&path),
            ) else {
                continue;
            };
            let names: Vec<String> = stem.split(NESTING).map(str::to_owned).collect();
            let entries = crate_tracks(&mut lib.tracks, &crate_paths(&bytes), &base);
            crates.push((names, entries));
        }
    }
    lib.playlists = crate_tree(&crates, &[]);
    Ok(lib)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn base_dir(serato: &Path, home: Option<&Path>) -> PathBuf {
    let parent = serato.parent().unwrap_or(serato);
    if home.is_some_and(|h| h.join("Music") == parent) {
        parent
            .components()
            .take_while(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
            .collect()
    } else {
        parent.to_path_buf()
    }
}

fn resolve(base: &Path, stored: &str) -> String {
    let drive_letter = stored.as_bytes().get(1) == Some(&b':');
    if stored.starts_with('/') || stored.starts_with('\\') || drive_letter {
        stored.to_owned()
    } else {
        base.join(stored).to_string_lossy().into_owned()
    }
}

fn records(bytes: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while rest.len() >= 8 {
        let tag = [rest[0], rest[1], rest[2], rest[3]];
        let len = u32::from_be_bytes([rest[4], rest[5], rest[6], rest[7]]) as usize;
        let Some(payload) = rest.get(8..8 + len) else {
            break;
        };
        out.push((tag, payload));
        rest = &rest[8 + len..];
    }
    out
}

fn utf16be(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_be_bytes(*c))
        .collect();
    String::from_utf16_lossy(&units)
        .trim_end_matches('\0')
        .to_owned()
}

fn be_u32(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?))
}

fn tracks(bytes: &[u8], base: &Path) -> Vec<ExternalTrack> {
    records(bytes)
        .into_iter()
        .filter(|(tag, _)| tag == b"otrk")
        .map(|(_, payload)| track(payload, base))
        .collect()
}

fn track(payload: &[u8], base: &Path) -> ExternalTrack {
    let mut t = ExternalTrack::default();
    let mut added_text = String::new();
    for (tag, value) in records(payload) {
        match &tag {
            b"pfil" => t.path = resolve(base, &utf16be(value)),
            b"tsng" => t.title = utf16be(value),
            b"tart" => t.artist = utf16be(value),
            b"talb" => t.album = utf16be(value),
            b"tgen" => t.genre = utf16be(value),
            b"tcom" => t.comment = utf16be(value),
            b"tlbl" => t.label = utf16be(value),
            b"tcmp" => t.producer = utf16be(value),
            b"tkey" => {
                let text = utf16be(value);
                let key = MusicalKey::parse(text.trim());
                t.key_text = key.map_or(text, MusicalKey::open_key);
                t.musical_key = key.map(MusicalKey::value);
            }
            b"tbpm" => t.bpm = leading_number(&utf16be(value)).filter(|b| *b > 0.0),
            b"tlen" => t.duration_s = duration(&utf16be(value)),
            b"tbit" => {
                t.bitrate = leading_number(&utf16be(value))
                    .filter(|k| *k > 0.0)
                    .map(|kbps| (kbps * 1000.0).round() as i64);
            }
            b"tsiz" => t.filesize_kib = size_kib(&utf16be(value)),
            b"uadd" => {
                if let Some(secs) = be_u32(value).filter(|s| *s > 0) {
                    t.import_date = crate::clock::LocalTime::from_unix(u64::from(secs)).date_text();
                }
            }
            b"tadd" => added_text = utf16be(value),
            _ => {}
        }
    }
    if t.import_date.is_empty()
        && let Ok(secs) = added_text.trim().parse::<u64>()
        && secs > 0
    {
        t.import_date = crate::clock::LocalTime::from_unix(secs).date_text();
    }
    t
}

fn leading_number(text: &str) -> Option<f64> {
    let text = text.trim();
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(text.len());
    text[..end].replace(',', ".").parse().ok()
}

fn duration(text: &str) -> Option<f64> {
    let mut total = 0.0;
    for part in text.trim().split(':') {
        total = total * 60.0 + part.trim().replace(',', ".").parse::<f64>().ok()?;
    }
    (total > 0.0).then_some(total)
}

fn size_kib(text: &str) -> Option<i64> {
    let number = leading_number(text)?;
    let unit = text.trim().to_ascii_uppercase();
    let factor = if unit.ends_with("GB") {
        1024.0 * 1024.0
    } else if unit.ends_with("MB") {
        1024.0
    } else if unit.ends_with("KB") {
        1.0
    } else {
        return None;
    };
    Some((number * factor).round() as i64).filter(|k| *k > 0)
}

fn crate_paths(bytes: &[u8]) -> Vec<String> {
    records(bytes)
        .into_iter()
        .filter(|(tag, _)| tag == b"otrk")
        .filter_map(|(_, payload)| {
            records(payload)
                .into_iter()
                .find(|(tag, _)| tag == b"ptrk")
                .map(|(_, v)| utf16be(v))
        })
        .collect()
}

fn crate_tracks(tracks: &mut Vec<ExternalTrack>, stored: &[String], base: &Path) -> Vec<usize> {
    let exact: HashMap<String, usize> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.path.clone(), i))
        .collect();
    let folded: HashMap<String, usize> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.path.to_lowercase(), i))
        .collect();
    let mut out = Vec::new();
    let mut added: HashMap<String, usize> = HashMap::new();
    for s in stored {
        let path = resolve(base, s);
        let found = exact
            .get(&path)
            .or_else(|| folded.get(&path.to_lowercase()))
            .or_else(|| added.get(&path))
            .copied();
        let index = found.unwrap_or_else(|| {
            tracks.push(ExternalTrack {
                path: path.clone(),
                ..ExternalTrack::default()
            });
            added.insert(path, tracks.len() - 1);
            tracks.len() - 1
        });
        out.push(index);
    }
    out
}

fn crate_tree(crates: &[(Vec<String>, Vec<usize>)], prefix: &[String]) -> Vec<ExternalNode> {
    let names: BTreeSet<&String> = crates
        .iter()
        .filter(|(path, _)| path.len() > prefix.len() && path.starts_with(prefix))
        .map(|(path, _)| &path[prefix.len()])
        .collect();
    let mut names: Vec<&String> = names.into_iter().collect();
    names.sort_by(|a, b| natural_cmp(a, b));
    names
        .into_iter()
        .map(|name| {
            let mut here = prefix.to_vec();
            here.push(name.clone());
            let own = crates.iter().find(|(path, _)| *path == here).map(|(_, t)| {
                ExternalNode::Playlist {
                    name: name.clone(),
                    tracks: t.clone(),
                }
            });
            let nested = crates
                .iter()
                .any(|(path, _)| path.len() > here.len() && path.starts_with(&here));
            if nested {
                let mut children: Vec<ExternalNode> = own.into_iter().collect();
                children.extend(crate_tree(crates, &here));
                ExternalNode::Folder {
                    name: name.clone(),
                    children,
                }
            } else {
                own.unwrap_or(ExternalNode::Playlist {
                    name: name.clone(),
                    tracks: Vec::new(),
                })
            }
        })
        .collect()
}

pub fn file_marks(path: &Path) -> Option<(Vec<CuePoint>, Option<Grid>)> {
    let kind = lofty::probe::Probe::open(path)
        .ok()?
        .guess_file_type()
        .ok()?
        .file_type()?;
    let options = ParseOptions::new().read_properties(false);
    let mut file = std::fs::File::open(path).ok()?;
    let (markers, grid): (Option<Vec<u8>>, Option<Vec<u8>>) = match kind {
        FileType::Mpeg => id3_marks(MpegFile::read_from(&mut file, options).ok()?.remove_id3v2()),
        FileType::Aiff => id3_marks(AiffFile::read_from(&mut file, options).ok()?.remove_id3v2()),
        FileType::Wav => id3_marks(WavFile::read_from(&mut file, options).ok()?.remove_id3v2()),
        FileType::Flac => {
            let tag = FlacFile::read_from(&mut file, options)
                .ok()?
                .remove_vorbis_comments()?;
            (
                tag.get("SERATO_MARKERS_V2")
                    .and_then(|t| text_field(t, "Serato Markers2")),
                tag.get("SERATO_BEATGRID")
                    .and_then(|t| text_field(t, "Serato BeatGrid")),
            )
        }
        FileType::Mp4 => {
            let ilst = Mp4File::read_from(&mut file, options).ok()?.remove_ilst()?;
            let field = |name: &'static str, inner: &str| {
                let ident = AtomIdent::Freeform {
                    mean: "com.serato.dj".into(),
                    name: name.into(),
                };
                ilst.get(&ident)?.data().find_map(|d| match d {
                    AtomData::UTF8(s) | AtomData::UTF16(s) => text_field(s, inner),
                    AtomData::Unknown { data, .. } => {
                        text_field(&String::from_utf8_lossy(data), inner)
                    }
                    _ => None,
                })
            };
            (
                field("markersv2", "Serato Markers2"),
                field("beatgrid", "Serato BeatGrid"),
            )
        }
        _ => return None,
    };
    if markers.is_none() && grid.is_none() {
        return None;
    }
    Some((
        markers.as_deref().map(markers2).unwrap_or_default(),
        grid.as_deref().and_then(beatgrid),
    ))
}

fn id3_marks(tag: Option<Id3v2Tag>) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let (mut markers, mut grid) = (None, None);
    for frame in tag.into_iter().flatten() {
        let Frame::Binary(binary) = &frame else {
            continue;
        };
        if frame.id().as_str() != "GEOB" {
            continue;
        }
        let Ok(geob) = GeneralEncapsulatedObject::parse(&binary.data, frame.flags()) else {
            continue;
        };
        match geob.descriptor.as_deref() {
            Some("Serato Markers2") => markers = Some(geob.data),
            Some("Serato BeatGrid") => grid = Some(geob.data),
            _ => {}
        }
    }
    (markers, grid)
}

fn lenient_base64(text: &str) -> Option<Vec<u8>> {
    let mut clean: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '=')
        .collect();
    if clean.len() % 4 == 1 {
        clean.push('A');
    }
    let engine = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    engine.decode(clean).ok()
}

fn text_field(text: &str, name: &str) -> Option<Vec<u8>> {
    let bytes = lenient_base64(text)?;
    let head = format!("application/octet-stream\0\0{name}\0");
    bytes.strip_prefix(head.as_bytes()).map(<[u8]>::to_vec)
}

pub fn markers2(payload: &[u8]) -> Vec<CuePoint> {
    let Some(body) = payload.strip_prefix(&[1, 1]) else {
        return Vec::new();
    };
    let end = body.iter().position(|b| *b == 0).unwrap_or(body.len());
    let Some(decoded) = lenient_base64(&String::from_utf8_lossy(&body[..end])) else {
        return Vec::new();
    };
    let Some(mut rest) = decoded.strip_prefix(&[1, 1]) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    while let Some(nul) = rest.iter().position(|b| *b == 0) {
        if nul == 0 {
            break;
        }
        let name = &rest[..nul];
        let Some(len) = rest.get(nul + 1..).and_then(be_u32) else {
            break;
        };
        let start = nul + 5;
        let Some(data) = rest.get(start..start + len as usize) else {
            break;
        };
        match name {
            b"CUE" => out.extend(cue(data)),
            b"LOOP" => out.extend(saved_loop(data)),
            _ => {}
        }
        rest = &rest[start + len as usize..];
    }
    out
}

fn c_text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn cue(data: &[u8]) -> Option<CuePoint> {
    let index = *data.get(1)?;
    let ms = be_u32(data.get(2..)?)?;
    let rgb = data.get(7..10)?;
    Some(CuePoint {
        kind: CuePoint::CUE,
        name: data.get(12..).map(c_text).unwrap_or_default(),
        start_s: f64::from(ms) / 1000.0,
        len_s: 0.0,
        hotcue: (index < HOT_CUES).then_some(index),
        display_order: 0,
        repeats: -1,
        color: Some(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])),
    })
}

fn saved_loop(data: &[u8]) -> Option<CuePoint> {
    let start = be_u32(data.get(2..)?)?;
    let end = be_u32(data.get(6..)?)?;
    let argb = data.get(14..18)?;
    Some(CuePoint {
        kind: CuePoint::LOOP,
        name: data.get(20..).map(c_text).unwrap_or_default(),
        start_s: f64::from(start) / 1000.0,
        len_s: f64::from(end.saturating_sub(start)) / 1000.0,
        hotcue: None,
        display_order: 0,
        repeats: -1,
        color: Some(format!("#{:02X}{:02X}{:02X}", argb[1], argb[2], argb[3])),
    })
}

pub fn beatgrid(payload: &[u8]) -> Option<Grid> {
    let count = be_u32(payload.get(2..)?)? as usize;
    if count == 0 {
        return None;
    }
    let raw: Vec<(f64, [u8; 4])> = (0..count)
        .map(|i| {
            let m = payload.get(6 + i * 8..14 + i * 8)?;
            let pos = f32::from_be_bytes(m[..4].try_into().ok()?);
            Some((f64::from(pos), m[4..8].try_into().ok()?))
        })
        .collect::<Option<_>>()?;
    let segments: Vec<(f64, f64)> = raw
        .iter()
        .enumerate()
        .map(|(i, &(pos, value))| match raw.get(i + 1) {
            Some(&(next, _)) => {
                let beats = f64::from(u32::from_be_bytes(value));
                (pos, beats * 60.0 / (next - pos))
            }
            None => (pos, f64::from(f32::from_be_bytes(value))),
        })
        .collect();
    let &(anchor_s, bpm) = segments.first()?;
    if !(bpm.is_finite() && bpm > 0.0) {
        return None;
    }
    let mut current = bpm;
    let mut markers_s = Vec::new();
    for &(pos, tempo) in &segments[1..] {
        if (tempo - current).abs() >= 0.005 && markers_s.len() < MAX_EXTRA_MARKERS {
            markers_s.push(pos);
            current = tempo;
        }
    }
    Some(Grid {
        bpm,
        anchor_s,
        markers_s,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(tag: [u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend(u32::try_from(payload.len()).unwrap().to_be_bytes());
        out.extend(payload);
        out
    }

    fn text(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(u16::to_be_bytes).collect()
    }

    fn database(tracks: &[Vec<u8>]) -> Vec<u8> {
        let mut out = rec(*b"vrsn", &text("2.0/Serato Scratch LIVE Database"));
        for t in tracks {
            out.extend(rec(*b"otrk", t));
        }
        out
    }

    fn crate_file(paths: &[&str]) -> Vec<u8> {
        let mut out = rec(*b"vrsn", &text("1.0/Serato ScratchLive Crate"));
        out.extend(rec(*b"ovct", &rec(*b"tvcn", &text("song"))));
        for p in paths {
            out.extend(rec(*b"otrk", &rec(*b"ptrk", &text(p))));
        }
        out
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("backspn-serato-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(FOLDER).join("Subcrates")).unwrap();
        dir
    }

    #[test]
    fn reads_tracks_and_nested_crates() {
        let drive = temp_dir("lib");
        let serato = drive.join(FOLDER);
        let one = [
            rec(*b"ttyp", &text("mp3")),
            rec(*b"pfil", &text("Music/one.mp3")),
            rec(*b"tsng", &text("One")),
            rec(*b"tart", &text("A")),
            rec(*b"tbpm", &text("128.00")),
            rec(*b"tkey", &text("Am")),
            rec(*b"tlen", &text("05:00.50")),
            rec(*b"tbit", &text("320.0kbps")),
            rec(*b"tsiz", &text("2.0MB")),
            rec(*b"uadd", &1_700_000_000u32.to_be_bytes()),
            rec(*b"bmis", &[0]),
        ]
        .concat();
        let two = [
            rec(*b"pfil", &text("Music/two.mp3")),
            rec(*b"tsng", &text("Two")),
        ]
        .concat();
        std::fs::write(serato.join(DATABASE), database(&[one, two])).unwrap();
        let sub = serato.join("Subcrates");
        std::fs::write(sub.join("Gigs.crate"), crate_file(&["Music/two.mp3"])).unwrap();
        std::fs::write(
            sub.join("Gigs%%Friday.crate"),
            crate_file(&["Music/ONE.mp3", "Music/two.mp3", "Music/new.mp3"]),
        )
        .unwrap();
        std::fs::write(sub.join("Gigs%%Friday%%Late.crate"), crate_file(&[])).unwrap();
        std::fs::write(sub.join("Ambient.crate"), crate_file(&["Music/one.mp3"])).unwrap();

        let lib = read(&serato).unwrap();
        let t = &lib.tracks[0];
        assert_eq!(t.path, drive.join("Music/one.mp3").to_string_lossy());
        assert_eq!((t.title.as_str(), t.artist.as_str()), ("One", "A"));
        assert_eq!(t.bpm, Some(128.0));
        assert_eq!(t.key_text, MusicalKey::parse("Am").unwrap().open_key());
        assert_eq!(t.duration_s, Some(300.5));
        assert_eq!((t.bitrate, t.filesize_kib), (Some(320_000), Some(2048)));
        assert_ne!(t.import_date, "");
        assert_eq!(lib.tracks.len(), 3, "the crate's unknown track is added");
        assert_eq!(
            lib.tracks[2].path,
            drive.join("Music/new.mp3").to_string_lossy()
        );

        let names: Vec<&str> = lib.playlists.iter().map(ExternalNode::name).collect();
        assert_eq!(names, ["Ambient", "Gigs"]);
        let ExternalNode::Folder { children, .. } = &lib.playlists[1] else {
            panic!("Gigs has sub-crates")
        };
        assert_eq!(
            children[0],
            ExternalNode::Playlist {
                name: "Gigs".into(),
                tracks: vec![1]
            }
        );
        let ExternalNode::Folder {
            children: friday, ..
        } = &children[1]
        else {
            panic!("Friday has a sub-crate")
        };
        assert_eq!(friday[0].all_tracks(), [0, 1, 2]);
        assert_eq!(friday[1].name(), "Late");
        assert_eq!(read(&drive).unwrap(), lib);
        std::fs::remove_dir_all(&drive).unwrap();
    }

    #[test]
    fn paths_are_relative_to_the_drive_root() {
        let home = Path::new("/Users/me");
        assert_eq!(
            base_dir(Path::new("/Users/me/Music/_Serato_"), Some(home)),
            Path::new("/")
        );
        assert_eq!(
            base_dir(Path::new("/Volumes/USB/_Serato_"), Some(home)),
            Path::new("/Volumes/USB")
        );
        assert_eq!(resolve(Path::new("/"), "Users/me/a.mp3"), "/Users/me/a.mp3");
        assert_eq!(
            resolve(Path::new("/Volumes/USB"), "/abs/a.mp3"),
            "/abs/a.mp3"
        );
        assert_eq!(resolve(Path::new("/x"), "C:\\a.mp3"), "C:\\a.mp3");
    }

    #[test]
    fn text_values_convert() {
        assert_eq!(duration("1:02:03"), Some(3723.0));
        assert_eq!(duration("x"), None);
        assert_eq!(size_kib("512KB"), Some(512));
        assert_eq!(size_kib("1GB"), Some(1_048_576));
        assert_eq!(size_kib("12"), None);
        assert_eq!(leading_number("98,5"), Some(98.5));
    }

    fn entry(name: &str, data: &[u8]) -> Vec<u8> {
        let mut out = name.as_bytes().to_vec();
        out.push(0);
        out.extend(u32::try_from(data.len()).unwrap().to_be_bytes());
        out.extend(data);
        out
    }

    #[test]
    fn markers2_gives_hot_cues_and_loops() {
        let mut cue = vec![0, 2];
        cue.extend(30_500u32.to_be_bytes());
        cue.extend([0, 0xCC, 0x00, 0x10, 0, 0]);
        cue.extend(b"Drop\0");
        let mut lp = vec![0, 0];
        lp.extend(10_000u32.to_be_bytes());
        lp.extend(12_000u32.to_be_bytes());
        lp.extend([0xFF; 4]);
        lp.extend([0, 0x27, 0xAA, 0xE1, 0, 0]);
        lp.extend(b"Intro\0");
        let mut decoded = vec![1, 1];
        decoded.extend(entry("COLOR", &[0, 0xFF, 0xFF, 0xFF]));
        decoded.extend(entry("CUE", &cue));
        decoded.extend(entry("LOOP", &lp));
        decoded.extend(entry("BPMLOCK", &[0]));
        decoded.push(0);
        let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(&decoded);
        let wrapped: Vec<String> = b64
            .as_bytes()
            .chunks(72)
            .map(|c| String::from_utf8(c.to_vec()).unwrap())
            .collect();
        let mut payload = vec![1, 1];
        payload.extend(wrapped.join("\n").as_bytes());
        payload.extend([0; 64]);

        let cues = markers2(&payload);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].kind, CuePoint::CUE);
        assert_eq!((cues[0].hotcue, cues[0].start_s), (Some(2), 30.5));
        assert_eq!(cues[0].name, "Drop");
        assert_eq!(cues[0].color.as_deref(), Some("#CC0010"));
        assert_eq!(cues[1].kind, CuePoint::LOOP);
        assert_eq!((cues[1].start_s, cues[1].len_s), (10.0, 2.0));
        assert_eq!(cues[1].name, "Intro");
        assert_eq!(cues[1].hotcue, None);
        assert_eq!(markers2(b"junk"), []);

        let mut field = b"application/octet-stream\0\0Serato Markers2\0".to_vec();
        field.extend(&payload);
        let text = base64::engine::general_purpose::STANDARD_NO_PAD.encode(&field);
        assert_eq!(text_field(&text, "Serato Markers2"), Some(payload));
        assert_eq!(text_field(&text, "Serato BeatGrid"), None);
    }

    #[test]
    fn beatgrid_gives_bpm_anchor_and_tempo_changes() {
        let marker = |pos: f32, value: [u8; 4]| [pos.to_be_bytes(), value].concat();
        let mut grid = vec![1, 0, 0, 0, 0, 1];
        grid.extend(marker(0.25, 115.0f32.to_be_bytes()));
        grid.push(0x37);
        let g = beatgrid(&grid).unwrap();
        assert_eq!((g.bpm, g.anchor_s, g.markers_s.len()), (115.0, 0.25, 0));

        let mut grid = vec![1, 0, 0, 0, 0, 3];
        grid.extend(marker(0.5, 4u32.to_be_bytes()));
        grid.extend(marker(2.5, 8u32.to_be_bytes()));
        grid.extend(marker(6.5, 125.0f32.to_be_bytes()));
        grid.push(0);
        let g = beatgrid(&grid).unwrap();
        assert_eq!((g.bpm, g.anchor_s, g.markers_s), (120.0, 0.5, vec![6.5]));
        assert_eq!(beatgrid(&[1, 0, 0, 0, 0, 0, 0]), None);
    }
}
