use std::path::PathBuf;

use super::*;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/nml/small.nml")
}

#[test]
fn files_read_elsewhere_are_added_once_in_one_go() {
    let mut lib = Library::open_in_memory().unwrap();
    let a = PathBuf::from("/music/Artist - Song.flac");
    let b = PathBuf::from("/music/Other.mp3");
    let ids = lib
        .add_read_files(vec![
            (a.clone(), FileFacts::default()),
            (b, FileFacts::default()),
        ])
        .unwrap();
    assert!(ids.iter().all(Option::is_some));
    let first = lib.track(ids[0].unwrap()).unwrap().unwrap();
    assert_eq!(
        (first.artist.as_str(), first.title.as_str()),
        ("Artist", "Song")
    );
    let again = lib.add_read_files(vec![(a, FileFacts::default())]).unwrap();
    assert_eq!(again, [ids[0]]);
}

fn imported() -> Library {
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_nml(fixture()).unwrap();
    lib
}

#[test]
fn import_reports_counts() {
    let mut lib = Library::open_in_memory().unwrap();
    let report = lib.import_nml(fixture()).unwrap();
    assert_eq!(
        report,
        ImportReport {
            tracks: 3,
            cues: 4,
            playlists: 1,
            smartlists: 1,
            folders: 1,
            missing_playlist_tracks: 0,
        }
    );
    let tracks = lib.tracks().unwrap();
    assert_eq!(tracks[0].title, "First Light (Club Mix)");
    let location = backspn_formats::nml::Location {
        dir: "/:Music/:Techno/:".into(),
        file: "Test Artist - First Light.flac".into(),
        volume: "DJ".into(),
        volume_id: None,
    };
    assert_eq!(
        tracks[0].path,
        crate::paths::local_path(&location).to_string_lossy()
    );
    #[cfg(target_os = "macos")]
    assert_eq!(
        tracks[0].path,
        "/Volumes/DJ/Music/Techno/Test Artist - First Light.flac"
    );
    assert_eq!(tracks[0].bpm, Some(136.000_015));
    assert!((tracks[0].grid_anchor_s.unwrap() - 0.005_832_434).abs() < 1e-12);
    assert!(tracks[0].analyzed && !tracks[2].analyzed);
    assert!(tracks[1].locked);
}

#[test]
fn analysis_and_hotcues_round_trip_through_nml() {
    let mut lib = imported();
    let id = 3;
    lib.save_analysis(
        id,
        &AnalysisUpdate {
            bpm: Some(128.0),
            grid_anchor_s: Some(0.25),
            musical_key: Some(9),
            key_text: Some("4d".into()),
            lufs: Some(-8.5),
            duration_s: Some(300.0),
            replace_locked: false,
            transients: Vec::new(),
        },
    )
    .unwrap();
    let cue = |kind, start_s, len_s, hotcue| CuePoint {
        kind,
        name: "n.n.".into(),
        start_s,
        len_s,
        hotcue,
        display_order: 0,
        repeats: -1,
        color: None,
    };
    lib.save_cues(
        id,
        &[
            cue(CuePoint::CUE, 10.0, 0.0, Some(0)),
            cue(CuePoint::LOAD, 15.0, 0.0, None),
            cue(CuePoint::LOOP, 20.0, 1.875, Some(2)),
            cue(CuePoint::FADE_OUT, 30.0, 0.0, Some(3)),
        ],
    )
    .unwrap();
    let t = lib.track(id).unwrap().unwrap();
    assert_eq!(t.bpm, Some(128.0));
    assert_eq!(t.grid_anchor_s, Some(0.25));
    assert_eq!(t.musical_key, Some(9));
    let cues = lib.cues(id).unwrap();
    assert!(cues.iter().any(CuePoint::is_beatgrid), "the analysis grid");
    let hot: Vec<_> = cues.iter().filter(|c| c.hotcue.is_some()).collect();
    assert_eq!(hot.len(), 3);
    assert_eq!((hot[1].hotcue, hot[1].len_s), (Some(2), 1.875));
    assert_eq!((hot[2].hotcue, hot[2].kind), (Some(3), CuePoint::FADE_OUT));
    assert!(
        cues.iter().any(|c| c.kind == CuePoint::LOAD),
        "cues without a hotcue are stored too"
    );

    let locked_before = lib.track(2).unwrap().unwrap();
    lib.save_analysis(
        2,
        &AnalysisUpdate {
            bpm: Some(99.0),
            grid_anchor_s: Some(1.0),
            musical_key: None,
            key_text: None,
            lufs: None,
            duration_s: None,
            replace_locked: false,
            transients: Vec::new(),
        },
    )
    .unwrap();
    assert_eq!(lib.track(2).unwrap().unwrap().bpm, locked_before.bpm);
    lib.save_analysis(
        2,
        &AnalysisUpdate {
            bpm: Some(99.0),
            replace_locked: true,
            ..AnalysisUpdate::default()
        },
    )
    .unwrap();
    assert_eq!(
        lib.track(2).unwrap().unwrap().bpm,
        Some(99.0),
        "tempo alone"
    );
    lib.set_locked(&[2], false).unwrap();
    assert!(!lib.track(2).unwrap().unwrap().locked);
}

#[test]
fn transients_are_cached_even_for_locked_tracks() {
    let mut lib = imported();
    assert_eq!(lib.transients(2).unwrap(), []);
    let transients = vec![Transient {
        position_s: 1.5,
        weight: 2.0,
        bass_weight: 3.0,
    }];
    lib.save_analysis(
        2,
        &AnalysisUpdate {
            transients: transients.clone(),
            ..AnalysisUpdate::default()
        },
    )
    .unwrap();
    assert_eq!(lib.transients(2).unwrap(), transients);
    lib.save_analysis(2, &AnalysisUpdate::default()).unwrap();
    assert_eq!(lib.transients(2).unwrap(), transients);
}

#[test]
fn search_matches_all_words_anywhere() {
    let lib = imported();
    assert_eq!(lib.search("first club").unwrap(), [1]);
    assert_eq!(lib.search("techno").unwrap(), [1], "genre and folder");
    assert_eq!(lib.search("").unwrap(), [1, 2, 3]);
    assert_eq!(lib.search("100%").unwrap(), Vec::<i64>::new());
}

#[test]
fn playlists_and_smartlists() {
    let mut lib = imported();
    let nodes = lib.nodes().unwrap();
    let names: Vec<_> = nodes.iter().map(|n| (n.name.as_str(), n.kind)).collect();
    assert_eq!(
        names,
        [
            ("Set 1", NodeKind::Playlist),
            ("Deep", NodeKind::Smartlist),
            ("Archive", NodeKind::Folder)
        ]
    );
    assert_eq!(lib.node_tracks(nodes[0].id).unwrap(), [2, 1]);
    assert_eq!(
        lib.node_tracks(nodes[1].id).unwrap(),
        [1],
        "comment set and label contains Fixture"
    );

    let archive = nodes[2].id;
    let new = lib
        .create_node(Some(archive), NodeKind::Playlist, "Warmup", None)
        .unwrap();
    lib.add_to_playlist(new, &[3, 1]).unwrap();
    lib.remove_from_playlist(new, 0).unwrap();
    assert_eq!(lib.node_tracks(new).unwrap(), [1]);
    assert!(
        lib.create_node(None, NodeKind::Smartlist, "Bad", Some("nonsense"))
            .is_err()
    );

    let all: Vec<String> = lib.nodes().unwrap().into_iter().map(|n| n.name).collect();
    assert_eq!(all, ["Set 1", "Deep", "Archive", "Warmup"]);

    lib.delete_node(archive).unwrap();
    assert_eq!(lib.nodes().unwrap().len(), 2);
}

#[test]
fn added_files_get_new_entries() {
    let mut lib = imported();
    let id = lib
        .add_file(std::path::Path::new(
            "/Volumes/DJ/New/Some Artist - Fresh Cut.wav",
        ))
        .unwrap();
    assert_eq!(
        lib.add_file(std::path::Path::new(
            "/Volumes/DJ/New/Some Artist - Fresh Cut.wav"
        ))
        .unwrap(),
        id
    );
    let t = lib.track(id).unwrap().unwrap();
    assert_eq!(
        (t.artist.as_str(), t.title.as_str()),
        ("Some Artist", "Fresh Cut")
    );
    assert_eq!(lib.tracks().unwrap().len(), 4);
}

#[test]
#[ignore = "needs BACKSPN_NML pointing to a copy of a real collection.nml"]
fn real_collection_imports() {
    let path = std::env::var("BACKSPN_NML").expect("BACKSPN_NML not set");
    let nml = backspn_formats::nml::Nml::from_path(&path).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    let report = lib.import_nml(&path).unwrap();
    eprintln!("{report:?}");
    assert_eq!(lib.tracks().unwrap().len(), nml.entries.len());
}

#[test]
fn import_keeps_the_extra_metadata() {
    let tracks = imported().tracks().unwrap();
    let first = &tracks[0];
    assert_eq!(first.bitrate, Some(1_148_000));
    assert_eq!(first.track_number, Some(8));
    assert_eq!(first.filesize_kib, Some(54_056));
    assert_eq!(first.release_date, "2014/5/16");
    assert_eq!(first.last_played, "2026/1/2");
    assert_eq!(tracks[1].track_number, None);
    assert_eq!(tracks[2].bitrate, None);
}

#[test]
fn older_databases_gain_the_extra_columns_filled_from_nml() {
    let dir = std::env::temp_dir().join(format!("backspn-migrate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("old.sqlite");
    let _ = std::fs::remove_file(&path);
    {
        let mut lib = Library::open(&path).unwrap();
        lib.import_nml(fixture()).unwrap();
        for (name, _) in ADDED_COLUMNS {
            lib.conn
                .execute_batch(&format!("ALTER TABLE tracks DROP COLUMN {name}"))
                .unwrap();
        }
    }
    let lib = Library::open(&path).unwrap();
    let tracks = lib.tracks().unwrap();
    assert_eq!(tracks[0].bitrate, Some(1_148_000));
    assert_eq!(tracks[0].last_played, "2026/1/2");
    drop(lib);
    std::fs::remove_dir_all(&dir).unwrap();
}

fn tagged_wav(name: &str) -> PathBuf {
    use lofty::config::WriteOptions;
    use lofty::tag::{Accessor, ItemKey, Tag, TagExt, TagType};
    let path = std::env::temp_dir().join(format!("backspn-tags-{name}-{}.wav", std::process::id()));
    let (rate, channels, bits) = (44_100u32, 2u16, 16u16);
    let data_len = rate * u32::from(channels) * u32::from(bits / 8);
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * u32::from(channels) * u32::from(bits / 8)).to_le_bytes());
    wav.extend_from_slice(&(channels * bits / 8).to_le_bytes());
    wav.extend_from_slice(&bits.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.resize(wav.len() + data_len as usize, 0);
    std::fs::write(&path, wav).unwrap();
    let mut tag = Tag::new(TagType::Id3v2);
    tag.set_title("Flow With Me".into());
    tag.set_artist("Ann".into());
    tag.set_genre("Techno".into());
    tag.insert_text(ItemKey::Publisher, "Set About".into());
    tag.insert_text(ItemKey::InitialKey, "Am".into());
    tag.insert_text(ItemKey::IntegerBpm, "137".into());
    tag.save_to_path(&path, WriteOptions::default()).unwrap();
    path
}

#[test]
fn added_files_take_their_tags() {
    let path = tagged_wav("add");
    let mut lib = Library::open_in_memory().unwrap();
    let id = lib.add_file(&path).unwrap();
    let t = lib.track(id).unwrap().unwrap();
    assert_eq!(
        (t.title.as_str(), t.artist.as_str()),
        ("Flow With Me", "Ann")
    );
    assert_eq!(
        (t.genre.as_str(), t.label.as_str()),
        ("Techno", "Set About")
    );
    assert_eq!((t.key_text.as_str(), t.musical_key), ("1m", Some(21)));
    assert_eq!(t.bpm, Some(137.0));
    assert!(t.duration_s.is_some_and(|d| (d - 1.0).abs() < 0.01));
    assert_eq!(t.grid_anchor_s, None, "a tag tempo gives no grid");
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn written_tags_read_back_and_keep_the_rest_of_the_file() {
    use backspn_formats::nml::KeyNotation;
    use lofty::config::WriteOptions;
    use lofty::id3::v2::{Frame, FrameId, Id3v2Tag, PopularimeterFrame, PrivateFrame};
    use lofty::tag::{ItemKey, TagExt};
    let path = tagged_wav("write");
    let mut other = Id3v2Tag::default();
    other.insert(Frame::Private(PrivateFrame::new(
        "other.example",
        vec![1u8, 2, 3],
    )));
    other.insert(Frame::Popularimeter(PopularimeterFrame::new(
        "other@example.com",
        0,
        12,
    )));
    other.save_to_path(&path, WriteOptions::default()).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    let id = lib.add_file(&path).unwrap();
    lib.set_text(id, TextField::Title, "New Title").unwrap();
    lib.set_text(id, TextField::Artist, "Bob").unwrap();
    lib.set_text(id, TextField::Label, "Label X").unwrap();
    lib.set_text(id, TextField::Remixer, "Cid").unwrap();
    lib.set_text(id, TextField::ReleaseDate, "2024/10/4")
        .unwrap();
    lib.set_key(id, Some(21)).unwrap();
    lib.set_track_number(id, Some(7)).unwrap();
    lib.set_rating(id, 204).unwrap();
    lib.conn
        .execute("UPDATE tracks SET bpm = 127.6 WHERE id = ?1", [id])
        .unwrap();
    lib.write_tags(id, KeyNotation::Camelot).unwrap();
    let stamped = lib.track(id).unwrap().unwrap();
    assert_eq!(
        stamped.modified_date,
        clock::LocalTime::now().date_text(),
        "the file's modification date"
    );
    assert!(stamped.modified_time.is_some());
    let t = tags::read(&path).unwrap();
    assert_eq!((t.title.as_str(), t.artist.as_str()), ("New Title", "Bob"));
    assert_eq!((t.label.as_str(), t.remixer.as_str()), ("Label X", "Cid"));
    assert_eq!(t.release_date, "2024/10/4");
    assert_eq!(t.track_number, Some(7));
    assert_eq!(t.bpm, Some(128.0), "whole BPM");
    assert_eq!(t.musical_key, Some(21));
    assert_eq!(t.rating, 204, "ours is the first rating, read back exactly");
    let id3 = |path: &Path| {
        let mut file = std::fs::File::open(path).unwrap();
        let wav = <lofty::iff::wav::WavFile as lofty::file::AudioFile>::read_from(
            &mut file,
            lofty::config::ParseOptions::new(),
        )
        .unwrap();
        wav.id3v2().unwrap().clone()
    };
    let tag = id3(&path);
    let text = |id: &'static str| {
        tag.get_text(&FrameId::Valid(std::borrow::Cow::Borrowed(id)))
            .map(str::to_owned)
    };
    assert_eq!(text("TKEY").as_deref(), Some("8A"), "Camelot");
    assert_eq!(text("TBPM").as_deref(), Some("128"));
    let ratings: Vec<(String, u8, u64)> = tag
        .into_iter()
        .filter_map(|f| match f {
            Frame::Popularimeter(p) => Some((p.email.to_string(), p.rating, p.counter)),
            _ => None,
        })
        .collect();
    assert_eq!(
        ratings,
        [
            (tags::RATING_EMAIL.to_owned(), 204, 0),
            ("other@example.com".to_owned(), 0, 12)
        ]
    );
    let tag = id3(&path);
    assert!(
        tag.into_iter()
            .any(|f| matches!(f, Frame::Private(p) if p.owner == "other.example" && *p.private_data == [1, 2, 3])),
        "private frames kept"
    );
    lib.set_rating(id, 0).unwrap();
    lib.write_tags(id, KeyNotation::OpenKey).unwrap();
    let tag = id3(&path);
    let popms = tag
        .into_iter()
        .filter(|f| matches!(f, Frame::Popularimeter(_)))
        .count();
    assert_eq!(popms, 2);
    assert_eq!(tags::read(&path).unwrap().rating, 0);
    let file = lofty::read_from_path(&path).unwrap();
    let generic = lofty::file::TaggedFileExt::primary_tag(&file).unwrap();
    assert_eq!(generic.get_string(ItemKey::InitialKey), Some("1m"));
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn writing_tags_of_a_missing_file_fails() {
    let mut lib = Library::open_in_memory().unwrap();
    let id = lib.add_file(Path::new("/nonexistent/Ann - X.mp3")).unwrap();
    let err = lib
        .write_tags(id, backspn_formats::nml::KeyNotation::OpenKey)
        .unwrap_err();
    assert!(err.to_string().contains("not taggable"), "{err}");
}

#[test]
fn edited_extra_fields_are_stored() {
    let mut lib = imported();
    let id = lib.tracks().unwrap()[0].id;
    lib.set_text(id, TextField::Remixer, "Ann").unwrap();
    lib.set_text(id, TextField::Comment2, "warm-up").unwrap();
    lib.set_text(id, TextField::ReleaseDate, "2024/10/4")
        .unwrap();
    lib.set_key(id, Some(21)).unwrap();
    lib.set_track_number(id, Some(7)).unwrap();
    lib.set_color(&[id], 4).unwrap();
    let t = lib.track(id).unwrap().unwrap();
    assert_eq!(
        (t.remixer.as_str(), t.comment2.as_str()),
        ("Ann", "warm-up")
    );
    assert_eq!(t.release_date, "2024/10/4");
    assert_eq!((t.musical_key, t.key_text.as_str()), (Some(21), "1m"));
    assert_eq!(t.track_number, Some(7));
    assert_eq!(t.color, 4);
}

#[test]
fn overviews_are_stored() {
    let mut lib = Library::open_in_memory().unwrap();
    let id = lib.add_file(std::path::Path::new("/m/A.mp3")).unwrap();
    assert_eq!(lib.track(id).unwrap().unwrap().overview, Vec::<u8>::new());
    lib.save_overview(id, &[1, 2, 3, 4]).unwrap();
    assert_eq!(lib.track(id).unwrap().unwrap().overview, [1, 2, 3, 4]);
}

#[test]
fn list_sorting_lives_in_the_collection() {
    let mut lib = imported();
    assert_eq!(lib.sorting("$COLLECTION").unwrap(), []);
    lib.set_sorting("$COLLECTION", &[(20, false), (2, true)])
        .unwrap();
    lib.set_sorting("Sets\\Friday", &[(4, true)]).unwrap();
    assert_eq!(
        lib.sorting("$COLLECTION").unwrap(),
        [(20, false), (2, true)]
    );
    assert_eq!(lib.sorting("Sets\\Friday").unwrap(), [(4, true)]);
    lib.set_sorting("$COLLECTION", &[]).unwrap();
    assert_eq!(lib.sorting("$COLLECTION").unwrap(), []);
    let mut fresh = Library::open_in_memory().unwrap();
    fresh.set_sorting("$COLLECTION", &[(2, false)]).unwrap();
    assert_eq!(fresh.sorting("$COLLECTION").unwrap(), [(2, false)]);
}

#[test]
#[ignore = "needs BACKSPN_HISTORY"]
fn real_history_imports() {
    let Some(path) = std::env::var_os("BACKSPN_HISTORY") else {
        return;
    };
    let mut lib = Library::open_in_memory().unwrap();
    let (session, count) = lib.import_history(std::path::Path::new(&path)).unwrap();
    assert!(count > 0);
    let entries = lib.history_entries(session).unwrap();
    assert_eq!(entries.len(), count);
    let known = entries.iter().filter(|e| e.track_id.is_some()).count();
    eprintln!(
        "{count} entries, {known} with a local track; first {:?}",
        entries[0]
    );
    assert!(entries[0].start_date > 0);
}

#[test]
fn covers_are_chosen_or_removed_per_track() {
    let mut lib = imported();
    let ids: Vec<i64> = lib.tracks().unwrap().iter().map(|t| t.id).collect();
    assert_eq!(
        lib.track(ids[0]).unwrap().unwrap().cover,
        None,
        "the file's"
    );
    lib.set_cover(&ids[..2], Some("abc.png")).unwrap();
    lib.set_cover(&ids[1..2], Some("")).unwrap();
    let covers: Vec<Option<String>> = lib.tracks().unwrap().into_iter().map(|t| t.cover).collect();
    assert_eq!(covers, [Some("abc.png".into()), Some(String::new()), None]);
}

#[test]
fn preferring_file_tags_touches_only_the_new_tracks() {
    let old = tagged_wav("prefer-old");
    let new = tagged_wav("prefer-new");
    let mut lib = Library::open_in_memory().unwrap();
    let a = lib.add_file(&old).unwrap();
    lib.set_text(a, TextField::Title, "Kept").unwrap();
    let known: std::collections::HashSet<i64> = [a].into();
    let b = lib.add_file(&new).unwrap();
    lib.set_text(b, TextField::Title, "From the NML").unwrap();
    let missing = lib.add_file(Path::new("/nonexistent/x.mp3")).unwrap();
    assert_eq!(
        lib.prefer_file_tags(&known).unwrap(),
        1,
        "one file unreadable"
    );
    let title = |lib: &Library, id| lib.track(id).unwrap().unwrap().title;
    assert_eq!(title(&lib, a), "Kept");
    assert_eq!(title(&lib, b), "Flow With Me");
    assert_eq!(title(&lib, missing), "x");
    std::fs::remove_file(&old).unwrap();
    std::fs::remove_file(&new).unwrap();
}

#[test]
fn imported_tracks_keep_their_audio_id() {
    let mut lib = imported();
    assert_eq!(
        lib.audio_id(1).unwrap().as_deref(),
        Some("AXh2fXyYlt/s7+7/muvcrZuc6w==")
    );
    let dir = std::env::temp_dir().join(format!("backspn-audioid-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("new.wav");
    std::fs::write(&file, b"RIFF").unwrap();
    let added = lib.add_file(&file).unwrap();
    assert_eq!(lib.audio_id(added).unwrap(), None);
    assert_eq!(lib.audio_id(999_999).unwrap(), None);
    assert_eq!(lib.stem_hash(added).unwrap(), None);
    lib.set_stem_hash(added, Some([1, 2, 3, 4])).unwrap();
    assert_eq!(lib.stem_hash(added).unwrap(), Some([1, 2, 3, 4]));
    lib.set_stem_hash(added, None).unwrap();
    assert_eq!(lib.stem_hash(added).unwrap(), None);
    let _ = std::fs::remove_dir_all(dir);
}

fn with_set(tag: &str) -> PathBuf {
    let original = std::fs::read_to_string(fixture()).unwrap();
    let set = r#"<SETS ENTRIES="1"><SET TITLE="Drums" QUANT_VAlUE="1" QUANT_STATE="1"><LOCATION DIR="/:Users/:x/:" FILE="2026y04m25d_14h26m56s1.set" VOLUME="Macintosh HD" VOLUMEID="Macintosh HD"></LOCATION>
<MODIFICATION_INFO AUTHOR_TYPE="importer"></MODIFICATION_INFO>
<INFO IMPORT_DATE="2026/4/25"></INFO>
<TEMPO BPM="136.000015"></TEMPO>
<SLOT KEYLOCK="1" FXENABLE="1" PUNCHMODE="0" ACTIVE_CELL_INDEX="0"><CELL INDEX="0" CELLNAME="First Light" COLOR="10" SYNC="1" REVERSE="0" MODE="1" TYPE="0" SPEED="1.000000" TRANSPOSE="0.000000" OFFSET="0.000000" NUDGE="0.000000" GAIN="0.500000" START_MARKER="0.000000" END_MARKER="0.000000" BPM="136.000015" DIR="/:Music/:Techno/:" FILE="Test Artist - First Light.flac" VOLUME="DJ"></CELL>
</SLOT>
<SLOT KEYLOCK="1" FXENABLE="1" PUNCHMODE="0" ACTIVE_CELL_INDEX="0"></SLOT>
<SLOT KEYLOCK="1" FXENABLE="1" PUNCHMODE="0" ACTIVE_CELL_INDEX="0"></SLOT>
<SLOT KEYLOCK="1" FXENABLE="1" PUNCHMODE="0" ACTIVE_CELL_INDEX="0"></SLOT>
</SET>
</SETS>
"#;
    let text = original.replace("<SETS ENTRIES=\"0\"></SETS>\n", set);
    let path = std::env::temp_dir().join(format!("backspn-set-{tag}-{}.nml", std::process::id()));
    std::fs::write(&path, &text).unwrap();
    path
}

#[test]
fn sample_sets_import_linked_to_their_tracks() {
    let path = with_set("linked");
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_nml(&path).unwrap();
    let sets = lib.sample_sets().unwrap();
    assert_eq!(sets.len(), 1);
    let set = &sets[0];
    assert_eq!(set.set.title, "Drums");
    assert_eq!(set.track_of(0, 0), Some(1), "the cell is the first track");
    assert_eq!(set.set.slots[0].cells[0].name, "First Light");
    let _ = std::fs::remove_file(path);
}

#[test]
fn saved_sets_are_stored_from_their_rows() {
    let path = with_set("saved");
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_nml(&path).unwrap();
    let _ = std::fs::remove_file(path);
    let mut set = backspn_formats::nml::SampleSet {
        title: "Tools".to_owned(),
        bpm: Some(128.0),
        ..backspn_formats::nml::SampleSet::default()
    };
    let mut cell = backspn_formats::nml::SetCell::new(2, backspn_formats::nml::Location::default());
    cell.name = "Locked".to_owned();
    cell.play_type = 1;
    set.slots[1].cells.push(cell);
    let mut tracks = vec![None; 64];
    tracks[16 + 2] = Some(2);
    let id = lib.save_sample_set(None, &set, &tracks).unwrap();
    assert_eq!(lib.sample_set_by_title("Tools").unwrap(), Some(id));
    let saved = lib.sample_set(id).unwrap().unwrap();
    assert_eq!(saved.set.title, "Tools");
    assert_eq!(saved.set.slots.len(), 4);
    let cell = &saved.set.slots[1].cells[0];
    assert_eq!((cell.index, cell.play_type), (2, 1));
    assert_eq!(saved.track_of(1, 2), Some(2), "linked to its track");
    assert_eq!(lib.sample_sets().unwrap().len(), 2);
    set.title = "Tools 2".to_owned();
    let id2 = lib.save_sample_set(Some(id), &set, &tracks).unwrap();
    assert_eq!(lib.sample_sets().unwrap().len(), 2);
    assert_eq!(lib.sample_set(id2).unwrap().unwrap().set.title, "Tools 2");
    lib.delete_sample_set(id2).unwrap();
    assert_eq!(lib.sample_sets().unwrap().len(), 1);
}

#[test]
fn sample_type_is_stored() {
    let mut lib = imported();
    assert_eq!(lib.track(1).unwrap().unwrap().sample_type, None);
    lib.set_sample_type(1, Some(1)).unwrap();
    assert_eq!(lib.track(1).unwrap().unwrap().sample_type, Some(1));
}

#[test]
fn a_rescan_takes_the_files_tags_again_and_its_stamp() {
    let path = tagged_wav("rescan");
    let mut lib = Library::open_in_memory().unwrap();
    let id = lib.add_file(&path).unwrap();
    let mtime = scan::file_mtime(&path);
    assert_eq!(lib.scan_stamp(id).unwrap(), mtime, "stamped when added");
    lib.set_text(id, TextField::Title, "Edited").unwrap();
    lib.conn
        .execute("UPDATE tracks SET bpm = 136.5 WHERE id = ?1", [id])
        .unwrap();
    let mut file = scan::read(&path).unwrap();
    file.mtime = Some(42);
    lib.store_scan(id, &file).unwrap();
    let t = lib.track(id).unwrap().unwrap();
    assert_eq!(t.title, "Flow With Me");
    assert_eq!(t.bpm, Some(136.5));
    assert_eq!(lib.scan_stamp(id).unwrap(), Some(42));
    assert!(!scan::changed(lib.scan_stamp(id).unwrap(), Some(42)));
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn a_track_without_a_tempo_takes_the_tags() {
    let path = tagged_wav("rescan-bpm");
    let mut lib = Library::open_in_memory().unwrap();
    let id = lib.add_file(&path).unwrap();
    lib.conn
        .execute("UPDATE tracks SET bpm = NULL WHERE id = ?1", [id])
        .unwrap();
    lib.store_scan(id, &scan::read(&path).unwrap()).unwrap();
    assert_eq!(lib.track(id).unwrap().unwrap().bpm, Some(137.0));
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn adding_the_scan_column_keeps_edits() {
    let dir = std::env::temp_dir().join(format!("backspn-scan-col-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("old.sqlite");
    let _ = std::fs::remove_file(&path);
    {
        let mut lib = Library::open(&path).unwrap();
        lib.import_nml(fixture()).unwrap();
        let id = lib.tracks().unwrap()[0].id;
        lib.set_text(id, TextField::Remixer, "Edited").unwrap();
        lib.conn
            .execute_batch("ALTER TABLE tracks DROP COLUMN scanned_mtime")
            .unwrap();
    }
    let lib = Library::open(&path).unwrap();
    assert_eq!(lib.tracks().unwrap()[0].remixer, "Edited");
    drop(lib);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stats_count_without_names() {
    let mut lib = Library::open_in_memory().unwrap();
    lib.add_read_files(vec![
        (PathBuf::from("/music/A.FLAC"), FileFacts::default()),
        (PathBuf::from("/music/B.mp3"), FileFacts::default()),
        (PathBuf::from("/music/C.mp3"), FileFacts::default()),
    ])
    .unwrap();
    let stats = lib.stats().unwrap();
    assert_eq!((stats.tracks, stats.analyzed), (3, 0));
    assert_eq!(stats.extensions.get("flac"), Some(&1));
    assert_eq!(stats.extensions.get("mp3"), Some(&2));
}
