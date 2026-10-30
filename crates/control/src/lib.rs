pub mod calibration;
pub mod controller;
pub mod device;
pub mod display;
pub mod feedback;
pub mod focus;
pub mod kontrol;
pub mod loop_recorder;
pub mod midi;
pub mod modifier;
mod move_size;
pub mod output;
pub mod pads;
pub mod poti;
pub mod profile;
pub mod relative;
pub mod s4mk3;
pub mod sample_deck;
pub mod session;
pub mod state;
pub mod stem;
pub mod strip;
pub mod table;
pub mod tsi_import;

use std::path::Path;

use serde::{Deserialize, Serialize};

pub use loop_recorder::LoopRecorderPart;
pub use move_size::{MoveMode, MoveSize};
pub use sample_deck::{CellMode, SamplePart, SampleSlotPart};
pub use stem::{LoadMode, StemPart};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MidiKind {
    Note,
    Cc,
    PitchBend,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Input {
    Midi {
        channel: u8,
        kind: MidiKind,
        number: u8,
    },
    Key(String),
    Hid(String),
    MidiPair {
        lsb: MidiId,
        msb: MidiId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MidiId {
    pub channel: u8,
    pub kind: MidiKind,
    pub number: u8,
}

impl MidiId {
    pub fn input(self) -> Input {
        Input::Midi {
            channel: self.channel,
            kind: self.kind,
            number: self.number,
        }
    }
}

impl Input {
    pub fn midi_id(&self) -> Option<MidiId> {
        match *self {
            Self::Midi {
                channel,
                kind,
                number,
            } => Some(MidiId {
                channel,
                kind,
                number,
            }),
            _ => None,
        }
    }
}

impl std::fmt::Display for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Midi {
                channel,
                kind,
                number,
            } => match kind {
                MidiKind::Note => {
                    write!(f, "Ch{:02} Note {}", channel + 1, midi::note_name(*number))
                }
                MidiKind::Cc => write!(f, "Ch{:02} CC {number:03}", channel + 1),
                MidiKind::PitchBend => write!(f, "Ch{:02} Pitch Bend", channel + 1),
            },
            Self::Key(k) => write!(f, "Key {k}"),
            Self::Hid(c) => write!(f, "HID {c}"),
            Self::MidiPair { lsb, msb } => write!(f, "{} + {}", lsb.input(), msb.input()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum Action {
    PlayPause {
        deck: usize,
    },
    Cue {
        deck: usize,
    },
    Cup {
        deck: usize,
    },
    Sync {
        deck: usize,
    },
    Master {
        deck: usize,
    },
    Keylock {
        deck: usize,
    },
    KeyAdjust {
        deck: usize,
    },
    KeyFine {
        deck: usize,
    },
    KeySemitone {
        deck: usize,
        direction: f64,
    },
    KeyReset {
        deck: usize,
    },
    Hotcue {
        deck: usize,
        slot: usize,
    },
    DeleteHotcue {
        deck: usize,
        slot: usize,
    },
    CueStore {
        deck: usize,
    },
    CueSetStore {
        deck: usize,
    },
    CueDelete {
        deck: usize,
    },
    CueJump {
        deck: usize,
        direction: f64,
    },
    CueMove {
        deck: usize,
    },
    Balance {
        channel: usize,
    },
    MasterBalance,
    Mute {
        channel: usize,
    },
    AutoFade {
        direction: f64,
    },
    DuplicateDeck {
        deck: usize,
        from: usize,
    },
    CopyPhase {
        deck: usize,
    },
    TempoSync {
        deck: usize,
    },
    PhaseSync {
        deck: usize,
    },
    MoveMode {
        deck: usize,
        mode: MoveMode,
    },
    MoveSize {
        deck: usize,
        size: MoveSize,
    },
    Move {
        deck: usize,
        direction: f64,
    },
    CueType {
        deck: usize,
        kind: i32,
    },
    MapHotcue {
        deck: usize,
        slot: usize,
    },
    MasterTap,
    MasterTick,
    ClockSend,
    ClockResync,
    ClockExternal,
    ClockLink,
    LiveInput {
        deck: usize,
    },
    LinkDownbeat,
    Modifier {
        index: usize,
    },
    RecordStop,
    RecordCut,
    RecordGain,
    LoadLastRecording {
        deck: usize,
    },
    LoadNext {
        deck: usize,
    },
    LoadPrevious {
        deck: usize,
    },
    DeckTick {
        deck: usize,
    },
    BeatTap {
        deck: usize,
    },
    LoopIn {
        deck: usize,
    },
    LoopOut {
        deck: usize,
    },
    LoopActive {
        deck: usize,
    },
    FastLoop {
        deck: usize,
    },
    LoopSize {
        deck: usize,
        beats: f64,
    },
    BeatJump {
        deck: usize,
        beats: f64,
    },
    LoopMove {
        deck: usize,
    },
    Tempo {
        deck: usize,
    },
    TempoRelative {
        deck: usize,
    },
    TempoBend {
        deck: usize,
        direction: f64,
    },
    Volume {
        channel: usize,
    },
    Gain {
        channel: usize,
    },
    Eq {
        channel: usize,
        band: usize,
    },
    EqKill {
        channel: usize,
        band: usize,
    },
    Filter {
        channel: usize,
    },
    MixerFxOn {
        channel: usize,
    },
    MixerFxSelect {
        slot: usize,
    },
    MonitorCue {
        channel: usize,
    },
    Crossfader,
    MasterVolume,
    CueVolume,
    CueMix,
    FxOn {
        unit: usize,
    },
    FxDryWet {
        unit: usize,
    },
    FxKnob {
        unit: usize,
        knob: usize,
    },
    FxButton {
        unit: usize,
        button: usize,
    },
    FxLfoReset,
    FxSnapshotStore {
        unit: usize,
    },
    FxSnapshotLoad {
        unit: usize,
    },
    FxMode {
        unit: usize,
    },
    FxSelect {
        unit: usize,
        slot: usize,
    },
    PatternStep {
        unit: usize,
        step: usize,
    },
    PatternReset {
        unit: usize,
    },
    PatternClear {
        unit: usize,
    },
    PatternRoll {
        unit: usize,
    },
    PatternKit {
        unit: usize,
    },
    BrowserUpDown,
    LoadSelected {
        deck: usize,
    },
    LoadSelectedAs {
        deck: usize,
        mode: LoadMode,
    },
    LoopRecorder {
        part: LoopRecorderPart,
    },
    Stem {
        deck: usize,
        stem: usize,
        part: StemPart,
    },
    #[serde(alias = "RemixSlot")]
    SampleSlot {
        deck: usize,
        slot: usize,
        part: SampleSlotPart,
    },
    #[serde(alias = "Remix")]
    SampleDeck {
        deck: usize,
        part: SamplePart,
    },
    #[serde(alias = "RemixCell")]
    SampleCell {
        deck: usize,
        slot: usize,
        cell: usize,
    },
    #[serde(alias = "RemixCellStopLoad")]
    SampleCellStopLoad {
        deck: usize,
        slot: usize,
        cell: usize,
    },
    #[serde(alias = "DeckFlavor")]
    DeckType {
        deck: usize,
    },
    Snap,
    Quantize,
    JogTouch {
        deck: usize,
    },
    JogTurn {
        deck: usize,
    },
    JogMode {
        deck: usize,
        turntable: bool,
    },
    MotorOff {
        deck: usize,
    },
    FxAssign {
        channel: usize,
        unit: usize,
    },
    FxAssignHeld {
        channel: usize,
        unit: usize,
    },
    XfaderAssign {
        channel: usize,
    },
    XfaderSide {
        channel: usize,
        right: bool,
    },
    XfaderCurve,
    ReturnToStart {
        deck: usize,
    },
    TempoReset {
        deck: usize,
    },
    TempoLock {
        deck: usize,
    },
    BrowserTree,
    JogSearch {
        deck: usize,
    },
    /// Moves the playhead to a share of the track, from 0 (start) to 1 (end).
    Seek {
        deck: usize,
    },
    GridAdjust {
        deck: usize,
    },
    PrepListToggle,
    PrepListJump,
    BrowserView,
    BrowserPreview,
    BrowserFavorites,
    BrowserFavoriteStep,
    PreviewLoad,
    PreviewSeek,
    LoopSizeStep {
        deck: usize,
    },
    TempoRange {
        deck: usize,
    },
    TempoRangeCycle {
        deck: usize,
    },
    FastLoopSize {
        deck: usize,
        beats: f64,
    },
    FaderStart {
        deck: usize,
        play: bool,
    },
    KeyShiftPad {
        deck: usize,
        pad: usize,
    },
    KeyShiftPage {
        deck: usize,
        direction: f64,
    },
    KeyboardPad {
        deck: usize,
        pad: usize,
    },
    KeyboardPage {
        deck: usize,
        direction: f64,
    },
    KeyboardDelete {
        deck: usize,
    },
    BeatJumpPad {
        deck: usize,
        pad: usize,
    },
    PadFx {
        deck: usize,
        bank: usize,
        pad: usize,
    },
    PadFxBeat {
        deck: usize,
        direction: f64,
    },
    BeatJumpPage {
        deck: usize,
        direction: f64,
        far: bool,
    },
    #[serde(alias = "Flux")]
    Slip {
        deck: usize,
    },
    Reverse {
        deck: usize,
    },
    Freeze {
        deck: usize,
    },
    DeckSize {
        deck: usize,
    },
    Slice {
        deck: usize,
        slot: usize,
    },
    DeckFocus {
        deck: usize,
    },
    FocusToggle,
    MicToggle,
    #[serde(alias = "FluxedLoop")]
    SlipRoll {
        deck: usize,
        beats: f64,
    },
}

impl Action {
    pub fn is_continuous(self) -> bool {
        match self {
            Self::SampleSlot { part, .. } => return part.is_continuous(),
            Self::SampleDeck { part, .. } => return part.is_continuous(),
            Self::LoopRecorder { part } => return part.is_continuous(),
            _ => {}
        }
        matches!(
            self,
            Self::Tempo { .. }
                | Self::Stem {
                    part: StemPart::Volume
                        | StemPart::Filter
                        | StemPart::FxAmount
                        | StemPart::FilterEngaged,
                    ..
                }
                | Self::DeckType { .. }
                | Self::KeyAdjust { .. }
                | Self::KeyFine { .. }
                | Self::Balance { .. }
                | Self::MasterBalance
                | Self::Volume { .. }
                | Self::Gain { .. }
                | Self::Eq { .. }
                | Self::Filter { .. }
                | Self::Crossfader
                | Self::MasterVolume
                | Self::CueVolume
                | Self::CueMix
                | Self::FxDryWet { .. }
                | Self::FxKnob { .. }
                | Self::FxSelect { .. }
                | Self::RecordGain
                | Self::BrowserUpDown
                | Self::BrowserTree
                | Self::BrowserFavoriteStep
                | Self::PreviewSeek
                | Self::FxMode { .. }
                | Self::JogTurn { .. }
                | Self::JogSearch { .. }
                | Self::Seek { .. }
                | Self::GridAdjust { .. }
                | Self::LoopSizeStep { .. }
                | Self::TempoRange { .. }
                | Self::LoopMove { .. }
                | Self::CueMove { .. }
                | Self::XfaderAssign { .. }
                | Self::XfaderCurve
        )
    }

    pub fn is_stepped(self) -> bool {
        !self.is_continuous()
            || matches!(
                self,
                Self::SampleSlot { .. }
                    | Self::SampleDeck { .. }
                    | Self::DeckType { .. }
                    | Self::BrowserUpDown
                    | Self::BrowserTree
                    | Self::BrowserFavoriteStep
                    | Self::FxMode { .. }
                    | Self::LoopSizeStep { .. }
                    | Self::TempoRange { .. }
                    | Self::FxSelect { .. }
                    | Self::LoopMove { .. }
                    | Self::CueMove { .. }
                    | Self::KeyAdjust { .. }
                    | Self::KeyFine { .. }
                    | Self::XfaderAssign { .. }
                    | Self::XfaderCurve
            )
    }

    pub fn deck_mut(&mut self) -> Option<&mut usize> {
        match self {
            Self::PlayPause { deck }
            | Self::LoadLastRecording { deck }
            | Self::LoadNext { deck }
            | Self::LoadPrevious { deck }
            | Self::Cue { deck }
            | Self::Cup { deck }
            | Self::Sync { deck }
            | Self::Master { deck }
            | Self::Keylock { deck }
            | Self::KeyAdjust { deck }
            | Self::KeyFine { deck }
            | Self::KeySemitone { deck, .. }
            | Self::KeyReset { deck }
            | Self::Hotcue { deck, .. }
            | Self::DeleteHotcue { deck, .. }
            | Self::CueStore { deck }
            | Self::CueSetStore { deck }
            | Self::CueDelete { deck }
            | Self::CueJump { deck, .. }
            | Self::CueMove { deck }
            | Self::CopyPhase { deck }
            | Self::TempoSync { deck }
            | Self::PhaseSync { deck }
            | Self::DuplicateDeck { deck, .. }
            | Self::MoveMode { deck, .. }
            | Self::MoveSize { deck, .. }
            | Self::Move { deck, .. }
            | Self::CueType { deck, .. }
            | Self::MapHotcue { deck, .. }
            | Self::BeatTap { deck }
            | Self::DeckTick { deck }
            | Self::LoopIn { deck }
            | Self::LoopOut { deck }
            | Self::LoopActive { deck }
            | Self::FastLoop { deck }
            | Self::LoopSize { deck, .. }
            | Self::BeatJump { deck, .. }
            | Self::LoopMove { deck }
            | Self::Tempo { deck }
            | Self::TempoRelative { deck }
            | Self::TempoBend { deck, .. }
            | Self::LoadSelected { deck }
            | Self::LoadSelectedAs { deck, .. }
            | Self::Stem { deck, .. }
            | Self::SampleSlot { deck, .. }
            | Self::SampleDeck { deck, .. }
            | Self::SampleCell { deck, .. }
            | Self::SampleCellStopLoad { deck, .. }
            | Self::DeckType { deck }
            | Self::JogTouch { deck }
            | Self::JogTurn { deck }
            | Self::JogMode { deck, .. }
            | Self::MotorOff { deck }
            | Self::ReturnToStart { deck }
            | Self::TempoReset { deck }
            | Self::TempoLock { deck }
            | Self::LiveInput { deck }
            | Self::JogSearch { deck }
            | Self::Seek { deck }
            | Self::GridAdjust { deck }
            | Self::LoopSizeStep { deck }
            | Self::TempoRange { deck }
            | Self::TempoRangeCycle { deck }
            | Self::FastLoopSize { deck, .. }
            | Self::FaderStart { deck, .. }
            | Self::KeyShiftPad { deck, .. }
            | Self::KeyShiftPage { deck, .. }
            | Self::KeyboardPad { deck, .. }
            | Self::KeyboardPage { deck, .. }
            | Self::KeyboardDelete { deck }
            | Self::BeatJumpPad { deck, .. }
            | Self::PadFx { deck, .. }
            | Self::PadFxBeat { deck, .. }
            | Self::BeatJumpPage { deck, .. }
            | Self::Slip { deck }
            | Self::Reverse { deck }
            | Self::Freeze { deck }
            | Self::DeckSize { deck }
            | Self::Slice { deck, .. }
            | Self::SlipRoll { deck, .. } => Some(deck),
            _ => None,
        }
    }

    pub fn channel_mut(&mut self) -> Option<&mut usize> {
        match self {
            Self::Balance { channel }
            | Self::Mute { channel }
            | Self::Volume { channel }
            | Self::Gain { channel }
            | Self::Eq { channel, .. }
            | Self::EqKill { channel, .. }
            | Self::Filter { channel }
            | Self::MixerFxOn { channel }
            | Self::MonitorCue { channel }
            | Self::FxAssign { channel, .. }
            | Self::FxAssignHeld { channel, .. }
            | Self::XfaderAssign { channel }
            | Self::XfaderSide { channel, .. } => Some(channel),
            _ => None,
        }
    }

    pub fn unit_mut(&mut self) -> Option<&mut usize> {
        match self {
            Self::FxOn { unit }
            | Self::FxDryWet { unit }
            | Self::FxKnob { unit, .. }
            | Self::FxButton { unit, .. }
            | Self::FxSelect { unit, .. }
            | Self::FxSnapshotStore { unit }
            | Self::FxSnapshotLoad { unit }
            | Self::FxMode { unit }
            | Self::PatternStep { unit, .. }
            | Self::PatternReset { unit }
            | Self::PatternClear { unit }
            | Self::PatternRoll { unit }
            | Self::PatternKit { unit } => Some(unit),
            _ => None,
        }
    }

    #[must_use]
    pub fn on_deck(mut self, deck: usize) -> Self {
        if let Some(d) = self.deck_mut() {
            *d = deck;
        } else if let Some(c) = self.channel_mut() {
            *c = deck;
        }
        self
    }

    pub fn has_deck(self) -> bool {
        self.on_deck(0) != self.on_deck(1)
    }

    pub fn default_interaction(self) -> Interaction {
        match self {
            Self::SampleSlot { part, .. } if part.is_held() => Interaction::Hold,
            Self::SampleSlot { part, .. } if part.is_toggle() => Interaction::Toggle,
            Self::SampleSlot { part, .. } if part.is_continuous() => Interaction::Relative,
            Self::SampleDeck { part, .. } if part.is_held() => Interaction::Hold,
            Self::LoopRecorder { part } if part.is_toggle() => Interaction::Toggle,
            Self::SampleDeck { part, .. } if part.is_continuous() => Interaction::Relative,
            Self::SampleCell { .. }
            | Self::MixerFxSelect { .. }
            | Self::FxAssignHeld { .. }
            | Self::MixerFxOn { .. }
            | Self::BrowserPreview
            | Self::BrowserFavorites
            | Self::Cue { .. }
            | Self::Cup { .. }
            | Self::TempoBend { .. }
            | Self::JogTouch { .. }
            | Self::FaderStart { .. }
            | Self::BeatJump { .. }
            | Self::Move { .. }
            | Self::KeyReset { .. }
            | Self::Slice { .. }
            | Self::Reverse { .. }
            | Self::Modifier { .. }
            | Self::PatternRoll { .. }
            | Self::SlipRoll { .. } => Interaction::Hold,
            Self::JogTurn { .. }
            | Self::JogSearch { .. }
            | Self::GridAdjust { .. }
            | Self::BrowserTree
            | Self::BrowserFavoriteStep
            | Self::PreviewSeek
            | Self::FxMode { .. }
            | Self::LoopSizeStep { .. }
            | Self::TempoRange { .. }
            | Self::FxSelect { .. }
            | Self::LoopMove { .. }
            | Self::KeyFine { .. }
            | Self::CueMove { .. }
            | Self::DeckType { .. }
            | Self::PatternKit { .. } => Interaction::Relative,
            Self::FxAssign { .. }
            | Self::Sync { .. }
            | Self::Keylock { .. }
            | Self::MonitorCue { .. }
            | Self::EqKill { .. }
            | Self::FxOn { .. }
            | Self::FxButton { .. }
            | Self::Snap
            | Self::Quantize
            | Self::MasterTick
            | Self::ClockSend
            | Self::ClockExternal
            | Self::RecordStop
            | Self::DeckTick { .. }
            | Self::Mute { .. }
            | Self::Slip { .. }
            | Self::SampleDeck {
                part: SamplePart::QuantizeOn,
                ..
            }
            | Self::Stem {
                part: StemPart::Mute | StemPart::FilterOn | StemPart::FxOn,
                ..
            } => Interaction::Toggle,
            a if a.is_continuous() => Interaction::Direct,
            _ => Interaction::Trigger,
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn label(self) -> String {
        let d = deck_letter;
        match self {
            Self::PlayPause { deck } => format!("Deck {} Play/Pause", d(deck)),
            Self::Cue { deck } => format!("Deck {} Cue", d(deck)),
            Self::Cup { deck } => format!("Deck {} CUP", d(deck)),
            Self::Sync { deck } => format!("Deck {} Sync", d(deck)),
            Self::Master { deck } => format!("Deck {} Master", d(deck)),
            Self::Keylock { deck } => format!("Deck {} Keylock", d(deck)),
            Self::KeyAdjust { deck } => format!("Deck {} Key Adjust", d(deck)),
            Self::KeyFine { deck } => format!("Deck {} Key Adjust Fine", d(deck)),
            Self::KeySemitone { deck, direction } => format!(
                "Deck {} Semitone {}",
                d(deck),
                if direction < 0.0 { "Down" } else { "Up" }
            ),
            Self::KeyReset { deck } => format!("Deck {} Key Reset", d(deck)),
            Self::Hotcue { deck, slot } => format!("Deck {} Hotcue {}", d(deck), slot + 1),
            Self::DeleteHotcue { deck, slot } => {
                format!("Deck {} Delete Hotcue {}", d(deck), slot + 1)
            }
            Self::CueStore { deck } => format!("Deck {} Store Cue/Loop", d(deck)),
            Self::CueSetStore { deck } => format!("Deck {} Set Cue and Store", d(deck)),
            Self::CueDelete { deck } => format!("Deck {} Delete Cue", d(deck)),
            Self::CueJump { deck, direction } => format!(
                "Deck {} {} Cue",
                d(deck),
                if direction < 0.0 { "Previous" } else { "Next" }
            ),
            Self::CueMove { deck } => format!("Deck {} Move Cue", d(deck)),
            Self::CopyPhase { deck } => format!("Deck {} Copy Phase from Master", d(deck)),
            Self::TempoSync { deck } => format!("Deck {} Tempo Sync", d(deck)),
            Self::PhaseSync { deck } => format!("Deck {} Phase Sync", d(deck)),
            Self::DuplicateDeck { deck, from } => {
                format!("Deck {} Duplicate Deck {}", d(deck), d(from))
            }
            Self::Balance { channel } => format!("Ch {} Balance", d(channel)),
            Self::MasterBalance => "Master Balance".into(),
            Self::Mute { channel } => format!("Ch {} Mute", d(channel)),
            Self::AutoFade { direction } => format!(
                "Auto X-Fade {}",
                if direction < 0.0 { "Left" } else { "Right" }
            ),
            Self::MoveMode { deck, mode } => format!("Deck {} Move Mode {}", d(deck), mode.label()),
            Self::MoveSize { deck, size } => format!("Deck {} Move Size {}", d(deck), size.label()),
            Self::Move { deck, direction } => format!(
                "Deck {} Move {}",
                d(deck),
                if direction < 0.0 { "Back" } else { "Forward" }
            ),
            Self::CueType { deck, kind } => format!(
                "Deck {} Cue Type {}",
                d(deck),
                ["Cue", "Fade-In", "Fade-Out", "Load", "Grid", "Loop"]
                    .get(usize::try_from(kind).unwrap_or(usize::MAX))
                    .unwrap_or(&"?")
            ),
            Self::MapHotcue { deck, slot } => format!("Deck {} Map Hotcue {}", d(deck), slot + 1),
            Self::MasterTap => "Master Tempo Tap".into(),
            Self::MasterTick => "Master Tempo Tick".into(),
            Self::ClockSend => "Master Tempo Clock Send".into(),
            Self::ClockResync => "Clock Trigger MIDI Sync".into(),
            Self::ClockExternal => "Clock Int/Ext".into(),
            Self::ClockLink => "Link On/Off".into(),
            Self::LiveInput { deck } => format!("Deck {} Live Input", d(deck)),
            Self::LinkDownbeat => "Link Reset Downbeat".into(),
            Self::Modifier { index } => format!("Modifier #{}", index + 1),
            Self::RecordStop => "Mix Recorder Record/Stop".into(),
            Self::RecordCut => "Mix Recorder Cut".into(),
            Self::RecordGain => "Mix Recorder Gain".into(),
            Self::LoadLastRecording { deck } => {
                format!("Deck {} Load Last Recording", deck_letter(deck))
            }
            Self::LoadNext { deck } => format!("Deck {} Load Next", d(deck)),
            Self::LoadPrevious { deck } => format!("Deck {} Load Previous", d(deck)),
            Self::DeckTick { deck } => format!("Deck {} Tick", d(deck)),
            Self::BeatTap { deck } => format!("Deck {} Beat Tap", d(deck)),
            Self::LoopIn { deck } => format!("Deck {} Loop In", d(deck)),
            Self::LoopOut { deck } => format!("Deck {} Loop Out", d(deck)),
            Self::LoopActive { deck } => format!("Deck {} Loop On/Off", d(deck)),
            Self::FastLoop { deck } => format!("Deck {} Loop Set", d(deck)),
            Self::LoopSize { deck, beats } => {
                format!("Deck {} Loop {}", d(deck), beats_label(beats))
            }
            Self::BeatJump { deck, beats } => {
                format!("Deck {} Jump {:+}", d(deck), beats_label(beats))
            }
            Self::LoopMove { deck } => format!("Deck {} Move by Loop Size", d(deck)),
            Self::Tempo { deck } => format!("Deck {} Tempo", d(deck)),
            Self::TempoRelative { deck } => format!("Deck {} Tempo (relative)", d(deck)),
            Self::TempoBend { deck, direction } => {
                format!(
                    "Deck {} Bend {}",
                    d(deck),
                    if direction < 0.0 { "−" } else { "+" }
                )
            }
            Self::Volume { channel } => format!("Ch {} Volume", d(channel)),
            Self::Gain { channel } => format!("Ch {} Gain", d(channel)),
            Self::Eq { channel, band } => format!(
                "Ch {} EQ {}",
                d(channel),
                ["Low", "Mid", "High", "Mid Low"].get(band).unwrap_or(&"?")
            ),
            Self::EqKill { channel, band } => format!(
                "Ch {} EQ {} Kill",
                d(channel),
                ["Low", "Mid", "High", "Mid Low"].get(band).unwrap_or(&"?")
            ),
            Self::Filter { channel } => format!("Ch {} Filter", d(channel)),
            Self::MixerFxOn { channel } => format!("Ch {} Mixer FX On", d(channel)),
            Self::MixerFxSelect { slot: 0 } => "Mixer FX Select Filter".to_owned(),
            Self::MixerFxSelect { slot } => format!("Mixer FX Select {slot}"),
            Self::MonitorCue { channel } => format!("Ch {} Cue", d(channel)),
            Self::Crossfader => "Crossfader".into(),
            Self::MasterVolume => "Master Volume".into(),
            Self::CueVolume => "Headphone Volume".into(),
            Self::CueMix => "Headphone Cue Mix".into(),
            Self::FxOn { unit } => format!("FX{} On", unit + 1),
            Self::FxDryWet { unit } => format!("FX{} Dry/Wet", unit + 1),
            Self::FxKnob { unit, knob } => format!("FX{} Knob {}", unit + 1, knob + 1),
            Self::FxButton { unit, button } => format!("FX{} Button {}", unit + 1, button + 1),
            Self::FxLfoReset => "Effect LFO Reset".into(),
            Self::FxSnapshotStore { unit } => format!("FX{} Store Snapshot", unit + 1),
            Self::FxSnapshotLoad { unit } => format!("FX{} Load Snapshot", unit + 1),
            Self::FxMode { unit } => format!("FX{} Unit Mode", unit + 1),
            Self::FxSelect { unit, slot } => format!("FX{} Effect {} Select", unit + 1, slot + 1),
            Self::PatternStep { unit, step } => {
                format!("FX{} Drum Machine Step {}", unit + 1, step + 1)
            }
            Self::PatternReset { unit } => format!("FX{} Drum Machine Reset", unit + 1),
            Self::PatternClear { unit } => format!("FX{} Drum Machine Clear", unit + 1),
            Self::PatternKit { unit } => format!("FX{} Drum Machine Kit", unit + 1),
            Self::PatternRoll { unit } => format!("FX{} Drum Machine Roll", unit + 1),
            Self::BrowserUpDown => "Browser Up/Down".into(),
            Self::LoadSelected { deck } => format!("Load Selected → {}", d(deck)),
            Self::LoadSelectedAs { deck, mode } => format!("{} → {}", mode.label(), d(deck)),
            Self::LoopRecorder { part } => format!("Loop Recorder {}", part.label()),
            Self::Stem { deck, stem, part } => {
                format!("Deck {} Stem {} {}", d(deck), stem + 1, part.label())
            }
            Self::SampleSlot { deck, slot, part } => {
                format!("Sample {} Slot {} {}", d(deck), slot + 1, part.label())
            }
            Self::SampleDeck { deck, part } => format!("Sample {} {}", d(deck), part.label()),
            Self::SampleCell { deck, slot, cell } => {
                format!("Sample {} Slot {} Cell {}", d(deck), slot + 1, cell + 1)
            }
            Self::SampleCellStopLoad { deck, slot, cell } => format!(
                "Sample {} Slot {} Cell {} Stop / Load",
                d(deck),
                slot + 1,
                cell + 1
            ),
            Self::DeckType { deck } => format!("Deck {} Type (track / sample)", d(deck)),
            Self::Snap => "Snap".into(),
            Self::Quantize => "Quantize".into(),
            Self::JogTouch { deck } => format!("Deck {} Jog Touch", d(deck)),
            Self::JogTurn { deck } => format!("Deck {} Jog Turn", d(deck)),
            Self::JogMode { deck, turntable } => format!(
                "Deck {} Jog Mode {}",
                d(deck),
                if turntable { "TT" } else { "Jog" }
            ),
            Self::MotorOff { deck } => format!("Deck {} Motor Off", d(deck)),
            Self::FxAssign { channel, unit } => {
                format!("Ch {} → FX{}", d(channel), unit + 1)
            }
            Self::FxAssignHeld { channel, unit } => {
                format!("Ch {} → FX{} (held)", d(channel), unit + 1)
            }
            Self::XfaderAssign { channel } => format!("Ch {} Xfader Assign", d(channel)),
            Self::XfaderSide { channel, right } => format!(
                "Ch {} X-Fader Assign {}",
                d(channel),
                if right { "Right" } else { "Left" }
            ),
            Self::XfaderCurve => "Crossfader Curve".into(),
            Self::ReturnToStart { deck } => format!("Deck {} Return to Start", d(deck)),
            Self::TempoReset { deck } => format!("Deck {} Tempo Reset", d(deck)),
            Self::TempoLock { deck } => format!("Deck {} Tempo Lock", d(deck)),
            Self::BrowserTree => "Browser Tree Up/Down".into(),
            Self::JogSearch { deck } => format!("Deck {} Jog Search", d(deck)),
            Self::Seek { deck } => format!("Deck {} Seek Position", d(deck)),
            Self::GridAdjust { deck } => format!("Deck {} Grid Adjust", d(deck)),
            Self::PrepListToggle => "Preparation List Add/Remove".into(),
            Self::PrepListJump => "Jump to Preparation List".into(),
            Self::BrowserPreview => "Preview Player (hold)".into(),
            Self::BrowserFavorites => "Favorites (hold)".into(),
            Self::BrowserFavoriteStep => "Favorites Selector".into(),
            Self::PreviewLoad => "Preview Player Load Selected".into(),
            Self::PreviewSeek => "Preview Player Seek".into(),
            Self::BrowserView => "Browser Maximize".into(),
            Self::LoopSizeStep { deck } => format!("Deck {} Loop Size", d(deck)),
            Self::TempoRange { deck } => format!("Deck {} Tempo Range", d(deck)),
            Self::TempoRangeCycle { deck } => format!("Deck {} Tempo Range 6/10/16/Wide", d(deck)),
            Self::FastLoopSize { deck, beats } => format!("Deck {} Loop {beats} or Exit", d(deck)),
            Self::FaderStart { deck, play } => format!(
                "Deck {} Fader Start {}",
                d(deck),
                if play { "Play" } else { "Cue" }
            ),
            Self::KeyShiftPad { deck, pad } => {
                format!("Deck {} Key Shift Pad {}", d(deck), pad + 1)
            }
            Self::KeyShiftPage { deck, direction } => format!(
                "Deck {} Key Shift Page {}",
                d(deck),
                if direction < 0.0 { "<" } else { ">" }
            ),
            Self::BeatJumpPad { deck, pad } => {
                format!("Deck {} Beat Jump Pad {}", d(deck), pad + 1)
            }
            Self::PadFx { deck, bank, pad } => {
                format!("Deck {} Pad FX {} Pad {}", d(deck), bank + 1, pad + 1)
            }
            Self::PadFxBeat { deck, direction } => format!(
                "Deck {} Pad FX Beat {}",
                d(deck),
                if direction < 0.0 { "<" } else { ">" }
            ),
            Self::KeyboardPad { deck, pad } => format!("Deck {} Keyboard Pad {}", d(deck), pad + 1),
            Self::KeyboardPage { deck, direction } => format!(
                "Deck {} Keyboard Page {}",
                d(deck),
                if direction < 0.0 { "<" } else { ">" }
            ),
            Self::KeyboardDelete { deck } => format!("Deck {} Keyboard Delete Cue", d(deck)),
            Self::BeatJumpPage {
                deck,
                direction,
                far,
            } => format!(
                "Deck {} Beat Jump Page {}{}",
                d(deck),
                if direction < 0.0 { "<" } else { ">" },
                if far { " Far" } else { "" }
            ),
            Self::Slip { deck } => format!("Deck {} Slip", d(deck)),
            Self::Reverse { deck } => format!("Deck {} Reverse", d(deck)),
            Self::Freeze { deck } => format!("Deck {} Freeze", d(deck)),
            Self::DeckSize { deck } => format!("Deck {} Size", d(deck)),
            Self::Slice { deck, slot } => format!("Deck {} Slice {}", d(deck), slot + 1),
            Self::DeckFocus { deck } => format!("Deck {} Focus", d(deck)),
            Self::FocusToggle => "Toggle Last Focus".into(),
            Self::MicToggle => "Microphone On/Off".into(),
            Self::SlipRoll { deck, beats } => {
                format!("Deck {} Loop Roll {}", d(deck), beats_label(beats))
            }
        }
    }
}

fn beats_label(beats: f64) -> String {
    let a = beats.abs();
    let s = if a >= 1.0 {
        format!("{a}")
    } else {
        format!("1/{}", (1.0 / a).round())
    };
    if beats < 0.0 { format!("-{s}") } else { s }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interaction {
    Trigger,
    Hold,
    Toggle,
    Direct,
    Relative,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct Mapping {
    pub input: Input,
    pub action: Action,
    pub interaction: Interaction,
    #[serde(default)]
    pub invert: bool,
    #[serde(default = "unit_scale", skip_serializing_if = "is_unit_scale")]
    pub scale: f32,
    #[serde(default, skip_serializing_if = "modifier::Gate::is_empty")]
    pub gate: modifier::Gate,
    #[serde(default, skip_serializing_if = "relative::Encoder::is_default")]
    pub encoder: relative::Encoder,
    #[serde(
        default = "full_sensitivity",
        skip_serializing_if = "is_full_sensitivity"
    )]
    pub sensitivity: f32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub acceleration: f32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub soft_takeover: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_repeat: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub on_release: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub focus: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub override_factory: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub positions: Vec<(u8, f32)>,
}

pub fn on_port(port: Option<&str>, source: Option<&str>) -> bool {
    port.is_none_or(|p| source == Some(p))
}

fn unit_scale() -> f32 {
    1.0
}

fn full_sensitivity() -> f32 {
    100.0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_full_sensitivity(v: &f32) -> bool {
    (*v - 100.0).abs() < f32::EPSILON
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

impl Mapping {
    pub fn new(input: Input, action: Action, interaction: Interaction) -> Self {
        Self {
            input,
            action,
            interaction,
            invert: false,
            scale: 1.0,
            gate: modifier::Gate::default(),
            encoder: relative::Encoder::default(),
            sensitivity: full_sensitivity(),
            acceleration: 0.0,
            soft_takeover: false,
            auto_repeat: false,
            on_release: false,
            focus: false,
            port: None,
            device: None,
            comment: String::new(),
            override_factory: false,
            positions: Vec::new(),
        }
    }

    pub fn target(&self, focus: usize) -> Action {
        if self.focus {
            self.action.on_deck(focus)
        } else {
            self.action
        }
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_unit_scale(scale: &f32) -> bool {
    (*scale - 1.0).abs() < f32::EPSILON
}

pub const RELATIVE_STEP: f32 = 1.0 / 64.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trigger {
    Press,
    Release,
    Set(f32),
    Delta(f32),
    Reset,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputEvent {
    pub input: Input,
    pub value: f32,
    pub raw: u8,
    pub pressed: Option<bool>,
}

const PAIR_MAX: f32 = 16_383.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PairRange {
    pub lsb: MidiId,
    pub msb: MidiId,
    pub min: f32,
    pub max: f32,
}

enum Pairing {
    Single,
    FirstHalf,
    Pair(InputEvent),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MappingSet {
    pub mappings: Vec<Mapping>,
    #[serde(default)]
    pub midi_inputs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<output::OutMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub midi_outputs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pair_ranges: Vec<PairRange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub port_messages: Vec<session::PortMessages>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<device::Device>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables: Vec<table::Table>,
}

impl MappingSet {
    pub fn set_pair_range(&mut self, range: PairRange) {
        self.pair_ranges
            .retain(|r| !(r.lsb == range.lsb && r.msb == range.msb));
        if (range.min, range.max) != (0.0, PAIR_MAX) {
            self.pair_ranges.push(range);
        }
    }

    pub fn default_keyboard() -> Vec<Mapping> {
        let key = |k: &str, action: Action| {
            Mapping::new(Input::Key(k.into()), action, action.default_interaction())
        };
        vec![
            key("Q", Action::Cue { deck: 0 }),
            key("W", Action::PlayPause { deck: 0 }),
            key("O", Action::Cue { deck: 1 }),
            key("P", Action::PlayPause { deck: 1 }),
        ]
    }

    pub fn load(path: &Path) -> std::io::Result<(Self, usize)> {
        let text = std::fs::read_to_string(path)?;
        Self::from_json(&text).map_err(std::io::Error::other)
    }

    pub fn from_json(text: &str) -> serde_json::Result<(Self, usize)> {
        #[derive(Deserialize)]
        struct Raw {
            mappings: Vec<serde_json::Value>,
            #[serde(default)]
            midi_inputs: Vec<String>,
            #[serde(default)]
            outputs: Vec<serde_json::Value>,
            #[serde(default)]
            midi_outputs: Vec<String>,
            #[serde(default)]
            pair_ranges: Vec<serde_json::Value>,
            #[serde(default)]
            port_messages: Vec<serde_json::Value>,
            #[serde(default)]
            devices: Vec<serde_json::Value>,
            #[serde(default)]
            tables: Vec<serde_json::Value>,
        }
        fn readable<T: serde::de::DeserializeOwned>(raw: Vec<serde_json::Value>) -> Vec<T> {
            raw.into_iter()
                .filter_map(|m| serde_json::from_value(m).ok())
                .collect()
        }
        fn table(mut raw: serde_json::Value) -> Option<(table::Table, usize, usize)> {
            let mut rows = |key: &str| match raw.as_object_mut().and_then(|o| o.remove(key)) {
                Some(serde_json::Value::Array(rows)) => rows,
                _ => Vec::new(),
            };
            let (mappings, outputs) = (rows("mappings"), rows("outputs"));
            let total = mappings.len() + outputs.len();
            let mut t: table::Table = serde_json::from_value(raw).ok()?;
            t.mappings = readable(mappings);
            t.outputs = readable(outputs);
            let read = t.mappings.len() + t.outputs.len();
            Some((t, total, read))
        }
        let raw: Raw = serde_json::from_str(text)?;
        let mut total = raw.mappings.len() + raw.outputs.len();
        let mut tables = Vec::new();
        let mut read_in_tables = 0;
        for (t, all, read) in raw.tables.into_iter().filter_map(table) {
            total += all;
            read_in_tables += read;
            tables.push(t);
        }
        let set = Self {
            mappings: readable(raw.mappings),
            midi_inputs: raw.midi_inputs,
            outputs: readable(raw.outputs),
            midi_outputs: raw.midi_outputs,
            pair_ranges: readable(raw.pair_ranges),
            port_messages: readable(raw.port_messages),
            devices: readable(raw.devices),
            tables,
        };
        let skipped = total - set.mappings.len() - set.outputs.len() - read_in_tables;
        Ok((set, skipped))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    pub fn overrides(&self, input: &Input) -> bool {
        self.mappings
            .iter()
            .any(|m| m.override_factory && &m.input == input)
    }

    pub fn upsert(&mut self, mapping: Mapping) {
        self.mappings.retain(|m| {
            !(m.input == mapping.input && m.action == mapping.action && m.port == mapping.port)
        });
        self.mappings.push(mapping);
    }

    pub fn map(&self, event: &InputEvent) -> Vec<(Action, Trigger)> {
        self.map_source(
            event,
            &mut relative::Source::default(),
            &feedback::Feedback::default(),
            &|_| None,
        )
    }

    pub fn map_source(
        &self,
        event: &InputEvent,
        source: &mut relative::Source,
        shown: &feedback::Feedback,
        current: &dyn Fn(Action) -> Option<f32>,
    ) -> Vec<(Action, Trigger)> {
        match self.pair(event, source) {
            Pairing::Single => self.map_one(event, source, shown, current),
            Pairing::FirstHalf => Vec::new(),
            Pairing::Pair(paired) => self.map_one(&paired, source, shown, current),
        }
    }

    fn pair(&self, event: &InputEvent, source: &mut relative::Source) -> Pairing {
        let Some(id) = event.input.midi_id() else {
            return Pairing::Single;
        };
        let previous = source.last_midi.replace((id, event.raw));
        let Some((before, before_raw)) = previous.filter(|(p, _)| *p != id) else {
            return Pairing::Single;
        };
        let port = source.port.as_deref();
        let mapped = |input: &Input| {
            self.mappings
                .iter()
                .any(|m| &m.input == input && on_port(m.port.as_deref(), port))
        };
        let completed = Input::MidiPair {
            lsb: id,
            msb: before,
        };
        if mapped(&completed) {
            let value = f32::from(u16::from(before_raw) << 7 | u16::from(event.raw));
            let (min, max) = self
                .pair_ranges
                .iter()
                .find(|r| r.lsb == id && r.msb == before)
                .map_or((0.0, PAIR_MAX), |r| (r.min, r.max));
            return Pairing::Pair(InputEvent {
                input: completed,
                value: (value - min) / (max - min),
                raw: before_raw,
                pressed: None,
            });
        }
        if mapped(&Input::MidiPair {
            lsb: before,
            msb: id,
        }) {
            return Pairing::FirstHalf;
        }
        Pairing::Single
    }

    fn map_one(
        &self,
        event: &InputEvent,
        source: &mut relative::Source,
        shown: &feedback::Feedback,
        current: &dyn Fn(Action) -> Option<f32>,
    ) -> Vec<(Action, Trigger)> {
        let before = source.modifiers;
        let previous = source.previous.get(&event.input).copied().unwrap_or(0);
        source.held.retain(|h| h.event.input != event.input);
        let mut out = Vec::new();
        for (i, m) in self.mappings.iter().enumerate() {
            if m.input != event.input || !on_port(m.port.as_deref(), source.port.as_deref()) {
                continue;
            }
            if m.auto_repeat && m.interaction == Interaction::Relative && steps_as_button(m, event)
            {
                source.held.push(relative::Held {
                    mapping: i,
                    event: event.clone(),
                    ticks: 0,
                });
            }
            let focus = self.target_deck(m, shown.focus);
            if !before.hold(&m.gate.conditions) || !state::hold_on(&m.gate.states, shown, focus) {
                continue;
            }
            let t = if m.interaction == Interaction::Relative
                && event.pressed.is_none()
                && m.encoder != relative::Encoder::Button
                && matches!(event.input, Input::Midi { .. })
            {
                let rest = m
                    .action
                    .is_stepped()
                    .then(|| source.rest.entry(i).or_default());
                relative_trigger(m, event, previous, rest)
            } else {
                trigger(m, event)
            };
            let Some(t) = t else {
                continue;
            };
            let action = m.target(focus);
            if let Trigger::Set(new) = t
                && m.soft_takeover
                && event.pressed.is_none()
                && let Some(value) = current(action)
            {
                let old = f32::from(previous) / 127.0;
                let old = if m.invert { 1.0 - old } else { old };
                if !relative::takes_over(value, old, new) {
                    continue;
                }
            }
            match action {
                Action::Modifier { index } => {
                    source
                        .modifiers
                        .apply(index, m.interaction, m.gate.value, t);
                }
                action => out.push((action, t)),
            }
        }
        if event.pressed.is_none() {
            source.previous.insert(event.input.clone(), event.raw);
        }
        out
    }
}

impl MappingSet {
    pub fn repeat_tick(
        &self,
        source: &mut relative::Source,
        shown: &feedback::Feedback,
    ) -> Vec<(Action, Trigger)> {
        let relative::Source {
            held, modifiers, ..
        } = source;
        let mut out = Vec::new();
        for h in held {
            h.ticks += 1;
            let Some(m) = self.mappings.get(h.mapping) else {
                continue;
            };
            let focus = self.target_deck(m, shown.focus);
            if h.ticks <= relative::REPEAT_DELAY
                || !modifiers.hold(&m.gate.conditions)
                || !state::hold_on(&m.gate.states, shown, focus)
            {
                continue;
            }
            let Some(t) = trigger(m, &h.event) else {
                continue;
            };
            match m.target(focus) {
                Action::Modifier { index } => {
                    modifiers.apply(index, m.interaction, m.gate.value, t);
                }
                action => out.push((action, t)),
            }
        }
        out
    }
}

fn relative_trigger(
    m: &Mapping,
    e: &InputEvent,
    previous: u8,
    rest: Option<&mut f32>,
) -> Option<Trigger> {
    let d = m.encoder.change(e.raw, previous);
    let d = if m.invert { -d } else { d };
    let steps = relative::steps(d, m.sensitivity, m.acceleration, rest) * m.scale;
    (steps != 0.0).then_some(Trigger::Delta(steps))
}

fn down(e: &InputEvent, swap: bool) -> bool {
    match e.pressed {
        Some(p) => p != swap,
        None if swap => e.value < 1.0,
        None => e.value > 0.0,
    }
}

fn steps_as_button(m: &Mapping, e: &InputEvent) -> bool {
    let button = e.pressed.is_some() || m.encoder == relative::Encoder::Button;
    button && down(e, m.on_release)
}

const CENTRE_ZONE: f32 = 0.04;

fn has_centre_zone(action: Action) -> bool {
    matches!(
        action,
        Action::Tempo { .. }
            | Action::Filter { .. }
            | Action::Eq { .. }
            | Action::Gain { .. }
            | Action::RecordGain
            | Action::Crossfader
    )
}

pub fn centre_zone(v: f32) -> f32 {
    let x = 2.0 * v - 1.0;
    let y = if x.abs() <= CENTRE_ZONE {
        0.0
    } else {
        x.signum() * (x.abs() - CENTRE_ZONE) / (1.0 - CENTRE_ZONE)
    };
    f32::midpoint(y, 1.0).clamp(0.0, 1.0)
}

pub fn trigger(m: &Mapping, e: &InputEvent) -> Option<Trigger> {
    if !m.positions.is_empty() {
        return m
            .positions
            .iter()
            .find(|(raw, _)| *raw == e.raw)
            .map(|&(_, v)| Trigger::Set(v));
    }
    let flip = |v: f32| if m.invert { 1.0 - v } else { v };
    let down = down(e, m.invert);
    match m.interaction {
        Interaction::Trigger | Interaction::Toggle => down.then_some(Trigger::Press),
        Interaction::Reset => down.then_some(Trigger::Reset),
        Interaction::Hold => Some(if down {
            Trigger::Press
        } else {
            Trigger::Release
        }),
        Interaction::Direct => match e.pressed {
            Some(down) => down.then_some(Trigger::Set(flip(1.0))),
            None if has_centre_zone(m.action)
                && !(matches!(m.action, Action::Tempo { .. })
                    && matches!(e.input, Input::Hid(_))) =>
            {
                Some(Trigger::Set(centre_zone(flip(e.value))))
            }
            None => Some(Trigger::Set(flip(e.value))),
        },
        Interaction::Relative => {
            let steps = if e.pressed.is_some() || m.encoder == relative::Encoder::Button {
                if !steps_as_button(m, e) {
                    return None;
                }
                1.0
            } else if matches!(e.input, Input::Hid(_)) {
                e.value
            } else {
                midi::relative_steps(e.raw)
            };
            let steps = steps * m.scale;
            (steps != 0.0).then_some(Trigger::Delta(if m.invert { -steps } else { steps }))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Learn {
    pub action: Action,
    recent: [Option<MidiId>; 2],
}

impl Learn {
    pub fn new(action: Action) -> Self {
        Self {
            action,
            recent: [None; 2],
        }
    }

    pub fn accept(&mut self, event: &InputEvent) -> Option<Mapping> {
        let continuous = self.action.is_continuous();
        let cc = event.input.midi_id().filter(|id| id.kind == MidiKind::Cc);
        if let (true, Some(id)) = (continuous, cc) {
            let [first, second] = self.recent;
            self.recent = [second, Some(id)];
            if let Some(pair) = first.zip(second).and_then(|(a, b)| cc_pair(a, b, id)) {
                return Some(Mapping::new(pair, self.action, Interaction::Direct));
            }
            if id.number < 64 && first.is_none() {
                return None;
            }
        }
        let usable = match (&event.input, event.pressed) {
            (Input::Key(_) | Input::Midi { .. } | Input::Hid(_), Some(true)) => true,
            (Input::Midi { kind, .. }, None) => continuous || *kind == MidiKind::Cc,
            (Input::Hid(_), None) => continuous,
            _ => false,
        };
        if !usable {
            return None;
        }
        let interaction = match (&event.input, continuous) {
            (
                Input::Midi {
                    kind: MidiKind::Note,
                    ..
                }
                | Input::Key(_),
                true,
            ) => Interaction::Relative,
            (Input::Hid(name), true) if s4mk3::is_encoder(name) => Interaction::Relative,
            (Input::Hid(_), true) => Interaction::Direct,
            _ => self.action.default_interaction(),
        };
        Some(Mapping::new(event.input.clone(), self.action, interaction))
    }
}

fn cc_pair(a: MidiId, b: MidiId, c: MidiId) -> Option<Input> {
    if a != c || a.channel != b.channel {
        return None;
    }
    let (msb, lsb) = if a.number < b.number { (a, b) } else { (b, a) };
    (msb.number < 32 && lsb.number == msb.number + 32).then_some(Input::MidiPair { lsb, msb })
}

pub fn deck_letter(deck: usize) -> char {
    char::from(b'A' + u8::try_from(deck).unwrap_or(25).min(25))
}

#[cfg(test)]
mod tests;
