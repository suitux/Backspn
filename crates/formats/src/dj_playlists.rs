use std::path::Path;

use crate::Error;
use crate::xml::{self, Element};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Library {
    pub tracks: Vec<Track>,
    pub playlists: Vec<Node>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub id: i64,
    pub name: String,
    pub artist: String,
    pub composer: String,
    pub album: String,
    pub genre: String,
    pub kind: String,
    pub size: u64,
    pub total_time: u32,
    pub disc_number: u32,
    pub track_number: u32,
    pub year: u32,
    pub average_bpm: f64,
    pub date_added: String,
    pub bit_rate: u32,
    pub sample_rate: u32,
    pub comments: String,
    pub play_count: u32,
    pub stars: u8,
    pub location: String,
    pub remixer: String,
    pub tonality: String,
    pub label: String,
    pub mix: String,
    pub colour: Option<Colour>,
    pub tempos: Vec<Tempo>,
    pub marks: Vec<Mark>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colour {
    Pink,
    Red,
    Orange,
    Yellow,
    Green,
    Aqua,
    Blue,
    Purple,
}

impl Colour {
    fn parse(text: &str) -> Option<Self> {
        const ALL: [(&str, Colour); 8] = [
            ("0xFF007F", Colour::Pink),
            ("0xFF0000", Colour::Red),
            ("0xFFA500", Colour::Orange),
            ("0xFFFF00", Colour::Yellow),
            ("0x00FF00", Colour::Green),
            ("0x25FDE9", Colour::Aqua),
            ("0x0000FF", Colour::Blue),
            ("0x660099", Colour::Purple),
        ];
        ALL.iter()
            .find(|(t, _)| t.eq_ignore_ascii_case(text))
            .map(|&(_, c)| c)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tempo {
    pub inizio: f64,
    pub bpm: f64,
    pub beat: u32,
    pub beats_per_bar: u32,
}

impl Tempo {
    fn valid(self) -> bool {
        self.bpm > 0.0 && self.beat >= 1 && self.beats_per_bar >= 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Beat {
    pub seconds: f64,
    pub bpm: f64,
    pub beat: u32,
    pub marker: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub name: String,
    pub start: f64,
    pub end: Option<f64>,
    pub hot_cue: Option<u8>,
    pub rgb: Option<[u8; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Folder { name: String, children: Vec<Node> },
    Playlist { name: String, entries: Vec<Entry> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Id(i64),
    Path(String),
}

pub fn read(path: &Path) -> Result<Library, Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&text)
}

pub fn parse(text: &str) -> Result<Library, Error> {
    let root = xml::parse(text)?.root;
    if !root.name.eq_ignore_ascii_case("DJ_PLAYLISTS") {
        return Err(Error::Invalid {
            element: root.name,
            message: "not a DJ_PLAYLISTS library".into(),
        });
    }
    let tree = child(&root, "PLAYLISTS")
        .and_then(|p| p.children.iter().find(|c| named(c, "NODE")))
        .filter(|n| {
            n.attr("Name")
                .is_some_and(|v| v.eq_ignore_ascii_case("ROOT"))
        });
    let by_id = tree.is_some_and(keyed_by_id);
    let mut tracks = Vec::new();
    for e in child(&root, "COLLECTION")
        .map(|c| c.children.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|e| named(e, "TRACK"))
    {
        let ordinal = tracks.len() as i64 + 1;
        tracks.push(track(e, by_id, ordinal));
    }
    let playlists = tree.map(|t| nodes(&t.children)).unwrap_or_default();
    Ok(Library { tracks, playlists })
}

fn named(e: &Element, name: &str) -> bool {
    e.name.eq_ignore_ascii_case(name)
}

fn child<'a>(e: &'a Element, name: &str) -> Option<&'a Element> {
    e.children.iter().find(|c| named(c, name))
}

fn text(e: &Element, key: &str) -> String {
    e.attr(key).unwrap_or_default().to_owned()
}

fn decimal(e: &Element, key: &str) -> f64 {
    e.attr(key)
        .and_then(|v| v.trim().replace(',', ".").parse().ok())
        .unwrap_or(0.0)
}

fn integer(e: &Element, key: &str) -> Option<i64> {
    let v = e.attr(key)?.trim();
    let sign_len = usize::from(v.starts_with(['-', '+']));
    let digits = v[sign_len..].bytes().take_while(u8::is_ascii_digit).count();
    v[..sign_len + digits].parse().ok()
}

fn unsigned(e: &Element, key: &str) -> u32 {
    integer(e, key)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

fn keyed_by_id(node: &Element) -> bool {
    match integer(node, "Type") {
        Some(1) => integer(node, "KeyType") == Some(0),
        Some(0) => node
            .children
            .iter()
            .all(|c| named(c, "NODE") && keyed_by_id(c)),
        _ => false,
    }
}

fn track(e: &Element, by_id: bool, ordinal: i64) -> Track {
    let total_time = decimal(e, "TotalTime");
    Track {
        id: if by_id {
            integer(e, "TrackID").unwrap_or(0)
        } else {
            ordinal
        },
        name: text(e, "Name"),
        artist: text(e, "Artist"),
        composer: text(e, "Composer"),
        album: text(e, "Album"),
        genre: text(e, "Genre"),
        kind: text(e, "Kind"),
        size: decimal(e, "Size").max(0.0) as u64,
        total_time: total_time.clamp(0.0, f64::from(u16::MAX)) as u32,
        disc_number: unsigned(e, "DiscNumber"),
        track_number: unsigned(e, "TrackNumber"),
        year: unsigned(e, "Year"),
        average_bpm: (decimal(e, "AverageBpm") * 100.0).trunc() / 100.0,
        date_added: text(e, "DateAdded"),
        bit_rate: unsigned(e, "BitRate"),
        sample_rate: decimal(e, "SampleRate").max(0.0) as u32,
        comments: text(e, "Comments"),
        play_count: unsigned(e, "PlayCount"),
        stars: u8::try_from((unsigned(e, "Rating") / 51).min(5)).unwrap_or(0),
        location: e.attr("Location").map(location).unwrap_or_default(),
        remixer: text(e, "Remixer"),
        tonality: text(e, "Tonality"),
        label: text(e, "Label"),
        mix: text(e, "Mix"),
        colour: e.attr("Colour").and_then(Colour::parse),
        tempos: tempos(e),
        marks: marks(e, total_time),
    }
}

pub fn location(url: &str) -> String {
    const PREFIX: &str = "file://localhost";
    let bytes = url.as_bytes();
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
    let decoded = String::from_utf8_lossy(&out);
    decoded
        .to_ascii_lowercase()
        .find(PREFIX)
        .map(|at| decoded[at + PREFIX.len()..].replace('\\', "/"))
        .unwrap_or_default()
}

fn tempos(e: &Element) -> Vec<Tempo> {
    e.children
        .iter()
        .filter(|c| named(c, "TEMPO"))
        .map(|t| Tempo {
            inizio: decimal(t, "Inizio"),
            bpm: decimal(t, "Bpm"),
            beat: unsigned(t, "Battito"),
            beats_per_bar: t
                .attr("Metro")
                .unwrap_or("4/4")
                .split('/')
                .next()
                .and_then(|n| n.trim().parse().ok())
                .unwrap_or(0),
        })
        .collect()
}

pub fn beats(tempos: &[Tempo], total: f64) -> Vec<Beat> {
    let mut beats: Vec<Beat> = Vec::new();
    let mut previous: Option<Tempo> = None;
    let fill = |beats: &mut Vec<Beat>, t: Tempo, until: Option<f64>| {
        let period = 60.0 / t.bpm;
        while let Some(last) = beats.last().copied() {
            let next = last.seconds + period;
            if next > total || until.is_some_and(|u| next > u - period / 2.0) {
                break;
            }
            beats.push(Beat {
                seconds: next,
                bpm: t.bpm,
                beat: last.beat % t.beats_per_bar + 1,
                marker: false,
            });
        }
    };
    for &t in tempos {
        if let Some(p) = previous
            && beats.last().is_some_and(|b| b.seconds < t.inizio)
        {
            fill(&mut beats, p, Some(t.inizio));
        }
        if !t.valid() {
            previous = None;
            continue;
        }
        let after_last = beats.last().is_none_or(|b| t.inizio > b.seconds);
        if after_last && (0.0..=total).contains(&t.inizio) {
            beats.push(Beat {
                seconds: t.inizio,
                bpm: t.bpm,
                beat: t.beat,
                marker: true,
            });
        }
        previous = Some(t);
    }
    if let Some(p) = previous {
        fill(&mut beats, p, None);
    }
    beats
}

const MEMORY_MARKS: usize = 10;

fn marks(e: &Element, total: f64) -> Vec<Mark> {
    let mut kept: Vec<Mark> = Vec::new();
    for m in e.children.iter().filter(|c| named(c, "POSITION_MARK")) {
        let loop_mark = match integer(m, "Type") {
            Some(0) => false,
            Some(4) => true,
            _ => continue,
        };
        let start = decimal(m, "Start");
        let end = decimal(m, "End");
        if !(0.0..=total).contains(&start) || (loop_mark && !(start..=total).contains(&end)) {
            continue;
        }
        let hot_cue = match integer(m, "Num") {
            Some(-1) => None,
            Some(n @ 0..=7) => Some(u8::try_from(n).unwrap_or(0)),
            _ => continue,
        };
        let channel = |k| integer(m, k).filter(|v| *v >= 0).map(|v| (v & 0xFF) as u8);
        let rgb = match (hot_cue, channel("Red"), channel("Green"), channel("Blue")) {
            (Some(_), Some(r), Some(g), Some(b)) => Some([r, g, b]),
            _ => None,
        };
        let mark = Mark {
            name: text(m, "Name"),
            start,
            end: loop_mark.then_some(end),
            hot_cue,
            rgb,
        };
        let ms = |s: f64| (s * 1000.0).round() as i64;
        let same = |k: &Mark| match hot_cue {
            Some(slot) => k.hot_cue == Some(slot),
            None => {
                k.hot_cue.is_none()
                    && ms(k.start) == ms(start)
                    && ms(k.end.unwrap_or(0.0)) == ms(end * f64::from(u8::from(loop_mark)))
            }
        };
        kept.retain(|k| !same(k));
        if hot_cue.is_none() && kept.iter().filter(|k| k.hot_cue.is_none()).count() >= MEMORY_MARKS
        {
            continue;
        }
        kept.push(mark);
    }
    kept
}

fn nodes(children: &[Element]) -> Vec<Node> {
    children
        .iter()
        .filter(|c| named(c, "NODE"))
        .filter_map(|n| {
            let name = text(n, "Name");
            match integer(n, "Type") {
                Some(0) => Some(Node::Folder {
                    name,
                    children: nodes(&n.children),
                }),
                Some(1) => Some(Node::Playlist {
                    name,
                    entries: entries(n),
                }),
                _ => None,
            }
        })
        .collect()
}

fn entries(list: &Element) -> Vec<Entry> {
    let by_id = match integer(list, "KeyType") {
        Some(0) => true,
        Some(1) => false,
        _ => return Vec::new(),
    };
    list.children
        .iter()
        .filter(|c| named(c, "TRACK"))
        .filter_map(|t| {
            if by_id {
                integer(t, "Key")
                    .or(Some(0))
                    .filter(|k| *k >= 0)
                    .map(Entry::Id)
            } else {
                let path = location(t.attr("Key").unwrap_or_default());
                (!path.is_empty()).then_some(Entry::Path(path))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::*;

    const FILE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <PRODUCT Name="x" Version="1" Company="y"/>
  <COLLECTION Entries="2">
    <TRACK TrackID="7" Name="One" Artist="A" Album="B" Genre="Techno" Kind="MP3 File"
      Size="1234,5" TotalTime="200.9" Year="2020" AverageBpm="128.009" DateAdded="2024-01-02"
      BitRate="320" SampleRate="44100" Comments="c" PlayCount="3" Rating="153"
      Location="file://localhost/Users/me/Music/Caf%C3%A9%20Mix.mp3" Tonality="Am" Label="L"
      Mix="Extended" Remixer="R" Colour="0x25fde9">
      <TEMPO Inizio="0.100" Bpm="128.00" Metro="4/4" Battito="1"/>
      <POSITION_MARK Name="" Type="0" Start="0.100" Num="-1"/>
      <POSITION_MARK Name="Drop" Type="0" Start="60.0" Num="0" Red="40" Green="226" Blue="20"/>
      <POSITION_MARK Name="Fade" Type="1" Start="10" Num="-1"/>
      <POSITION_MARK Name="Loop" Type="4" Start="30" End="31.875" Num="-1"/>
      <POSITION_MARK Name="Late" Type="0" Start="999" Num="1"/>
      <POSITION_MARK Name="Drop2" Type="0" Start="61" Num="0"/>
    </TRACK>
    <TRACK TrackID="9" Name="Two" Location="file:///Users/me/x.mp3" Rating="255" TotalTime="10"/>
  </COLLECTION>
  <PLAYLISTS>
    <NODE Type="0" Name="ROOT" Count="2">
      <NODE Type="1" Name="Set" KeyType="0" Entries="3">
        <TRACK Key="9"/><TRACK Key="7"/><TRACK Key="-1"/>
      </NODE>
      <NODE Type="0" Name="Folder" Count="1">
        <NODE Type="1" Name="Inner" KeyType="0" Entries="1"><TRACK Key="7"/></NODE>
      </NODE>
    </NODE>
  </PLAYLISTS>
</DJ_PLAYLISTS>"#;

    #[test]
    fn tracks_read_with_every_field() {
        let lib = parse(FILE).unwrap();
        let t = &lib.tracks[0];
        assert_eq!(t.id, 7);
        assert_eq!((t.name.as_str(), t.artist.as_str()), ("One", "A"));
        assert_eq!(t.size, 1234);
        assert_eq!(t.total_time, 200);
        assert!((t.average_bpm - 128.0).abs() < 1e-9);
        assert_eq!(t.stars, 3);
        assert_eq!(t.location, "/Users/me/Music/Café Mix.mp3");
        assert_eq!(t.colour, Some(Colour::Aqua));
        assert_eq!(t.tonality, "Am");
        let two = &lib.tracks[1];
        assert_eq!((two.id, two.stars), (9, 5));
        assert_eq!(two.location, "", "file:/// without localhost gives no path");
    }

    #[test]
    fn marks_keep_cues_and_loops_only() {
        let t = &parse(FILE).unwrap().tracks[0];
        let names: Vec<&str> = t.marks.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["", "Loop", "Drop2"]);
        assert_eq!(t.marks[1].end, Some(31.875));
        assert_eq!(t.marks[2].hot_cue, Some(0));
        assert_eq!(t.marks[2].rgb, None);
        assert_eq!(t.marks[0].hot_cue, None);
    }

    #[test]
    fn memory_marks_stop_at_ten() {
        let mut marks = String::new();
        for i in 0..12 {
            write!(marks, r#"<POSITION_MARK Type="0" Start="{i}" Num="-1"/>"#).unwrap();
        }
        let file = format!(
            r#"<DJ_PLAYLISTS><COLLECTION><TRACK TotalTime="100">{marks}</TRACK></COLLECTION></DJ_PLAYLISTS>"#
        );
        assert_eq!(parse(&file).unwrap().tracks[0].marks.len(), 10);
    }

    #[test]
    fn playlists_follow_the_tree() {
        let lib = parse(FILE).unwrap();
        let Node::Playlist { name, entries } = &lib.playlists[0] else {
            panic!()
        };
        assert_eq!(name, "Set");
        assert_eq!(entries, &[Entry::Id(9), Entry::Id(7)]);
        let Node::Folder { children, .. } = &lib.playlists[1] else {
            panic!()
        };
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn a_node_of_another_type_is_skipped_and_turns_ids_off() {
        let file = FILE.replace(
            r#"<TRACK Key="7"/></NODE>"#,
            r#"<TRACK Key="7"/></NODE><NODE Type="5" Name="Odd"/>"#,
        );
        let lib = parse(&file).unwrap();
        let Node::Folder { children, .. } = &lib.playlists[1] else {
            panic!()
        };
        assert_eq!(children.len(), 1);
        assert_eq!((lib.tracks[0].id, lib.tracks[1].id), (1, 2));
    }

    #[test]
    fn one_list_by_location_numbers_the_tracks_in_order() {
        let file = FILE.replace(
            r#"<NODE Type="1" Name="Inner" KeyType="0""#,
            r#"<NODE Type="1" Name="Inner" KeyType="1""#,
        );
        let lib = parse(&file).unwrap();
        assert_eq!((lib.tracks[0].id, lib.tracks[1].id), (1, 2));
    }

    #[test]
    fn without_root_there_are_no_playlists_and_other_roots_fail() {
        let file = FILE.replace(r#"Name="ROOT""#, r#"Name="Top""#);
        assert_eq!(parse(&file).unwrap().playlists, []);
        assert!(parse("<NML/>").is_err());
    }

    #[test]
    fn locations_decode_right() {
        assert_eq!(location("file://localhost/C:/a%20b.mp3"), "/C:/a b.mp3");
        assert_eq!(location("FILE://LOCALHOST/x%2"), "/x%2");
        assert_eq!(location("file://localhost/a+b"), "/a+b");
        assert_eq!(location("/plain/path.mp3"), "");
    }

    #[test]
    fn tempo_markers_make_a_beat_list() {
        let t = |inizio, bpm| Tempo {
            inizio,
            bpm,
            beat: 1,
            beats_per_bar: 4,
        };
        let grid = beats(&[t(0.0, 120.0), t(2.2, 60.0)], 5.0);
        let at: Vec<f64> = grid.iter().map(|b| b.seconds).collect();
        assert_eq!(at, [0.0, 0.5, 1.0, 1.5, 2.2, 3.2, 4.2]);
        assert_eq!(grid[1].beat, 2);
        assert!(grid[4].marker && !grid[5].marker);
        assert!(
            beats(&[t(0.0, 0.0)], 5.0).is_empty(),
            "a BPM of 0 does not count"
        );
    }
}
