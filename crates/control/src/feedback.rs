pub const DECKS: usize = 4;
pub const FX_UNITS: usize = 4;
pub const HOTCUES: usize = 8;
pub const STEMS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HotcueKind {
    #[default]
    Empty,
    Cue,
    FadeIn,
    FadeOut,
    Load,
    Grid,
    Loop,
}

impl HotcueKind {
    pub fn type_code(self) -> i32 {
        match self {
            Self::Empty => -1,
            Self::Cue => 0,
            Self::FadeIn => 1,
            Self::FadeOut => 2,
            Self::Load => 3,
            Self::Grid => 4,
            Self::Loop => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StemFeedback {
    pub colour: [u8; 3],
    pub muted: bool,
    pub filter_on: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct SampleSlotFeedback {
    pub state: u8,
    pub keylock: bool,
    pub punch: bool,
    pub one_shot: bool,
    pub gate: bool,
    pub cells: [u8; 16],
    pub selected: u8,
    pub selected_colour: Option<[u8; 3]>,
    pub muted: bool,
    pub filter_on: bool,
    /// One bit per cell: the [`SampleFeedback::cell_property`] is on.
    pub property_cells: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct SampleFeedback {
    pub playing: bool,
    pub quantize_on: bool,
    pub quantize_index: u8,
    pub page: u8,
    pub sequencer_on: bool,
    pub recording: bool,
    pub cell_mode: crate::sample_deck::CellMode,
    pub cell_property: crate::sample_deck::CellProperty,
    pub slots: [SampleSlotFeedback; 4],
}

pub fn loop_size_index(beats: f64) -> u8 {
    if beats.is_nan() || beats <= 0.0 {
        return 5;
    }
    (beats.log2().round() + 5.0).clamp(0.0, 10.0) as u8
}

/// Load, in thousandths of the callback's time budget, from which a
/// controller warns that audio may drop out.
const AUDIO_OVERLOAD_PERMILLE: u64 = 800;

pub fn audio_overloaded(load_permille: u64) -> bool {
    load_permille >= AUDIO_OVERLOAD_PERMILLE
}

/// How long a controller keeps warning after the audio dropped out.
pub const DROPOUT_WARNING: std::time::Duration = std::time::Duration::from_secs(3);

/// Follows the engine's dropout count and tells whether one happened
/// recently. Dropouts counted before the first reading are not reported.
#[derive(Debug, Clone, Copy, Default)]
pub struct DropoutWatch {
    seen: Option<u64>,
    last: Option<std::time::Instant>,
}

impl DropoutWatch {
    pub fn update(&mut self, dropouts: u64, now: std::time::Instant) -> bool {
        if self.seen.is_some_and(|seen| dropouts > seen) {
            self.last = Some(now);
        }
        self.seen = Some(dropouts);
        self.last
            .is_some_and(|at| now.saturating_duration_since(at) < DROPOUT_WARNING)
    }
}

impl FxFeedback {
    pub fn playing_sixteenth(&self) -> Option<u8> {
        let cells = u32::from(self.pattern_cells.max(1));
        self.pattern_step
            .map(|s| u8::try_from(u32::from(s) * 16 / cells).unwrap_or(u8::MAX))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlatterFeedback {
    pub motor_on: bool,
    pub rpm: f32,
    pub motor_off: u32,
}

impl Default for PlatterFeedback {
    fn default() -> Self {
        Self {
            motor_on: false,
            rpm: 100.0 / 3.0,
            motor_off: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct DeckFeedback {
    pub loaded: bool,
    pub playing: bool,
    pub synced: bool,
    pub master: bool,
    pub slip: bool,
    pub reverse: bool,
    pub keylock: bool,
    pub loop_active: bool,
    pub in_loop: bool,
    pub tempo_range: u8,
    pub loop_size: u8,
    pub turntable: bool,
    pub platter: PlatterFeedback,
    pub position_s: f64,
    pub duration_s: f64,
    pub end_warning: bool,
    pub tempo_locked: bool,
    pub live: bool,
    pub pad_fx: [u16; 2],
    pub tempo: f64,
    pub hotcues: [HotcueKind; HOTCUES],
    pub stems: Option<[StemFeedback; STEMS]>,
    pub sample_deck: Option<SampleFeedback>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ChannelFeedback {
    pub cue: bool,
    pub mixer_fx: u8,
    pub mixer_fx_on: bool,
    pub fx_assign: [bool; FX_UNITS],
    pub level: f32,
    pub mute: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MixerKnobs {
    pub fader: [f32; DECKS],
    pub gain: [f32; DECKS],
    pub eq: [[f32; 4]; DECKS],
    /// EQ bands cut to silence, in the same band order as `eq`.
    pub kill: [[bool; 4]; DECKS],
    pub filter: [f32; DECKS],
    pub crossfader: f32,
    pub master: f32,
    pub cue_volume: f32,
    pub cue_mix: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FxFeedback {
    pub on: bool,
    pub buttons: [bool; 3],
    pub mode: u8,
    pub pattern_step: Option<u8>,
    pub pattern_steps: u16,
    pub pattern_cells: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BrowserFeedback {
    pub maximized: bool,
    pub favorites: bool,
    pub preview: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Feedback {
    pub decks: [DeckFeedback; DECKS],
    pub channels: [ChannelFeedback; DECKS],
    pub fx: [FxFeedback; FX_UNITS],
    pub quantize: bool,
    pub jog_tension: u8,
    pub browser: BrowserFeedback,
    pub master_level: [f32; 2],
    pub knobs: MixerKnobs,
    pub ticks: [bool; DECKS],
    pub master_tick: bool,
    pub focus: usize,
    pub tempo_fader_relative: bool,
    pub loop_recorder: LoopRecorderFeedback,
    pub microphone: bool,
    /// The audio callback is using most of its time budget.
    pub audio_overloaded: bool,
    /// FX units shown in the app (2 or 4).
    pub fx_units: u8,
    /// The audio dropped out a moment ago.
    pub audio_dropout: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LoopRecorderFeedback {
    pub state: u8,
    pub undo: u8,
    pub size: u8,
    pub source: u8,
    pub dry_wet: f32,
    pub position: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropout_warns_for_a_while() {
        let start = std::time::Instant::now();
        let mut w = DropoutWatch::default();
        assert!(!w.update(5, start), "earlier dropouts are not news");
        assert!(!w.update(5, start + std::time::Duration::from_secs(1)));
        let at = start + std::time::Duration::from_secs(2);
        assert!(w.update(6, at));
        assert!(w.update(6, at + DROPOUT_WARNING / 2));
        assert!(!w.update(6, at + DROPOUT_WARNING));
    }

    #[test]
    fn audio_overloads_from_four_fifths_of_the_budget() {
        assert!(!audio_overloaded(799));
        assert!(audio_overloaded(800));
        assert!(audio_overloaded(1200));
    }

    #[test]
    fn loop_sizes_index_by_powers_of_two() {
        assert_eq!(loop_size_index(1.0 / 32.0), 0);
        assert_eq!(loop_size_index(1.0), 5);
        assert_eq!(loop_size_index(4.0), 7);
        assert_eq!(loop_size_index(32.0), 10);
        assert_eq!(loop_size_index(128.0), 10);
        assert_eq!(loop_size_index(0.0), 5);
    }
}
