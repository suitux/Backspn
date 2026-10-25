use eframe::egui::Color32;

use super::classic::{centred, tint};
use super::{Canvas, fonts, text};

const BLACK: Color32 = Color32::BLACK;
const WHITE: Color32 = Color32::WHITE;
const GREY_TEXT: Color32 = Color32::from_rgb(0x80, 0x80, 0x80);
const DARK_GREY: Color32 = Color32::from_rgb(0x28, 0x28, 0x28);
const GREY: Color32 = Color32::from_rgb(0x46, 0x46, 0x46);
const BRIGHT_GREY: Color32 = Color32::from_rgb(0x55, 0x55, 0x55);
const BOX_GREY: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
const LOOP_ON: Color32 = Color32::from_rgb(0x00, 0xFF, 0x46);
const RED: Color32 = Color32::from_rgb(0xFF, 0x00, 0x50);
const CYAN: Color32 = Color32::from_rgb(0x60, 0xDC, 0xFF);

const LEFT: i32 = 6;
const RIGHT: i32 = 162;
const BOX: i32 = 150;
const CELL: i32 = 72;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScreenCell {
    pub number: usize,
    pub name: String,
    pub colour: Option<Color32>,
    pub looped: bool,
    pub playing: bool,
}

impl ScreenCell {
    fn shown(&self) -> Color32 {
        match self.colour {
            None => BRIGHT_GREY,
            Some(c) if self.playing => c,
            Some(c) => mid(c),
        }
    }
}

fn mid(c: Color32) -> Color32 {
    tint(c, 0.6)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlotOverlay {
    pub browsing: bool,
    pub volume: f32,
    pub muted: bool,
    pub filter: f32,
    pub filter_on: bool,
    pub around: [Option<ScreenCell>; 3],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SampleScreen {
    pub title: String,
    pub bpm: f64,
    pub quantize: String,
    pub quantize_on: bool,
    pub beat: String,
    pub loop_beats: f64,
    pub loop_active: bool,
    pub recording: bool,
    pub cells: [ScreenCell; 4],
    pub slot: Option<SlotOverlay>,
}

pub(super) fn render(r: &SampleScreen) -> Canvas {
    let mut c = Canvas::new(BLACK);
    match &r.slot {
        Some(slot) if slot.browsing => browsing(&mut c, slot),
        Some(slot) => mixing(&mut c, slot),
        None => deck(&mut c, r),
    }
    c
}

fn header(c: &mut Canvas, title: &str, background: Color32) {
    c.rect((LEFT, 3), (2 * BOX + 6, 33), background);
    text(c, &fonts().sans, 22.0, (12.0, 5.0), 294.0, WHITE, title);
}

fn deck(c: &mut Canvas, r: &SampleScreen) {
    header(c, &r.title, if r.recording { RED } else { BLACK });
    c.rect((LEFT, 42), (BOX, 33), DARK_GREY);
    c.rect((LEFT + 1, 43), (BOX - 2, 31), BLACK);
    centred(
        c,
        24.0,
        (LEFT, 42, BOX, 33),
        WHITE,
        &format!("{:.2}", r.bpm),
    );
    c.rect((RIGHT, 42), (BOX, 33), DARK_GREY);
    centred(c, 24.0, (RIGHT, 42, BOX, 33), WHITE, &r.quantize);
    if r.quantize_on {
        let w = super::text_width(&fonts().sans, 24.0, &r.quantize);
        let x = RIGHT as f32 + f32::midpoint(BOX as f32, w) + 3.0;
        text(c, &fonts().sans, 24.0, (x, 44.0), 30.0, CYAN, "●");
    }
    c.rect((LEFT, 81), (BOX, 72), BOX_GREY);
    centred(c, 45.0, (LEFT, 81, BOX, 72), WHITE, &r.beat);
    let (bg, ink) = if r.loop_active {
        (LOOP_ON, BLACK)
    } else {
        (GREY, WHITE)
    };
    c.rect((RIGHT, 81), (BOX, 72), bg);
    centred(
        c,
        45.0,
        (RIGHT, 81, BOX, 72),
        ink,
        &super::beats_label(r.loop_beats),
    );
    for (i, cell) in r.cells.iter().enumerate() {
        let x = LEFT + i as i32 * (CELL + 6);
        draw_cell(c, (x, 159), cell, cell.shown());
    }
}

fn draw_cell(c: &mut Canvas, (x, y): (i32, i32), cell: &ScreenCell, colour: Color32) {
    c.rect((x, y), (CELL, CELL), colour);
    if cell.colour.is_none() {
        return;
    }
    let (cx, cy) = (x + CELL / 2, y + CELL / 2);
    if cell.looped {
        for dy in -16..=16 {
            for dx in -16..=16 {
                let d = f64::from(dx * dx + dy * dy).sqrt();
                if (11.0..=16.0).contains(&d) {
                    c.rect((cx + dx, cy + dy), (1, 1), BLACK);
                }
            }
        }
    } else {
        for dx in 0..=24 {
            let h = 14 - dx * 14 / 24;
            c.rect((cx - 10 + dx, cy - h), (1, 2 * h + 1), BLACK);
        }
    }
}

fn mixing(c: &mut Canvas, slot: &SlotOverlay) {
    let cell = slot.around[1].clone().unwrap_or_default();
    header(c, &cell.name, BLACK);
    let (bright, mid_c) = match cell.colour {
        Some(col) => (col, mid(col)),
        None => (BRIGHT_GREY, GREY),
    };
    for (left, label, on, value, centred_slider) in [
        (LEFT, "Volume", !slot.muted, slot.volume, false),
        (RIGHT, "Filter", slot.filter_on, slot.filter, true),
    ] {
        let bg = if on { mid_c } else { tint(mid_c, 0.5) };
        c.rect((left, 42), (BOX, 33), bg);
        centred(
            c,
            24.0,
            (left, 42, BOX, 33),
            if on { WHITE } else { GREY_TEXT },
            label,
        );
        c.rect((left, 81), (BOX, 72), bg);
        let fill = if on { bright } else { mid_c };
        let at = ((BOX - 5) as f32 * value.clamp(0.0, 1.0)).round() as i32;
        let (from, to) = if centred_slider {
            (at.min(72), at.max(72) + 5)
        } else {
            (0, at + 5)
        };
        c.rect((left + from, 81), (to - from, 72), fill);
        if centred_slider {
            c.rect((left + 72, 81), (5, 72), BLACK);
        }
        c.rect((left + at, 81), (5, 72), if on { WHITE } else { GREY_TEXT });
    }
}

fn browsing(c: &mut Canvas, slot: &SlotOverlay) {
    for (row, cell) in slot.around.iter().enumerate() {
        let y = 7 + row as i32 * (CELL + 6);
        if row == 1 {
            c.rect((LEFT, y), (2 * BOX + 6, CELL), GREY);
        }
        let Some(cell) = cell else { continue };
        let colour = if cell.colour.is_some() {
            cell.shown()
        } else {
            BRIGHT_GREY
        };
        draw_cell(c, (LEFT, y), cell, colour);
        let name = if cell.colour.is_none() {
            format!("Cell {}", cell.number)
        } else {
            cell.name.clone()
        };
        let x = (LEFT + CELL + 9) as f32;
        text(
            c,
            &fonts().sans,
            24.0,
            (x, y as f32 + 21.0),
            312.0 - x,
            WHITE,
            &name,
        );
    }
}
