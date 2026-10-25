use std::path::{Path, PathBuf};
use std::sync::Arc;

use backspn_engine::{DeckState, Grid, Track};
use backspn_library::clock::LocalTime;

use crate::recorder::wav::{Format, WavWriter};

pub const MAX_LOOP_S: f64 = 48.0;
pub const FALLBACK_BPM: f64 = 120.0;

pub fn range(state: &DeckState, loop_beats: f64) -> Option<(f64, f64)> {
    if !state.loaded {
        return None;
    }
    if let Some(range) = active_loop(state) {
        return Some(range);
    }
    let beat_s = state
        .grid
        .map_or(60.0 / FALLBACK_BPM, |g| g.period_at(state.position_s));
    let start = state.position_s.max(0.0);
    Some((start, start + loop_beats.max(0.0) * beat_s))
}

pub fn active_loop(state: &DeckState) -> Option<(f64, f64)> {
    if state.loaded
        && state.loop_active
        && let (Some(start), Some(end)) = (state.loop_start_s, state.loop_end_s)
        && end > start
    {
        return Some((start, end));
    }
    None
}

pub fn grid_anchor(grid: Option<&Grid>, start_s: f64) -> Option<f64> {
    let grid = grid?;
    let beat = (grid.beat_at(start_s) - 1e-6).ceil();
    Some((grid.time_of(beat) - start_s).max(0.0))
}

pub fn bpm(state: &DeckState, start_s: f64) -> f64 {
    in_range(
        state
            .grid
            .map_or(FALLBACK_BPM, |g| 60.0 / g.period_at(start_s)),
    )
}

pub fn in_range(bpm: f64) -> f64 {
    if (40.0..=250.0).contains(&bpm) {
        bpm
    } else {
        FALLBACK_BPM
    }
}

pub fn cut(track: &Track, start_s: f64, end_s: f64) -> Track {
    let rate = track.sample_rate();
    let first = (start_s.max(0.0) * f64::from(rate)) as usize;
    let last = (end_s.max(start_s) * f64::from(rate)) as usize;
    let mut samples = Vec::with_capacity(2 * (last - first));
    for frame in first..last {
        let [l, r] = track.frame(frame as isize);
        samples.push(l);
        samples.push(r);
    }
    Track::from_stereo(rate, samples)
}

pub fn name(source: &str, now: LocalTime) -> String {
    format!(
        "{source} {}-{:02}-{:02} {:02}-{:02}-{:02}",
        now.year,
        now.month,
        now.day,
        now.seconds / 3600,
        now.seconds / 60 % 60,
        now.seconds % 60
    )
}

pub fn write_wav(dir: &Path, name: &str, track: &Track) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let safe: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut path = dir.join(format!("{safe}.wav"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{safe} {n}.wav"));
        n += 1;
    }
    let mut writer = WavWriter::create(&path, Format::Pcm16, track.sample_rate())?;
    writer.write(track.samples(), 1.0)?;
    writer.finish()?;
    Ok(path)
}

#[derive(Debug, Clone)]
pub struct Captured {
    pub track: Arc<Track>,
    pub name: String,
    pub bpm: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> DeckState {
        DeckState {
            loaded: true,
            position_s: 2.0,
            duration_s: 60.0,
            grid: Some(Grid::new(120.0, 0.0)),
            ..DeckState::default()
        }
    }

    #[test]
    fn the_active_loop_wins_over_the_loop_size() {
        let mut s = state();
        assert_eq!(range(&s, 4.0), Some((2.0, 4.0)), "4 beats at 120 BPM");
        s.loop_active = true;
        s.loop_start_s = Some(5.0);
        s.loop_end_s = Some(6.0);
        assert_eq!(range(&s, 4.0), Some((5.0, 6.0)));
        s.loaded = false;
        assert_eq!(range(&s, 4.0), None);
    }

    #[test]
    fn a_saved_piece_keeps_the_grid_beats() {
        let grid = Grid::new(120.0, 0.25);
        assert_eq!(grid_anchor(Some(&grid), 2.25), Some(0.0), "on a beat");
        let anchor = grid_anchor(Some(&grid), 2.0).unwrap();
        assert!((anchor - 0.25).abs() < 1e-9, "{anchor}");
        assert_eq!(grid_anchor(None, 2.0), None);
        let mut s = state();
        assert_eq!(active_loop(&s), None, "no loop on");
        s.loop_active = true;
        s.loop_start_s = Some(5.0);
        s.loop_end_s = Some(6.0);
        assert_eq!(active_loop(&s), Some((5.0, 6.0)));
    }

    #[test]
    fn tempo_out_of_range_falls_back() {
        let mut s = state();
        assert!((bpm(&s, 0.0) - 120.0).abs() < 1e-9);
        s.grid = Some(Grid::new(300.0, 0.0));
        assert!((bpm(&s, 0.0) - FALLBACK_BPM).abs() < 1e-9);
        assert!((in_range(97.5) - 97.5).abs() < 1e-9);
        assert!((in_range(0.0) - FALLBACK_BPM).abs() < 1e-9);
    }

    #[test]
    fn cutting_copies_the_frames_and_pads_the_end() {
        let samples: Vec<f32> = (0..200).flat_map(|i| [i as f32, -(i as f32)]).collect();
        let track = Track::from_stereo(100, samples);
        let piece = cut(&track, 0.5, 1.0);
        assert_eq!(piece.frames(), 50);
        assert_eq!(piece.frame(0), [50.0, -50.0]);
        let past = cut(&track, 1.5, 2.5);
        assert_eq!(past.frames(), 100);
        assert_eq!(past.frame(60), [0.0, 0.0], "silence past the end");
    }

    #[test]
    fn wav_files_round_trip_and_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("backspn-capture-{}", std::process::id()));
        let track = Track::from_stereo(8_000, vec![0.25; 2 * 800]);
        let a = write_wav(&dir, "Loop: test", &track).unwrap();
        let b = write_wav(&dir, "Loop: test", &track).unwrap();
        assert_ne!(a, b);
        assert!(
            a.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("Loop_ test")
        );
        let back = backspn_decode::decode_file(&a).unwrap();
        assert_eq!(back.frames(), 800);
        assert!((back.frame(10)[0] - 0.25).abs() < 1e-3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn names_carry_the_time() {
        let now = LocalTime {
            year: 2026,
            month: 4,
            day: 25,
            seconds: 14 * 3600 + 26 * 60 + 56,
        };
        assert_eq!(name("Kick", now), "Kick 2026-04-25 14-26-56");
    }
}
