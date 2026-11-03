//! Screens built around what the DJ reads in the booth: where the beat is in
//! the phrase, how far the next cue is, how the other deck lines up, and the
//! stems under the fingers. Rules are pure functions with tests below.

#![allow(
    clippy::many_single_char_names,
    reason = "drawing code: c canvas, t theme, f fonts, s deck state, as in the other layouts"
)]

use backspn_engine::{DeckState, HOTCUES};
use backspn_formats::nml::MusicalKey;
use eframe::egui::Color32;

use super::{
    Canvas, Partner, ScreenDeck, StemOverlay, WIDTH, chip, deck_color, draw_band_overview,
    draw_zoomed_in, fonts, remaining, text, text_right, text_width, title,
};
use crate::theme::Theme;
use crate::waveform::WaveStyle;

pub const BEATS_PER_BAR: i64 = 4;
/// Bars in a phrase as the counter shows it: four groups of four bars.
pub const PHRASE_BARS: i64 = 16;

/// Where a beat falls: its beat in the bar and bar in the phrase, both from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeatPos {
    pub beat: u8,
    pub bar: u8,
    pub phrase: i64,
}

pub fn beat_pos(beat: f64) -> BeatPos {
    let n = beat.floor() as i64;
    let bars = n.div_euclid(BEATS_PER_BAR);
    BeatPos {
        beat: (n.rem_euclid(BEATS_PER_BAR) + 1) as u8,
        bar: (bars.rem_euclid(PHRASE_BARS) + 1) as u8,
        phrase: bars.div_euclid(PHRASE_BARS) + 1,
    }
}

/// Beats from the playhead to a time on the track, on the track's grid.
pub fn beats_to(s: &DeckState, to_s: f64) -> Option<f64> {
    let grid = s.grid?;
    Some(grid.beat_at(to_s) - grid.beat_at(s.position_s))
}

/// The first hotcue ahead of the playhead: its number from 1 and its time.
pub fn next_cue(s: &DeckState) -> Option<(usize, f64)> {
    const AHEAD_S: f64 = 0.05;
    s.hotcues
        .iter()
        .enumerate()
        .filter_map(|(n, h)| h.map(|h| (n + 1, h.start_s)))
        .filter(|&(_, at)| at > s.position_s + AHEAD_S)
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// A countdown in whole bars and beats, rounded down to the beat: "8.2" is
/// eight bars and two beats.
pub fn bars_beats(beats: f64) -> String {
    let n = beats.max(0.0).floor() as i64;
    format!("{}.{}", n / BEATS_PER_BAR, n % BEATS_PER_BAR)
}

/// How far this deck's beat is ahead of the other's, in beats from −0.5 to 0.5.
pub fn phase_offset(a: &DeckState, b: &DeckState) -> Option<f64> {
    let fa = a.grid?.beat_at(a.position_s);
    let fb = b.grid?.beat_at(b.position_s);
    let d = (fa - fb).rem_euclid(1.0);
    Some(if d >= 0.5 { d - 1.0 } else { d })
}

/// How the two keys sit together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFit {
    Same,
    Harmonic,
    Semitone,
    Clash,
}

impl KeyFit {
    pub fn of(a: MusicalKey, b: MusicalKey) -> Self {
        if a == b {
            Self::Same
        } else if a.matches(b) {
            Self::Harmonic
        } else if a.adjacent(b) {
            Self::Semitone
        } else {
            Self::Clash
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Same => "SAME KEY",
            Self::Harmonic => "HARMONIC",
            Self::Semitone => "±1 SEMITONE",
            Self::Clash => "CLASH",
        }
    }

    fn colour(self, t: &Theme) -> Color32 {
        match self {
            Self::Same | Self::Harmonic => t.play,
            Self::Semitone => t.cue,
            Self::Clash => t.danger,
        }
    }
}

/// The deck the screen compares with: the tempo master when it is another
/// deck, else a loaded deck on the other side, playing ones first.
pub fn partner_of(
    deck: usize,
    master: Option<usize>,
    loaded: &[bool],
    playing: &[bool],
) -> Option<usize> {
    if let Some(m) = master
        && m != deck
        && loaded.get(m).copied().unwrap_or(false)
    {
        return Some(m);
    }
    let others = (0..loaded.len()).filter(|&d| d != deck && loaded[d]);
    others.clone().min_by_key(|&d| {
        let opposite = d % 2 != deck % 2;
        (!playing.get(d).copied().unwrap_or(false), !opposite, d)
    })
}

fn bpm_text(s: &DeckState, digits: usize) -> String {
    s.effective_bpm()
        .map_or_else(|| "—".to_owned(), |b| format!("{b:.digits$}"))
}

fn header(c: &mut Canvas, deck: &ScreenDeck, t: &Theme, height: i32) {
    let f = fonts();
    let colour = deck_color(deck.index, t);
    c.rect((0, 0), (WIDTH as i32, height), t.surface);
    c.rect((0, 0), (height, height), colour);
    let letter = char::from(b'A' + deck.index as u8).to_string();
    let w = text_width(&f.sans, 18.0, &letter);
    text(
        c,
        &f.sans,
        18.0,
        ((height as f32 - w) / 2.0, 1.0),
        w + 1.0,
        Theme::on(colour),
        &letter,
    );
    let shown = if deck.state.loaded {
        title(deck)
    } else {
        "Empty"
    };
    let ink = if deck.state.loaded {
        t.text
    } else {
        t.text_dim
    };
    text(
        c,
        &f.sans,
        15.0,
        (height as f32 + 6.0, 3.0),
        180.0,
        ink,
        shown,
    );
    let time_colour = if deck.end_warning { t.danger } else { t.text };
    text_right(
        c,
        &f.mono,
        15.0,
        (WIDTH as f32 - 6.0, 4.0),
        time_colour,
        &remaining(deck.state),
    );
}

/// Four boxes for the beats of the bar, the current one lit.
fn beat_boxes(
    c: &mut Canvas,
    (left, top): (i32, i32),
    (w, h): (i32, i32),
    on: u8,
    lit: Color32,
    t: &Theme,
) {
    let gap = 6;
    let cell = (w - gap * 3) / 4;
    for i in 0..4u8 {
        let x = left + i32::from(i) * (cell + gap);
        let fill = if i + 1 == on { lit } else { t.well };
        c.rect((x, top), (cell, h), fill);
    }
}

/// Sixteen bars in four groups, the bars played filled and the current one lit.
fn phrase_bars(
    c: &mut Canvas,
    (left, top): (i32, i32),
    w: i32,
    pos: BeatPos,
    lit: Color32,
    t: &Theme,
) {
    let (gap, group_gap, h) = (3, 9, 14);
    let cell = (w - gap * 12 - group_gap * 3) / 16;
    let mut x = left;
    for bar in 1..=16u8 {
        let fill = match bar.cmp(&pos.bar) {
            std::cmp::Ordering::Less => t.text_faint,
            std::cmp::Ordering::Equal => lit,
            std::cmp::Ordering::Greater => t.well,
        };
        c.rect((x, top), (cell, h), fill);
        x += cell + if bar % 4 == 0 { group_gap } else { gap };
    }
}

fn countdown(
    c: &mut Canvas,
    (left, top): (i32, i32),
    label: &str,
    value: &str,
    warn: bool,
    t: &Theme,
) {
    let f = fonts();
    text(
        c,
        &f.mono,
        12.0,
        (left as f32, top as f32),
        140.0,
        t.text_faint,
        label,
    );
    let ink = if warn { t.danger } else { t.text };
    text(
        c,
        &f.mono,
        30.0,
        (left as f32, top as f32 + 14.0),
        150.0,
        ink,
        value,
    );
}

/// Beat in the bar, bar in the phrase, and the bars to the next cue and to the end.
pub(super) fn render_phrase(deck: &ScreenDeck, style: &WaveStyle, t: &Theme) -> Canvas {
    let f = fonts();
    let s = deck.state;
    let mut c = Canvas::new(t.base);
    header(&mut c, deck, t, 24);
    let lit = deck_color(deck.index, t);
    let Some(grid) = s.grid.filter(|_| s.loaded) else {
        let msg = if s.loaded {
            "No beat grid"
        } else {
            "Load a track"
        };
        let w = text_width(&f.sans, 20.0, msg);
        text(
            &mut c,
            &f.sans,
            20.0,
            ((WIDTH as f32 - w) / 2.0, 100.0),
            w + 1.0,
            t.text_dim,
            msg,
        );
        return c;
    };
    let pos = beat_pos(grid.beat_at(s.position_s));
    beat_boxes(&mut c, (10, 32), (300, 40), pos.beat, lit, t);
    let label = format!("BAR {} / {}", pos.bar, PHRASE_BARS);
    text(&mut c, &f.mono, 22.0, (10.0, 80.0), 200.0, t.text, &label);
    text_right(
        &mut c,
        &f.mono,
        14.0,
        (310.0, 86.0),
        t.text_dim,
        &format!("PHRASE {}", pos.phrase),
    );
    phrase_bars(&mut c, (10, 110), 300, pos, lit, t);
    match next_cue(s).and_then(|(n, at)| beats_to(s, at).map(|b| (n, b))) {
        Some((n, beats)) => countdown(
            &mut c,
            (10, 136),
            &format!("TO CUE {n}"),
            &bars_beats(beats),
            beats < 8.0,
            t,
        ),
        None => countdown(&mut c, (10, 136), "TO CUE", "—", false, t),
    }
    let to_end = beats_to(s, s.duration_s).unwrap_or(0.0);
    countdown(
        &mut c,
        (170, 136),
        "TO END",
        &bars_beats(to_end),
        deck.end_warning,
        t,
    );
    text(
        &mut c,
        &f.mono,
        10.0,
        (10.0, 186.0),
        300.0,
        t.text_faint,
        "BARS.BEATS",
    );
    draw_band_overview(&mut c, deck, style, t, (204, 232));
    c
}

/// This deck against the other one: tempo, key and beat phase, with both
/// waveforms stacked so the beats can be lined up by eye.
pub(super) fn render_mix(deck: &ScreenDeck, style: &WaveStyle, t: &Theme) -> Canvas {
    let f = fonts();
    let s = deck.state;
    let mut c = Canvas::new(t.base);
    header(&mut c, deck, t, 24);
    let window = deck.zoom_s;
    draw_zoomed_in(
        &mut c,
        s,
        deck.peaks,
        style,
        t,
        window,
        (0, WIDTH as i32),
        (28, 92),
    );
    let Some(p) = &deck.partner else {
        let msg = "No other deck loaded";
        let w = text_width(&f.sans, 18.0, msg);
        text(
            &mut c,
            &f.sans,
            18.0,
            ((WIDTH as f32 - w) / 2.0, 140.0),
            w + 1.0,
            t.text_dim,
            msg,
        );
        return c;
    };
    draw_zoomed_in(
        &mut c,
        p.state,
        p.peaks,
        style,
        t,
        window,
        (0, WIDTH as i32),
        (96, 160),
    );
    let other = deck_color(p.index, t);
    c.rect((0, 96), (4, 64), other);
    let letter = char::from(b'A' + p.index as u8).to_string();
    text(&mut c, &f.mono, 12.0, (7.0, 97.0), 20.0, other, &letter);
    mix_readouts(&mut c, deck, p, t, 166, [10, 120, 226]);
    c
}

/// Tempo and phase against the other deck and how the keys fit, in three
/// columns starting at `cols`.
pub(super) fn mix_readouts(
    c: &mut Canvas,
    deck: &ScreenDeck,
    p: &Partner,
    t: &Theme,
    top: i32,
    cols: [i32; 3],
) {
    let f = fonts();
    let s = deck.state;
    let (y, cap) = (top as f32 + 16.0, top as f32);
    let [a, b, k] = cols.map(|x| x as f32);
    text(c, &f.mono, 11.0, (a, cap), 100.0, t.text_faint, "TEMPO");
    let tempo = match (s.effective_bpm(), p.state.effective_bpm()) {
        (Some(a), Some(b)) => {
            let d = a - b;
            let ink = if d.abs() < 0.05 { t.play } else { t.text };
            (format!("{d:+.2}"), ink)
        }
        _ => ("—".to_owned(), t.text_dim),
    };
    text(c, &f.mono, 26.0, (a, y), 110.0, tempo.1, &tempo.0);
    text(c, &f.mono, 11.0, (b, cap), 100.0, t.text_faint, "PHASE");
    let phase = phase_offset(s, p.state).map_or_else(
        || ("—".to_owned(), t.text_dim),
        |d| {
            let ink = if d.abs() < 0.03 { t.play } else { t.text };
            (format!("{d:+.2}"), ink)
        },
    );
    text(c, &f.mono, 26.0, (b, y), 100.0, phase.1, &phase.0);
    let mine = deck.key.and_then(MusicalKey::parse);
    text(c, &f.mono, 11.0, (k, cap), 90.0, t.text_faint, "KEY");
    match (mine, p.key) {
        (Some(a), Some(b)) => {
            let fit = KeyFit::of(a, b);
            let pair = format!("{}→{}", a.camelot(), b.camelot());
            text(c, &f.mono, 16.0, (k, y), 90.0, t.text, &pair);
            chip(c, (cols[2], top + 40), fit.label(), true, fit.colour(t), t);
        }
        _ => {
            text(c, &f.mono, 16.0, (k, y), 90.0, t.text_dim, "—");
        }
    }
    text(
        c,
        &f.mono,
        10.0,
        (a, top as f32 + 48.0),
        210.0,
        t.text_faint,
        "BPM vs other · beats ahead",
    );
}

/// The hotcues in order with their time and the bars from the playhead.
pub(super) fn render_cues(deck: &ScreenDeck, t: &Theme) -> Canvas {
    let f = fonts();
    let s = deck.state;
    let mut c = Canvas::new(t.base);
    header(&mut c, deck, t, 24);
    let next = next_cue(s).map(|(n, _)| n);
    let row_h = 26;
    for n in 0..HOTCUES {
        let top = 28 + n as i32 * row_h;
        let Some(h) = s.hotcues[n] else {
            c.rect((6, top), (30, row_h - 4), t.well);
            text(
                &mut c,
                &f.mono,
                16.0,
                (14.0, top as f32 + 2.0),
                20.0,
                t.text_faint,
                &(n + 1).to_string(),
            );
            continue;
        };
        let is_loop = h.loop_s.is_some();
        let fill = if is_loop { t.play } else { t.accent };
        c.rect((6, top), (30, row_h - 4), fill);
        text(
            &mut c,
            &f.mono,
            16.0,
            (14.0, top as f32 + 2.0),
            20.0,
            Theme::on(fill),
            &(n + 1).to_string(),
        );
        if Some(n + 1) == next {
            c.rect((40, top), (4, row_h - 4), deck_color(deck.index, t));
        }
        let kind = if is_loop { "LOOP" } else { "CUE" };
        text(
            &mut c,
            &f.mono,
            12.0,
            (50.0, top as f32 + 5.0),
            50.0,
            t.text_dim,
            kind,
        );
        text(
            &mut c,
            &f.mono,
            17.0,
            (96.0, top as f32 + 2.0),
            100.0,
            t.text,
            &crate::deck_view::clock(h.start_s),
        );
        let ahead = beats_to(s, h.start_s);
        let rel = match ahead {
            Some(b) if b >= 0.0 => format!("in {}", bars_beats(b)),
            Some(b) => format!("-{}", bars_beats(-b)),
            None => String::new(),
        };
        let ink = if Some(n + 1) == next {
            t.text
        } else {
            t.text_dim
        };
        text_right(&mut c, &f.mono, 17.0, (314.0, top as f32 + 2.0), ink, &rel);
    }
    c
}

fn stem_column(
    c: &mut Canvas,
    (left, top): (i32, i32),
    (w, h): (i32, i32),
    stem: &StemOverlay,
    big: bool,
    t: &Theme,
) {
    let f = fonts();
    let on = !stem.muted;
    let colour = if on {
        stem.colour
    } else {
        stem.colour.gamma_multiply(0.35)
    };
    c.rect((left, top), (w, 4), colour);
    let name_size = if big { 16.0 } else { 13.0 };
    let ink = if on { t.text } else { t.text_dim };
    text(
        c,
        &f.sans,
        name_size,
        (left as f32 + 4.0, top as f32 + 6.0),
        w as f32 - 8.0,
        ink,
        &stem.name,
    );
    let bar_top = top + if big { 28 } else { 24 };
    let bar_h = h - (bar_top - top) - if big { 26 } else { 18 };
    c.rect((left + 4, bar_top), (w - 8, bar_h), t.well);
    let filled = (bar_h as f32 * stem.volume.clamp(0.0, 1.0)).round() as i32;
    c.rect(
        (left + 4, bar_top + bar_h - filled),
        (w - 8, filled),
        colour,
    );
    let bottom = (top + h - if big { 22 } else { 16 }) as f32;
    let small = if big { 13.0 } else { 11.0 };
    if stem.muted {
        text(
            c,
            &f.mono,
            small,
            (left as f32 + 4.0, bottom),
            w as f32,
            t.danger,
            "MUTE",
        );
    } else {
        let pct = format!("{:.0}", stem.volume.clamp(0.0, 1.0) * 100.0);
        text(
            c,
            &f.mono,
            small,
            (left as f32 + 4.0, bottom),
            w as f32,
            t.text_dim,
            &pct,
        );
    }
    let filter = if stem.filter_on {
        let v = (stem.filter - 0.5) * 2.0;
        if v.abs() < 0.02 {
            "F 0".to_owned()
        } else if v < 0.0 {
            format!("LP{:.0}", -v * 100.0)
        } else {
            format!("HP{:.0}", v * 100.0)
        }
    } else {
        "F off".to_owned()
    };
    let fink = if stem.filter_on { t.text } else { t.text_faint };
    text_right(
        c,
        &f.mono,
        small,
        ((left + w - 4) as f32, bottom),
        fink,
        &filter,
    );
}

/// Four stem columns: level, mute and filter of each.
pub(super) fn render_stems(deck: &ScreenDeck, style: &WaveStyle, t: &Theme) -> Canvas {
    let f = fonts();
    let mut c = Canvas::new(t.base);
    header(&mut c, deck, t, 24);
    if let Some(stems) = &deck.stems {
        let w = (WIDTH as i32 - 10) / 4;
        for (i, stem) in stems.iter().enumerate() {
            stem_column(&mut c, (5 + i as i32 * w, 30), (w - 4, 168), stem, true, t);
        }
    } else {
        let msg = if deck.state.loaded {
            "No stems for this track"
        } else {
            "Load a track"
        };
        let w = text_width(&f.sans, 18.0, msg);
        text(
            &mut c,
            &f.sans,
            18.0,
            ((WIDTH as f32 - w) / 2.0, 100.0),
            w + 1.0,
            t.text_dim,
            msg,
        );
    }
    let bpm = bpm_text(deck.state, 1);
    text(&mut c, &f.mono, 12.0, (10.0, 202.0), 80.0, t.text_dim, &bpm);
    draw_band_overview(&mut c, deck, style, t, (218, 234));
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use backspn_engine::{CueKind, HotcuePoint};

    fn grid(bpm: f64) -> backspn_engine::Grid {
        backspn_engine::Grid {
            bpm,
            anchor_s: 0.0,
            ..backspn_engine::Grid::FIXED
        }
    }

    #[test]
    fn beats_fall_in_their_bar_and_phrase() {
        assert_eq!(
            beat_pos(0.0),
            BeatPos {
                beat: 1,
                bar: 1,
                phrase: 1
            }
        );
        assert_eq!(
            beat_pos(3.9),
            BeatPos {
                beat: 4,
                bar: 1,
                phrase: 1
            }
        );
        assert_eq!(
            beat_pos(4.0),
            BeatPos {
                beat: 1,
                bar: 2,
                phrase: 1
            }
        );
        assert_eq!(
            beat_pos(63.0),
            BeatPos {
                beat: 4,
                bar: 16,
                phrase: 1
            }
        );
        assert_eq!(
            beat_pos(64.0),
            BeatPos {
                beat: 1,
                bar: 1,
                phrase: 2
            }
        );
        assert_eq!(
            beat_pos(-1.0),
            BeatPos {
                beat: 4,
                bar: 16,
                phrase: 0
            },
            "before the grid starts"
        );
    }

    #[test]
    fn countdowns_read_in_bars_and_beats() {
        assert_eq!(bars_beats(34.7), "8.2");
        assert_eq!(bars_beats(0.4), "0.0");
        assert_eq!(bars_beats(-3.0), "0.0");
    }

    #[test]
    fn the_next_cue_is_the_first_ahead() {
        let mut s = DeckState {
            position_s: 10.0,
            ..DeckState::default()
        };
        let cue = |at| {
            Some(HotcuePoint {
                start_s: at,
                kind: CueKind::Cue,
                loop_s: None,
            })
        };
        s.hotcues[0] = cue(5.0);
        s.hotcues[3] = cue(40.0);
        s.hotcues[5] = cue(20.0);
        assert_eq!(next_cue(&s), Some((6, 20.0)));
        s.position_s = 50.0;
        assert_eq!(next_cue(&s), None);
    }

    #[test]
    fn beats_and_phase_follow_the_grid() {
        let a = DeckState {
            grid: Some(grid(120.0)),
            position_s: 1.25,
            ..DeckState::default()
        };
        assert!((beats_to(&a, 3.25).unwrap() - 4.0).abs() < 1e-9);
        let b = DeckState {
            position_s: 1.0,
            ..a
        };
        assert!(
            (phase_offset(&a, &b).unwrap() - 0.5).abs() < 1e-9
                || (phase_offset(&a, &b).unwrap() + 0.5).abs() < 1e-9
        );
        let c = DeckState {
            position_s: 1.2,
            ..a
        };
        assert!((phase_offset(&a, &c).unwrap() - 0.1).abs() < 1e-9, "ahead");
        assert!((phase_offset(&c, &a).unwrap() + 0.1).abs() < 1e-9, "behind");
        assert_eq!(phase_offset(&a, &DeckState::default()), None);
    }

    #[test]
    fn keys_fit_by_the_wheel() {
        let k = |s| MusicalKey::parse(s).unwrap();
        assert_eq!(KeyFit::of(k("8A"), k("8A")), KeyFit::Same);
        assert_eq!(KeyFit::of(k("8A"), k("9A")), KeyFit::Harmonic);
        assert_eq!(KeyFit::of(k("8A"), k("8B")), KeyFit::Harmonic);
        assert_eq!(KeyFit::of(k("8A"), k("3A")), KeyFit::Semitone);
        assert_eq!(KeyFit::of(k("8A"), k("2B")), KeyFit::Clash);
    }

    #[test]
    fn the_partner_is_the_master_then_the_other_side() {
        let loaded = [true, true, true, false];
        let none = [false; 4];
        assert_eq!(partner_of(0, Some(2), &loaded, &none), Some(2));
        assert_eq!(
            partner_of(0, Some(0), &loaded, &none),
            Some(1),
            "opposite side"
        );
        assert_eq!(
            partner_of(0, None, &loaded, &[false, false, true, false]),
            Some(2),
            "playing first"
        );
        assert_eq!(
            partner_of(0, Some(3), &loaded, &none),
            Some(1),
            "an empty master is skipped"
        );
        assert_eq!(
            partner_of(0, None, &[true, false, false, false], &none),
            None
        );
    }
}
