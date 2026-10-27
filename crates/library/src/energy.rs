//! Estimated energy of a track on a 1–10 scale, from what the library
//! already stores: a value written in the comment wins, otherwise loudness,
//! tempo and how much of the track plays near its peak level.

use crate::{TrackRow, overview};

pub const MIN: f64 = 1.0;
pub const MAX: f64 = 10.0;

const QUIET_LUFS: f64 = -16.0;
const LOUD_LUFS: f64 = -6.0;
const SLOW_BPM: f64 = 90.0;
const FAST_BPM: f64 = 145.0;
/// Columns at or above this share of the track's peak count as full drive.
const DRIVE_LEVEL: f64 = 0.7;
/// Columns below this amplitude are silence and do not count.
const SILENCE: u8 = 8;

const LOUDNESS_WEIGHT: f64 = 0.45;
const TEMPO_WEIGHT: f64 = 0.30;
const DRIVE_WEIGHT: f64 = 0.25;

pub fn estimate(row: &TrackRow) -> Option<f64> {
    if let Some(tagged) = tagged(&row.comment).or_else(|| tagged(&row.comment2)) {
        return Some(tagged);
    }
    let parts = [
        (
            LOUDNESS_WEIGHT,
            row.lufs.map(|l| unit(l, QUIET_LUFS, LOUD_LUFS)),
        ),
        (
            TEMPO_WEIGHT,
            row.bpm
                .filter(|b| *b > 0.0)
                .map(|b| unit(b, SLOW_BPM, FAST_BPM)),
        ),
        (DRIVE_WEIGHT, drive(&row.overview)),
    ];
    let (weight, sum) = parts
        .iter()
        .filter_map(|(w, v)| v.map(|v| (w, w * v)))
        .fold((0.0, 0.0), |(tw, ts), (w, v)| (tw + w, ts + v));
    (weight > 0.0).then(|| MIN + (MAX - MIN) * sum / weight)
}

/// An energy written by the user in a comment, such as "Energy 7".
fn tagged(comment: &str) -> Option<f64> {
    let lower = comment.to_ascii_lowercase();
    let at = lower.find("energy")? + "energy".len();
    let rest = lower[at..].trim_start_matches([' ', ':', '-', '=']);
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let value: u8 = digits.parse().ok()?;
    (1..=10).contains(&value).then_some(f64::from(value))
}

/// Share of the audible columns that play near the track's peak level.
fn drive(overview: &[u8]) -> Option<f64> {
    let peak = overview::amplitudes(overview).max()?;
    if peak < SILENCE {
        return None;
    }
    let threshold = f64::from(peak) * DRIVE_LEVEL;
    let (audible, driven) = overview::amplitudes(overview)
        .filter(|a| *a >= SILENCE)
        .fold((0u32, 0u32), |(n, d), a| {
            (n + 1, d + u32::from(f64::from(a) >= threshold))
        });
    Some(f64::from(driven) / f64::from(audible))
}

fn unit(value: f64, low: f64, high: f64) -> f64 {
    ((value - low) / (high - low)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(lufs: Option<f64>, bpm: Option<f64>) -> TrackRow {
        TrackRow {
            bpm,
            lufs,
            ..TrackRow::default()
        }
    }

    fn overview(amplitudes: &[u8]) -> Vec<u8> {
        amplitudes
            .iter()
            .flat_map(|&a| [a, 0, 0, 0, 0, 0, 0])
            .collect()
    }

    #[test]
    fn comment_value_wins() {
        let mut r = row(Some(-20.0), Some(80.0));
        r.comment = "8A - Energy 7".into();
        assert_eq!(estimate(&r), Some(7.0));
        r.comment = "energy:10, peak time".into();
        assert_eq!(estimate(&r), Some(10.0));
        r.comment = "energy 0".into();
        assert_eq!(estimate(&r), Some(MIN), "out of range is ignored");
        r.comment.clear();
        r.comment2 = "ENERGY=3".into();
        assert_eq!(estimate(&r), Some(3.0));
    }

    #[test]
    fn loud_fast_tracks_score_higher() {
        let quiet = estimate(&row(Some(-16.0), Some(90.0))).unwrap();
        let loud = estimate(&row(Some(-6.0), Some(145.0))).unwrap();
        assert!((quiet - MIN).abs() < 1e-9);
        assert!((loud - MAX).abs() < 1e-9);
        let mid = estimate(&row(Some(-11.0), Some(117.5))).unwrap();
        assert!((mid - 5.5).abs() < 1e-9);
    }

    #[test]
    fn missing_parts_are_left_out() {
        assert_eq!(estimate(&row(None, None)), None);
        let tempo_only = estimate(&row(None, Some(145.0))).unwrap();
        assert!((tempo_only - MAX).abs() < 1e-9);
    }

    #[test]
    fn drive_counts_columns_near_the_peak_and_skips_silence() {
        assert_eq!(drive(&[]), None);
        assert_eq!(drive(&overview(&[0, 2])), None);
        let half = drive(&overview(&[0, 0, 200, 190, 100, 50])).unwrap();
        assert!((half - 0.5).abs() < 1e-9);
        let mut r = row(None, None);
        r.overview = overview(&[200, 200]);
        assert_eq!(estimate(&r), Some(MAX));
    }
}
