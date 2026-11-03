use backspn_engine::CueKind;
use eframe::egui::Color32;

use super::{Canvas, ScreenDeck, draw_band_overview_in, fonts, text, text_width, title};
use crate::theme::Theme;
use crate::waveform::WaveStyle;

const BLACK: Color32 = Color32::BLACK;
const WHITE: Color32 = Color32::WHITE;

const LEFT: i32 = 6;
const RIGHT: i32 = 162;
const BOX: i32 = 152;

fn loop_label(beats: f64) -> String {
    super::beats_label(beats)
}

fn remaining(position_s: f64, duration_s: f64, loaded: bool) -> String {
    if !loaded || position_s > duration_s {
        return "00:00".into();
    }
    let left = duration_s.floor() - position_s.floor();
    let left = left.max(0.0) as u64;
    format!("-{:02}:{:02}", left / 60, left % 60)
}

pub(super) fn centred(
    canvas: &mut Canvas,
    size: f32,
    (left, top, width, height): (i32, i32, i32, i32),
    color: Color32,
    label: &str,
) {
    let font = &fonts().sans;
    let room = width as f32 - 4.0;
    let shown = text_width(font, size, label).min(room);
    let y = top as f32 + (height as f32 - size * 1.2) / 2.0;
    let x = left as f32 + (width as f32 - shown) / 2.0;
    text(canvas, font, size, (x, y), room, color, label);
}

pub(super) fn tint(color: Color32, alpha: f32) -> Color32 {
    let m = |v: u8| (f32::from(v) * alpha).round() as u8;
    Color32::from_rgb(m(color.r()), m(color.g()), m(color.b()))
}

fn hotcue_color(kind: CueKind, t: &Theme) -> Color32 {
    match kind {
        CueKind::Grid => t.text,
        CueKind::Cue | CueKind::Other(_) => t.accent,
        CueKind::FadeIn | CueKind::FadeOut => t.master,
        CueKind::Load => t.cue,
        CueKind::Loop => t.play,
    }
}

/// A box with a thin outline: `bg` inside, `t.border` around it.
fn field(c: &mut Canvas, (left, top, width, height): (i32, i32, i32, i32), bg: Color32, t: &Theme) {
    c.rect((left, top), (width, height), t.border);
    c.rect((left + 1, top + 1), (width - 2, height - 2), bg);
}

pub(super) fn render(deck: &ScreenDeck, style: &WaveStyle, t: &Theme) -> Canvas {
    let s = deck.state;
    let mut c = Canvas::new(t.base);
    let head = if s.loaded {
        title(deck)
    } else {
        "No Track Loaded"
    };
    text(
        &mut c,
        &fonts().sans,
        22.0,
        (12.0, 5.0),
        294.0,
        t.text,
        head,
    );

    let time = (LEFT, 38, BOX, 52);
    let red = deck.end_warning && (deck.clock_ms / 500).is_multiple_of(2);
    let (bg, ink) = if red {
        (t.danger, BLACK)
    } else {
        (t.well, t.text)
    };
    field(&mut c, time, bg, t);
    centred(
        &mut c,
        30.0,
        time,
        ink,
        &remaining(s.position_s, s.duration_s, s.loaded),
    );

    let tempo = (RIGHT, 38, BOX, 52);
    field(&mut c, tempo, t.well, t);
    let bpm = if s.loaded {
        s.effective_bpm()
            .map_or_else(|| "0.00".into(), |b| format!("{b:.2}"))
    } else {
        "0.00".into()
    };
    centred(&mut c, 30.0, tempo, t.text, &bpm);

    let key_box = (LEFT, 94, BOX, 62);
    let has_key = s.loaded && deck.key.is_some();
    let (bg, ink) = match (s.keylock, has_key) {
        (true, true) => {
            let k = deck.key_color.unwrap_or(t.accent);
            (tint(k, 0.3), k)
        }
        (true, false) => (tint(t.accent, 0.7), WHITE),
        (false, _) => (t.well, t.text),
    };
    field(&mut c, key_box, bg, t);
    let key = match (deck.key, has_key) {
        (Some(k), true) => k.to_owned(),
        _ if s.key_adjust.abs() > 0.01 => format!("{:+.2}", s.key_adjust),
        _ => "No key".into(),
    };
    centred(&mut c, 40.0, key_box, ink, &key);

    let loop_box = (RIGHT, 94, BOX, 62);
    let (bg, ink) = if s.loop_active {
        (t.accent, WHITE)
    } else {
        (t.well, t.text)
    };
    field(&mut c, loop_box, bg, t);
    centred(&mut c, 40.0, loop_box, ink, &loop_label(deck.loop_beats));

    let (x0, x1, y0) = (LEFT, RIGHT + BOX, 160);
    draw_band_overview_in(&mut c, deck, style, t, (x0, x1), (y0, 232));
    if s.loaded && s.duration_s > 0.0 {
        let at = |t: f64| x0 + (t / s.duration_s * f64::from(x1 - x0)).round() as i32;
        for (n, h_cue) in s.hotcues.iter().enumerate() {
            if let Some(h_cue) = h_cue {
                let hx = at(h_cue.start_s) - 6;
                c.rect((hx, y0), (13, 13), hotcue_color(h_cue.kind, t));
                text(
                    &mut c,
                    &fonts().sans,
                    12.0,
                    (hx as f32 + 3.0, y0 as f32),
                    10.0,
                    BLACK,
                    &(n + 1).to_string(),
                );
            }
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_time_is_whole_seconds() {
        assert_eq!(remaining(10.4, 70.9, true), "-01:00");
        assert_eq!(remaining(80.0, 70.0, true), "00:00");
        assert_eq!(remaining(0.0, 70.0, false), "00:00");
    }
}
