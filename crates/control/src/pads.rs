#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySlot {
    Semitone(i32),
    Up,
    Down,
    Reset,
    Sync,
    Empty,
}

const KEY_PAGES: [[KeySlot; 8]; 5] = {
    use KeySlot::{Down, Empty, Reset, Semitone as S, Sync, Up};
    [
        [S(12), Empty, Empty, Empty, S(8), S(9), S(10), S(11)],
        [S(4), S(5), S(6), S(7), S(0), S(1), S(2), S(3)],
        [S(-4), S(-3), S(-2), S(-1), S(-8), S(-7), S(-6), S(-5)],
        [S(-12), S(-11), S(-10), S(-9), Empty, Empty, Empty, Empty],
        [Sync, Up, S(7), S(12), Reset, Down, S(-5), S(-12)],
    ]
};

pub const KEY_PAGE_START: u8 = 2;
pub const BEAT_JUMP_PAGE_START: u8 = 5;

pub fn key_slot(page: u8, pad: usize) -> KeySlot {
    let (page, pad) = if pad < 8 {
        (usize::from(page), pad)
    } else {
        (usize::from(page) + 1, pad - 8)
    };
    page.checked_sub(1)
        .and_then(|p| KEY_PAGES.get(p))
        .and_then(|row| row.get(pad))
        .copied()
        .unwrap_or(KeySlot::Empty)
}

pub fn key_page(page: u8, direction: f64) -> u8 {
    step_page(page, direction, 5)
}

const JUMPS: [Option<f64>; 12] = [
    None,
    Some(0.125),
    Some(0.25),
    Some(0.5),
    Some(1.0),
    Some(2.0),
    Some(4.0),
    Some(8.0),
    Some(16.0),
    Some(32.0),
    Some(64.0),
    Some(128.0),
];

pub fn beat_jump(page: u8, pad: usize) -> Option<(Option<f64>, bool)> {
    let slot = (2 * usize::from(page) + pad).checked_sub(2)?;
    let size = *JUMPS.get(slot / 2)?;
    let forward = slot % 2 == 1;
    Some((size.map(|b| if forward { b } else { -b }), forward))
}

pub fn beat_jump_page(page: u8, direction: f64, far: bool) -> u8 {
    if !far {
        return step_page(page, direction, 9);
    }
    let middle = BEAT_JUMP_PAGE_START;
    if direction < 0.0 {
        if page > middle { middle } else { 1 }
    } else if page < middle {
        middle
    } else {
        9
    }
}

fn step_page(page: u8, direction: f64, last: u8) -> u8 {
    let page = if direction < 0.0 {
        page.saturating_sub(1)
    } else {
        page.saturating_add(1)
    };
    page.clamp(1, last)
}

const RB_RANGES: [f32; 4] = [0.06, 0.10, 0.16, 1.0];

pub fn next_tempo_range(range: f32, tempo: f32) -> f32 {
    let lowest = RB_RANGES
        .iter()
        .copied()
        .find(|r| tempo.abs() <= *r + 1e-6)
        .unwrap_or(1.0);
    let next = RB_RANGES
        .iter()
        .copied()
        .find(|r| *r > range + 1e-6)
        .unwrap_or(lowest);
    if (range - 1.0).abs() < 1e-6 {
        lowest
    } else {
        next.max(lowest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_shift_pages() {
        assert_eq!(key_slot(2, 0), KeySlot::Semitone(4));
        assert_eq!(key_slot(2, 4), KeySlot::Semitone(0));
        assert_eq!(key_slot(1, 1), KeySlot::Empty);
        assert_eq!(key_slot(5, 0), KeySlot::Sync);
        assert_eq!(key_slot(5, 4), KeySlot::Reset);
        assert_eq!(key_slot(4, 8), KeySlot::Sync, "16 pads: the next page");
        assert_eq!(key_slot(5, 8), KeySlot::Empty);
        assert_eq!(
            (key_page(1, -1.0), key_page(5, 1.0), key_page(2, 1.0)),
            (1, 5, 3)
        );
    }

    #[test]
    fn beat_jump_pages() {
        let page5: Vec<_> = (0..8)
            .map(|p| beat_jump(5, p).unwrap().0.unwrap())
            .collect();
        assert_eq!(page5, [-1.0, 1.0, -2.0, 2.0, -4.0, 4.0, -8.0, 8.0]);
        assert_eq!(beat_jump(1, 0), Some((None, false)), "FINE back");
        assert_eq!(beat_jump(1, 1), Some((None, true)));
        assert_eq!(beat_jump(9, 7).unwrap().0, Some(128.0));
        assert_eq!(beat_jump(9, 8), None);
        assert_eq!(beat_jump_page(5, 1.0, false), 6);
        assert_eq!(beat_jump_page(9, 1.0, false), 9);
        assert_eq!(beat_jump_page(3, 1.0, true), 5);
        assert_eq!(beat_jump_page(5, 1.0, true), 9);
        assert_eq!(beat_jump_page(7, -1.0, true), 5);
        assert_eq!(beat_jump_page(5, -1.0, true), 1);
    }

    #[test]
    fn tempo_range_button_skips_ranges_below_the_tempo() {
        assert!((next_tempo_range(0.06, 0.0) - 0.10).abs() < 1e-6);
        assert!((next_tempo_range(0.16, 0.0) - 1.0).abs() < 1e-6);
        assert!((next_tempo_range(1.0, 0.0) - 0.06).abs() < 1e-6);
        assert!((next_tempo_range(1.0, 0.12) - 0.16).abs() < 1e-6);
        assert!((next_tempo_range(0.06, 0.12) - 0.16).abs() < 1e-6);
        assert!((next_tempo_range(0.08, 0.0) - 0.10).abs() < 1e-6);
    }
}
