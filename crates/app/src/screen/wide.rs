//! Layouts for the 480 × 272 panels. Under each of these screens sit four
//! faders, four knobs and four buttons, which play the deck's stems by
//! default, so the wide layouts end with a strip naming what each one does.

#![allow(
    clippy::many_single_char_names,
    reason = "drawing code: c canvas, t theme, f fonts, s deck state, as in the other layouts"
)]

use super::booth::{bars_beats, beat_pos, mix_readouts};
use super::{
    Canvas, Layout, ScreenDeck, StemOverlay, WIDE, beats_label, chip, deck_color,
    draw_band_overview_in, draw_zoomed_in, fonts, remaining, text, text_right, text_width, title,
};
use crate::theme::Theme;
use crate::waveform::WaveStyle;

const W: i32 = WIDE.0 as i32;
const PAD: i32 = 8;

pub(super) fn render(deck: &ScreenDeck, style: &WaveStyle, t: &Theme, layout: Layout) -> Canvas {
    match layout {
        Layout::WideWaveform => render_waveform(deck, style, t),
        Layout::WideMix => render_mix(deck, style, t),
        _ => render_deck(deck, style, t),
    }
}

fn header(c: &mut Canvas, deck: &ScreenDeck, t: &Theme, height: i32, big: bool) {
    let f = fonts();
    let colour = deck_color(deck.index, t);
    c.rect((0, 0), (W, height), t.surface);
    c.rect((0, 0), (height, height), colour);
    let letter = char::from(b'A' + deck.index as u8).to_string();
    let size = height as f32 * 0.7;
    let lw = text_width(&f.sans, size, &letter);
    text(
        c,
        &f.sans,
        size,
        ((height as f32 - lw) / 2.0, height as f32 * 0.08),
        lw + 1.0,
        Theme::on(colour),
        &letter,
    );
    let time = remaining(deck.state);
    let time_size = if big { 26.0 } else { 18.0 };
    let time_w = text_width(&f.mono, time_size, &time);
    let time_colour = if deck.end_warning { t.danger } else { t.text };
    text_right(
        c,
        &f.mono,
        time_size,
        ((W - PAD) as f32, if big { 2.0 } else { 3.0 }),
        time_colour,
        &time,
    );
    let left = height as f32 + 8.0;
    let room = W as f32 - left - time_w - 2.0 * PAD as f32;
    if !deck.state.loaded {
        text(c, &f.sans, 17.0, (left, 4.0), room, t.text_dim, "Empty");
        return;
    }
    let (artist, name) = deck
        .name
        .split_once(" - ")
        .map_or(("", title(deck)), |(a, n)| (a, n));
    if big {
        text(c, &f.sans, 17.0, (left, 1.0), room, t.text, name);
        text(c, &f.sans, 12.0, (left, 21.0), room, t.text_dim, artist);
    } else {
        text(c, &f.sans, 15.0, (left, 3.0), room, t.text, name);
    }
}

/// BPM, tempo, key, where the beat is, the loop and the sync state in one row.
fn readouts(c: &mut Canvas, deck: &ScreenDeck, t: &Theme, top: i32, size: f32) {
    let f = fonts();
    let s = deck.state;
    let bpm = s
        .effective_bpm()
        .map_or_else(|| "—".to_owned(), |b| format!("{b:.2}"));
    let mut x = PAD as f32;
    x += text(c, &f.mono, size, (x, top as f32), 140.0, t.text, &bpm) + 6.0;
    let pct = (s.tempo - 1.0) * 100.0;
    let small = (size * 0.48).round();
    text(
        c,
        &f.mono,
        small,
        (x, top as f32 + size - small - 2.0),
        70.0,
        t.text_dim,
        &format!("{pct:+.1}%"),
    );
    x += 62.0;
    if let Some(key) = deck.key {
        text(
            c,
            &f.mono,
            size * 0.85,
            (x, top as f32 + size * 0.1),
            60.0,
            deck.key_color.unwrap_or(t.text),
            key,
        );
    }
    x += 62.0;
    if let Some(grid) = s.grid.filter(|_| s.loaded) {
        let pos = beat_pos(grid.beat_at(s.position_s));
        let beat = format!("{}.{}", pos.bar, pos.beat);
        let caption = text(
            c,
            &f.mono,
            10.0,
            (x, top as f32 + size * 0.85 - 10.0),
            30.0,
            t.text_faint,
            "BAR",
        );
        text(
            c,
            &f.mono,
            size * 0.85,
            (x + caption + 4.0, top as f32 + size * 0.1),
            70.0,
            t.text,
            &beat,
        );
    }
    let chip_top = top + ((size - 18.0) / 2.0).round() as i32;
    let mut right = W - PAD;
    for (label, on, colour) in [
        ("MASTER", deck.master, t.master),
        ("SYNC", deck.synced, t.sync),
    ] {
        let w = text_width(&f.mono, 12.0, label).ceil() as i32 + 12;
        right -= w;
        chip(c, (right, chip_top), label, on, colour, t);
        right -= 6;
    }
    let label = format!("LOOP {}", beats_label(deck.loop_beats));
    let w = text_width(&f.mono, 12.0, &label).ceil() as i32 + 12;
    chip(c, (right - w, chip_top), &label, s.loop_active, t.play, t);
}

/// What the four faders, knobs and buttons under the screen play: each stem's
/// level (fader), filter (knob) and mute (button).
fn controls_strip(c: &mut Canvas, deck: &ScreenDeck, t: &Theme, (top, height): (i32, i32)) {
    let f = fonts();
    c.rect((0, top), (W, height), t.surface);
    let Some(stems) = &deck.stems else {
        let msg = if deck.state.loaded {
            "Faders, knobs and buttons below: stems, once the track has them"
        } else {
            "Faders, knobs and buttons below play the stems"
        };
        let w = text_width(&f.sans, 13.0, msg).min(W as f32 - 16.0);
        text(
            c,
            &f.sans,
            13.0,
            ((W as f32 - w) / 2.0, (top + height / 2 - 9) as f32),
            W as f32 - 16.0,
            t.text_faint,
            msg,
        );
        return;
    };
    let col = W / 4;
    for (i, stem) in stems.iter().enumerate() {
        let left = i as i32 * col;
        stem_cell(c, (left + 3, top + 3), (col - 6, height - 6), stem, t);
    }
}

fn stem_cell(
    c: &mut Canvas,
    (left, top): (i32, i32),
    (w, h): (i32, i32),
    stem: &StemOverlay,
    t: &Theme,
) {
    let f = fonts();
    let on = !stem.muted;
    let colour = if on {
        stem.colour
    } else {
        stem.colour.gamma_multiply(0.35)
    };
    c.rect((left, top), (w, h), t.well);
    c.rect((left, top), (4, h), colour);
    let ink = if on { t.text } else { t.text_dim };
    let name_size = (h as f32 * 0.3).clamp(11.0, 15.0);
    text(
        c,
        &f.sans,
        name_size,
        (left as f32 + 9.0, top as f32 + 2.0),
        w as f32 * 0.6,
        ink,
        &stem.name,
    );
    let state = if stem.muted { "MUTE" } else { "ON" };
    let state_ink = if stem.muted { t.danger } else { t.text_faint };
    text_right(
        c,
        &f.mono,
        10.0,
        ((left + w - 4) as f32, top as f32 + 4.0),
        state_ink,
        state,
    );
    let bar_top = top + (name_size * 1.3) as i32 + 4;
    let bar_h = ((h - (bar_top - top)) / 2 - 2).max(3);
    let bar_w = w - 13;
    let level = (bar_w as f32 * stem.volume.clamp(0.0, 1.0)).round() as i32;
    c.rect((left + 9, bar_top), (bar_w, bar_h), t.base);
    c.rect((left + 9, bar_top), (level, bar_h), colour);
    let filter_top = bar_top + bar_h + 3;
    c.rect((left + 9, filter_top), (bar_w, bar_h), t.base);
    let mid = left + 9 + bar_w / 2;
    if stem.filter_on {
        let at = left + 9 + (bar_w as f32 * stem.filter.clamp(0.0, 1.0)).round() as i32;
        let (a, b) = (mid.min(at), mid.max(at));
        c.rect((a, filter_top), ((b - a).max(2), bar_h), t.text_dim);
    }
    c.rect((mid, filter_top), (1, bar_h), t.text_faint);
}

fn render_deck(deck: &ScreenDeck, style: &WaveStyle, t: &Theme) -> Canvas {
    let mut c = Canvas::sized(WIDE, t.base);
    header(&mut c, deck, t, 36, true);
    let s = deck.state;
    draw_zoomed_in(
        &mut c,
        s,
        deck.peaks,
        style,
        t,
        deck.zoom_s,
        (0, W),
        (38, 146),
    );
    draw_band_overview_in(&mut c, deck, style, t, (PAD, W - PAD), (150, 168));
    readouts(&mut c, deck, t, 172, 30.0);
    controls_strip(&mut c, deck, t, (210, 62));
    c
}

fn render_waveform(deck: &ScreenDeck, style: &WaveStyle, t: &Theme) -> Canvas {
    let f = fonts();
    let mut c = Canvas::sized(WIDE, t.base);
    header(&mut c, deck, t, 26, false);
    let s = deck.state;
    draw_zoomed_in(
        &mut c,
        s,
        deck.peaks,
        style,
        t,
        deck.zoom_s,
        (0, W),
        (28, 178),
    );
    if let Some(grid) = s.grid.filter(|_| s.loaded) {
        let pos = beat_pos(grid.beat_at(s.position_s));
        let label = format!("{} / {}", pos.bar, super::booth::PHRASE_BARS);
        text(&mut c, &f.mono, 12.0, (6.0, 31.0), 80.0, t.text_dim, &label);
        for i in 0..4u8 {
            let fill = if i + 1 == pos.beat {
                deck_color(deck.index, t)
            } else {
                t.border
            };
            c.rect((W - 70 + i32::from(i) * 16, 32), (12, 8), fill);
        }
    }
    if let Some((n, at)) = super::booth::next_cue(s)
        && let Some(beats) = super::booth::beats_to(s, at)
    {
        let label = format!("CUE {n} IN {}", bars_beats(beats));
        let ink = if beats < 8.0 { t.danger } else { t.text_dim };
        text_right(&mut c, &f.mono, 12.0, ((W - 6) as f32, 162.0), ink, &label);
    }
    draw_band_overview_in(&mut c, deck, style, t, (PAD, W - PAD), (181, 193));
    readouts(&mut c, deck, t, 196, 22.0);
    controls_strip(&mut c, deck, t, (224, 48));
    c
}

fn render_mix(deck: &ScreenDeck, style: &WaveStyle, t: &Theme) -> Canvas {
    let f = fonts();
    let mut c = Canvas::sized(WIDE, t.base);
    header(&mut c, deck, t, 26, false);
    let s = deck.state;
    draw_zoomed_in(
        &mut c,
        s,
        deck.peaks,
        style,
        t,
        deck.zoom_s,
        (0, W),
        (28, 112),
    );
    let Some(p) = &deck.partner else {
        c.rect((0, 114), (W, 84), t.well);
        let msg = "No other deck loaded";
        let w = text_width(&f.sans, 18.0, msg);
        text(
            &mut c,
            &f.sans,
            18.0,
            ((W as f32 - w) / 2.0, 144.0),
            w + 1.0,
            t.text_dim,
            msg,
        );
        controls_strip(&mut c, deck, t, (224, 48));
        return c;
    };
    draw_zoomed_in(
        &mut c,
        p.state,
        p.peaks,
        style,
        t,
        deck.zoom_s,
        (0, W),
        (114, 198),
    );
    let other = deck_color(p.index, t);
    c.rect((0, 114), (5, 84), other);
    let letter = char::from(b'A' + p.index as u8).to_string();
    text(&mut c, &f.mono, 13.0, (8.0, 115.0), 20.0, other, &letter);
    mix_readouts(&mut c, deck, p, t, 200, [8, 170, 320]);
    c
}
