use std::cmp::Ordering;

use backspn_formats::dj_playlists::{self, Colour, Mark};
use backspn_formats::music_library::{MusicLibrary, MusicPlaylist};
use backspn_formats::nml::MusicalKey;
use rusqlite::{OptionalExtension, params, types::Value};

use crate::{CuePoint, ImportReport, Library, NodeKind, Result, paths};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExternalLibrary {
    pub tracks: Vec<ExternalTrack>,
    pub playlists: Vec<ExternalNode>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExternalTrack {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub comment: String,
    pub remixer: String,
    pub producer: String,
    pub mix: String,
    pub key_text: String,
    pub musical_key: Option<u8>,
    pub bpm: Option<f64>,
    pub duration_s: Option<f64>,
    pub rating: i64,
    pub playcount: i64,
    pub import_date: String,
    pub last_played: String,
    pub bitrate: Option<i64>,
    pub track_number: Option<i64>,
    pub filesize_kib: Option<i64>,
    pub color: u8,
    pub grid: Option<Grid>,
    pub cues: Vec<CuePoint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub bpm: f64,
    pub anchor_s: f64,
    pub markers_s: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExternalNode {
    Folder {
        name: String,
        children: Vec<ExternalNode>,
    },
    Playlist {
        name: String,
        tracks: Vec<usize>,
    },
}

impl ExternalNode {
    pub fn name(&self) -> &str {
        match self {
            Self::Folder { name, .. } | Self::Playlist { name, .. } => name,
        }
    }

    pub fn all_tracks(&self) -> Vec<usize> {
        let mut out = Vec::new();
        self.collect(&mut out);
        let mut seen = std::collections::HashSet::new();
        out.retain(|t| seen.insert(*t));
        out
    }

    fn collect(&self, out: &mut Vec<usize>) {
        match self {
            Self::Folder { children, .. } => children.iter().for_each(|c| c.collect(out)),
            Self::Playlist { tracks, .. } => out.extend(tracks),
        }
    }
}

const MAX_EXTRA_MARKERS: usize = 7;

impl ExternalLibrary {
    pub fn from_dj_playlists(lib: &dj_playlists::Library) -> Self {
        let tracks: Vec<ExternalTrack> = lib.tracks.iter().map(track_from_xml).collect();
        let index = |entry: &dj_playlists::Entry| match entry {
            dj_playlists::Entry::Id(id) => lib.tracks.iter().position(|t| t.id == *id),
            dj_playlists::Entry::Path(p) => lib
                .tracks
                .iter()
                .position(|t| t.location.eq_ignore_ascii_case(p)),
        };
        Self {
            playlists: xml_nodes(&lib.playlists, &index),
            tracks,
        }
    }
}

impl ExternalLibrary {
    pub fn from_music(lib: &MusicLibrary) -> Self {
        let tracks = lib
            .tracks
            .iter()
            .map(|t| ExternalTrack {
                path: t.path.clone(),
                title: t.title.clone(),
                artist: t.artist.clone(),
                album: t.album.clone(),
                genre: t.genre.clone(),
                comment: t.comments.clone(),
                bpm: (t.bpm > 0).then_some(f64::from(t.bpm)),
                duration_s: (t.seconds > 0).then_some(f64::from(t.seconds)),
                rating: i64::from(t.rating),
                import_date: t.date_added.clone(),
                last_played: t.last_played.clone(),
                bitrate: (t.bitrate > 0).then_some(i64::from(t.bitrate)),
                track_number: (t.track_number > 0).then_some(i64::from(t.track_number)),
                filesize_kib: (t.size_kib > 0)
                    .then(|| i64::try_from(t.size_kib).unwrap_or(i64::MAX)),
                ..ExternalTrack::default()
            })
            .collect();
        let index: std::collections::HashMap<i64, usize> = lib
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id, i))
            .collect();
        let playlists = if lib.playlists.iter().any(|p| !p.tracks.is_empty()) {
            music_nodes(&lib.playlists, None, &index)
        } else {
            Vec::new()
        };
        Self { tracks, playlists }
    }
}

fn music_nodes(
    all: &[MusicPlaylist],
    parent: Option<&str>,
    index: &std::collections::HashMap<i64, usize>,
) -> Vec<ExternalNode> {
    let mut children: Vec<&MusicPlaylist> = all
        .iter()
        .filter(|p| p.parent.as_deref() == parent && Some(p.id.as_str()) != parent)
        .collect();
    let group = |p: &MusicPlaylist| {
        if p.master {
            0
        } else if p.folder {
            1
        } else if p.smart {
            2
        } else {
            3
        }
    };
    children.sort_by(|a, b| {
        group(a)
            .cmp(&group(b))
            .then_with(|| natural_cmp(&a.name, &b.name))
    });
    children
        .into_iter()
        .map(|p| {
            if p.folder {
                ExternalNode::Folder {
                    name: p.name.clone(),
                    children: music_nodes(all, Some(&p.id), index),
                }
            } else {
                ExternalNode::Playlist {
                    name: p.name.clone(),
                    tracks: p
                        .tracks
                        .iter()
                        .filter_map(|id| index.get(id).copied())
                        .collect(),
                }
            }
        })
        .collect()
}

pub fn natural_cmp(left: &str, right: &str) -> Ordering {
    let (mut left, mut right) = (left.chars().peekable(), right.chars().peekable());
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(l), Some(r)) if l.is_ascii_digit() && r.is_ascii_digit() => {
                let order = number(&mut left).cmp(&number(&mut right));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(l), Some(r)) => {
                let order = l.to_lowercase().cmp(r.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                left.next();
                right.next();
            }
        }
    }
}

fn number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u128 {
    let mut value: u128 = 0;
    while let Some(digit) = chars.peek().and_then(|c| c.to_digit(10)) {
        value = value.saturating_mul(10).saturating_add(u128::from(digit));
        chars.next();
    }
    value
}

fn xml_nodes(
    list: &[dj_playlists::Node],
    index: &dyn Fn(&dj_playlists::Entry) -> Option<usize>,
) -> Vec<ExternalNode> {
    list.iter()
        .map(|n| match n {
            dj_playlists::Node::Folder { name, children } => ExternalNode::Folder {
                name: name.clone(),
                children: xml_nodes(children, index),
            },
            dj_playlists::Node::Playlist { name, entries } => ExternalNode::Playlist {
                name: name.clone(),
                tracks: entries.iter().filter_map(index).collect(),
            },
        })
        .collect()
}

fn track_from_xml(t: &dj_playlists::Track) -> ExternalTrack {
    let key = MusicalKey::parse(t.tonality.trim());
    ExternalTrack {
        path: t.location.clone(),
        title: t.name.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        genre: t.genre.clone(),
        label: t.label.clone(),
        comment: t.comments.clone(),
        remixer: t.remixer.clone(),
        producer: t.composer.clone(),
        mix: t.mix.clone(),
        key_text: key.map_or_else(|| t.tonality.clone(), MusicalKey::open_key),
        musical_key: key.map(MusicalKey::value),
        bpm: (t.average_bpm > 0.0).then_some(t.average_bpm),
        duration_s: (t.total_time > 0).then_some(f64::from(t.total_time)),
        rating: i64::from(t.stars) * 51,
        playcount: i64::from(t.play_count),
        import_date: date(&t.date_added),
        last_played: String::new(),
        bitrate: (t.bit_rate > 0).then_some(i64::from(t.bit_rate) * 1000),
        track_number: (t.track_number > 0).then_some(i64::from(t.track_number)),
        filesize_kib: (t.size > 0).then(|| i64::try_from(t.size / 1024).unwrap_or(i64::MAX)),
        color: t.colour.map_or(0, color),
        grid: grid(&t.tempos, f64::from(t.total_time)),
        cues: t.marks.iter().map(cue).collect(),
    }
}

fn color(c: Colour) -> u8 {
    match c {
        Colour::Red => 1,
        Colour::Orange => 2,
        Colour::Yellow => 3,
        Colour::Green => 4,
        Colour::Aqua | Colour::Blue => 5,
        Colour::Purple => 6,
        Colour::Pink => 7,
    }
}

fn date(text: &str) -> String {
    let mut parts = text.trim().split('-').map(|p| p.parse::<u32>().ok());
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(Some(y)), Some(Some(m)), Some(Some(d)), None)
            if (1..=12).contains(&m) && (1..=31).contains(&d) =>
        {
            format!("{y}/{m}/{d}")
        }
        _ => String::new(),
    }
}

fn grid(tempos: &[dj_playlists::Tempo], total: f64) -> Option<Grid> {
    let beats = dj_playlists::beats(tempos, total);
    let first = beats.first()?;
    let mut bpm = first.bpm;
    let mut markers_s = Vec::new();
    for b in beats.iter().skip(1).filter(|b| b.marker) {
        if (b.bpm - bpm).abs() >= 0.005 && markers_s.len() < MAX_EXTRA_MARKERS {
            markers_s.push(b.seconds);
            bpm = b.bpm;
        }
    }
    Some(Grid {
        bpm: first.bpm,
        anchor_s: first.seconds,
        markers_s,
    })
}

fn cue(m: &Mark) -> CuePoint {
    let len_s = m.end.map_or(0.0, |e| e - m.start);
    CuePoint {
        kind: if m.end.is_some() {
            CuePoint::LOOP
        } else {
            CuePoint::CUE
        },
        name: m.name.clone(),
        start_s: m.start,
        len_s,
        hotcue: m.hot_cue,
        display_order: 0,
        repeats: -1,
        color: m.rgb.map(|[r, g, b]| format!("#{r:02X}{g:02X}{b:02X}")),
    }
}

fn tree_name(name: &str) -> String {
    let name = name
        .trim()
        .trim_start_matches('_')
        .replace(['$', '\\'], "-");
    if name.is_empty() {
        "Imported".to_owned()
    } else {
        name
    }
}

impl Library {
    pub fn import_external_track(&mut self, track: &ExternalTrack) -> Result<i64> {
        Ok(self.import_external_tracks(&[track])?[0])
    }

    pub fn import_external_tracks(&mut self, tracks: &[&ExternalTrack]) -> Result<Vec<i64>> {
        let volume = paths::startup_volume();
        let tx = self.conn.transaction()?;
        let ids = tracks
            .iter()
            .map(|t| import_in(&tx, t, &volume))
            .collect::<Result<Vec<i64>>>()?;
        tx.commit()?;
        Ok(ids)
    }

    pub fn sync_song_info(&mut self, tracks: &[ExternalTrack]) -> Result<usize> {
        let volume = paths::startup_volume();
        let tx = self.conn.transaction()?;
        let mut changed = 0;
        for t in tracks.iter().filter(|t| !t.path.is_empty()) {
            changed += tx.execute(
                "UPDATE tracks SET title = ?2, artist = ?3, album = ?4, rating = ?5, dirty = 1 \
                 WHERE path = ?1 AND (title, artist, album, rating) IS NOT (?2, ?3, ?4, ?5)",
                params![
                    paths::canonical(&t.path, &volume),
                    t.title,
                    t.artist,
                    t.album,
                    t.rating
                ],
            )?;
        }
        tx.commit()?;
        Ok(changed)
    }

    pub fn import_external_node(
        &mut self,
        lib: &ExternalLibrary,
        node: &ExternalNode,
        parent: Option<i64>,
    ) -> Result<ImportReport> {
        let wanted: Vec<usize> = node
            .all_tracks()
            .into_iter()
            .filter(|&t| lib.tracks.get(t).is_some_and(|t| !t.path.is_empty()))
            .collect();
        let tracks: Vec<&ExternalTrack> = wanted.iter().map(|&t| &lib.tracks[t]).collect();
        let ids: std::collections::HashMap<usize, i64> = wanted
            .into_iter()
            .zip(self.import_external_tracks(&tracks)?)
            .collect();
        let mut report = ImportReport {
            tracks: ids.len(),
            ..ImportReport::default()
        };
        self.copy_node(node, parent, &ids, &mut report)?;
        Ok(report)
    }

    fn copy_node(
        &mut self,
        node: &ExternalNode,
        parent: Option<i64>,
        ids: &std::collections::HashMap<usize, i64>,
        report: &mut ImportReport,
    ) -> Result<()> {
        let name = self.unique_node_name(parent, &tree_name(node.name()))?;
        match node {
            ExternalNode::Folder { children, .. } => {
                let id = self.create_node(parent, NodeKind::Folder, &name, None)?;
                for child in children {
                    self.copy_node(child, Some(id), ids, report)?;
                }
            }
            ExternalNode::Playlist { tracks, .. } => {
                let entries: Vec<i64> = tracks.iter().filter_map(|t| ids.get(t).copied()).collect();
                let id = self.create_node(parent, NodeKind::Playlist, &name, None)?;
                self.add_to_playlist(id, &entries)?;
                report.playlists += 1;
            }
        }
        Ok(())
    }
}

fn import_in(tx: &rusqlite::Connection, track: &ExternalTrack, volume: &str) -> Result<i64> {
    let path = paths::canonical(&track.path, volume);
    let known: Option<i64> = tx
        .query_row("SELECT id FROM tracks WHERE path = ?1", [&path], |r| {
            r.get(0)
        })
        .optional()?;
    let id = if let Some(id) = known {
        id
    } else {
        let id = crate::insert_file(
            tx,
            std::path::Path::new(&track.path),
            &path,
            crate::FileFacts::default(),
        )?;
        tx.execute("UPDATE tracks SET dirty = 0 WHERE id = ?1", [id])?;
        id
    };
    fill_fields(tx, id, track, known.is_some())?;
    fill_grid(tx, id, track)?;
    fill_cues(tx, id, track)?;
    Ok(id)
}

fn fill_fields(
    tx: &rusqlite::Connection,
    id: i64,
    track: &ExternalTrack,
    only_empty: bool,
) -> Result<()> {
    let text = |v: &str| (!v.is_empty()).then(|| Value::Text(v.to_owned()));
    let int = |v: Option<i64>| v.map(Value::Integer);
    let fields: [(&str, Option<Value>); 20] = [
        ("title", text(&track.title)),
        ("artist", text(&track.artist)),
        ("album", text(&track.album)),
        ("genre", text(&track.genre)),
        ("label", text(&track.label)),
        ("comment", text(&track.comment)),
        ("remixer", text(&track.remixer)),
        ("producer", text(&track.producer)),
        ("mix", text(&track.mix)),
        ("key_text", text(&track.key_text)),
        ("musical_key", int(track.musical_key.map(i64::from))),
        ("duration_s", track.duration_s.map(Value::Real)),
        ("rating", int((track.rating > 0).then_some(track.rating))),
        (
            "playcount",
            int((track.playcount > 0).then_some(track.playcount)),
        ),
        ("import_date", text(&track.import_date)),
        ("last_played", text(&track.last_played)),
        ("bitrate", int(track.bitrate)),
        ("track_number", int(track.track_number)),
        ("filesize_kib", int(track.filesize_kib)),
        (
            "color",
            int((track.color > 0).then_some(i64::from(track.color))),
        ),
    ];
    for (column, value) in fields {
        let Some(value) = value else { continue };
        let guard = if only_empty {
            format!(" AND ({column} IS NULL OR {column} = '' OR {column} = 0)")
        } else {
            String::new()
        };
        tx.execute(
            &format!("UPDATE tracks SET {column} = ?2 WHERE id = ?1{guard}"),
            params![id, value],
        )?;
    }
    Ok(())
}

fn fill_grid(tx: &rusqlite::Connection, id: i64, track: &ExternalTrack) -> Result<()> {
    let gridded: bool = tx.query_row(
        "SELECT bpm IS NOT NULL AND grid_anchor_s IS NOT NULL FROM tracks WHERE id = ?1",
        [id],
        |r| r.get(0),
    )?;
    if let Some(g) = track.grid.as_ref().filter(|_| !gridded) {
        tx.execute(
            "UPDATE tracks SET bpm = ?2, grid_anchor_s = ?3, analyzed = 1 WHERE id = ?1",
            params![id, g.bpm, g.anchor_s],
        )?;
        for &at in &g.markers_s {
            tx.execute(
                "INSERT INTO cues (track_id, position, kind, name, start_s, len_s, hotcue, \
                 display_order, repeats) VALUES (?1, -1, ?2, '', ?3, 0, -1, 0, -1)",
                params![id, CuePoint::GRID, at],
            )?;
        }
    } else if let Some(bpm) = track.bpm {
        tx.execute(
            "UPDATE tracks SET bpm = ?2 WHERE id = ?1 AND bpm IS NULL",
            params![id, bpm],
        )?;
    }
    Ok(())
}

fn fill_cues(tx: &rusqlite::Connection, id: i64, track: &ExternalTrack) -> Result<()> {
    let has_cues: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM cues WHERE track_id = ?1 AND NOT (kind = ?2 AND hotcue < 0))",
        params![id, CuePoint::GRID],
        |r| r.get(0),
    )?;
    if has_cues {
        return Ok(());
    }
    for (position, c) in track.cues.iter().enumerate() {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"<DJ_PLAYLISTS>
  <COLLECTION>
    <TRACK TrackID="1" Name="One" Artist="A" TotalTime="300" AverageBpm="128.00" Rating="204"
      DateAdded="2024-01-02" Tonality="Am" Colour="0xFF007F" Size="2048000" BitRate="320"
      Location="file://localhost/music/one.mp3">
      <TEMPO Inizio="0.5" Bpm="128" Metro="4/4" Battito="1"/>
      <TEMPO Inizio="60.5" Bpm="130" Metro="4/4" Battito="1"/>
      <POSITION_MARK Name="Drop" Type="0" Start="30" Num="2" Red="255" Green="0" Blue="16"/>
      <POSITION_MARK Name="" Type="4" Start="10" End="12" Num="-1"/>
    </TRACK>
    <TRACK TrackID="2" Name="Two" TotalTime="100" Location="file://localhost/music/two.mp3"/>
  </COLLECTION>
  <PLAYLISTS>
    <NODE Type="0" Name="ROOT">
      <NODE Type="0" Name="_Gigs">
        <NODE Type="1" Name="Friday" KeyType="0"><TRACK Key="2"/><TRACK Key="1"/></NODE>
      </NODE>
    </NODE>
  </PLAYLISTS>
</DJ_PLAYLISTS>"#;

    fn external() -> ExternalLibrary {
        ExternalLibrary::from_dj_playlists(&dj_playlists::parse(FILE).unwrap())
    }

    #[test]
    fn xml_tracks_in_the_collection_terms() {
        let lib = external();
        let t = &lib.tracks[0];
        assert_eq!(t.path, "/music/one.mp3");
        assert_eq!((t.rating, t.color), (204, 7));
        assert_eq!(t.import_date, "2024/1/2");
        assert_eq!(t.key_text, MusicalKey::parse("Am").unwrap().open_key());
        assert_eq!(t.filesize_kib, Some(2000));
        assert_eq!(t.bitrate, Some(320_000));
        let g = t.grid.as_ref().unwrap();
        assert_eq!(
            (g.bpm, g.anchor_s, g.markers_s.clone()),
            (128.0, 0.5, vec![60.5])
        );
        assert_eq!(t.cues[0].hotcue, Some(2));
        assert_eq!(t.cues[0].color.as_deref(), Some("#FF0010"));
        assert_eq!((t.cues[1].kind, t.cues[1].len_s), (CuePoint::LOOP, 2.0));
        assert_eq!(lib.playlists[0].all_tracks(), [1, 0]);
    }

    #[test]
    fn music_lists_keep_their_order() {
        let list = |name: &str, id: &str, parent: Option<&str>| MusicPlaylist {
            name: name.into(),
            id: id.into(),
            parent: parent.map(Into::into),
            tracks: vec![7],
            ..MusicPlaylist::default()
        };
        let music = MusicLibrary {
            tracks: vec![backspn_formats::music_library::MusicTrack {
                id: 7,
                title: "T".into(),
                path: "/m/t.mp3".into(),
                bpm: 120,
                ..Default::default()
            }],
            playlists: vec![
                list("Set 10", "a", None),
                list("set 9", "b", None),
                MusicPlaylist {
                    smart: true,
                    ..list("Recent", "c", None)
                },
                MusicPlaylist {
                    folder: true,
                    ..list("Gigs", "d", None)
                },
                list("Inner", "e", Some("d")),
                MusicPlaylist {
                    master: true,
                    ..list("Library", "f", None)
                },
            ],
        };
        let lib = ExternalLibrary::from_music(&music);
        let names: Vec<&str> = lib.playlists.iter().map(ExternalNode::name).collect();
        assert_eq!(names, ["Library", "Gigs", "Recent", "set 9", "Set 10"]);
        let ExternalNode::Folder { children, .. } = &lib.playlists[1] else {
            panic!()
        };
        assert_eq!(children[0].all_tracks(), [0]);
        assert_eq!(lib.tracks[0].bpm, Some(120.0));
        let empty = MusicLibrary {
            playlists: vec![MusicPlaylist::default()],
            ..music
        };
        assert_eq!(ExternalLibrary::from_music(&empty).playlists, []);
    }

    #[test]
    fn colours_and_dates_convert() {
        assert_eq!(color(Colour::Aqua), 5);
        assert_eq!(color(Colour::Purple), 6);
        assert_eq!(date("2024-13-01"), "");
        assert_eq!(date("x"), "");
        assert_eq!(tree_name("_Gigs"), "Gigs");
        assert_eq!(tree_name("A$B\\C"), "A-B-C");
        assert_eq!(tree_name("__"), "Imported");
    }

    #[test]
    fn a_new_track_takes_the_library_data_and_an_old_one_keeps_its_own() {
        let mut lib = Library::open_in_memory().unwrap();
        let ext = external();
        let id = lib.import_external_track(&ext.tracks[0]).unwrap();
        let row = lib.track_by_path("/music/one.mp3").unwrap().unwrap();
        assert_eq!((row.id, row.title.as_str(), row.rating), (id, "One", 204));
        assert_eq!(row.bpm, Some(128.0));
        let cues = lib.cues(id).unwrap();
        assert_eq!(cues.iter().filter(|c| c.is_beatgrid()).count(), 1);
        assert_eq!(cues.iter().filter(|c| !c.is_beatgrid()).count(), 2);

        let mut changed = ext.tracks[0].clone();
        changed.title = "Other".into();
        changed.genre = "Techno".into();
        changed.cues.truncate(1);
        assert_eq!(lib.import_external_track(&changed).unwrap(), id);
        let row = lib.track_by_path("/music/one.mp3").unwrap().unwrap();
        assert_eq!((row.title.as_str(), row.genre.as_str()), ("One", "Techno"));
        assert_eq!(
            lib.cues(id)
                .unwrap()
                .iter()
                .filter(|c| !c.is_beatgrid())
                .count(),
            2
        );
    }

    #[test]
    fn sync_song_info_overwrites_four_fields_of_known_tracks() {
        let mut lib = Library::open_in_memory().unwrap();
        let ext = external();
        let id = lib.import_external_track(&ext.tracks[0]).unwrap();
        let mut newer = ext.tracks.clone();
        newer[0].title = "New".into();
        newer[0].genre = "Not synced".into();
        newer[0].rating = 51;
        assert_eq!(
            lib.sync_song_info(&newer).unwrap(),
            1,
            "the other track is not there"
        );
        let row = lib.track(id).unwrap().unwrap();
        assert_eq!((row.title.as_str(), row.rating), ("New", 51));
        assert_eq!(row.genre, "");
        assert_eq!(
            lib.sync_song_info(&newer).unwrap(),
            0,
            "nothing left to change"
        );
    }

    #[test]
    fn a_folder_comes_in_with_its_playlists() {
        let mut lib = Library::open_in_memory().unwrap();
        let ext = external();
        let report = lib
            .import_external_node(&ext, &ext.playlists[0], None)
            .unwrap();
        assert_eq!((report.tracks, report.playlists), (2, 1));
        let nodes = lib.nodes().unwrap();
        let folder = nodes.iter().find(|n| n.name == "Gigs").unwrap();
        let list = nodes.iter().find(|n| n.parent == Some(folder.id)).unwrap();
        assert_eq!(list.name, "Friday");
        let titles: Vec<String> = lib
            .node_tracks(list.id)
            .unwrap()
            .iter()
            .map(|&t| lib.track(t).unwrap().unwrap().title)
            .collect();
        assert_eq!(titles, ["Two", "One"]);
    }
}
