pub const SLICE_SIZES: [f64; 8] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
pub const SLICES: usize = 8;
pub const DEFAULT_SIZE: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadChange {
    pub changed: bool,
    pub first: Option<usize>,
    pub last: Option<usize>,
    pub newest: Option<usize>,
    pub was_held: bool,
}

impl PadChange {
    pub fn new(before: u16, now: u16) -> Self {
        let lowest = |bits: u16| (bits != 0).then(|| bits.trailing_zeros() as usize);
        let highest = |bits: u16| (bits != 0).then(|| 15 - bits.leading_zeros() as usize);
        Self {
            changed: before != now,
            first: lowest(now),
            last: highest(now),
            newest: highest(now & !before),
            was_held: before != 0,
        }
    }
}

pub fn round_beat(beats: f64) -> f64 {
    beats.round()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Freeze {
    pub enabled: bool,
    pub start_s: f64,
    pub loop_beats: f64,
    pub size_index: usize,
    pub held: u16,
    pub last_slice: Option<usize>,
}

impl Default for Freeze {
    fn default() -> Self {
        Self {
            enabled: false,
            start_s: 0.0,
            loop_beats: 0.0,
            size_index: DEFAULT_SIZE,
            held: 0,
            last_slice: None,
        }
    }
}

impl Freeze {
    pub fn slice_beats(&self, loop_active: bool) -> f64 {
        if loop_active {
            self.loop_beats / SLICES as f64
        } else {
            SLICE_SIZES[self.size_index.min(SLICE_SIZES.len() - 1)]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_changes() {
        let c = PadChange::new(0, 0b0010_0100);
        assert_eq!(
            c,
            PadChange {
                changed: true,
                first: Some(2),
                last: Some(5),
                newest: Some(5),
                was_held: false
            }
        );
        let c = PadChange::new(0b0010_0100, 0b0000_0100);
        assert_eq!(
            (c.first, c.last, c.newest, c.was_held),
            (Some(2), Some(2), None, true)
        );
        assert!(!PadChange::new(3, 3).changed);
        assert_eq!(PadChange::new(1, 0).first, None);
    }

    #[test]
    fn slice_length() {
        let f = Freeze {
            loop_beats: 4.0,
            ..Freeze::default()
        };
        assert_eq!(f.slice_beats(false), 1.0);
        assert_eq!(f.slice_beats(true), 0.5);
        assert_eq!(round_beat(2.5), 3.0);
        assert_eq!(round_beat(-2.5), -3.0);
    }
}
