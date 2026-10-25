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
    pub slots: [SampleSlotFeedback; 4],
}

pub fn loop_size_index(beats: f64) -> u8 {
    if beats.is_nan() || beats <= 0.0 {
        return 5;
    }
    (beats.log2().round() + 5.0).clamp(0.0, 10.0) as u8
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
    fn loop_sizes_index_by_powers_of_two() {
        assert_eq!(loop_size_index(1.0 / 32.0), 0);
        assert_eq!(loop_size_index(1.0), 5);
        assert_eq!(loop_size_index(4.0), 7);
        assert_eq!(loop_size_index(32.0), 10);
        assert_eq!(loop_size_index(128.0), 10);
        assert_eq!(loop_size_index(0.0), 5);
    }
}
