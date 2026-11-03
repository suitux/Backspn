//! One-bit displays drawn on the computer and sent as HID output reports:
//! a small pixel canvas, a 5×7 font and the report encoding.

pub const WIDTH: usize = 128;
pub const HEIGHT: usize = 64;
const PAGES: usize = HEIGHT / 8;
/// Pixel bytes carried by one report.
const DATA: usize = 256;
const PAGES_PER_REPORT: usize = DATA / WIDTH;
const HEADER: usize = 8;

const GLYPH_W: usize = 5;
const GLYPH_H: usize = 7;
const ADVANCE: usize = GLYPH_W + 1;

/// Each byte holds eight pixels of one column, the lowest bit on top.
#[derive(Clone, PartialEq, Eq)]
pub struct Canvas([u8; WIDTH * PAGES]);

impl std::fmt::Debug for Canvas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lit = self.0.iter().map(|b| b.count_ones()).sum::<u32>();
        write!(f, "Canvas({lit} lit)")
    }
}

impl Default for Canvas {
    fn default() -> Self {
        Self([0; WIDTH * PAGES])
    }
}

impl Canvas {
    pub fn lit(&self, x: usize, y: usize) -> bool {
        x < WIDTH && y < HEIGHT && self.0[y / 8 * WIDTH + x] & (1 << (y % 8)) != 0
    }

    pub fn set(&mut self, x: usize, y: usize, on: bool) {
        if x >= WIDTH || y >= HEIGHT {
            return;
        }
        let byte = &mut self.0[y / 8 * WIDTH + x];
        if on {
            *byte |= 1 << (y % 8);
        } else {
            *byte &= !(1 << (y % 8));
        }
    }

    pub fn fill(&mut self, (x, y): (usize, usize), (w, h): (usize, usize), on: bool) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, on);
            }
        }
    }

    pub fn outline(&mut self, (x, y): (usize, usize), (w, h): (usize, usize)) {
        if w == 0 || h == 0 {
            return;
        }
        self.fill((x, y), (w, 1), true);
        self.fill((x, y + h - 1), (w, 1), true);
        self.fill((x, y), (1, h), true);
        self.fill((x + w - 1, y), (1, h), true);
    }

    /// Draws `text` from its top left corner; `on` false draws dark text,
    /// for labels over a filled box.
    pub fn text(&mut self, (x, y): (usize, usize), scale: usize, text: &str, on: bool) {
        for (i, c) in text.chars().enumerate() {
            let rows = glyph(c);
            let left = x + i * ADVANCE * scale;
            for (row, bits) in rows.iter().enumerate() {
                for col in 0..GLYPH_W {
                    if bits & (0x10 >> col) != 0 {
                        self.fill((left + col * scale, y + row * scale), (scale, scale), on);
                    }
                }
            }
        }
    }

    pub fn text_centred(&mut self, y: usize, scale: usize, text: &str) {
        let x = WIDTH.saturating_sub(text_width(text, scale)) / 2;
        self.text((x, y), scale, text, true);
    }

    pub fn text_right(&mut self, (right, y): (usize, usize), scale: usize, text: &str) {
        let x = right.saturating_sub(text_width(text, scale));
        self.text((x, y), scale, text, true);
    }
}

pub fn text_width(text: &str, scale: usize) -> usize {
    let n = text.chars().count();
    (n * ADVANCE).saturating_sub(1) * scale
}

pub const fn text_height(scale: usize) -> usize {
    GLYPH_H * scale
}

/// The reports that redraw a whole display: two pixel rows of eight per
/// report, after a header with the column, the first row of eight, the
/// width and the number of rows of eight. A cleared bit lights a pixel.
pub fn reports(id: u8, canvas: &Canvas) -> Vec<Vec<u8>> {
    (0..PAGES)
        .step_by(PAGES_PER_REPORT)
        .map(|page| {
            let mut r = Vec::with_capacity(1 + HEADER + DATA);
            r.push(id);
            r.extend_from_slice(&0u16.to_le_bytes());
            r.extend_from_slice(&u16::try_from(page).unwrap_or(0).to_le_bytes());
            r.extend_from_slice(&u16::try_from(WIDTH).unwrap_or(0).to_le_bytes());
            r.extend_from_slice(&u16::try_from(PAGES_PER_REPORT).unwrap_or(0).to_le_bytes());
            let pixels = &canvas.0[page * WIDTH..(page + PAGES_PER_REPORT) * WIDTH];
            r.extend(pixels.iter().map(|&b| !b));
            r
        })
        .collect()
}

#[rustfmt::skip]
fn glyph(c: char) -> [u8; GLYPH_H] {
    match c.to_ascii_uppercase() {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1E, 0x01, 0x01, 0x0E, 0x01, 0x01, 0x1E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        '/' => [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10],
        '-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        '+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
        ':' => [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x0C, 0x00],
        '%' => [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03],
        _ => [0; GLYPH_H],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_lights_the_glyph_pixels() {
        let mut c = Canvas::default();
        c.text((0, 0), 1, "1", true);
        assert!(c.lit(2, 0), "top of the stem");
        assert!(!c.lit(0, 0));
        assert!(c.lit(1, 6) && c.lit(3, 6), "foot");
        let mut big = Canvas::default();
        big.text((10, 10), 2, "1", true);
        assert!(big.lit(14, 10) && big.lit(15, 11));
        assert_eq!(text_width("AB", 2), 22);
    }

    #[test]
    fn a_full_redraw_takes_four_reports() {
        let mut c = Canvas::default();
        c.set(3, 9, true);
        let r = reports(0xE3, &c);
        assert_eq!(r.len(), 4);
        assert!(
            r.iter()
                .all(|r| r.len() == 1 + HEADER + DATA && r[0] == 0xE3)
        );
        assert_eq!(&r[1][1..9], &[0, 0, 2, 0, 128, 0, 2, 0]);
        let byte = r[0][1 + HEADER + WIDTH + 3];
        assert_eq!(byte, !0x02, "row 9 is bit 1 of the second row of eight");
        assert_eq!(r[0][1 + HEADER], 0xFF, "dark pixels stay set");
    }
}
