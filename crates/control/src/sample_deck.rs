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
    StopActive,
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
            Self::TriggerSelected | Self::StopSelected | Self::StopActive | Self::DeletePattern => {
                0
            }
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
            Self::StopActive => "Stop Active Cell",
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
    CellTypeModifier,
    CellSizeModifier,
    CellAdjust,
    CellAdjustReset,
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
            Self::SequencerRecord
            | Self::CellTypeModifier
            | Self::CellSizeModifier
            | Self::CellAdjust
            | Self::CellAdjustReset => 0,
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
            Self::CellTypeModifier => "Cell Type Modifier",
            Self::CellSizeModifier => "Cell Size Modifier",
            Self::CellAdjust => "Cell Adjust",
            Self::CellAdjustReset => "Cell Adjust Reset",
        }
    }

    pub fn is_continuous(self) -> bool {
        matches!(
            self,
            Self::QuantizeSelector
                | Self::Page
                | Self::CaptureSource
                | Self::PlayModeAll
                | Self::CellAdjust
        )
    }

    pub fn is_held(self) -> bool {
        matches!(
            self,
            Self::CellLoadModifier
                | Self::CellDeleteModifier
                | Self::CellReverseModifier
                | Self::CellCaptureModifier
                | Self::CellTypeModifier
                | Self::CellSizeModifier
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
    /// A cell pressed toggles the chosen [`CellProperty`] of that cell.
    Type,
    /// A cell held is the one whose length the adjust control changes.
    Size,
}

/// The cell setting that [`CellMode::Type`] toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellProperty {
    #[default]
    PlayType,
    TriggerType,
    Sync,
}

impl CellProperty {
    pub const ALL: [Self; 3] = [Self::PlayType, Self::TriggerType, Self::Sync];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&p| p == self).unwrap_or(0)
    }
}

/// How a mode button moved through its press: whether a cell was used
/// while it was held, and whether it was pressed again to leave a latched
/// mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModeLatch {
    pub used: bool,
    pub leaving: bool,
}

/// The cell mode after a press or release of the button for `mode`.
///
/// TYPE latches: a short press with no cell used keeps the mode on until
/// the button is pressed again; held while cells are used, it ends on
/// release. SIZE toggles on each press. The other modes last while held.
pub fn mode_button(
    current: CellMode,
    mode: CellMode,
    latch: &mut ModeLatch,
    pressed: bool,
) -> CellMode {
    match (mode, pressed) {
        (CellMode::Type, true) if current == CellMode::Trigger => {
            *latch = ModeLatch::default();
            CellMode::Type
        }
        (CellMode::Type, true) if current == CellMode::Type => {
            latch.leaving = true;
            current
        }
        (CellMode::Type, false) if current == CellMode::Type => {
            if latch.used || latch.leaving {
                CellMode::Trigger
            } else {
                current
            }
        }
        (CellMode::Size, true) => match current {
            CellMode::Trigger => CellMode::Size,
            CellMode::Size => CellMode::Trigger,
            other => other,
        },
        (CellMode::Type | CellMode::Size, _) => current,
        (_, true) if current == CellMode::Trigger => mode,
        (_, false) if current == mode => CellMode::Trigger,
        _ => current,
    }
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
    fn type_latches_on_a_short_press_and_ends_after_use() {
        let mut latch = ModeLatch::default();
        let press = |m, l: &mut ModeLatch| mode_button(m, CellMode::Type, l, true);
        let release = |m, l: &mut ModeLatch| mode_button(m, CellMode::Type, l, false);
        let m = press(CellMode::Trigger, &mut latch);
        assert_eq!(m, CellMode::Type);
        let m = release(m, &mut latch);
        assert_eq!(m, CellMode::Type, "a tap keeps it");
        let m = press(m, &mut latch);
        assert_eq!(m, CellMode::Type, "until pressed again");
        assert_eq!(release(m, &mut latch), CellMode::Trigger);
        let m = press(CellMode::Trigger, &mut latch);
        latch.used = true;
        assert_eq!(release(m, &mut latch), CellMode::Trigger, "held and used");
        assert_eq!(
            press(CellMode::Load, &mut latch),
            CellMode::Load,
            "another mode wins"
        );
    }

    #[test]
    fn size_toggles_and_the_others_last_while_held() {
        let mut latch = ModeLatch::default();
        let m = mode_button(CellMode::Trigger, CellMode::Size, &mut latch, true);
        assert_eq!(m, CellMode::Size);
        let m = mode_button(m, CellMode::Size, &mut latch, false);
        assert_eq!(m, CellMode::Size);
        assert_eq!(
            mode_button(m, CellMode::Size, &mut latch, true),
            CellMode::Trigger
        );
        let m = mode_button(CellMode::Trigger, CellMode::Load, &mut latch, true);
        assert_eq!(m, CellMode::Load);
        assert_eq!(
            mode_button(m, CellMode::Load, &mut latch, false),
            CellMode::Trigger
        );
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
