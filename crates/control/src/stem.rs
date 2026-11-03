use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StemPart {
    Volume,
    Mute,
    FilterOn,
    Filter,
    FxOn,
    FxAmount,
    /// The filter amount, switching the filter on once moved off centre.
    FilterEngaged,
}

impl StemPart {
    pub const ALL: [Self; 6] = [
        Self::Volume,
        Self::Mute,
        Self::FilterOn,
        Self::Filter,
        Self::FxOn,
        Self::FxAmount,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Volume => "Volume",
            Self::Mute => "Mute",
            Self::FilterOn => "Filter On",
            Self::Filter => "Filter",
            Self::FxOn => "FX On",
            Self::FxAmount => "FX Amount",
            Self::FilterEngaged => "Filter (switches on)",
        }
    }

    pub fn command(self) -> u32 {
        match self {
            Self::Volume => 251,
            Self::Mute => 259,
            Self::FilterOn => 250,
            Self::Filter => 249,
            Self::FxOn => 239,
            Self::FxAmount => 232,
            Self::FilterEngaged => 0,
        }
    }

    pub fn from_command(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.command() == id)
    }

    pub fn is_continuous(self) -> bool {
        matches!(
            self,
            Self::Volume | Self::Filter | Self::FxAmount | Self::FilterEngaged
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LoadMode {
    Alternative,
    Track,
    Stems,
}

impl LoadMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Alternative => "Load Selected (loading alternative)",
            Self::Track => "Load Selected (load as track)",
            Self::Stems => "Load Selected (load as stems)",
        }
    }
}

pub fn slot_assignment(assignment: u32) -> Option<(usize, usize)> {
    let index = usize::try_from(assignment.checked_sub(8)?).ok()?;
    (index < 16).then_some((index / 4, index % 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_assignments_give_deck_and_stem() {
        assert_eq!(slot_assignment(8), Some((0, 0)));
        assert_eq!(slot_assignment(11), Some((0, 3)));
        assert_eq!(slot_assignment(12), Some((1, 0)));
        assert_eq!(slot_assignment(23), Some((3, 3)));
        assert_eq!(slot_assignment(4), None);
        assert_eq!(slot_assignment(24), None);
    }

    #[test]
    fn parts_round_trip_their_ids() {
        for part in StemPart::ALL {
            assert_eq!(StemPart::from_command(part.command()), Some(part));
        }
        assert_eq!(StemPart::from_command(100), None);
    }
}
