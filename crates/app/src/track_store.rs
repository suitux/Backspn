use backspn_analysis::Analysis;
use backspn_engine::{Grid, HOTCUES, HotcuePoint};
use backspn_formats::nml::MusicalKey;
use backspn_library::{AnalysisUpdate, CuePoint, TrackRow, Transient};

use crate::deck_view::LoadRequest;

pub fn load_request(track: &TrackRow, cues: &[CuePoint], cue_to_marker: bool) -> LoadRequest {
    let path = track.path.split('#').next().unwrap_or(&track.path);
    let mut request = LoadRequest::file(path.into());
    if !track.title.is_empty() {
        request.name = if track.artist.is_empty() {
            track.title.clone()
        } else {
            format!("{} - {}", track.artist, track.title)
        };
        request.title.clone_from(&track.title);
    }
    request.artist.clone_from(&track.artist);
    request.comment.clone_from(&track.comment);
    request.track_id = Some(track.id);
    request.grid = match (track.bpm, track.grid_anchor_s) {
        (Some(bpm), Some(anchor_s)) if bpm > 0.0 => Some(Grid {
            bpm,
            anchor_s,
            ..Grid::FIXED
        }),
        _ => None,
    };
    request.cue_s = cues
        .iter()
        .find(|c| c.hotcue.is_none() && matches!(c.kind, CuePoint::LOAD | CuePoint::CUE))
        .filter(|_| cue_to_marker)
        .map(|c| c.start_s);
    request.cues = cues.to_vec();
    request.key_text = (!track.key_text.is_empty()).then(|| track.key_text.clone());
    request.lufs = track.lufs;
    request.locked = track.locked;
    request.cover = crate::covers::source(track, &crate::covers::store_dir());
    request
}

pub fn onset(t: &Transient) -> backspn_analysis::Transient {
    backspn_analysis::Transient {
        position_s: t.position_s,
        weight: t.weight,
        bass_weight: t.bass_weight,
    }
}

pub fn analysed_grid(g: &crate::browser::AnalysedGrid, old: Option<Grid>) -> Option<Grid> {
    Some(Grid {
        bpm: g.bpm?,
        anchor_s: g.anchor_s.or(old.map(|o| o.anchor_s))?,
        ..Grid::FIXED
    })
}

pub fn grid_update(grid: Grid) -> AnalysisUpdate {
    AnalysisUpdate {
        bpm: Some(grid.bpm),
        grid_anchor_s: Some(grid.anchor_s),
        ..AnalysisUpdate::default()
    }
}

pub fn analysis_update(analysis: &Analysis, keep_grid: bool, known_key: bool) -> AnalysisUpdate {
    let grid = analysis.beatgrid.filter(|_| !keep_grid);
    let key = analysis.key.filter(|_| !known_key);
    AnalysisUpdate {
        bpm: grid.map(|g| g.bpm),
        grid_anchor_s: grid.map(|g| g.anchor_s),
        musical_key: key.map(backspn_analysis::Key::nml_value),
        key_text: key
            .and_then(|k| MusicalKey::from_value(k.nml_value()))
            .map(MusicalKey::open_key),
        lufs: analysis
            .loudness
            .map(|l| l.integrated_lufs)
            .filter(|l| l.is_finite()),
        duration_s: None,
        replace_locked: false,
        transients: analysis
            .transients
            .iter()
            .map(|t| Transient {
                position_s: t.position_s,
                weight: t.weight,
                bass_weight: t.bass_weight,
            })
            .collect(),
    }
}

pub fn batch_update(analysis: &Analysis, parts: &crate::analysis_queue::Parts) -> AnalysisUpdate {
    let full = analysis_update(analysis, false, false);
    AnalysisUpdate {
        bpm: full.bpm.filter(|_| parts.bpm || parts.grid),
        grid_anchor_s: full.grid_anchor_s.filter(|_| parts.grid),
        musical_key: full.musical_key.filter(|_| parts.key),
        key_text: full.key_text.filter(|_| parts.key),
        lufs: full.lufs.filter(|_| parts.gain),
        duration_s: None,
        replace_locked: parts.replace_locked,
        transients: full.transients,
    }
}

pub type HotcueSet = [Option<HotcuePoint>; HOTCUES];

pub struct Watch {
    track_id: i64,
    after_generation: u64,
    last: Option<HotcueSet>,
}

impl Watch {
    pub fn new(track_id: i64, generation: u64) -> Self {
        Self {
            track_id,
            after_generation: generation,
            last: None,
        }
    }

    pub fn observe(
        &mut self,
        track_id: i64,
        generation: u64,
        loaded: bool,
        hotcues: HotcueSet,
    ) -> Option<HotcueSet> {
        if self.track_id != track_id {
            *self = Self::new(track_id, generation);
        }
        if generation <= self.after_generation || !loaded || self.last == Some(hotcues) {
            return None;
        }
        let baseline = self.last.is_none();
        self.last = Some(hotcues);
        (!baseline).then_some(hotcues)
    }
}

pub fn duplicate_source(
    target: usize,
    decks: impl Iterator<Item = (Option<i64>, bool)>,
    track: i64,
) -> Option<usize> {
    let mut found = None;
    for (d, (id, playing)) in decks.enumerate() {
        if d == target || id != Some(track) {
            continue;
        }
        if playing {
            return Some(d);
        }
        found = found.or(Some(d));
    }
    found
}

#[cfg(test)]
mod tests {
    use backspn_engine::CueKind;

    use super::*;

    #[test]
    fn duplicates_the_deck_with_the_same_track_preferring_a_playing_one() {
        let decks = [
            (Some(7), false),
            (None, false),
            (Some(7), true),
            (Some(9), true),
        ];
        assert_eq!(duplicate_source(1, decks.into_iter(), 7), Some(2));
        assert_eq!(duplicate_source(2, decks.into_iter(), 7), Some(0));
        assert_eq!(duplicate_source(0, decks.into_iter(), 9), Some(3));
        assert_eq!(duplicate_source(3, decks.into_iter(), 9), None);
        assert_eq!(duplicate_source(1, decks.into_iter(), 5), None);
    }

    #[test]
    fn hotcues_are_saved_only_for_user_changes_of_the_loaded_track() {
        let none: HotcueSet = [None; HOTCUES];
        let cue = |start_s| {
            Some(HotcuePoint {
                start_s,
                kind: CueKind::Cue,
                loop_s: None,
            })
        };
        let mut old = none;
        old[0] = cue(10.0);
        let mut restored = none;
        restored[2] = cue(5.0);
        let mut edited = restored;
        edited[3] = cue(8.0);

        let mut w = Watch::new(7, 4);
        assert_eq!(
            w.observe(7, 4, true, old),
            None,
            "old track's points never stored"
        );
        assert_eq!(
            w.observe(7, 5, true, restored),
            None,
            "baseline from the library"
        );
        assert_eq!(w.observe(7, 5, true, restored), None);
        assert_eq!(w.observe(7, 5, true, edited), Some(edited));
        assert_eq!(w.observe(8, 5, true, edited), None);
        assert_eq!(w.observe(8, 6, true, none), None);
    }

    fn cue(kind: i32, start_s: f64, len_s: f64, hotcue: Option<u8>) -> CuePoint {
        CuePoint {
            kind,
            name: String::new(),
            start_s,
            len_s,
            hotcue,
            display_order: 0,
            repeats: -1,
            color: None,
        }
    }

    #[test]
    fn a_library_track_loads_with_its_names_grid_hotcues_and_cue() {
        let track = TrackRow {
            id: 4,
            path: "/m/a.mp3#2".into(),
            title: "Track".into(),
            artist: "Artist".into(),
            bpm: Some(128.0),
            grid_anchor_s: Some(0.1),
            key_text: "6m".into(),
            ..TrackRow::default()
        };
        let cues = [
            cue(CuePoint::CUE, 1.0, 0.0, Some(0)),
            cue(CuePoint::LOOP, 2.0, 4.0, Some(1)),
            cue(CuePoint::LOAD, 3.0, 0.0, None),
            cue(CuePoint::CUE, 5.0, 0.0, None),
            cue(CuePoint::CUE, 6.0, 0.0, Some(20)),
            cue(CuePoint::FADE_IN, 7.0, 0.0, Some(2)),
        ];
        let r = load_request(&track, &cues, true);
        assert_eq!(r.path, std::path::PathBuf::from("/m/a.mp3"));
        assert_eq!(r.name, "Artist - Track");
        assert_eq!(r.track_id, Some(4));
        assert_eq!(
            r.grid,
            Some(Grid {
                bpm: 128.0,
                anchor_s: 0.1,
                ..Grid::FIXED
            })
        );
        assert_eq!(r.cues, cues);
        assert_eq!(r.cue_s, Some(3.0), "first load/cue marker");
        assert_eq!(r.key_text.as_deref(), Some("6m"));
        assert_eq!(load_request(&track, &cues, false).cue_s, None);
    }

    #[test]
    fn analysed_tempo_keeps_the_old_marker_unless_the_grid_was_set() {
        let old = Grid {
            bpm: 120.0,
            anchor_s: 0.3,
            ..Grid::FIXED
        };
        let g = |bpm, anchor_s| crate::browser::AnalysedGrid {
            track: 1,
            bpm,
            anchor_s,
        };
        let tempo = analysed_grid(&g(Some(126.0), None), Some(old)).unwrap();
        assert_eq!((tempo.bpm, tempo.anchor_s), (126.0, 0.3));
        let grid = analysed_grid(&g(Some(126.0), Some(0.1)), Some(old)).unwrap();
        assert_eq!(grid.anchor_s, 0.1);
        assert_eq!(analysed_grid(&g(Some(126.0), None), None), None);
        assert_eq!(analysed_grid(&g(None, Some(0.1)), Some(old)), None);
    }

    #[test]
    fn untitled_tracks_keep_the_file_name_and_zero_bpm_means_no_grid() {
        let track = TrackRow {
            path: "/m/Some File.flac".into(),
            bpm: Some(0.0),
            grid_anchor_s: Some(0.0),
            ..TrackRow::default()
        };
        let r = load_request(&track, &[], true);
        assert_eq!(r.name, "Some File");
        assert_eq!(r.grid, None);
    }

    #[test]
    fn analysis_keeps_a_locked_grid_and_a_known_key() {
        let analysis = Analysis {
            beatgrid: Some(backspn_analysis::Beatgrid {
                bpm: 124.0,
                anchor_s: 0.5,
            }),
            key: Some(backspn_analysis::Key::from_nml_value(21).unwrap()),
            loudness: None,
            transients: Vec::new(),
        };
        let all = analysis_update(&analysis, false, false);
        assert_eq!((all.bpm, all.grid_anchor_s), (Some(124.0), Some(0.5)));
        assert_eq!(all.musical_key, Some(21));
        assert!(all.key_text.is_some());
        let kept = analysis_update(&analysis, true, true);
        assert_eq!(
            (kept.bpm, kept.musical_key, kept.key_text),
            (None, None, None)
        );
    }
}
