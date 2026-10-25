use std::path::PathBuf;

use backspn_formats::nml::{CueKind, Nml, Node};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/nml")
        .join(name)
}

#[test]
fn reads_fixture_collection() {
    let nml = Nml::from_path(fixture("small.nml")).unwrap();
    assert_eq!(nml.version, "20");
    assert_eq!(nml.program.as_deref(), Some("Fixture Writer"));
    assert_eq!(nml.entries.len(), 3);

    let first = &nml.entries[0];
    assert_eq!(first.title.as_deref(), Some("First Light (Club Mix)"));
    assert_eq!(first.artist.as_deref(), Some("Test Artist & Friend"));
    assert_eq!(first.album.as_ref().unwrap().track, Some(8));
    assert_eq!(first.info.ranking, Some(204));
    assert_eq!(first.info.filesize_kib, Some(54056));
    assert_eq!(first.tempo.unwrap().bpm, Some(136.000_015));
    assert_eq!(first.loudness.unwrap().perceived_db, Some(-1.364_929));
    let key = first.musical_key.unwrap();
    assert_eq!(
        (key.open_key(), key.to_string()),
        ("6m".into(), "G#m".into())
    );
    assert_eq!(first.info.key_text.as_deref(), Some("6m"));

    assert_eq!(first.cues.len(), 3);
    let grid = first.grid_cue().unwrap();
    assert_eq!(grid.name, "AutoGrid");
    assert_eq!(grid.grid_bpm, Some(136.000_015));
    assert_eq!(grid.hotcue, None);
    let drop = &first.cues[1];
    assert_eq!((drop.kind, drop.hotcue), (CueKind::Cue, Some(0)));
    assert_eq!(drop.color.as_deref(), Some("#FF0000"));
    let lp = &first.cues[2];
    assert_eq!(lp.kind, CueKind::Loop);
    assert!((lp.len_ms - 7_058.823_529).abs() < 1e-9);

    let locked = &nml.entries[1];
    assert!(locked.locked);
    assert_eq!(locked.location.volume, "Macintosh HD");
    assert_eq!(
        locked.location.relative_path(),
        "Users/someone/Music/locked.mp3"
    );

    let raw = &nml.entries[2];
    assert!(!raw.locked);
    assert_eq!(raw.tempo.unwrap().bpm, None);
    assert!(raw.musical_key.is_none() && raw.cues.is_empty() && raw.album.is_none());
}

#[test]
fn reads_fixture_playlists() {
    let nml = Nml::from_path(fixture("small.nml")).unwrap();
    let root = nml.playlists.as_ref().unwrap();
    assert_eq!(root.name(), "$ROOT");
    let names: Vec<_> = root.walk().iter().map(|n| n.name()).collect();
    assert_eq!(names, ["$ROOT", "Set 1", "Deep", "Archive"]);

    let Node::Folder { children, .. } = root else {
        panic!("root is not a folder")
    };
    let Node::Playlist { entries, uuid, .. } = &children[0] else {
        panic!("not a playlist")
    };
    assert_eq!(uuid.as_deref(), Some("0123456789abcdef0123456789abcdef"));
    let titles: Vec<_> = entries
        .iter()
        .map(|k| nml.entry_by_key(k).unwrap().title.as_deref().unwrap())
        .collect();
    assert_eq!(titles, ["Locked Groove", "First Light (Club Mix)"]);

    let Node::Smartlist { query, .. } = &children[1] else {
        panic!("not a smartlist")
    };
    assert_eq!(
        query.as_deref(),
        Some(r#"$COMMENT <> "" & $LABEL % "Fixture""#)
    );
}

#[test]
fn keeps_unmodelled_sections_in_document() {
    let nml = Nml::from_path(fixture("small.nml")).unwrap();
    let indexing = nml.document.root.child("INDEXING").unwrap();
    assert_eq!(indexing.children[0].attr("PATH"), Some("Set 1"));
    assert!(nml.document.root.child("SETS").is_some());
}

#[test]
fn rejects_non_nml_root() {
    assert!("<FOO VERSION=\"1\"></FOO>".parse::<Nml>().is_err());
    assert!("<NML></NML>".parse::<Nml>().is_err());
}

#[test]
#[ignore = "needs BACKSPN_NML pointing to a copy of a real collection.nml"]
fn reads_real_collection() {
    let path = std::env::var("BACKSPN_NML").expect("BACKSPN_NML not set");
    let nml = Nml::from_path(&path).unwrap();

    let declared: usize = nml
        .document
        .root
        .child("COLLECTION")
        .and_then(|c| c.attr("ENTRIES"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        nml.entries.len(),
        declared,
        "ENTRIES attribute vs parsed entries"
    );

    let mut key_checked = 0;
    for entry in &nml.entries {
        if let (Some(key), Some(text)) = (entry.musical_key, entry.info.key_text.as_deref()) {
            assert_eq!(key.open_key(), text, "key mismatch in {:?}", entry.title);
            key_checked += 1;
        }
        let grid_bpm = entry.grid_cue().and_then(|g| g.grid_bpm);
        if let (Some(grid_bpm), Some(bpm)) = (grid_bpm, entry.tempo.and_then(|t| t.bpm)) {
            assert!(
                (grid_bpm - bpm).abs() < 0.01,
                "grid/tempo BPM differ in {:?}",
                entry.title
            );
        }
    }

    let mut missing = Vec::new();
    let mut resolved = 0;
    if let Some(root) = &nml.playlists {
        for node in root.walk() {
            if let Node::Playlist { entries, .. } = node {
                for key in entries {
                    match nml.entry_by_key(key) {
                        Some(_) => resolved += 1,
                        None => missing.push(key.clone()),
                    }
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "playlist keys without entry: {missing:?}"
    );
    eprintln!(
        "{} entries, {key_checked} keys checked, {resolved} playlist keys resolved",
        nml.entries.len()
    );
}

#[test]
#[ignore = "needs BACKSPN_TSI pointing to a copy of a real .tsi file"]
fn reads_real_settings_mappings() {
    let path = std::env::var("BACKSPN_TSI").expect("BACKSPN_TSI not set");
    let tsi = backspn_formats::tsi::Tsi::from_path(path).unwrap();
    assert_ne!(tsi.devices.len(), 0, "no mapped devices");
    for d in &tsi.devices {
        eprintln!("{} / {:?}: {} mappings", d.entry, d.name, d.mappings.len());
        assert!(
            d.mappings.iter().all(|m| !m.control.is_empty()),
            "unbound mapping in {}",
            d.name
        );
    }
}
