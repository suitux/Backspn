use std::fmt;
use std::path::Path;
use std::str::FromStr;

use crate::Error;
use crate::xml::{self, Document, Element};

mod sets;
pub use sets::{CELLS as SET_CELLS, SLOTS as SET_SLOTS, SampleSet, SetCell, SetSlot};

#[derive(Debug, Clone)]
pub struct Nml {
    pub version: String,
    pub program: Option<String>,
    pub entries: Vec<Entry>,
    pub playlists: Option<Node>,
    pub sets: Vec<SampleSet>,
    pub document: Document,
}

impl Nml {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })?;
        text.parse()
    }

    pub fn from_document(document: Document) -> Result<Self, Error> {
        let root = &document.root;
        if root.name != "NML" {
            return Err(invalid(root, format!("root element is <{}>", root.name)));
        }
        let version = required(root, "VERSION")?.to_owned();
        let program = root
            .child("HEAD")
            .and_then(|h| h.attr("PROGRAM"))
            .map(str::to_owned);
        let entries = match root.child("COLLECTION") {
            Some(c) => c
                .children_named("ENTRY")
                .map(Entry::from_element)
                .collect::<Result<_, _>>()?,
            None => Vec::new(),
        };
        let playlists = root
            .child("PLAYLISTS")
            .and_then(|p| p.child("NODE"))
            .map(Node::from_element)
            .transpose()?;
        let sets = match root.child("SETS") {
            Some(c) => c
                .children_named("SET")
                .map(SampleSet::from_element)
                .collect::<Result<_, _>>()?,
            None => Vec::new(),
        };
        Ok(Self {
            version,
            program,
            entries,
            playlists,
            sets,
            document,
        })
    }

    pub fn entry_by_key(&self, key: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.location.primary_key() == key)
    }
}

impl FromStr for Nml {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_document(xml::parse(s)?)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Entry {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub audio_id: Option<String>,
    pub modified_date: Option<String>,
    pub modified_time: Option<u32>,
    pub locked: bool,
    pub location: Location,
    pub album: Option<Album>,
    pub info: Info,
    pub tempo: Option<Tempo>,
    pub loudness: Option<Loudness>,
    pub musical_key: Option<MusicalKey>,
    pub loop_info: Option<LoopInfo>,
    pub cues: Vec<Cue>,
}

impl Entry {
    fn from_element(e: &Element) -> Result<Self, Error> {
        let location = e
            .child("LOCATION")
            .ok_or_else(|| invalid(e, "ENTRY without LOCATION".into()))
            .and_then(Location::from_element)?;
        Ok(Self {
            title: owned(e, "TITLE"),
            artist: owned(e, "ARTIST"),
            audio_id: owned(e, "AUDIO_ID"),
            modified_date: owned(e, "MODIFIED_DATE"),
            modified_time: num(e, "MODIFIED_TIME")?,
            locked: e.attr("LOCK") == Some("1"),
            location,
            album: e.child("ALBUM").map(Album::from_element).transpose()?,
            info: e
                .child("INFO")
                .map(Info::from_element)
                .transpose()?
                .unwrap_or_default(),
            tempo: e.child("TEMPO").map(Tempo::from_element).transpose()?,
            loudness: e
                .child("LOUDNESS")
                .map(Loudness::from_element)
                .transpose()?,
            musical_key: e
                .child("MUSICAL_KEY")
                .map(|k| {
                    let value: u8 =
                        num(k, "VALUE")?.ok_or_else(|| invalid(k, "missing VALUE".into()))?;
                    MusicalKey::from_value(value)
                        .ok_or_else(|| invalid(k, format!("key value {value} out of range")))
                })
                .transpose()?,
            loop_info: e
                .child("LOOPINFO")
                .map(LoopInfo::from_element)
                .transpose()?,
            cues: e
                .children_named("CUE_V2")
                .map(Cue::from_element)
                .collect::<Result<_, _>>()?,
        })
    }

    pub fn grid_cue(&self) -> Option<&Cue> {
        self.cues.iter().find(|c| c.kind == CueKind::Grid)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Location {
    pub dir: String,
    pub file: String,
    pub volume: String,
    pub volume_id: Option<String>,
}

impl Location {
    pub(crate) fn from_element(e: &Element) -> Result<Self, Error> {
        Ok(Self {
            dir: required(e, "DIR")?.to_owned(),
            file: required(e, "FILE")?.to_owned(),
            volume: required(e, "VOLUME")?.to_owned(),
            volume_id: owned(e, "VOLUMEID"),
        })
    }

    pub fn primary_key(&self) -> String {
        format!("{}{}{}", self.volume, self.dir, self.file)
    }

    pub fn relative_path(&self) -> String {
        let dir: Vec<&str> = self.dir.split("/:").filter(|s| !s.is_empty()).collect();
        let mut path = dir.join("/");
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(&self.file);
        path
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Album {
    pub title: Option<String>,
    pub track: Option<u32>,
}

impl Album {
    fn from_element(e: &Element) -> Result<Self, Error> {
        Ok(Self {
            title: owned(e, "TITLE"),
            track: num(e, "TRACK")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Info {
    pub bitrate: Option<u32>,
    pub genre: Option<String>,
    pub comment: Option<String>,
    pub label: Option<String>,
    pub producer: Option<String>,
    pub cover_art_id: Option<String>,
    pub key_text: Option<String>,
    pub playtime: Option<u32>,
    pub playtime_float: Option<f64>,
    pub import_date: Option<String>,
    pub release_date: Option<String>,
    pub last_played: Option<String>,
    pub flags: Option<u32>,
    pub filesize_kib: Option<u64>,
    pub ranking: Option<u8>,
    pub playcount: Option<u32>,
}

impl Info {
    fn from_element(e: &Element) -> Result<Self, Error> {
        Ok(Self {
            bitrate: num(e, "BITRATE")?,
            genre: owned(e, "GENRE"),
            comment: owned(e, "COMMENT"),
            label: owned(e, "LABEL"),
            producer: owned(e, "PRODUCER"),
            cover_art_id: owned(e, "COVERARTID"),
            key_text: owned(e, "KEY"),
            playtime: num(e, "PLAYTIME")?,
            playtime_float: num(e, "PLAYTIME_FLOAT")?,
            import_date: owned(e, "IMPORT_DATE"),
            release_date: owned(e, "RELEASE_DATE"),
            last_played: owned(e, "LAST_PLAYED"),
            flags: num(e, "FLAGS")?,
            filesize_kib: num(e, "FILESIZE")?,
            ranking: num(e, "RANKING")?,
            playcount: num(e, "PLAYCOUNT")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Tempo {
    pub bpm: Option<f64>,
    pub bpm_quality: Option<f64>,
}

impl Tempo {
    fn from_element(e: &Element) -> Result<Self, Error> {
        Ok(Self {
            bpm: num(e, "BPM")?,
            bpm_quality: num(e, "BPM_QUALITY")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoopInfo {
    pub sample_type: Option<i32>,
    pub original_title: Option<String>,
    pub original_loop_size: Option<f64>,
    pub original_loop_start: Option<f64>,
}

impl LoopInfo {
    pub const LOOP: i32 = 0;
    pub const ONE_SHOT: i32 = 1;

    fn from_element(e: &Element) -> Result<Self, Error> {
        Ok(Self {
            sample_type: num::<i32>(e, "SAMPLE_TYPE_INFO")?.filter(|t| *t >= 0),
            original_title: owned(e, "ORIGINAL_TITLE"),
            original_loop_size: num(e, "ORIGINAL_LOOP_SIZE")?,
            original_loop_start: num(e, "ORIGINAL_LOOP_START")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Loudness {
    pub peak_db: Option<f64>,
    pub perceived_db: Option<f64>,
    pub analyzed_db: Option<f64>,
}

impl Loudness {
    fn from_element(e: &Element) -> Result<Self, Error> {
        Ok(Self {
            peak_db: num(e, "PEAK_DB")?,
            perceived_db: num(e, "PERCEIVED_DB")?,
            analyzed_db: num(e, "ANALYZED_DB")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CueKind {
    Cue,
    FadeIn,
    FadeOut,
    Load,
    Grid,
    Loop,
    Other(i32),
}

impl From<i32> for CueKind {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::Cue,
            1 => Self::FadeIn,
            2 => Self::FadeOut,
            3 => Self::Load,
            4 => Self::Grid,
            5 => Self::Loop,
            other => Self::Other(other),
        }
    }
}

impl From<CueKind> for i32 {
    fn from(kind: CueKind) -> Self {
        match kind {
            CueKind::Cue => 0,
            CueKind::FadeIn => 1,
            CueKind::FadeOut => 2,
            CueKind::Load => 3,
            CueKind::Grid => 4,
            CueKind::Loop => 5,
            CueKind::Other(v) => v,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub name: String,
    pub display_order: i32,
    pub kind: CueKind,
    pub start_ms: f64,
    pub len_ms: f64,
    pub repeats: i32,
    pub hotcue: Option<u8>,
    pub color: Option<String>,
    pub grid_bpm: Option<f64>,
}

impl Cue {
    fn from_element(e: &Element) -> Result<Self, Error> {
        let kind: i32 = num(e, "TYPE")?.ok_or_else(|| invalid(e, "cue without TYPE".into()))?;
        let hotcue: i32 = num(e, "HOTCUE")?.unwrap_or(-1);
        Ok(Self {
            name: owned(e, "NAME").unwrap_or_default(),
            display_order: num(e, "DISPL_ORDER")?.unwrap_or(0),
            kind: kind.into(),
            start_ms: num(e, "START")?.ok_or_else(|| invalid(e, "cue without START".into()))?,
            len_ms: num(e, "LEN")?.unwrap_or(0.0),
            repeats: num(e, "REPEATS")?.unwrap_or(-1),
            hotcue: u8::try_from(hotcue).ok(),
            color: owned(e, "COLOR"),
            grid_bpm: e
                .child("GRID")
                .map(|g| num(g, "BPM"))
                .transpose()?
                .flatten(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyNotation {
    Sharps,
    Flats,
    #[default]
    OpenKey,
    Camelot,
}

impl KeyNotation {
    pub fn from_value(value: u8) -> Self {
        match value {
            0 => Self::Sharps,
            1 => Self::Flats,
            3 => Self::Camelot,
            _ => Self::OpenKey,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MusicalKey(u8);

const PITCH_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const OPEN_KEY_MAJOR: [u8; 12] = [1, 8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6];

impl MusicalKey {
    pub fn from_value(value: u8) -> Option<Self> {
        (value < 24).then_some(Self(value))
    }

    pub fn value(self) -> u8 {
        self.0
    }

    pub fn is_minor(self) -> bool {
        self.0 >= 12
    }

    pub fn tonic(self) -> u8 {
        self.0 % 12
    }

    pub fn camelot(self) -> String {
        let open = self.open_key_number();
        let number = (open + 6) % 12 + 1;
        format!("{number}{}", if self.is_minor() { 'A' } else { 'B' })
    }

    pub fn parse(text: &str) -> Option<Self> {
        let wanted = text.to_lowercase();
        (0..24u8).map(Self).find(|k| {
            [k.to_string(), k.flat_name(), k.camelot(), k.open_key()]
                .iter()
                .any(|name| name.to_lowercase() == wanted)
        })
    }

    pub fn name(self, notation: KeyNotation) -> String {
        match notation {
            KeyNotation::Sharps => self.to_string(),
            KeyNotation::Flats => self.flat_name(),
            KeyNotation::OpenKey => self.open_key(),
            KeyNotation::Camelot => self.camelot(),
        }
    }

    pub fn sort_rank(self) -> u8 {
        (self.open_key_number() - 1) * 2 + if self.is_minor() { 1 } else { 2 }
    }

    fn wheel(self) -> (u8, bool) {
        (self.open_key_number() - 1, self.is_minor())
    }

    pub fn matches(self, other: Self) -> bool {
        let ((a, ma), (b, mb)) = (self.wheel(), other.wheel());
        let steps = a.abs_diff(b).min(12 - a.abs_diff(b));
        (steps == 0) || (steps == 1 && ma == mb)
    }

    pub fn adjacent(self, other: Self) -> bool {
        let shifted = |up: u8| {
            let tonic = (self.tonic() + up) % 12;
            Self(if self.is_minor() { tonic + 12 } else { tonic })
        };
        self.matches(other) || shifted(1).matches(other) || shifted(11).matches(other)
    }

    pub fn shifted(self, semitones: f32) -> Option<Self> {
        let mut x = f32::from(self.tonic()) + semitones + 12.0;
        while (x + 0.5).floor() >= 12.0 {
            x -= 12.0;
        }
        if self.is_minor() {
            x += 12.0;
        }
        let nearest = (x + 0.5).floor();
        ((0.0..24.0).contains(&nearest) && (x - nearest).abs() <= 0.051)
            .then_some(Self(nearest as u8))
    }

    fn flat_name(self) -> String {
        const FLAT_NAMES: [&str; 12] = [
            "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
        ];
        let name = FLAT_NAMES[usize::from(self.tonic())];
        if self.is_minor() {
            format!("{name}m")
        } else {
            name.to_owned()
        }
    }

    fn open_key_number(self) -> u8 {
        let major = if self.is_minor() {
            (self.tonic() + 3) % 12
        } else {
            self.tonic()
        };
        OPEN_KEY_MAJOR[usize::from(major)]
    }

    pub fn open_key(self) -> String {
        let mode = if self.is_minor() { 'm' } else { 'd' };
        format!("{}{mode}", self.open_key_number())
    }
}

impl fmt::Display for MusicalKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = PITCH_NAMES[usize::from(self.tonic())];
        if self.is_minor() {
            write!(f, "{name}m")
        } else {
            f.write_str(name)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Folder {
        name: String,
        children: Vec<Node>,
    },
    Playlist {
        name: String,
        uuid: Option<String>,
        entries: Vec<String>,
    },
    Smartlist {
        name: String,
        uuid: Option<String>,
        query: Option<String>,
    },
    Other {
        kind: String,
        name: String,
    },
}

impl Node {
    fn from_element(e: &Element) -> Result<Self, Error> {
        let name = owned(e, "NAME").unwrap_or_default();
        let kind = required(e, "TYPE")?;
        Ok(match kind {
            "FOLDER" => Self::Folder {
                name,
                children: e
                    .child("SUBNODES")
                    .map(|s| {
                        s.children_named("NODE")
                            .map(Node::from_element)
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default(),
            },
            "PLAYLIST" => {
                let list = e.child("PLAYLIST");
                Self::Playlist {
                    name,
                    uuid: list.and_then(|l| owned(l, "UUID")),
                    entries: list
                        .into_iter()
                        .flat_map(|l| l.children_named("ENTRY"))
                        .filter_map(|en| en.child("PRIMARYKEY"))
                        .filter_map(|k| owned(k, "KEY"))
                        .collect(),
                }
            }
            "SMARTLIST" => {
                let list = e.child("SMARTLIST");
                Self::Smartlist {
                    name,
                    uuid: list.and_then(|l| owned(l, "UUID")),
                    query: list
                        .and_then(|l| l.child("SEARCH_EXPRESSION"))
                        .and_then(|s| owned(s, "QUERY")),
                }
            }
            other => Self::Other {
                kind: other.to_owned(),
                name,
            },
        })
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Folder { name, .. }
            | Self::Playlist { name, .. }
            | Self::Smartlist { name, .. }
            | Self::Other { name, .. } => name,
        }
    }

    pub fn walk(&self) -> Vec<&Node> {
        let mut out = vec![self];
        if let Self::Folder { children, .. } = self {
            for child in children {
                out.extend(child.walk());
            }
        }
        out
    }
}

pub(crate) fn owned(e: &Element, key: &str) -> Option<String> {
    e.attr(key).map(str::to_owned)
}

pub(crate) fn required<'a>(e: &'a Element, key: &str) -> Result<&'a str, Error> {
    e.attr(key)
        .ok_or_else(|| invalid(e, format!("missing attribute {key}")))
}

pub(crate) fn num<T: FromStr>(e: &Element, key: &str) -> Result<Option<T>, Error> {
    e.attr(key)
        .map(|v| {
            v.trim()
                .parse()
                .map_err(|_| invalid(e, format!("attribute {key}={v:?} is not a valid number")))
        })
        .transpose()
}

pub(crate) fn invalid(e: &Element, message: String) -> Error {
    Error::Invalid {
        element: e.name.clone(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_kind_ids_round_trip() {
        for id in [0, 1, 2, 3, 4, 5, 9, -1] {
            assert_eq!(i32::from(CueKind::from(id)), id);
        }
        assert_eq!(CueKind::from(5), CueKind::Loop);
    }

    #[test]
    fn open_key_matches_its_table() {
        let expected = [
            "1d", "8d", "3d", "10d", "5d", "12d", "7d", "2d", "9d", "4d", "11d", "6d", "10m", "5m",
            "12m", "7m", "2m", "9m", "4m", "11m", "6m", "1m", "8m", "3m",
        ];
        for (value, open) in (0u8..).zip(expected) {
            assert_eq!(MusicalKey::from_value(value).unwrap().open_key(), open);
        }
        assert_eq!(MusicalKey::from_value(24), None);
        assert_eq!(MusicalKey::from_value(20).unwrap().to_string(), "G#m");
    }

    #[test]
    fn shifted_key_wraps_keeps_the_mode_and_needs_a_whole_semitone() {
        let key = |v| MusicalKey::from_value(v).unwrap();
        assert_eq!(key(21).shifted(0.0), Some(key(21)), "Am stays");
        assert_eq!(key(21).shifted(3.0), Some(key(12)), "Am + 3 = Cm");
        assert_eq!(key(11).shifted(1.0), Some(key(0)), "B + 1 = C");
        assert_eq!(key(0).shifted(-1.0), Some(key(11)), "C − 1 = B");
        assert_eq!(key(0).shifted(-12.0), Some(key(0)));
        assert_eq!(key(4).shifted(12.0), Some(key(4)));
        assert_eq!(key(4).shifted(2.04), Some(key(6)), "within 0.051");
        assert_eq!(key(4).shifted(2.3), None, "between keys");
    }

    #[test]
    fn harmonic_matches_and_adjacent_keys() {
        let key = |v| MusicalKey::from_value(v).unwrap();
        let am = key(21);
        for (other, matching) in [
            (21, true),
            (0, true),
            (16, true),
            (14, true),
            (7, false),
            (2, false),
        ] {
            assert_eq!(am.matches(key(other)), matching, "Am ~ {}", key(other));
        }
        assert!(am.adjacent(key(22)) && am.adjacent(key(20)) && am.adjacent(key(1)));
        assert!(!am.adjacent(key(2)));
        assert_eq!(am.name(KeyNotation::Flats), "Am");
        assert_eq!(key(13).name(KeyNotation::Flats), "Dbm");
        assert_eq!(am.name(KeyNotation::Camelot), "8A");
        assert_eq!(KeyNotation::from_value(2), KeyNotation::OpenKey);
        let ranks = [
            2, 16, 6, 20, 10, 24, 14, 4, 18, 8, 22, 12, 19, 9, 23, 13, 3, 17, 7, 21, 11, 1, 15, 5,
        ];
        for (v, rank) in (0u8..).zip(ranks) {
            assert_eq!(key(v).sort_rank(), rank);
        }
    }

    #[test]
    fn parses_the_four_notations() {
        let camelot = [
            "8B", "3B", "10B", "5B", "12B", "7B", "2B", "9B", "4B", "11B", "6B", "1B", "5A", "12A",
            "7A", "2A", "9A", "4A", "11A", "6A", "1A", "8A", "3A", "10A",
        ];
        for (value, text) in (0u8..).zip(camelot) {
            let key = MusicalKey::from_value(value).unwrap();
            assert_eq!(key.camelot(), text);
            for name in [
                key.to_string(),
                key.flat_name(),
                key.camelot(),
                key.open_key(),
            ] {
                assert_eq!(MusicalKey::parse(&name), Some(key), "{name}");
            }
        }
        assert_eq!(MusicalKey::parse("dbm"), MusicalKey::from_value(13));
        assert_eq!(MusicalKey::parse("12a"), MusicalKey::from_value(13));
        assert_eq!(MusicalKey::parse("C major"), None);
        assert_eq!(MusicalKey::parse(""), None);
    }

    #[test]
    fn location_paths() {
        let loc = Location {
            dir: "/:Music/:House/:".into(),
            file: "a b.flac".into(),
            volume: "DJ".into(),
            volume_id: None,
        };
        assert_eq!(loc.primary_key(), "DJ/:Music/:House/:a b.flac");
        assert_eq!(loc.relative_path(), "Music/House/a b.flac");
    }
}
