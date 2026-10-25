use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MoveMode {
    BeatJump,
    Loop,
    LoopIn,
    LoopOut,
}

impl MoveMode {
    pub const ALL: [Self; 4] = [Self::BeatJump, Self::Loop, Self::LoopIn, Self::LoopOut];

    pub fn label(self) -> &'static str {
        match self {
            Self::BeatJump => "Beatjump",
            Self::Loop => "Loop",
            Self::LoopIn => "Loop In",
            Self::LoopOut => "Loop Out",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct MoveSize(u8);

impl TryFrom<u8> for MoveSize {
    type Error = &'static str;

    fn try_from(index: u8) -> Result<Self, Self::Error> {
        Self::new(index).ok_or("move size out of range")
    }
}

impl From<MoveSize> for u8 {
    fn from(size: MoveSize) -> Self {
        size.0
    }
}

impl MoveSize {
    pub const ULTRA_FINE: Self = Self(0);
    pub const FINE: Self = Self(1);
    pub const LOOP: Self = Self(12);

    pub fn new(index: u8) -> Option<Self> {
        (index <= Self::LOOP.0).then_some(Self(index))
    }

    pub fn index(self) -> u8 {
        self.0
    }

    pub fn beats(self) -> Option<f64> {
        match self.0 {
            0 => Some(1.0 / 320.0),
            12 => None,
            i => Some(2f64.powi(i32::from(i) - 6)),
        }
    }

    pub fn is_fine(self) -> bool {
        self.0 < 2
    }

    pub fn label(self) -> String {
        match self.beats() {
            Some(b) if b < 1.0 => format!("1/{}", (1.0 / b).round()),
            Some(b) => format!("{b}"),
            None => "Loop".into(),
        }
    }

    pub fn of_loop(beats: f64) -> Self {
        let index = (beats.log2().round() + 6.0).clamp(1.0, 11.0);
        Self(index as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_in_beats() {
        let beats = |i| MoveSize::new(i).and_then(MoveSize::beats);
        assert_eq!(beats(0), Some(1.0 / 320.0));
        assert_eq!(beats(1), Some(1.0 / 32.0));
        assert_eq!(beats(2), Some(1.0 / 16.0));
        assert_eq!(beats(6), Some(1.0));
        assert_eq!(beats(11), Some(32.0));
        assert_eq!(beats(12), None);
        assert_eq!(MoveSize::new(13), None);
        assert!(MoveSize::FINE.is_fine() && !MoveSize::new(2).unwrap().is_fine());
    }

    #[test]
    fn loop_sizes_map_one_index_up() {
        assert_eq!(MoveSize::of_loop(1.0 / 32.0), MoveSize::FINE);
        assert_eq!(MoveSize::of_loop(4.0).beats(), Some(4.0));
        assert_eq!(MoveSize::of_loop(32.0).index(), 11);
    }

    #[test]
    fn labels() {
        let label = |i| MoveSize::new(i).unwrap().label();
        assert_eq!(label(0), "1/320");
        assert_eq!(label(2), "1/16");
        assert_eq!(label(6), "1");
        assert_eq!(label(11), "32");
        assert_eq!(label(12), "Loop");
    }
}
