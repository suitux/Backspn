use serde::{Deserialize, Serialize};

pub const SLOTS: usize = 4;
pub const CELLS: usize = 16;
pub const CELL_TRIGGER_BASE: u32 = 601;
pub const CELL_STATE_BASE: u32 = 665;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SampleSlotPart {
    Primary,
    Secondary,
    Retrigger,
    RetriggerPlay,
    Play,
    PlayMode,
    TriggerType,
    Keylock,
    Punch,
    SizeDouble,
    SizeHalf,
    SizeReset,
    SizeAdjust,
    Unload,
    LoadFromList,
    Capture,
    CaptureLoopRecorder,
    CopyFromSlot,
    State,
    SelectedCell,
    TriggerSelected,
    StopSelected,
    DeletePattern,
}

impl SampleSlotPart {
    pub const ALL: [Self; 20] = [
        Self::Primary,
        Self::Secondary,
        Self::Retrigger,
        Self::RetriggerPlay,
        Self::Play,
        Self::PlayMode,
        Self::TriggerType,
        Self::Keylock,
        Self::Punch,
        Self::SizeDouble,
        Self::SizeHalf,
        Self::SizeReset,
        Self::SizeAdjust,
        Self::Unload,
        Self::LoadFromList,
        Self::Capture,
        Self::CaptureLoopRecorder,
        Self::CopyFromSlot,
        Self::State,
        Self::SelectedCell,
    ];

    pub fn command(self) -> u32 {
        match self {
            Self::Primary => 2004,
            Self::Secondary => 236,
            Self::Retrigger => 260,
            Self::RetriggerPlay => 258,
            Self::Play => 241,
            Self::PlayMode => 265,
            Self::TriggerType => 2003,
            Self::Keylock => 237,
            Self::Punch => 235,
            Self::SizeDouble => 266,
            Self::SizeHalf => 267,
            Self::SizeReset => 268,
            Self::SizeAdjust => 2000,
            Self::Unload => 246,
            Self::LoadFromList => 244,
            Self::Capture => 245,
            Self::CaptureLoopRecorder => 263,
            Self::CopyFromSlot => 264,
            Self::State => 247,
            Self::SelectedCell => 193,
            Self::TriggerSelected | Self::StopSelected | Self::DeletePattern => 0,
        }
    }

    pub fn from_command(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.command() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Primary => "Capture/Trigger/Mute",
            Self::Secondary => "Stop/Delete/Load from List",
            Self::Retrigger => "Retrigger",
            Self::RetriggerPlay => "Retrigger Play",
            Self::Play => "Play",
            Self::PlayMode => "Play Mode",
            Self::TriggerType => "Trigger Type",
            Self::Keylock => "Keylock",
            Self::Punch => "Punch",
            Self::SizeDouble => "Size x2",
            Self::SizeHalf => "Size /2",
            Self::SizeReset => "Size Reset",
            Self::SizeAdjust => "Size Adjust",
            Self::Unload => "Unload",
            Self::LoadFromList => "Load from List",
            Self::Capture => "Capture from Deck",
            Self::CaptureLoopRecorder => "Capture from Loop Recorder",
            Self::CopyFromSlot => "Copy from Slot",
            Self::State => "State",
            Self::SelectedCell => "Selected Cell",
            Self::TriggerSelected => "Trigger Selected Cell",
            Self::StopSelected => "Stop Selected Cell",
            Self::DeletePattern => "Delete Pattern",
        }
    }

    pub fn is_continuous(self) -> bool {
        matches!(
            self,
            Self::SizeAdjust
                | Self::PlayMode
                | Self::TriggerType
                | Self::SelectedCell
                | Self::CopyFromSlot
        )
    }

    pub fn is_held(self) -> bool {
        matches!(
            self,
            Self::Primary
                | Self::Retrigger
                | Self::RetriggerPlay
                | Self::Play
                | Self::TriggerSelected
        )
    }

    pub fn is_toggle(self) -> bool {
        matches!(self, Self::Keylock | Self::Punch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SamplePart {
    QuantizeSelector,
    QuantizeOn,
    SaveSet,
    LoadSet,
    Page,
    PlayAll,
    TriggerAll,
    PlayModeAll,
    CaptureSource,
    CellLoadModifier,
    CellDeleteModifier,
    CellReverseModifier,
    CellCaptureModifier,
    SequencerOn,
    SequencerRecord,
}

impl SamplePart {
    pub const ALL: [Self; 14] = [
        Self::QuantizeSelector,
        Self::QuantizeOn,
        Self::SaveSet,
        Self::LoadSet,
        Self::Page,
        Self::PlayAll,
        Self::TriggerAll,
        Self::PlayModeAll,
        Self::CaptureSource,
        Self::CellLoadModifier,
        Self::CellDeleteModifier,
        Self::CellReverseModifier,
        Self::CellCaptureModifier,
        Self::SequencerOn,
    ];

    pub fn command(self) -> u32 {
        match self {
            Self::QuantizeSelector => 229,
            Self::QuantizeOn => 230,
            Self::SaveSet => 234,
            Self::LoadSet => 233,
            Self::Page => 733,
            Self::PlayAll => 255,
            Self::TriggerAll => 256,
            Self::PlayModeAll => 242,
            Self::CaptureSource => 2002,
            Self::CellLoadModifier => 729,
            Self::CellDeleteModifier => 730,
            Self::CellReverseModifier => 731,
            Self::CellCaptureModifier => 732,
            Self::SequencerOn => 734,
            Self::SequencerRecord => 0,
        }
    }

    pub fn from_command(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.command() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::QuantizeSelector => "Quantize Selector",
            Self::QuantizeOn => "Quantize On",
            Self::SaveSet => "Save Sample Set",
            Self::LoadSet => "Load Set from List",
            Self::Page => "Sample Page",
            Self::PlayAll => "Play All Slots",
            Self::TriggerAll => "Trigger All Slots",
            Self::PlayModeAll => "Play Mode All Slots",
            Self::CaptureSource => "Capture Source",
            Self::CellLoadModifier => "Cell Load Modifier",
            Self::CellDeleteModifier => "Cell Delete Modifier",
            Self::CellReverseModifier => "Cell Reverse Modifier",
            Self::CellCaptureModifier => "Cell Capture Modifier",
            Self::SequencerOn => "Sequencer On",
            Self::SequencerRecord => "Sequencer Recording On",
        }
    }

    pub fn is_continuous(self) -> bool {
        matches!(
            self,
            Self::QuantizeSelector | Self::Page | Self::CaptureSource | Self::PlayModeAll
        )
    }

    pub fn is_held(self) -> bool {
        matches!(
            self,
            Self::CellLoadModifier
                | Self::CellDeleteModifier
                | Self::CellReverseModifier
                | Self::CellCaptureModifier
                | Self::TriggerAll
        )
    }
}

pub fn cell_of(command: u32) -> Option<(usize, usize)> {
    let index = if (CELL_TRIGGER_BASE..CELL_STATE_BASE).contains(&command) {
        command - CELL_TRIGGER_BASE
    } else if (CELL_STATE_BASE..CELL_STATE_BASE + (SLOTS * CELLS) as u32).contains(&command) {
        command - CELL_STATE_BASE
    } else {
        return None;
    };
    let index = index as usize;
    Some((index / CELLS, index % CELLS))
}

pub fn cell_command(slot: usize, cell: usize) -> u32 {
    CELL_TRIGGER_BASE + (slot * CELLS + cell) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellMode {
    #[default]
    Trigger,
    Load,
    Delete,
    Reverse,
    Capture,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_round_trip_their_ids() {
        for part in SampleSlotPart::ALL {
            assert_eq!(SampleSlotPart::from_command(part.command()), Some(part));
        }
        for part in SamplePart::ALL {
            assert_eq!(SamplePart::from_command(part.command()), Some(part));
        }
        assert_eq!(SampleSlotPart::from_command(999), None);
    }

    #[test]
    fn cell_ids_cover_the_grid() {
        assert_eq!(cell_of(601), Some((0, 0)));
        assert_eq!(cell_of(601 + 16 * 2 + 5), Some((2, 5)));
        assert_eq!(cell_of(664), Some((3, 15)));
        assert_eq!(cell_of(665), Some((0, 0)), "the state of cell 1");
        assert_eq!(cell_of(728), Some((3, 15)));
        assert_eq!(cell_of(729), None);
        assert_eq!(cell_command(2, 5), 601 + 37);
    }
}
