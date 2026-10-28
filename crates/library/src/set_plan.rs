//! Plans a DJ set from a session brief: picks and orders tracks so tempo and
//! energy follow the requested curve, keys mix, artists do not repeat and
//! the set fills the requested time.

use std::collections::HashSet;
use std::fmt;
use std::hash::BuildHasher;

use backspn_formats::nml::MusicalKey;
use serde::{Deserialize, Serialize};

use crate::smart::parse_stored_date;
use crate::suggest::bpm_offset;
use crate::{TrackRow, energy};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Curve {
    /// Straight line from start to end.
    #[default]
    Linear,
    /// Rises from start to end.
    Warmup,
    /// Climbs to the peak near the end, then settles on the end value.
    Build,
    /// Climbs to the peak at `peak_at_pct`, then falls to the end value.
    PeakTime,
    /// Two rises to the peak with a dip between them.
    Wave,
    /// Reaches the peak early and holds it until the last stretch.
    Plateau,
    /// Falls from the start (or peak) to the end value.
    Cooldown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Harmonic {
    /// Same key, relative key or a neighbour on the wheel.
    Strict,
    /// Also one semitone up or down.
    #[default]
    Relaxed,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Familiarity {
    #[default]
    Mix,
    /// Prefers tracks played few times.
    Fresh,
    /// Prefers tracks played many times.
    Classics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionStyle {
    /// Long overlap with the bass swapped halfway.
    #[default]
    Blend,
    /// Straight swap on a beat.
    Cut,
    /// Both faders move evenly across the overlap.
    Fade,
    /// The outgoing track filters away while the incoming one opens up.
    FilterSweep,
    /// The faders chop on the beat before the swap.
    Chop,
    /// The outgoing track echoes away and the next one drops in.
    EchoOut,
    /// The outgoing track loops its last beats while it fades.
    LoopOut,
    /// The outgoing track brakes to a stop and the next one drops in.
    Brake,
    /// The outgoing track spins back and the next one drops in.
    Backspin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transition {
    pub style: TransitionStyle,
    /// Length of the overlap for the long styles.
    pub bars: u32,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            style: TransitionStyle::Blend,
            bars: 16,
        }
    }
}

impl Transition {
    /// Beats the two tracks share.
    pub fn beats(self) -> f64 {
        let long = f64::from(self.bars.max(1)) * 4.0;
        match self.style {
            TransitionStyle::Blend
            | TransitionStyle::Fade
            | TransitionStyle::FilterSweep
            | TransitionStyle::Chop
            | TransitionStyle::LoopOut => long,
            TransitionStyle::Cut => 4.0,
            TransitionStyle::EchoOut | TransitionStyle::Brake | TransitionStyle::Backspin => 12.0,
        }
    }

    /// Minutes two tracks play together at `bpm`.
    pub fn overlap_min(self, bpm: Option<f64>) -> f64 {
        let bpm = bpm.filter(|b| *b > 0.0).unwrap_or(DEFAULT_BPM);
        self.beats() / bpm
    }
}

/// Where the tracks come from; the caller resolves it to rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Playlist(String),
    Smartlist(String),
}

/// What the session should sound like. Every field is optional.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionBrief {
    pub duration_min: Option<f64>,
    pub track_count: Option<usize>,
    /// Minutes each track plays; none plays the whole track.
    pub play_minutes: Option<f64>,

    pub bpm_start: Option<f64>,
    pub bpm_peak: Option<f64>,
    pub bpm_end: Option<f64>,
    pub bpm_min: Option<f64>,
    pub bpm_max: Option<f64>,
    /// Largest tempo change between two tracks, in percent.
    pub max_tempo_shift_pct: Option<f64>,
    pub keylock: Option<bool>,

    pub energy_start: Option<f64>,
    pub energy_peak: Option<f64>,
    pub energy_end: Option<f64>,
    pub curve: Option<Curve>,
    pub peak_at_pct: Option<f64>,

    pub harmonic: Harmonic,
    /// Key of the first track, in any notation ("8A", "Am").
    pub start_key: Option<String>,

    pub genres_include: Vec<String>,
    pub genres_exclude: Vec<String>,
    /// Free words matched against genre, comments, title and album.
    pub mood: Option<String>,
    pub artists_include: Vec<String>,
    pub artists_exclude: Vec<String>,
    pub labels_include: Vec<String>,
    pub labels_exclude: Vec<String>,
    pub release_year_from: Option<i32>,
    pub release_year_to: Option<i32>,

    pub source: Option<Source>,
    /// Stars, 0–5.
    pub min_rating: Option<u8>,
    pub familiarity: Familiarity,
    pub skip_played_since_days: Option<u32>,
    pub no_artist_repeat_within: Option<usize>,

    pub must_play: Vec<i64>,
    pub opener: Option<i64>,
    pub closer: Option<i64>,

    pub transition: Transition,
}

/// Where a plan starts: from scratch, or after a track already playing when
/// the rest of a set is planned again.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Start {
    pub after: Option<i64>,
    /// Share of the session already played, 0–1.
    pub from_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedTrack {
    pub id: i64,
    pub artist: String,
    pub title: String,
    pub bpm: Option<f64>,
    pub key: String,
    pub energy: Option<f64>,
    pub start_min: f64,
    pub play_min: f64,
    pub target_energy: Option<f64>,
    pub target_bpm: Option<f64>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Warning {
    NoCandidates,
    FewCandidates { found: usize, wanted: usize },
    ShortOfTime { planned_min: f64, wanted_min: f64 },
    NotFound { ids: Vec<i64> },
    UnknownKey { text: String },
    TempoJumps { count: usize },
    KeyClashes { count: usize },
    UnknownEnergy { count: usize },
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCandidates => write!(f, "no track matches the filters"),
            Self::FewCandidates { found, wanted } => {
                write!(f, "only {found} tracks match the filters, {wanted} wanted")
            }
            Self::ShortOfTime {
                planned_min,
                wanted_min,
            } => write!(
                f,
                "the set lasts {planned_min:.0} min of the {wanted_min:.0} wanted"
            ),
            Self::NotFound { ids } => write!(f, "tracks not available: {ids:?}"),
            Self::UnknownKey { text } => write!(f, "unknown key \"{text}\""),
            Self::TempoJumps { count } => {
                write!(f, "{count} transitions change tempo more than allowed")
            }
            Self::KeyClashes { count } => write!(f, "{count} transitions clash in key"),
            Self::UnknownEnergy { count } => {
                write!(f, "{count} tracks have no energy estimate")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SetPlan {
    pub tracks: Vec<PlannedTrack>,
    pub warnings: Vec<Warning>,
    pub candidates: usize,
    pub total_min: f64,
}

const DEFAULT_DURATION_MIN: f64 = 60.0;
const DEFAULT_TRACK_MIN: f64 = 6.0;
const DEFAULT_BPM: f64 = 124.0;
const DEFAULT_MAX_SHIFT: f64 = 0.06;
const DEFAULT_PEAK_AT: f64 = 0.7;
const DEFAULT_ARTIST_GAP: usize = 4;
const SHORTEST_TRACK_S: f64 = 60.0;
const BEAM: usize = 8;
const BRANCH: usize = 4;

const CLASH: f64 = 4.0;
const JUMP: f64 = 5.0;
const ARTIST_REPEAT: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Targets {
    start: f64,
    peak: f64,
    end: f64,
}

impl Targets {
    fn new(start: Option<f64>, peak: Option<f64>, end: Option<f64>) -> Option<Self> {
        let start = start.or(peak).or(end)?;
        let end = end.or(peak).unwrap_or(start);
        let peak = peak.unwrap_or(start.max(end));
        Some(Self { start, peak, end })
    }

    fn at(self, curve: Curve, peak_at: f64, p: f64) -> f64 {
        let Self { start, peak, end } = self;
        let mid = f64::midpoint(start, peak);
        let points: &[(f64, f64)] = match curve {
            Curve::Linear | Curve::Warmup => &[(0.0, start), (1.0, end)],
            Curve::Cooldown => &[(0.0, start.max(peak)), (1.0, end)],
            Curve::Build => &[(0.0, start), (0.9, peak), (1.0, end)],
            Curve::PeakTime => &[(0.0, start), (peak_at, peak), (1.0, end)],
            Curve::Plateau => &[(0.0, start), (0.2, peak), (0.85, peak), (1.0, end)],
            Curve::Wave => &[
                (0.0, start),
                (0.25, peak),
                (0.45, mid),
                (0.7, peak),
                (1.0, end),
            ],
        };
        interpolate(points, p.clamp(0.0, 1.0))
    }
}

fn interpolate(points: &[(f64, f64)], p: f64) -> f64 {
    points
        .windows(2)
        .find(|w| p <= w[1].0)
        .map_or(points[points.len() - 1].1, |w| {
            let ((x0, y0), (x1, y1)) = (w[0], w[1]);
            if x1 - x0 < 1e-9 {
                y1
            } else {
                y0 + (y1 - y0) * (p - x0) / (x1 - x0)
            }
        })
}

#[derive(Debug)]
struct Shape {
    curve: Curve,
    peak_at: f64,
    energy: Option<Targets>,
    bpm: Option<Targets>,
}

impl Shape {
    fn new(brief: &SessionBrief) -> Self {
        let peak_at = brief
            .peak_at_pct
            .map_or(DEFAULT_PEAK_AT, |p| (p / 100.0).clamp(0.05, 0.95));
        let curve = brief.curve.unwrap_or(if brief.peak_at_pct.is_some() {
            Curve::PeakTime
        } else {
            Curve::Linear
        });
        let energy =
            Targets::new(brief.energy_start, brief.energy_peak, brief.energy_end).map(|t| {
                Targets {
                    start: t.start.clamp(energy::MIN, energy::MAX),
                    peak: t.peak.clamp(energy::MIN, energy::MAX),
                    end: t.end.clamp(energy::MIN, energy::MAX),
                }
            });
        Self {
            curve,
            peak_at,
            energy,
            bpm: Targets::new(brief.bpm_start, brief.bpm_peak, brief.bpm_end),
        }
    }

    fn energy(&self, p: f64) -> Option<f64> {
        self.energy.map(|t| t.at(self.curve, self.peak_at, p))
    }

    fn bpm(&self, p: f64) -> Option<f64> {
        self.bpm.map(|t| t.at(self.curve, self.peak_at, p))
    }
}

#[derive(Debug)]
struct Candidate<'a> {
    row: &'a TrackRow,
    energy: Option<f64>,
    key: Option<MusicalKey>,
    artist: String,
    play_min: f64,
    preference: f64,
}

impl<'a> Candidate<'a> {
    fn new(row: &'a TrackRow, play_min: f64, preference: f64) -> Self {
        Self {
            row,
            energy: energy::estimate(row),
            key: row.musical_key.and_then(MusicalKey::from_value),
            artist: row.artist.trim().to_lowercase(),
            play_min,
            preference,
        }
    }
}

fn lower(words: &[String]) -> Vec<String> {
    words
        .iter()
        .map(|w| w.trim().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

fn mentions(text: &str, words: &[String]) -> bool {
    let text = text.to_lowercase();
    words.iter().any(|w| text.contains(w.as_str()))
}

fn year(release_date: &str) -> Option<i32> {
    let digits: String = release_date.trim().chars().take(4).collect();
    (digits.len() == 4).then(|| digits.parse().ok()).flatten()
}

#[derive(Debug)]
struct Filters {
    genres_in: Vec<String>,
    genres_out: Vec<String>,
    artists_in: Vec<String>,
    artists_out: Vec<String>,
    labels_in: Vec<String>,
    labels_out: Vec<String>,
}

impl Filters {
    fn new(brief: &SessionBrief) -> Self {
        Self {
            genres_in: lower(&brief.genres_include),
            genres_out: lower(&brief.genres_exclude),
            artists_in: lower(&brief.artists_include),
            artists_out: lower(&brief.artists_exclude),
            labels_in: lower(&brief.labels_include),
            labels_out: lower(&brief.labels_exclude),
        }
    }

    fn keeps(&self, brief: &SessionBrief, t: &TrackRow, today: i64) -> bool {
        let text_ok = |text: &str, wanted: &[String], unwanted: &[String]| {
            (wanted.is_empty() || mentions(text, wanted)) && !mentions(text, unwanted)
        };
        let in_range = |value: Option<f64>, low: Option<f64>, high: Option<f64>| {
            (low.is_none() && high.is_none())
                || value.is_some_and(|v| low.is_none_or(|l| v >= l) && high.is_none_or(|h| v <= h))
        };
        let recently_played = brief.skip_played_since_days.is_some_and(|days| {
            parse_stored_date(&t.last_played).is_some_and(|d| today - d < i64::from(days))
        });
        t.sample_type.is_none()
            && t.duration_s.is_none_or(|d| d >= SHORTEST_TRACK_S)
            && text_ok(&t.genre, &self.genres_in, &self.genres_out)
            && text_ok(&t.artist, &self.artists_in, &self.artists_out)
            && text_ok(&t.label, &self.labels_in, &self.labels_out)
            && in_range(t.bpm, brief.bpm_min, brief.bpm_max)
            && in_range(
                year(&t.release_date).map(f64::from),
                brief.release_year_from.map(f64::from),
                brief.release_year_to.map(f64::from),
            )
            && brief.min_rating.is_none_or(|stars| t.rating / 51 >= stars)
            && !recently_played
    }
}

/// Tracks that pass the brief's filters, in library order.
pub fn matching<'a>(brief: &SessionBrief, rows: &'a [TrackRow], today: i64) -> Vec<&'a TrackRow> {
    let filters = Filters::new(brief);
    rows.iter()
        .filter(|t| filters.keeps(brief, t, today))
        .collect()
}

fn candidates<'a, S: BuildHasher>(
    brief: &SessionBrief,
    rows: &'a [TrackRow],
    played: &HashSet<i64, S>,
    pinned: &HashSet<i64>,
    today: i64,
) -> Vec<Candidate<'a>> {
    let filters = Filters::new(brief);
    let mood: Vec<String> = brief
        .mood
        .as_deref()
        .unwrap_or_default()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .map(str::to_lowercase)
        .collect();
    let chosen: Vec<&TrackRow> = rows
        .iter()
        .filter(|t| !played.contains(&t.id))
        .filter(|t| pinned.contains(&t.id) || filters.keeps(brief, t, today))
        .collect();
    let most_played = chosen.iter().map(|t| t.playcount).max().unwrap_or(0).max(1);
    chosen
        .into_iter()
        .map(|t| {
            let full = t.duration_s.map_or(DEFAULT_TRACK_MIN, |d| d / 60.0);
            let play = brief.play_minutes.map_or(full, |m| m.min(full));
            let play_min = (play - brief.transition.overlap_min(t.bpm)).max(1.0);
            let plays = f64::from(t.playcount) / f64::from(most_played);
            let familiarity = match brief.familiarity {
                Familiarity::Mix => 0.0,
                Familiarity::Fresh => plays * 0.8,
                Familiarity::Classics => (1.0 - plays) * 0.8,
            };
            let mood_hits = if mood.is_empty() {
                0.0
            } else {
                let text = [
                    t.genre.as_str(),
                    &t.comment,
                    &t.comment2,
                    &t.title,
                    &t.album,
                    &t.mix,
                ]
                .join(" ")
                .to_lowercase();
                mood.iter().filter(|w| text.contains(w.as_str())).count() as f64 / mood.len() as f64
            };
            let stars = f64::from(t.rating / 51) / 5.0;
            Candidate::new(t, play_min, familiarity - 0.6 * mood_hits - 0.3 * stars)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyFit {
    Unknown,
    Fits,
    Adjacent,
    Clash,
}

fn key_fit(a: Option<MusicalKey>, b: Option<MusicalKey>) -> KeyFit {
    match (a, b) {
        (Some(a), Some(b)) if a.matches(b) => KeyFit::Fits,
        (Some(a), Some(b)) if a.adjacent(b) => KeyFit::Adjacent,
        (Some(_), Some(_)) => KeyFit::Clash,
        _ => KeyFit::Unknown,
    }
}

fn key_cost(rule: Harmonic, fit: KeyFit, same: bool) -> f64 {
    match (rule, fit) {
        (Harmonic::Off, _) => 0.0,
        (_, KeyFit::Fits) if same => 0.0,
        (_, KeyFit::Fits) => 0.2,
        (_, KeyFit::Unknown) => 0.6,
        (Harmonic::Relaxed, KeyFit::Adjacent) => 0.7,
        (Harmonic::Strict, KeyFit::Adjacent) => CLASH,
        (_, KeyFit::Clash) => CLASH * 1.5,
    }
}

#[derive(Debug)]
struct Rules {
    harmonic: Harmonic,
    max_shift: f64,
    artist_gap: usize,
    start_key: Option<MusicalKey>,
}

#[derive(Debug, Clone, Copy)]
struct Step {
    cost: f64,
    jump: bool,
    clash: bool,
}

fn step(
    rules: &Rules,
    shape: &Shape,
    previous: Option<&Candidate>,
    recent_artists: &[&str],
    c: &Candidate,
    p: f64,
) -> Step {
    let energy = match (shape.energy(p), c.energy) {
        (Some(t), Some(e)) => (e - t).abs() / 1.5,
        (Some(_), None) => 1.0,
        (None, _) => 0.0,
    };
    let tempo = match (shape.bpm(p), c.row.bpm.filter(|b| *b > 0.0)) {
        (Some(t), Some(b)) => (b - t).abs() / t / 0.03,
        (Some(_), None) => 1.0,
        (None, _) => 0.0,
    };
    let shift = previous
        .and_then(|prev| prev.row.bpm.filter(|b| *b > 0.0))
        .zip(c.row.bpm.filter(|b| *b > 0.0))
        .map(|(a, b)| bpm_offset(a, b));
    let jump = shift.is_some_and(|s| s > rules.max_shift);
    let shift_cost = match shift {
        Some(s) if s > rules.max_shift => JUMP,
        Some(s) => 0.5 * s / rules.max_shift,
        None => 0.0,
    };
    let previous_key = previous.map_or(rules.start_key, |prev| prev.key);
    let fit = if previous.is_none() && rules.start_key.is_none() {
        KeyFit::Fits
    } else {
        key_fit(previous_key, c.key)
    };
    let clash = rules.harmonic != Harmonic::Off && matches!(fit, KeyFit::Clash)
        || rules.harmonic == Harmonic::Strict && fit == KeyFit::Adjacent;
    let same_key = previous_key.is_some() && previous_key == c.key;
    let repeat = !c.artist.is_empty()
        && recent_artists
            .iter()
            .rev()
            .take(rules.artist_gap)
            .any(|a| *a == c.artist);
    Step {
        cost: energy
            + tempo
            + shift_cost
            + key_cost(rules.harmonic, fit, same_key)
            + if repeat { ARTIST_REPEAT } else { 0.0 }
            + c.preference,
        jump,
        clash,
    }
}

#[derive(Debug, Clone, Default)]
struct Path {
    picks: Vec<usize>,
    progress: Vec<f64>,
    elapsed: f64,
    cost: f64,
}

#[derive(Debug)]
struct Goal {
    total_min: f64,
    count: Option<usize>,
    /// Typical minutes a track plays, so the last one meets the curve's end.
    track_min: f64,
}

impl Goal {
    /// Share of the set (0–1) at which the next track starts: by position
    /// when a track count is asked for, by time otherwise. The last track
    /// lands on the end of the curve.
    fn progress(&self, path: &Path) -> f64 {
        let share = match self.count {
            Some(n) => path.picks.len() as f64 / (n.max(2) - 1) as f64,
            None => path.elapsed / (self.total_min - self.track_min).max(self.track_min),
        };
        share.clamp(0.0, 1.0)
    }

    fn done(&self, path: &Path) -> bool {
        self.count
            .map_or(path.elapsed >= self.total_min - 0.5, |n| {
                path.picks.len() >= n
            })
    }

    fn closing(&self, path: &Path, closer_min: f64) -> bool {
        self.count
            .map_or(path.elapsed + closer_min >= self.total_min, |n| {
                path.picks.len() + 1 >= n
            })
    }
}

/// The share of the set where a pinned track fits the curve best.
fn ideal_progress(shape: &Shape, c: &Candidate, from: f64) -> f64 {
    (0..=50)
        .map(|i| from + (1.0 - from) * f64::from(i) / 50.0)
        .map(|p| {
            let e = shape
                .energy(p)
                .zip(c.energy)
                .map_or(0.0, |(t, e)| (t - e).abs());
            let b = shape
                .bpm(p)
                .zip(c.row.bpm)
                .map_or(0.0, |(t, b)| (t - b).abs() / t * 30.0);
            (p, e + b)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(from, |(p, _)| p)
}

#[derive(Debug)]
struct Planner<'a> {
    pool: Vec<Candidate<'a>>,
    rules: Rules,
    shape: Shape,
    goal: Goal,
    from: f64,
    opener: Option<usize>,
    closer: Option<usize>,
    /// Pinned tracks with the share of the set where each fits best.
    musts: Vec<(f64, usize)>,
    after: Option<Candidate<'a>>,
}

impl Planner<'_> {
    fn progress_at(&self, path: &Path) -> f64 {
        self.from + (1.0 - self.from) * self.goal.progress(path)
    }

    fn reserved(&self, i: usize) -> bool {
        Some(i) == self.opener || Some(i) == self.closer || self.musts.iter().any(|m| m.1 == i)
    }

    /// A pinned track that has to come next, if any.
    fn forced(&self, path: &Path, p: f64) -> Option<usize> {
        let used = |i: usize| path.picks.contains(&i);
        let closer_min = self.closer.map_or(0.0, |c| self.pool[c].play_min);
        if path.picks.is_empty() && self.opener.is_some() {
            self.opener
        } else if self.closer.is_some_and(|c| !used(c)) && self.goal.closing(path, closer_min) {
            self.closer
        } else {
            self.musts
                .iter()
                .find(|(at, i)| !used(*i) && *at <= p)
                .map(|(_, i)| *i)
        }
    }

    fn expand(&self, path: &Path, next: &mut Vec<Path>) {
        let p = self.progress_at(path);
        let forced = self.forced(path, p);
        let previous = path
            .picks
            .last()
            .map(|i| &self.pool[*i])
            .or(self.after.as_ref());
        let artists: Vec<&str> = self
            .after
            .iter()
            .map(|a| a.artist.as_str())
            .chain(path.picks.iter().map(|i| self.pool[*i].artist.as_str()))
            .collect();
        let options = |keep: &dyn Fn(usize) -> bool| -> Vec<(f64, usize)> {
            (0..self.pool.len())
                .filter(|i| !path.picks.contains(i) && keep(*i))
                .map(|i| {
                    let s = step(
                        &self.rules,
                        &self.shape,
                        previous,
                        &artists,
                        &self.pool[i],
                        p,
                    );
                    (s.cost, i)
                })
                .collect()
        };
        let mut options = options(&|i| forced.map_or(!self.reserved(i), |f| f == i));
        if options.is_empty() {
            // Only reserved tracks are left: play them rather than stop.
            options = (0..self.pool.len())
                .filter(|i| !path.picks.contains(i))
                .map(|i| {
                    let s = step(
                        &self.rules,
                        &self.shape,
                        previous,
                        &artists,
                        &self.pool[i],
                        p,
                    );
                    (s.cost, i)
                })
                .collect();
        }
        options.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (cost, i) in options.into_iter().take(BRANCH) {
            let mut picks = path.picks.clone();
            picks.push(i);
            let mut progress = path.progress.clone();
            progress.push(p);
            next.push(Path {
                picks,
                progress,
                elapsed: path.elapsed + self.pool[i].play_min,
                cost: path.cost + cost,
            });
        }
    }

    fn unplaced(&self, path: &Path) -> usize {
        self.musts
            .iter()
            .filter(|m| !path.picks.contains(&m.1))
            .count()
            + usize::from(self.closer.is_some_and(|c| !path.picks.contains(&c)))
    }

    fn search(&self) -> Path {
        let limit = self
            .goal
            .count
            .unwrap_or(self.pool.len())
            .min(self.pool.len());
        let open = |p: &Path| !self.goal.done(p) && p.picks.len() < limit;
        let mut beam = vec![Path::default()];
        while beam.iter().any(open) {
            let mut next = Vec::new();
            for path in &beam {
                if open(path) {
                    self.expand(path, &mut next);
                } else {
                    next.push(path.clone());
                }
            }
            let mean = |p: &Path| p.cost / p.picks.len().max(1) as f64;
            next.sort_by(|a, b| mean(a).total_cmp(&mean(b)).then(a.picks.cmp(&b.picks)));
            next.dedup_by(|a, b| a.picks == b.picks);
            next.truncate(BEAM);
            beam = next;
        }
        beam.into_iter()
            .min_by(|a, b| {
                self.unplaced(a)
                    .cmp(&self.unplaced(b))
                    .then(a.cost.total_cmp(&b.cost))
                    .then(a.picks.cmp(&b.picks))
            })
            .unwrap_or_default()
    }

    fn finish(&self, best: &Path, pinned: &HashSet<i64>, mut warnings: Vec<Warning>) -> SetPlan {
        let wanted_tracks = self.goal.count.unwrap_or(0);
        if wanted_tracks > self.pool.len() {
            warnings.push(Warning::FewCandidates {
                found: self.pool.len(),
                wanted: wanted_tracks,
            });
        }
        if self.goal.count.is_none() && best.elapsed < self.goal.total_min * 0.9 {
            warnings.push(Warning::ShortOfTime {
                planned_min: best.elapsed,
                wanted_min: self.goal.total_min,
            });
        }
        let (mut jumps, mut clashes, mut unknown) = (0, 0, 0);
        let mut tracks = Vec::with_capacity(best.picks.len());
        let mut start_min = self.goal.total_min / (1.0 - self.from) * self.from;
        let mut previous = self.after.as_ref();
        let mut artists: Vec<&str> = self.after.iter().map(|a| a.artist.as_str()).collect();
        for (&i, &p) in best.picks.iter().zip(&best.progress) {
            let c = &self.pool[i];
            let s = step(&self.rules, &self.shape, previous, &artists, c, p);
            jumps += usize::from(s.jump);
            clashes += usize::from(s.clash);
            unknown += usize::from(c.energy.is_none());
            let target_energy = self.shape.energy(p);
            let target_bpm = self.shape.bpm(p);
            tracks.push(PlannedTrack {
                id: c.row.id,
                artist: c.row.artist.clone(),
                title: c.row.title.clone(),
                bpm: c.row.bpm,
                key: c.key.map(MusicalKey::camelot).unwrap_or_default(),
                energy: c.energy,
                start_min,
                play_min: c.play_min,
                target_energy,
                target_bpm,
                reason: reason(c, previous, target_energy, target_bpm, pinned),
            });
            start_min += c.play_min;
            artists.push(&c.artist);
            previous = Some(c);
        }
        if jumps > 0 {
            warnings.push(Warning::TempoJumps { count: jumps });
        }
        if clashes > 0 {
            warnings.push(Warning::KeyClashes { count: clashes });
        }
        if unknown > 0 && self.shape.energy.is_some() {
            warnings.push(Warning::UnknownEnergy { count: unknown });
        }
        SetPlan {
            tracks,
            warnings,
            candidates: self.pool.len(),
            total_min: best.elapsed,
        }
    }
}

pub fn plan<S: BuildHasher>(
    brief: &SessionBrief,
    rows: &[TrackRow],
    played: &HashSet<i64, S>,
    today: i64,
    start: Start,
) -> SetPlan {
    let mut warnings = Vec::new();
    let from = start.from_pct.clamp(0.0, 0.99);
    let wanted_min = brief
        .duration_min
        .filter(|d| *d > 0.0)
        .unwrap_or(DEFAULT_DURATION_MIN);
    let pinned: HashSet<i64> = brief
        .must_play
        .iter()
        .chain(&brief.opener)
        .chain(&brief.closer)
        .copied()
        .collect();
    let pool = candidates(brief, rows, played, &pinned, today);
    let mut missing: Vec<i64> = pinned
        .iter()
        .filter(|id| !pool.iter().any(|c| c.row.id == **id))
        .copied()
        .collect();
    missing.sort_unstable();
    if !missing.is_empty() {
        warnings.push(Warning::NotFound { ids: missing });
    }
    if pool.is_empty() {
        warnings.push(Warning::NoCandidates);
        return SetPlan {
            warnings,
            ..SetPlan::default()
        };
    }
    let start_key = brief.start_key.as_deref().and_then(|text| {
        let key = MusicalKey::parse(text.trim());
        if key.is_none() {
            warnings.push(Warning::UnknownKey { text: text.into() });
        }
        key
    });
    let shape = Shape::new(brief);
    let index = |id: Option<i64>| id.and_then(|id| pool.iter().position(|c| c.row.id == id));
    let opener = index(brief.opener);
    let closer = index(brief.closer).filter(|c| Some(*c) != opener);
    let mut musts: Vec<(f64, usize)> = brief
        .must_play
        .iter()
        .filter_map(|id| index(Some(*id)))
        .filter(|i| Some(*i) != opener && Some(*i) != closer)
        .map(|i| (ideal_progress(&shape, &pool[i], from), i))
        .collect();
    musts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    musts.dedup_by_key(|m| m.1);
    let after = start
        .after
        .and_then(|id| rows.iter().find(|t| t.id == id))
        .map(|row| Candidate::new(row, 0.0, 0.0));
    let planner = Planner {
        rules: Rules {
            harmonic: brief.harmonic,
            max_shift: brief
                .max_tempo_shift_pct
                .filter(|p| *p > 0.0)
                .map_or(DEFAULT_MAX_SHIFT, |p| p / 100.0),
            artist_gap: brief.no_artist_repeat_within.unwrap_or(DEFAULT_ARTIST_GAP),
            start_key,
        },
        shape,
        goal: Goal {
            total_min: wanted_min * (1.0 - from),
            count: brief.track_count.filter(|n| *n > 0),
            track_min: pool.iter().map(|c| c.play_min).sum::<f64>() / pool.len() as f64,
        },
        from,
        opener,
        closer,
        musts,
        after,
        pool,
    };
    let best = planner.search();
    planner.finish(&best, &pinned, warnings)
}

fn reason(
    c: &Candidate,
    previous: Option<&Candidate>,
    target_energy: Option<f64>,
    target_bpm: Option<f64>,
    pinned: &HashSet<i64>,
) -> String {
    let mut parts = Vec::new();
    if pinned.contains(&c.row.id) {
        parts.push("requested".to_owned());
    }
    match (c.energy, target_energy) {
        (Some(e), Some(t)) => parts.push(format!("energy {e:.1} (target {t:.1})")),
        (None, Some(t)) => parts.push(format!("energy unknown (target {t:.1})")),
        _ => {}
    }
    match (c.row.bpm, target_bpm) {
        (Some(b), Some(t)) => parts.push(format!("{b:.1} BPM (target {t:.0})")),
        (Some(b), None) => parts.push(format!("{b:.1} BPM")),
        _ => {}
    }
    if let Some(prev) = previous {
        let fit = match key_fit(prev.key, c.key) {
            KeyFit::Fits => "key fits",
            KeyFit::Adjacent => "key a semitone away",
            KeyFit::Clash => "key clashes",
            KeyFit::Unknown => "key unknown",
        };
        parts.push(fit.to_owned());
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: u8 = 0;
    const G: u8 = 7;
    const D: u8 = 2;
    const C_SHARP: u8 = 1;
    const A_MINOR: u8 = 21;
    const F_SHARP: u8 = 6;

    fn track(id: i64, bpm: f64, key: u8, energy: u8) -> TrackRow {
        TrackRow {
            id,
            artist: format!("Artist {id}"),
            title: format!("Title {id}"),
            genre: "House".into(),
            bpm: Some(bpm),
            musical_key: Some(key),
            duration_s: Some(360.0),
            comment: format!("Energy {energy}"),
            ..TrackRow::default()
        }
    }

    fn ids(plan: &SetPlan) -> Vec<i64> {
        plan.tracks.iter().map(|t| t.id).collect()
    }

    fn run(brief: &SessionBrief, rows: &[TrackRow]) -> SetPlan {
        plan(brief, rows, &HashSet::new(), 0, Start::default())
    }

    fn library() -> Vec<TrackRow> {
        (1..=40)
            .map(|i| {
                let energy = (i % 10 + 1) as u8;
                let bpm = 118.0 + f64::from(i % 12);
                let key = [C, G, D, A_MINOR][(i % 4) as usize];
                track(i64::from(i), bpm, key, energy)
            })
            .collect()
    }

    #[test]
    fn curves_pass_through_their_points() {
        let t = Targets::new(Some(3.0), Some(8.0), Some(5.0)).unwrap();
        assert!((t.at(Curve::PeakTime, 0.5, 0.0) - 3.0).abs() < 1e-9);
        assert!((t.at(Curve::PeakTime, 0.5, 0.5) - 8.0).abs() < 1e-9);
        assert!((t.at(Curve::PeakTime, 0.5, 0.75) - 6.5).abs() < 1e-9);
        assert!((t.at(Curve::PeakTime, 0.5, 1.0) - 5.0).abs() < 1e-9);
        assert!((t.at(Curve::Linear, 0.5, 0.5) - 4.0).abs() < 1e-9);
        assert!((t.at(Curve::Plateau, 0.5, 0.5) - 8.0).abs() < 1e-9);
        let only_end = Targets::new(None, None, Some(6.0)).unwrap();
        assert_eq!(
            (only_end.start, only_end.peak, only_end.end),
            (6.0, 6.0, 6.0)
        );
        assert_eq!(Targets::new(None, None, None), None);
    }

    #[test]
    fn energy_follows_the_curve() {
        let brief = SessionBrief {
            track_count: Some(8),
            energy_start: Some(2.0),
            energy_peak: Some(9.0),
            energy_end: Some(5.0),
            curve: Some(Curve::PeakTime),
            peak_at_pct: Some(70.0),
            harmonic: Harmonic::Off,
            ..SessionBrief::default()
        };
        let plan = run(&brief, &library());
        assert_eq!(plan.tracks.len(), 8);
        for t in &plan.tracks {
            let gap = (t.energy.unwrap() - t.target_energy.unwrap()).abs();
            assert!(gap <= 1.0, "{t:?}");
        }
        let energies: Vec<f64> = plan.tracks.iter().map(|t| t.energy.unwrap()).collect();
        let peak = energies.iter().copied().fold(0.0, f64::max);
        assert!(energies[0] <= 3.0 && peak >= 8.0, "{energies:?}");
    }

    #[test]
    fn the_last_track_meets_the_end_of_the_curve() {
        let brief = SessionBrief {
            track_count: Some(3),
            energy_start: Some(7.0),
            energy_end: Some(8.5),
            harmonic: Harmonic::Off,
            ..SessionBrief::default()
        };
        let plan = run(&brief, &library());
        let targets: Vec<f64> = plan
            .tracks
            .iter()
            .map(|t| t.target_energy.unwrap())
            .collect();
        assert_eq!(targets, [7.0, 7.75, 8.5]);
        let timed = SessionBrief {
            duration_min: Some(30.0),
            energy_start: Some(2.0),
            energy_end: Some(9.0),
            transition: Transition {
                style: TransitionStyle::Cut,
                bars: 16,
            },
            ..SessionBrief::default()
        };
        let plan = run(&timed, &library());
        let last = plan.tracks.last().unwrap().target_energy.unwrap();
        assert!(last > 8.9, "{last}");
    }

    #[test]
    fn duration_fills_the_time_with_overlaps() {
        let brief = SessionBrief {
            duration_min: Some(30.0),
            transition: Transition {
                style: TransitionStyle::Cut,
                bars: 16,
            },
            ..SessionBrief::default()
        };
        let plan = run(&brief, &library());
        assert_eq!(plan.tracks.len(), 5, "six-minute tracks fill 30 minutes");
        assert!(
            (plan.total_min - 30.0).abs() < 0.2,
            "a cut overlaps one bar"
        );
        let second = plan.tracks[1].start_min;
        assert!((plan.tracks[2].start_min - 2.0 * second).abs() < 1e-9);
        let blend = SessionBrief {
            duration_min: Some(30.0),
            ..SessionBrief::default()
        };
        let plan = run(&blend, &library());
        assert_eq!(plan.tracks.len(), 6, "blends overlap, so one more track");
    }

    #[test]
    fn strict_harmony_keeps_keys_on_the_wheel() {
        let rows: Vec<TrackRow> = [C, G, D, F_SHARP, C_SHARP, A_MINOR, G, D]
            .iter()
            .enumerate()
            .map(|(i, &k)| track(i as i64 + 1, 124.0, k, 5))
            .collect();
        let brief = SessionBrief {
            track_count: Some(5),
            harmonic: Harmonic::Strict,
            start_key: Some("C".into()),
            ..SessionBrief::default()
        };
        let plan = run(&brief, &rows);
        assert!(!plan.warnings.contains(&Warning::KeyClashes { count: 1 }));
        let keys: Vec<Option<MusicalKey>> = plan
            .tracks
            .iter()
            .map(|t| MusicalKey::parse(&t.key))
            .collect();
        for pair in keys.windows(2) {
            assert!(pair[0].unwrap().matches(pair[1].unwrap()), "{keys:?}");
        }
        assert!(!ids(&plan).contains(&4) && !ids(&plan).contains(&5));
    }

    #[test]
    fn tempo_moves_in_small_steps() {
        let rows: Vec<TrackRow> = [120.0, 121.0, 140.0, 122.5, 124.0, 150.0, 125.0]
            .iter()
            .enumerate()
            .map(|(i, &b)| track(i as i64 + 1, b, C, 5))
            .collect();
        let brief = SessionBrief {
            track_count: Some(5),
            bpm_start: Some(120.0),
            bpm_end: Some(125.0),
            max_tempo_shift_pct: Some(3.0),
            ..SessionBrief::default()
        };
        let plan = run(&brief, &rows);
        assert_eq!(ids(&plan), [1, 2, 4, 5, 7]);
        assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    }

    #[test]
    fn artists_do_not_repeat_close_together() {
        let mut rows = library();
        for t in &mut rows {
            t.artist = if t.id % 2 == 0 { "Same" } else { "Other" }.into();
        }
        rows.push(TrackRow {
            artist: "Third".into(),
            ..track(99, 124.0, C, 5)
        });
        let brief = SessionBrief {
            track_count: Some(6),
            no_artist_repeat_within: Some(1),
            harmonic: Harmonic::Off,
            ..SessionBrief::default()
        };
        let plan = run(&brief, &rows);
        for pair in plan.tracks.windows(2) {
            assert_ne!(pair[0].artist, pair[1].artist, "{:?}", ids(&plan));
        }
    }

    #[test]
    fn pinned_tracks_are_placed() {
        let brief = SessionBrief {
            track_count: Some(6),
            energy_start: Some(2.0),
            energy_end: Some(9.0),
            opener: Some(5),
            closer: Some(9),
            must_play: vec![12, 404],
            harmonic: Harmonic::Off,
            ..SessionBrief::default()
        };
        let plan = run(&brief, &library());
        let ids = ids(&plan);
        assert_eq!(ids.len(), 6);
        assert_eq!(ids[0], 5);
        assert_eq!(ids[5], 9);
        assert!(ids.contains(&12));
        assert_eq!(plan.warnings[0], Warning::NotFound { ids: vec![404] });
    }

    #[test]
    fn filters_narrow_the_pool() {
        let mut rows = library();
        rows[0].genre = "Techno".into();
        rows[1].genre = "Deep House".into();
        rows[1].release_date = "1996/4/1".into();
        rows[2].rating = 255;
        rows[3].last_played = "1970/1/5".into();
        rows[4].sample_type = Some(1);
        let pool = |brief: &SessionBrief, today: i64| {
            candidates(brief, &rows, &HashSet::<i64>::new(), &HashSet::new(), today).len()
        };
        let genre = SessionBrief {
            genres_exclude: vec!["techno".into()],
            ..SessionBrief::default()
        };
        assert_eq!(pool(&genre, 0), 38, "samples and techno are out");
        let deep = SessionBrief {
            genres_include: vec!["Deep".into()],
            ..SessionBrief::default()
        };
        assert_eq!(pool(&deep, 0), 1);
        let nineties = SessionBrief {
            release_year_from: Some(1990),
            release_year_to: Some(1999),
            ..SessionBrief::default()
        };
        assert_eq!(pool(&nineties, 0), 1);
        let rated = SessionBrief {
            min_rating: Some(4),
            ..SessionBrief::default()
        };
        assert_eq!(pool(&rated, 0), 1);
        let fresh = SessionBrief {
            skip_played_since_days: Some(7),
            ..SessionBrief::default()
        };
        assert_eq!(pool(&fresh, 6), 38, "played 2 days ago");
        assert_eq!(pool(&fresh, 20), 39, "played long ago");
        let tempo = SessionBrief {
            bpm_min: Some(120.0),
            bpm_max: Some(121.0),
            ..SessionBrief::default()
        };
        assert_eq!(pool(&tempo, 0), 8);
    }

    #[test]
    fn played_tracks_and_empty_pools_are_reported() {
        let rows = library();
        let played: HashSet<i64> = rows.iter().map(|t| t.id).collect();
        let plan = plan(
            &SessionBrief::default(),
            &rows,
            &played,
            0,
            Start::default(),
        );
        assert_eq!(plan.tracks, []);
        assert_eq!(plan.warnings, [Warning::NoCandidates]);
        let few = run(
            &SessionBrief {
                track_count: Some(10),
                ..SessionBrief::default()
            },
            &rows[..3],
        );
        assert_eq!(few.tracks.len(), 3);
        assert!(few.warnings.contains(&Warning::FewCandidates {
            found: 3,
            wanted: 10
        }));
    }

    #[test]
    fn replanning_continues_after_the_playing_track() {
        let rows = library();
        let brief = SessionBrief {
            duration_min: Some(60.0),
            energy_start: Some(2.0),
            energy_end: Some(9.0),
            harmonic: Harmonic::Strict,
            ..SessionBrief::default()
        };
        let played = HashSet::from([1, 2, 3]);
        let rest = plan(
            &brief,
            &rows,
            &played,
            0,
            Start {
                after: Some(3),
                from_pct: 0.5,
            },
        );
        assert!(!ids(&rest).iter().any(|id| played.contains(id)));
        assert!((rest.tracks[0].start_min - 30.0).abs() < 1e-9);
        assert!(rest.tracks[0].target_energy.unwrap() >= 5.5);
        let first_key = MusicalKey::parse(&rest.tracks[0].key).unwrap();
        let playing = MusicalKey::from_value(rows[2].musical_key.unwrap()).unwrap();
        assert!(playing.matches(first_key));
    }

    #[test]
    fn brief_reads_partial_json() {
        let brief: SessionBrief = serde_json::from_str(
            r#"{"duration_min": 45, "harmonic": "strict", "curve": "peak_time",
                "genres_include": ["tech house"], "transition": {"bars": 32},
                "unknown_field": true}"#,
        )
        .unwrap();
        assert_eq!(brief.duration_min, Some(45.0));
        assert_eq!(brief.harmonic, Harmonic::Strict);
        assert_eq!(brief.curve, Some(Curve::PeakTime));
        assert_eq!(brief.transition.bars, 32);
        assert_eq!(brief.transition.style, TransitionStyle::Blend);
    }

    #[test]
    fn warnings_read_as_text() {
        assert_eq!(
            Warning::FewCandidates {
                found: 6,
                wanted: 14
            }
            .to_string(),
            "only 6 tracks match the filters, 14 wanted"
        );
    }
}
