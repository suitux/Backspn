use serde::{Deserialize, Serialize};

use crate::Trigger;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LoopRecorderPart {
    Record,
    Size,
    DryWet,
    Play,
    Delete,
    Source,
    Position,
    Undo,
    State,
}

impl LoopRecorderPart {
    pub const ALL: [Self; 9] = [
        Self::Record,
        Self::Size,
        Self::DryWet,
        Self::Play,
        Self::Delete,
        Self::Source,
        Self::Position,
        Self::Undo,
        Self::State,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Record => "Record",
            Self::Size => "Size",
            Self::DryWet => "Dry/Wet Adjust",
            Self::Play => "Play/Pause",
            Self::Delete => "Delete",
            Self::Source => "Source Select",
            Self::Position => "Playback Position",
            Self::Undo => "Undo/Redo",
            Self::State => "State",
        }
    }

    pub fn command(self) -> u32 {
        match self {
            Self::Record => 280,
            Self::Size => 281,
            Self::DryWet => 282,
            Self::Play => 283,
            Self::Delete => 284,
            Self::Source => 285,
            Self::Position => 286,
            Self::Undo => 287,
            Self::State => 289,
        }
    }

    pub fn from_command(id: u32) -> Option<Self> {
        match id {
            288 => Some(Self::Undo),
            _ => Self::ALL.into_iter().find(|p| p.command() == id),
        }
    }

    pub fn is_continuous(self) -> bool {
        matches!(self, Self::DryWet)
    }

    pub fn is_toggle(self) -> bool {
        matches!(self, Self::Record | Self::Play)
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]
pub fn list_after(at: usize, n: usize, trigger: Trigger) -> Option<usize> {
    match trigger {
        Trigger::Press => Some((at + 1) % n),
        Trigger::Delta(d) if d.round() != 0.0 => {
            Some((at as i64 + d.round() as i64).rem_euclid(n as i64) as usize)
        }
        Trigger::Set(v) => Some(((v.clamp(0.0, 1.0) * (n - 1) as f32).round() as usize).min(n - 1)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip_and_undo_state_reads_as_undo() {
        for p in LoopRecorderPart::ALL {
            assert_eq!(LoopRecorderPart::from_command(p.command()), Some(p));
        }
        assert_eq!(
            LoopRecorderPart::from_command(288),
            Some(LoopRecorderPart::Undo)
        );
        assert_eq!(LoopRecorderPart::from_command(290), None);
    }

    #[test]
    fn lists_cycle() {
        assert_eq!(list_after(3, 4, Trigger::Press), Some(0));
        assert_eq!(list_after(0, 4, Trigger::Delta(-1.0)), Some(3));
        assert_eq!(list_after(0, 4, Trigger::Set(1.0)), Some(3));
        assert_eq!(list_after(1, 4, Trigger::Release), None);
    }
}
