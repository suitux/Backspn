use backspn_engine::CueKind;
use eframe::egui::Color32;

use super::{Canvas, ScreenDeck, fonts, text, text_width, title, waveform_band};
use crate::waveform::WaveStyle;

const BLACK: Color32 = Color32::BLACK;
const WHITE: Color32 = Color32::WHITE;
const DARK_GREY: Color32 = Color32::from_rgb(0x28, 0x28, 0x28);
const GREY: Color32 = Color32::from_rgb(0x46, 0x46, 0x46);
const LOOP_ON: Color32 = Color32::from_rgb(0x00, 0xFF, 0x46);
const RED: Color32 = Color32::from_rgb(0xFF, 0x00, 0x50);
const CYAN: Color32 = Color32::from_rgb(0x60, 0xDC, 0xFF);
const PLAYHEAD: Color32 = Color32::from_rgb(0xFF, 0x38, 0x1A);

const LEFT: i32 = 6;
const RIGHT: i32 = 162;
const BOX: i32 = 150;

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

fn hotcue_color(kind: CueKind) -> Color32 {
    match kind {
        CueKind::Grid => WHITE,
        CueKind::Cue | CueKind::Other(_) => CYAN,
        CueKind::FadeIn | CueKind::FadeOut => Color32::from_rgb(0xFF, 0x78, 0x00),
        CueKind::Load => Color32::from_rgb(0xFF, 0xFF, 0x00),
        CueKind::Loop => Color32::from_rgb(0x28, 0xFF, 0x28),
    }
}

pub(super) fn render(deck: &ScreenDeck, style: &WaveStyle) -> Canvas {
    let s = deck.state;
    let mut c = Canvas::new(BLACK);
    let head = if s.loaded {
        title(deck)
    } else {
        "No Track Loaded"
    };
    text(&mut c, &fonts().sans, 22.0, (12.0, 5.0), 294.0, WHITE, head);
    c.rect((LEFT, 42), (BOX, 33), DARK_GREY);
    c.rect((LEFT + 2, 44), (BOX - 4, 29), BLACK);
    let bpm = if s.loaded {
        s.effective_bpm()
            .map_or_else(|| "0.00".into(), |b| format!("{b:.2}"))
    } else {
        "0.00".into()
    };
    centred(&mut c, 24.0, (LEFT, 42, BOX, 33), WHITE, &bpm);
    let has_key = s.loaded && deck.key.is_some();
    let (bg, ink) = match (s.keylock, has_key) {
        (true, true) => {
            let k = deck.key_color.unwrap_or(CYAN);
            (tint(k, 0.3), k)
        }
        (true, false) => (tint(CYAN, 0.7), WHITE),
        (false, _) => (DARK_GREY, WHITE),
    };
    c.rect((RIGHT, 42), (BOX, 33), bg);
    let key = match (deck.key, has_key) {
        (Some(k), true) => k.to_owned(),
        _ if s.key_adjust.abs() > 0.01 => format!("{:+.2}", s.key_adjust),
        _ => "No key".into(),
    };
    centred(&mut c, 24.0, (RIGHT, 42, BOX, 33), ink, &key);
    let red = deck.end_warning && (deck.clock_ms / 500).is_multiple_of(2);
    let (bg, ink) = if red { (RED, BLACK) } else { (GREY, WHITE) };
    c.rect((LEFT, 81), (BOX, 72), bg);
    centred(
        &mut c,
        45.0,
        (LEFT, 81, BOX, 72),
        ink,
        &remaining(s.position_s, s.duration_s, s.loaded),
    );
    let (bg, ink) = if s.loop_active {
        (LOOP_ON, BLACK)
    } else {
        (GREY, WHITE)
    };
    c.rect((RIGHT, 81), (BOX, 72), bg);
    centred(
        &mut c,
        45.0,
        (RIGHT, 81, BOX, 72),
        ink,
        &loop_label(deck.loop_beats),
    );
    if let (Some(peaks), true) = (deck.peaks, s.loaded && s.duration_s > 0.0) {
        let (x0, y0, w, h) = (12, 159, 300, 72);
        let whole = (0.0, s.duration_s);
        let at = |t: f64| x0 + (t / s.duration_s * f64::from(w)).round() as i32;
        waveform_band(
            &mut c,
            peaks,
            style,
            whole,
            s.position_s,
            (x0, x0 + w),
            (y0, y0 + h),
        );
        for y in y0..y0 + h {
            for x in x0..at(s.position_s).min(x0 + w) {
                c.blend((x, y), BLACK, 175.0 / 255.0);
            }
        }
        for (n, h_cue) in s.hotcues.iter().enumerate() {
            if let Some(h_cue) = h_cue {
                let color = hotcue_color(h_cue.kind);
                let hx = at(h_cue.start_s) - 6;
                c.rect((hx, y0), (13, 13), color);
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
        c.rect((at(s.position_s) - 1, y0), (2, h), PLAYHEAD);
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
