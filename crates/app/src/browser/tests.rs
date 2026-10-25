use std::path::Path;

use super::files::{is_inside_other_app_data, remove_files, subfolders};
use super::table::Pending;
use super::toolbar::list_statistics;
use super::*;

fn browser(files: &[&str]) -> Browser {
    let mut b = Browser::with_library(Library::open_in_memory().map_err(|e| e.to_string()));
    if !files.is_empty() {
        let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        assert_eq!(b.add_files(&paths).len(), files.len());
    }
    b
}

fn titles(b: &Browser) -> Vec<String> {
    b.view
        .iter()
        .map(|&id| b.row(id).unwrap().title.clone())
        .collect()
}

#[test]
fn tree_encoder_steps_through_collection_playlists_and_folders() {
    let mut b = browser(&["/m/A.mp3"]);
    let lib = b.library().unwrap();
    let folder = lib
        .create_node(None, NodeKind::Folder, "Sets", None)
        .unwrap();
    let inner = lib
        .create_node(Some(folder), NodeKind::Playlist, "Inner", None)
        .unwrap();
    let top = lib
        .create_node(None, NodeKind::Playlist, "Top", None)
        .unwrap();
    b.refresh();
    b.folders = vec![PathBuf::from("/m")];
    assert_eq!(
        b.sources(),
        [
            Source::Collection,
            Source::Node(inner),
            Source::Node(top),
            Source::Folder(PathBuf::from("/m")),
            Source::Recordings,
            Source::Loops
        ]
    );
    b.move_source(1);
    assert_eq!(b.source, Source::Node(inner));
    b.move_source(6);
    assert_eq!(b.source, Source::Loops, "clamped at the end");
    b.move_source(-9);
    assert_eq!(b.source, Source::Collection);
}

#[test]
fn hidden_nodes_leave_the_tree_and_the_encoder() {
    let mut b = browser(&["/m/A.mp3"]);
    b.show(Source::Loops);
    b.set_hidden_nodes(&[TreeNode::Loops, TreeNode::Music]);
    assert_eq!(b.source, Source::Collection, "the hidden list gives way");
    assert!(!b.shown(TreeNode::Loops) && b.shown(TreeNode::Recordings));
    assert_eq!(b.sources(), [Source::Collection, Source::Recordings]);
    b.show(Source::Recordings);
    b.set_hidden_nodes(&[TreeNode::Music]);
    assert_eq!(b.source, Source::Recordings, "a shown one stays");
    assert_eq!(TreeNode::of(&Source::Set(3)), Some(TreeNode::SampleSets));
    assert_eq!(
        TreeNode::of(&Source::External(Outside::Program(0), Vec::new())),
        Some(TreeNode::DjSoftware)
    );
    assert_eq!(TreeNode::of(&Source::Collection), None);
    assert_eq!(TreeNode::of(&Source::Node(1)), None);
}

#[test]
fn hiding_archive_leaves_the_current_history() {
    let mut b = browser(&["/m/A.mp3"]);
    b.current_history = Some(2);
    b.show(Source::History(2));
    b.set_hidden_nodes(&[TreeNode::Archive]);
    assert_eq!(b.source, Source::History(2), "the current session stays");
    b.set_hidden_nodes(&[]);
    b.show(Source::History(1));
    b.set_hidden_nodes(&[TreeNode::Archive]);
    assert_eq!(b.source, Source::Collection, "an archived one gives way");
}

#[test]
fn preparation_list_toggles_the_selected_track() {
    let mut b = browser(&["/m/A.mp3", "/m/B.mp3"]);
    b.move_selection(1);
    let track = b.selected().unwrap();
    b.toggle_preparation();
    let prep = b
        .nodes
        .iter()
        .find(|n| n.name == PREPARATION)
        .map(|n| n.id)
        .expect("created on first use");
    assert_eq!(b.library().unwrap().node_tracks(prep).unwrap(), [track]);
    b.toggle_preparation();
    assert_eq!(
        b.library().unwrap().node_tracks(prep).unwrap(),
        Vec::<i64>::new()
    );
    b.show_preparation();
    assert_eq!(b.source, Source::Node(prep));
}

#[test]
fn adds_files_once_and_lists_them() {
    let mut b = browser(&["/m/Bob - Beta.mp3", "/m/Ann - Alpha.flac"]);
    assert_eq!(titles(&b), ["Beta", "Alpha"]);
    assert_eq!(b.row(b.view[0]).unwrap().artist, "Bob");
    let again = b.add_files(&[PathBuf::from("/m/Bob - Beta.mp3")]);
    assert_eq!(again, [b.view[0]], "same path, same id");
    assert_eq!(b.view.len(), 2);
}

#[test]
fn search_and_sort_shape_the_view() {
    let mut b = browser(&[
        "/m/Bob - Beta.mp3",
        "/m/Ann - Alpha.mp3",
        "/m/Cid - Gamma.mp3",
    ]);
    b.sort = vec![(BrowserColumn::Title, true)];
    b.recompute();
    assert_eq!(titles(&b), ["Alpha", "Beta", "Gamma"]);
    b.sort = vec![(BrowserColumn::Artist, false)];
    b.recompute();
    assert_eq!(titles(&b), ["Gamma", "Beta", "Alpha"]);
    b.search = "a".into();
    b.recompute();
    assert_eq!(b.view.len(), 3);
    b.search = "bob beta".into();
    b.recompute();
    assert_eq!(titles(&b), ["Beta"]);
}

#[test]
fn home_and_end_jump_to_the_ends() {
    let mut b = browser(&["/m/A.mp3", "/m/B.mp3", "/m/C.mp3"]);
    b.move_selection(i32::MAX);
    assert_eq!(b.selected(), Some(b.view[2]), "End with nothing selected");
    b.move_selection(i32::MIN);
    assert_eq!(b.selected(), Some(b.view[0]));
    b.move_selection(i32::MAX);
    assert_eq!(b.selected(), Some(b.view[2]));
}

#[test]
fn selection_moves_and_clamps() {
    let mut b = browser(&["/m/A.mp3", "/m/B.mp3", "/m/C.mp3"]);
    assert_eq!(b.selected(), None);
    b.move_selection(1);
    assert_eq!(b.selected(), Some(b.view[0]), "first move selects the top");
    b.move_selection(5);
    assert_eq!(b.selected(), Some(b.view[2]), "clamped at the bottom");
    b.move_selection(-1);
    assert_eq!(b.selected(), Some(b.view[1]), "negative moves up");
    b.move_selection(-9);
    assert_eq!(b.selected(), Some(b.view[0]));
}

fn music_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("backspn-browser-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(dir.join("Sets")).unwrap();
    std::fs::create_dir_all(dir.join(".cache")).unwrap();
    for f in [
        "Bob - Beta.mp3",
        "alpha.flac",
        "notes.txt",
        ".hidden.mp3",
        "Sets/x.wav",
    ] {
        std::fs::write(dir.join(f), b"").unwrap();
    }
    dir
}

#[test]
fn folders_list_audio_files_without_importing() {
    let dir = music_dir("list");
    let mut b = browser(&[]);
    assert!(b.add_folder(dir.clone()).is_some());
    assert!(b.add_folder(dir.clone()).is_none(), "listed once");
    assert_eq!(titles(&b), ["alpha", "Beta"]);
    assert!(b.view.iter().all(|&id| id < 0), "not in the library yet");
    assert_eq!(b.rows.len(), 0);
    assert_eq!(b.row(b.view[1]).unwrap().artist, "Bob");
    assert_eq!(subfolders(&dir), [dir.join("Sets")]);
    b.search = "bob".into();
    b.recompute();
    assert_eq!(titles(&b), ["Beta"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn music_folders_import_their_new_files() {
    let dir = music_dir("startup");
    let mut b = browser(&[]);
    b.folders = vec![dir.clone()];
    b.import_music_folders();
    for _ in 0..500 {
        b.continue_folder_import();
        if b.folder_import.is_none() && !b.rows.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(b.rows.len(), 3);
    b.import_music_folders();
    for _ in 0..200 {
        b.continue_folder_import();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(b.rows.len(), 3, "known files are not added again");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn loading_a_folder_file_imports_it() {
    let dir = music_dir("import");
    let mut b = browser(&[]);
    b.add_folder(dir.clone());
    let external = b.view[1];
    b.selection.select(1);
    let id = b.resolve(external).unwrap();
    assert!(id > 0);
    assert_eq!(b.selected(), Some(id), "selection follows the import");
    assert_eq!(b.rows.len(), 1);
    assert!(b.view.contains(&id), "the folder now shows the library row");
    assert_eq!(b.resolve(id), Some(id));
    b.set_folders(Vec::new());
    assert_eq!(
        b.source,
        Source::Collection,
        "removing the folder leaves it"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn history_lists_the_session_in_play_order() {
    let mut b = browser(&["/m/A.mp3", "/m/B.mp3"]);
    let (a, bb) = (b.view[0], b.view[1]);
    let now = backspn_library::clock::LocalTime::now();
    let lib = b.library().unwrap();
    let old = lib.start_history_session(now).unwrap();
    lib.add_history_entry(old, a, 0, now).unwrap();
    let session = lib.start_history_session(now).unwrap();
    lib.add_history_entry(session, bb, 1, now).unwrap();
    lib.add_history_entry(session, a, 0, now).unwrap();
    b.history_changed(session);
    assert_eq!(b.histories.len(), 2);
    b.show(Source::History(session));
    assert_eq!(b.view, [bb, a]);
    b.selection.select(0);
    b.apply(Pending::RemoveFromHistory(session));
    assert_eq!(b.view, [a], "deleting from the history keeps the track");
    assert_eq!(b.rows.len(), 2);
    b.library().unwrap().delete_history_session(old).unwrap();
    b.show(Source::History(old));
    b.refresh();
    assert_eq!(b.source, Source::Collection, "a deleted archive is left");
}

#[test]
fn playlist_edits_follow_the_rows_shown() {
    let mut b = browser(&["/m/Cid - C.mp3", "/m/Ann - A.mp3", "/m/Bob - B.mp3"]);
    let ids = b.view.clone();
    let lib = b.library().unwrap();
    let list = lib
        .create_node(None, NodeKind::Playlist, "Set", None)
        .unwrap();
    lib.add_to_playlist(list, &ids).unwrap();
    b.refresh();
    b.show(Source::Node(list));
    b.sort = vec![(BrowserColumn::Title, true)];
    b.recompute();
    assert_eq!(titles(&b), ["A", "B", "C"]);
    b.selection.select(0);
    b.apply(Pending::RemoveFrom(list));
    assert_eq!(titles(&b), ["B", "C"]);
    b.apply(Pending::Consolidate(list));
    b.sort.clear();
    b.recompute();
    assert_eq!(titles(&b), ["B", "C"], "consolidated in the order shown");
    b.apply(Pending::MoveEntries(list, vec![1], 0));
    assert_eq!(titles(&b), ["C", "B"]);
    b.apply(Pending::Click(0, false, false));
    b.apply(Pending::Click(1, true, false));
    assert_eq!(b.selected_rows().len(), 2);
    b.apply(Pending::Rate(204));
    assert!(b.view.iter().all(|&id| b.row(id).unwrap().rating == 204));
}

#[test]
fn jumping_to_a_track_shows_and_selects_it() {
    let mut b = browser(&["/m/A.mp3", "/m/B.mp3", "/m/C.mp3"]);
    let target = b.view[2];
    let list = b
        .library()
        .unwrap()
        .create_node(None, NodeKind::Playlist, "Set", None)
        .unwrap();
    b.refresh();
    b.show(Source::Node(list));
    b.search = "zzz".into();
    b.jump_to_track(target);
    assert_eq!(b.source, Source::Collection);
    assert_eq!(b.selected(), Some(target));
}

#[test]
fn each_list_remembers_its_sorting() {
    let mut b = browser(&["/m/Bob - B.mp3", "/m/Ann - A.mp3"]);
    let list = b
        .library()
        .unwrap()
        .create_node(None, NodeKind::Playlist, "Set", None)
        .unwrap();
    b.refresh();
    b.sort = vec![(BrowserColumn::Title, false)];
    b.remember_sorting();
    b.show(Source::Node(list));
    assert_eq!(b.sort, [], "the playlist has no sorting yet");
    b.show(Source::Collection);
    assert_eq!(b.sort, [(BrowserColumn::Title, false)]);
    assert_eq!(titles(&b), ["B", "A"]);
    assert_eq!(BrowserColumn::from_attribute(20), Some(BrowserColumn::Key));
}

#[test]
fn statistics_count_time_and_size() {
    let t = |d: f64, k: u64| TrackRow {
        duration_s: Some(d),
        filesize_kib: Some(k),
        ..TrackRow::default()
    };
    let (a, b) = (t(3000.0, 10_240), t(700.0, 20_480));
    assert_eq!(list_statistics(&[&a, &b]), "2 tracks · 1:01:40 · 30.0 MB");
    assert_eq!(list_statistics(&[]), "0 tracks · 0:00 · 0.0 MB");
}

#[test]
fn missing_library_is_harmless() {
    let mut b = Browser::with_library(Err("gone".into()));
    b.move_selection(1);
    assert_eq!(b.selected(), None);
    assert_eq!(b.add_files(&[PathBuf::from("/m/A.mp3")]), Vec::<i64>::new());
}

#[test]
fn protects_other_app_folder() {
    assert!(is_inside_other_app_data(Path::new(&format!(
        "/Users/x/{}/App 1.0/collection.nml",
        backspn_formats::interop::DATA_DIR
    ))));
    assert!(!is_inside_other_app_data(Path::new("/tmp/collection.nml")));
}

#[test]
fn removing_files_reports_what_could_not_be_deleted() {
    let dir = std::env::temp_dir().join(format!("backspn-remove-{}", std::process::id()));
    let other_app = dir.join(backspn_formats::interop::DATA_FOLDER);
    std::fs::create_dir_all(&other_app).unwrap();
    let gone = dir.join("a.mp3");
    let kept = other_app.join("b.mp3");
    std::fs::write(&gone, b"x").unwrap();
    std::fs::write(&kept, b"x").unwrap();
    let problems = remove_files(&[gone.clone(), kept.clone(), dir.join("missing.mp3")]);
    assert!(!gone.exists());
    assert!(kept.exists(), "never inside the other app's data folder");
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems[1].contains("could not be located"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn recordings_join_their_hidden_list_and_not_the_tree() {
    let mut b = browser(&["/m/A.mp3"]);
    b.add_recordings(&[PathBuf::from("/rec/Recording_2026-10-20_21h00m00s.wav")]);
    assert!(b.nodes.is_empty(), "the list is not a tree node");
    b.show(Source::Recordings);
    assert_eq!(titles(&b), ["Recording_2026-10-20_21h00m00s"]);
    assert_eq!(b.rows.len(), 2, "and the collection has it");
}

#[test]
fn saved_loops_join_the_loops_node_with_a_locked_beatgrid() {
    let mut b = browser(&["/m/A.mp3"]);
    let capture = b
        .add_sample(&PathBuf::from("/s/Kick 2026.wav"), 120.0, 2.0, None)
        .unwrap();
    let saved = b
        .add_sample(
            &PathBuf::from("/s/Loop Recorder.wav"),
            125.0,
            7.68,
            Some(0.0),
        )
        .unwrap();
    assert!(b.nodes.is_empty(), "the list is not a tree node");
    b.show(Source::Loops);
    assert_eq!(titles(&b), ["Kick 2026", "Loop Recorder"]);
    let row = b.library_track(saved).unwrap();
    assert_eq!((row.bpm, row.grid_anchor_s), (Some(125.0), Some(0.0)));
    assert!(row.locked, "analysis keeps the recorded tempo");
    let row = b.library_track(capture).unwrap();
    assert_eq!((row.grid_anchor_s, row.locked), (None, false));
    b.move_selection(1);
    b.apply(Pending::RemoveFrom(b.loops.unwrap()));
    assert_eq!(b.loop_tracks().len(), 1);
    assert_eq!(b.rows.len(), 3, "the collection keeps both");
}

#[test]
fn a_folder_becomes_a_playlist_of_its_own_files() {
    let dir = std::env::temp_dir().join(format!("backspn-folder-list-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Sub")).unwrap();
    for f in ["A.mp3", "B.wav", "notes.txt", "Sub/C.mp3"] {
        std::fs::write(dir.join(f), b"").unwrap();
    }
    let mut b = browser(&[]);
    b.import_folder_as_playlist(&dir, "Gig").unwrap();
    assert!(matches!(b.source, Source::Node(_)), "the new list is shown");
    let mut shown = titles(&b);
    shown.sort();
    assert_eq!(shown, ["A", "B"], "audio files, not the subfolder's");
    assert!(b.import_folder_as_playlist(&dir, "_bad").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dragged_node_moves_and_its_favorite_follows() {
    let mut b = browser(&[]);
    let lib = b.library().unwrap();
    let folder = lib
        .create_node(None, NodeKind::Folder, "Gigs", None)
        .unwrap();
    let list = lib
        .create_node(None, NodeKind::Playlist, "Peak", None)
        .unwrap();
    b.refresh();
    b.favorites[5] = "$PLAYLIST/tPeak".to_owned();
    b.move_node(list, Some(folder));
    assert_eq!(b.favorites[5], "$PLAYLIST/tGigs/tPeak");
    b.move_node(folder, Some(folder));
    assert!(b.nodes.iter().any(|n| n.id == folder && n.parent.is_none()));
    b.move_node(list, None);
    assert_eq!(b.favorites[5], "$PLAYLIST/tPeak");
}

#[test]
fn load_next_steps_the_selection() {
    let mut b = browser(&["/m/A.mp3", "/m/B.mp3", "/m/C.mp3"]);
    assert_eq!(b.step_selection(-1), None, "nothing before no selection");
    let first = b.step_selection(1).unwrap();
    assert_eq!(b.selected(), Some(first));
    let second = b.step_selection(1).unwrap();
    assert_ne!(first, second);
    assert_eq!(b.step_selection(-1), Some(first));
    b.step_selection(1);
    b.step_selection(1);
    assert_eq!(b.step_selection(1), None, "past the end");
    assert_eq!(b.selection.cursor(), Some(2), "the selection stays");
}

#[test]
fn deleting_recordings_purges_their_files_but_keeps_the_entries() {
    let dir = std::env::temp_dir().join(format!("backspn-purge-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (dir.join("Recording_a.wav"), dir.join("Recording_b.wav"));
    std::fs::write(&a, b"").unwrap();
    std::fs::write(&b, b"").unwrap();
    let mut b_ = browser(&[]);
    b_.add_recordings(&[a.clone(), b.clone()]);
    b_.show(Source::Recordings);
    let first = titles(&b_).iter().position(|t| t == "Recording_a").unwrap();
    b_.selection.select(first);
    b_.delete_recordings();
    assert!(!a.exists(), "the file is gone");
    assert!(b.exists(), "the other stays");
    assert_eq!(titles(&b_), ["Recording_b"]);
    assert_eq!(b_.rows.len(), 2, "the collection keeps both entries");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saved_cues_reach_the_preview_column() {
    let mut b = browser(&["/m/A.mp3"]);
    let id = b.rows[0].id;
    let lib = b.library().unwrap();
    lib.save_analysis(
        id,
        &backspn_library::AnalysisUpdate {
            duration_s: Some(100.0),
            ..backspn_library::AnalysisUpdate::default()
        },
    )
    .unwrap();
    b.refresh();
    assert!(b.cue_marks(id).is_empty());
    let cue = backspn_library::CuePoint {
        kind: backspn_library::CuePoint::CUE,
        name: String::new(),
        start_s: 50.0,
        len_s: 0.0,
        hotcue: Some(0),
        display_order: 0,
        repeats: -1,
        color: None,
    };
    b.library().unwrap().save_cues(id, &[cue]).unwrap();
    assert!(b.cue_marks(id).is_empty(), "read once, until told");
    b.refresh_track(id);
    let marks = b.cue_marks(id);
    assert_eq!((marks.len(), marks[0].at), (1, 0.5));
}

#[test]
fn a_chosen_cover_shows_on_the_tracks_at_once() {
    let mut b = browser(&["/m/A.mp3", "/m/B.mp3"]);
    let ids: Vec<i64> = b.rows.iter().map(|r| r.id).collect();
    b.set_cover(&ids[..1], "abc.png");
    let store = Path::new("/covers");
    let source = |b: &Browser, id| crate::covers::source(b.row(id).unwrap(), store);
    assert_eq!(
        source(&b, ids[0]),
        crate::covers::Source::Stored("/covers/abc.png".into())
    );
    assert_eq!(
        source(&b, ids[1]),
        crate::covers::Source::File("/m/B.mp3".into())
    );
    b.set_cover(&ids[..1], "");
    assert_eq!(source(&b, ids[0]), crate::covers::Source::None);
}

#[test]
fn list_edits_write_one_track_and_check_what_was_typed() {
    let mut b = browser(&["/m/A.mp3", "/m/B.mp3"]);
    let (a, other) = (b.view[0], b.view[1]);
    b.selection.select(0);
    assert!(b.edit_selected());
    let mut edit = b.inline.clone().unwrap();
    assert_eq!((edit.id, edit.column), (a, BrowserColumn::Title));
    assert_eq!(edit.text, b.row(a).unwrap().title, "starts with the cell");
    edit.text = "New".into();
    b.commit_edit(&edit);
    assert_eq!(b.row(a).unwrap().title, "New");
    assert_ne!(b.row(other).unwrap().title, "New", "only that track");
    edit.text.clear();
    b.commit_edit(&edit);
    assert_eq!(b.row(a).unwrap().title, "New");
    assert!(b.message.take().is_some());
    b.inline = None;
    b.start_edit(a, BrowserColumn::Plays);
    let mut edit = b.inline.clone().unwrap();
    edit.text = "4".into();
    b.commit_edit(&edit);
    assert_eq!(b.row(a).unwrap().playcount, 4);
    edit.text = "-1".into();
    b.commit_edit(&edit);
    assert_eq!(b.row(a).unwrap().playcount, 4);
    b.inline = None;
    b.start_edit(a, BrowserColumn::Time);
    assert!(b.inline.is_none());
    b.selection.select_all(2);
    assert!(!b.edit_selected());
    b.selection.select(0);
    b.inplace_edit = false;
    assert!(!b.edit_selected());
    b.start_edit(a, BrowserColumn::Title);
    assert!(b.inline.is_none());
}

#[test]
fn a_sample_set_lists_its_tracks_and_is_deleted_after_asking() {
    use backspn_formats::nml::{Location, SampleSet, SetCell};
    let mut b = browser(&["/m/A.wav", "/m/B.wav"]);
    let (a, bb) = (b.view[0], b.view[1]);
    let mut set = SampleSet {
        title: "Drums".to_owned(),
        ..SampleSet::default()
    };
    let at = |file: &str| Location {
        dir: "/:m/:".to_owned(),
        file: file.to_owned(),
        volume: String::new(),
        volume_id: None,
    };
    set.slots[0].cells = vec![SetCell::new(0, at("A.wav")), SetCell::new(1, at("B.wav"))];
    set.slots[1].cells = vec![SetCell::new(0, at("A.wav"))];
    let mut tracks = vec![None; 64];
    tracks[0] = Some(a);
    tracks[1] = Some(bb);
    tracks[16] = Some(a);
    let id = b.save_library_set(None, &set, &tracks).unwrap();
    assert_eq!(super::tree::set_label(&b.sets[0].set), "Drums (2)");
    b.show(Source::Set(id));
    assert_eq!(b.view, [a, bb], "each track once");
    assert_eq!(b.shown_set(), Some(id));
    b.answer(dialogs::BrowserAsk::DeleteSets(vec![id]));
    assert_eq!(b.sets, []);
    assert_eq!(b.source, Source::Collection, "the deleted set closes");
}

fn plain_wav(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("backspn-browser-{name}-{}.wav", std::process::id()));
    let (rate, data) = (44_100u32, 44_100u32 * 4);
    let mut wav = Vec::new();
    for part in [
        &b"RIFF"[..],
        &(36 + data).to_le_bytes(),
        b"WAVEfmt ",
        &16u32.to_le_bytes(),
    ] {
        wav.extend_from_slice(part);
    }
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    wav.resize(wav.len() + data as usize, 0);
    std::fs::write(&path, wav).unwrap();
    path
}

fn finish_rescan(b: &mut Browser) -> String {
    let start = std::time::Instant::now();
    while !b.rescan.is_idle() || b.rescan.total > 0 {
        b.collect_rescan();
        assert!(start.elapsed().as_secs() < 10, "rescan timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    b.activity.entries().last().unwrap().text.clone()
}

#[test]
fn full_rescan_reads_every_file_and_quick_scan_only_the_modified() {
    let file = plain_wav("rescan");
    let mut b = browser(&[]);
    b.set_context(&egui::Context::default());
    let id = b.add_files(std::slice::from_ref(&file))[0];
    b.rescan(&[id], false);
    assert_eq!(
        finish_rescan(&mut b),
        "Rescan done: 1 read, 0 unchanged, 0 failed"
    );
    b.rescan(&[id], true);
    assert_eq!(
        finish_rescan(&mut b),
        "Rescan done: 0 read, 1 unchanged, 0 failed"
    );
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(later)
        .unwrap();
    b.rescan(&[id], true);
    assert_eq!(
        finish_rescan(&mut b),
        "Rescan done: 1 read, 0 unchanged, 0 failed"
    );
    std::fs::remove_file(&file).unwrap();
}

#[test]
fn analyze_and_scan_ask_before_they_start() {
    let file = plain_wav("ask");
    let mut b = browser(&[]);
    b.set_context(&egui::Context::default());
    let id = b.add_files(std::slice::from_ref(&file))[0];
    b.ask_rescan(vec![id], true);
    assert!(b.ask.is_some(), "the scan asks first");
    assert!(
        b.rescan.is_idle() && b.rescan.total == 0,
        "nothing queued yet"
    );
    b.ask = None;
    b.answer(dialogs::BrowserAsk::Rescan(vec![id], true));
    assert_eq!(b.rescan.total, 1, "Yes queues it");
    b.ask_analyse(vec![id]);
    assert!(b.ask.is_some(), "the analysis asks first");
    b.ask = None;
    b.ask_analyse(Vec::new());
    b.ask_rescan(vec![-1], false);
    assert!(b.ask.is_none());
    finish_rescan(&mut b);
    std::fs::remove_file(&file).unwrap();
}
