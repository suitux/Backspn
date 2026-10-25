mod barber_pole;
mod beat_chopper;
mod beat_delay;
mod beat_lfo;
mod beat_pitch;
mod beat_reverb;
mod brake;
pub(crate) mod color;
mod color_reverb;
mod delay;
mod delay_classic;
mod delay_t3;
mod digital_lofi;
mod drive;
mod echo;
mod filter_family;
mod formant;
mod gate;
mod gater;
mod glass_reverb;
mod lofi;
mod macros;
mod metaverb;
mod modulation;
mod multi_tap;
mod pulse_hold;
mod ramp_delay;
mod release_echo;
mod reverb;
mod reverb_t3;
mod reverse_grain;
mod rising_echo;
mod roll;
mod shepard;
mod slicer;
mod step_stutter;
mod sync_lfo;
mod tank;
mod tape_delay;
mod transpose;
mod turntable;

use serde::{Deserialize, Serialize};

use crate::{Context, Effect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum EffectKind {
    #[default]
    None,
    Delay,
    Reverb,
    #[serde(alias = "LadderFilter")]
    Filter,
    Flanger,
    Phaser,
    Gater,
    BeatChopper,
    LoFi,
    Ringmod,
    Echo,
    PingPong,
    PitchEcho,
    CoilEcho,
    LowCutEcho,
    LowCutPitchEcho,
    DelayT3,
    TapeDelay,
    ReverbT3,
    GlassReverb,
    Turntable,
    Slicer,
    BeatDelay,
    ReverseDelay,
    MultiTapDelay,
    BeatPitch,
    Roll,
    SlipRoll,
    ReverseRoll,
    RisingEcho,
    VinylBrake,
    ReleaseBrake,
    BackSpin,
    ReleaseEcho,
    ShepardSaw,
    ShepardTri,
    ShepardJet,
    RampDelay,
    ReverseGrain,
    Drive,
    StepStutter,
    AutoStepStutter,
    BarberPole,
    FormantFilter,
    PeakFilter,
    TransposeStretch,
    LadderLfo,
    LadderPulse,
    SvfFilter,
    SvfLfo,
    SvfPulse,
    FlangerPulse,
    FlangerHold,
    PhaserPulse,
    PhaserHold,
    Trans,
    Pan,
    BeatFilter,
    BeatFlanger,
    BeatPhaser,
    BeatReverb,
    Stretch,
    MacroTunnel,
    #[serde(alias = "MacroLaserCut")]
    MacroZapCut,
    #[serde(alias = "MacroGrainPhase")]
    MacroGrainSweep,
    MacroBassMachine,
    MacroStretchFast,
    MacroStretchSlow,
    MacroPolar,
    MacroHorizon,
    MacroZap,
    MacroDark,
    MacroFlight,
    ModFilter,
    ModDecay,
}

impl EffectKind {
    pub fn is_modifier(self) -> bool {
        matches!(self, Self::ModFilter | Self::ModDecay)
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Self::ModFilter => "Mod: Filter",
            Self::ModDecay => "Mod: Decay",
            Self::MacroTunnel => "M: Tunnel",
            Self::MacroZapCut => "M: Zap Cut",
            Self::MacroGrainSweep => "M: Grain",
            Self::MacroBassMachine => "M: Bass",
            Self::MacroStretchFast => "M: Str.Fast",
            Self::MacroStretchSlow => "M: Str.Slow",
            Self::MacroPolar => "M: Polar",
            Self::MacroHorizon => "M: Horizon",
            Self::MacroZap => "M: Zap",
            Self::MacroDark => "M: Dark",
            Self::MacroFlight => "M: Flight",
            Self::LowCutPitchEcho => "Lo Cut Pitch",
            Self::ReverseDelay => "Rev Delay",
            Self::MultiTapDelay => "Multi Tap",
            Self::ReleaseBrake => "Rel. Brake",
            Self::ReleaseEcho => "Rel. Echo",
            Self::ReverseGrain => "Rev Grain",
            Self::FormantFilter => "Formant",
            Self::TransposeStretch => "Transpose",
            Self::FlangerPulse => "Flanger Pls.",
            Self::AutoStepStutter => "Auto Stutter",
            _ => self.info().name,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectInfo {
    pub name: &'static str,
    pub params: [&'static str; 3],
    pub defaults: [f32; 3],
    pub buttons: [&'static str; 3],
    pub neutral_amount: f32,
    pub group_mix: bool,
}

impl EffectKind {
    pub const ALL: [Self; 75] = [
        Self::None,
        Self::Delay,
        Self::Reverb,
        Self::Filter,
        Self::Flanger,
        Self::Phaser,
        Self::Gater,
        Self::BeatChopper,
        Self::LoFi,
        Self::Ringmod,
        Self::Echo,
        Self::PingPong,
        Self::PitchEcho,
        Self::CoilEcho,
        Self::LowCutEcho,
        Self::LowCutPitchEcho,
        Self::DelayT3,
        Self::TapeDelay,
        Self::ReverbT3,
        Self::GlassReverb,
        Self::Turntable,
        Self::Slicer,
        Self::BeatDelay,
        Self::ReverseDelay,
        Self::MultiTapDelay,
        Self::BeatPitch,
        Self::Roll,
        Self::SlipRoll,
        Self::ReverseRoll,
        Self::RisingEcho,
        Self::VinylBrake,
        Self::ReleaseBrake,
        Self::BackSpin,
        Self::ReleaseEcho,
        Self::ShepardSaw,
        Self::ShepardTri,
        Self::ShepardJet,
        Self::RampDelay,
        Self::ReverseGrain,
        Self::Drive,
        Self::StepStutter,
        Self::AutoStepStutter,
        Self::BarberPole,
        Self::FormantFilter,
        Self::PeakFilter,
        Self::TransposeStretch,
        Self::LadderLfo,
        Self::LadderPulse,
        Self::SvfFilter,
        Self::SvfLfo,
        Self::SvfPulse,
        Self::FlangerPulse,
        Self::FlangerHold,
        Self::PhaserPulse,
        Self::PhaserHold,
        Self::Trans,
        Self::Pan,
        Self::BeatFilter,
        Self::BeatFlanger,
        Self::BeatPhaser,
        Self::BeatReverb,
        Self::Stretch,
        Self::MacroTunnel,
        Self::MacroZapCut,
        Self::MacroGrainSweep,
        Self::MacroBassMachine,
        Self::MacroStretchFast,
        Self::MacroStretchSlow,
        Self::MacroPolar,
        Self::MacroHorizon,
        Self::MacroZap,
        Self::MacroDark,
        Self::MacroFlight,
        Self::ModFilter,
        Self::ModDecay,
    ];

    pub fn info(self) -> EffectInfo {
        let info = self.engine_info();
        if !self.mirrored() {
            return info;
        }
        let [p0, p1, p2] = info.params;
        let [d0, d1, d2] = info.defaults;
        let [b0, b1, _] = info.buttons;
        EffectInfo {
            params: [p2, p1, p0],
            defaults: [d2, d1, d0],
            buttons: ["RST", b1, b0],
            ..info
        }
    }

    pub fn mirrored(self) -> bool {
        use EffectKind as K;
        matches!(
            self,
            K::Delay
                | K::Reverb
                | K::Filter
                | K::Flanger
                | K::Phaser
                | K::Gater
                | K::BeatChopper
                | K::LoFi
                | K::Ringmod
                | K::DelayT3
                | K::TapeDelay
                | K::ReverbT3
                | K::GlassReverb
                | K::Turntable
                | K::Slicer
                | K::RampDelay
                | K::ReverseGrain
                | K::Drive
                | K::StepStutter
                | K::AutoStepStutter
                | K::BarberPole
                | K::FormantFilter
                | K::PeakFilter
                | K::TransposeStretch
                | K::SvfFilter
                | K::LadderLfo
                | K::LadderPulse
                | K::SvfLfo
                | K::SvfPulse
                | K::FlangerPulse
                | K::FlangerHold
                | K::PhaserPulse
                | K::PhaserHold
                | K::MacroTunnel
                | K::MacroZapCut
                | K::MacroGrainSweep
                | K::MacroBassMachine
                | K::MacroStretchFast
                | K::MacroStretchSlow
                | K::MacroPolar
                | K::MacroHorizon
                | K::MacroZap
                | K::MacroDark
                | K::MacroFlight
        )
    }

    fn engine_info(self) -> EffectInfo {
        let info = |name, params, defaults, buttons| EffectInfo {
            name,
            params,
            defaults,
            buttons,
            neutral_amount: 0.0,
            group_mix: true,
        };
        match self {
            Self::None => info("—", ["", "", ""], [0.0; 3], ["", "", ""]),
            Self::Delay => info(
                "Delay",
                ["RATE", "FEEDB", "FILTER"],
                [0.499_75, 0.5, 0.5],
                ["SPR", "FRZ", ""],
            ),
            Self::Reverb => info(
                "Reverb",
                ["SIZE", "LP", "HP"],
                [0.502, 0.898, 0.498],
                ["FRZ", "", ""],
            ),
            Self::Filter => EffectInfo {
                neutral_amount: 0.5,
                ..info(
                    "Filter",
                    ["LP", "RES", "HP"],
                    [1.0, 0.498, 0.0],
                    ["CLUB", "TYPE B", ""],
                )
            },
            Self::Flanger => EffectInfo {
                group_mix: false,
                ..info(
                    "Flanger",
                    ["RATE", "FEEDB", "SPREAD"],
                    [0.373, 0.498, 0.502],
                    ["FR.R", "UP", ""],
                )
            },
            Self::Phaser => EffectInfo {
                neutral_amount: 0.498,
                group_mix: false,
                ..info(
                    "Phaser",
                    ["RATE", "FEEDB", "SPREAD"],
                    [0.341, 0.0, 0.498],
                    ["FR.R", "UP", ""],
                )
            },
            Self::Gater => info(
                "Gater",
                ["RATE", "SHAPE", "NOISE"],
                [0.702, 0.498, 0.498],
                ["STT", "MTE", ""],
            ),
            Self::BeatChopper => EffectInfo {
                group_mix: false,
                ..info(
                    "Beat Chopper",
                    ["LENGTH", "ROTATE", "GATE"],
                    [0.49, 0.498, 0.502],
                    ["REV", "WRP", ""],
                )
            },
            Self::LoFi => info(
                "LoFi",
                ["SRATE", "SMOOTH", "BITS"],
                [0.0, 0.498, 0.0],
                ["SPR", "", ""],
            ),
            Self::Ringmod => EffectInfo {
                neutral_amount: 0.498,
                group_mix: false,
                ..info(
                    "Ringmod",
                    ["PITCH", "RAW", "MOD"],
                    [0.498, 0.498, 1.0],
                    ["", "", ""],
                )
            },
            _ => self.added_info(),
        }
    }
}

impl EffectKind {
    fn added_info(self) -> EffectInfo {
        let echo = |name, third, buttons| EffectInfo {
            name,
            params: ["BEAT", "FEEDB", third],
            defaults: [3.0 / 9.0, 0.5, 0.5],
            buttons,
            neutral_amount: 0.0,
            group_mix: true,
        };
        let modifier = |name| EffectInfo {
            name,
            params: [""; 3],
            defaults: [0.0; 3],
            buttons: [""; 3],
            neutral_amount: 0.5,
            group_mix: true,
        };
        match self {
            Self::PingPong => echo("Ping Pong", "R RATIO", ["REV", "FREEZE", ""]),
            Self::PitchEcho => echo("Pitch Echo", "PITCH", ["LO CUT", "HI CUT", ""]),
            Self::CoilEcho => echo("Coil Echo", "PITCH", ["LO CUT", "HI CUT", ""]),
            Self::LowCutEcho => echo("Low Cut Echo", "SWING", ["REVERSE", "", ""]),
            Self::LowCutPitchEcho => echo("Low Cut Pitch Echo", "PITCH", ["REVERSE", "FREEZE", ""]),
            Self::DelayT3
            | Self::TapeDelay
            | Self::ReverbT3
            | Self::GlassReverb
            | Self::Turntable
            | Self::Slicer
            | Self::FlangerPulse
            | Self::FlangerHold
            | Self::PhaserPulse
            | Self::PhaserHold => self.first_bank_info(),
            Self::FormantFilter
            | Self::PeakFilter
            | Self::TransposeStretch
            | Self::LadderLfo
            | Self::LadderPulse
            | Self::SvfFilter
            | Self::SvfLfo
            | Self::SvfPulse => self.first_bank_filter_info(),
            Self::RampDelay
            | Self::ReverseGrain
            | Self::Drive
            | Self::StepStutter
            | Self::AutoStepStutter
            | Self::BarberPole => self.first_bank_rack_info(),
            Self::BeatDelay
            | Self::ReverseDelay
            | Self::MultiTapDelay
            | Self::BeatPitch
            | Self::Roll
            | Self::SlipRoll
            | Self::ReverseRoll
            | Self::RisingEcho
            | Self::VinylBrake
            | Self::ReleaseBrake
            | Self::BackSpin
            | Self::ReleaseEcho
            | Self::ShepardSaw
            | Self::ShepardTri
            | Self::ShepardJet => self.second_bank_info(),
            Self::Trans
            | Self::Pan
            | Self::BeatFilter
            | Self::BeatFlanger
            | Self::BeatPhaser
            | Self::BeatReverb
            | Self::Stretch => self.second_bank_lfo_info(),
            Self::MacroTunnel
            | Self::MacroZapCut
            | Self::MacroGrainSweep
            | Self::MacroBassMachine
            | Self::MacroStretchFast
            | Self::MacroStretchSlow
            | Self::MacroPolar
            | Self::MacroHorizon
            | Self::MacroZap
            | Self::MacroDark
            | Self::MacroFlight => self.macro_info(),
            Self::ModFilter => modifier("Effect Modifier: Filter"),
            Self::ModDecay => modifier("Effect Modifier: Decay"),
            _ => echo("Echo", "FILTER", ["REVERSE", "FREEZE", ""]),
        }
    }

    fn first_bank_info(self) -> EffectInfo {
        match self {
            Self::FlangerPulse => EffectInfo {
                neutral_amount: 0.502,
                ..plain(
                    "Flanger Pulse",
                    ["AMOUNT", "FEEDB", "SHAPE"],
                    [0.502, 0.502, 0.506],
                    ["SPR", "FB-"],
                )
            },
            Self::FlangerHold => EffectInfo {
                neutral_amount: 0.498,
                ..plain(
                    "Flanger Hold",
                    ["PITCH", "FEEDB", ""],
                    [0.498, 1.0, 0.0],
                    ["SPR", "FB-"],
                )
            },
            Self::PhaserPulse => EffectInfo {
                neutral_amount: 0.502,
                group_mix: false,
                ..plain(
                    "Phaser Pulse",
                    ["AMOUNT", "FEEDB", "SHAPE"],
                    [0.698, 0.502, 0.502],
                    ["SPR", "8 PL"],
                )
            },
            Self::PhaserHold => EffectInfo {
                neutral_amount: 0.502,
                group_mix: false,
                ..plain(
                    "Phaser Hold",
                    ["PITCH", "FEEDB", ""],
                    [0.498, 0.502, 0.0],
                    ["SPR", "8 PL"],
                )
            },
            Self::DelayT3 => EffectInfo {
                neutral_amount: 0.698,
                group_mix: false,
                ..plain(
                    "Delay II",
                    ["RATE", "FEEDB", "FILTER"],
                    [0.541, 0.498, 0.498],
                    ["FR.R", "FRZ"],
                )
            },
            Self::TapeDelay => EffectInfo {
                neutral_amount: 0.5,
                group_mix: false,
                ..plain(
                    "Tape Delay",
                    ["SPEED", "FBK", "FILT"],
                    [0.498, 0.502, 0.502],
                    ["ACCL", "FRZ"],
                )
            },
            Self::ReverbT3 => plain(
                "Reverb II",
                ["SIZE", "LP", "HP"],
                [0.498, 0.541, 0.498],
                ["FRZ", ""],
            ),
            Self::GlassReverb => plain(
                "Glass Reverb",
                ["SIZE", "COLOR", "ICING"],
                [0.498, 0.753, 0.498],
                ["FRZ", ""],
            ),
            Self::Turntable => EffectInfo {
                buttons: ["REW", "RCK", ""],
                neutral_amount: 0.502,
                group_mix: false,
                ..plain(
                    "Turntable",
                    ["B.SPD", "R.SPD", "AMNT"],
                    [0.502, 0.498, 0.498],
                    ["", ""],
                )
            },
            _ => EffectInfo {
                group_mix: false,
                ..plain(
                    "Slicer",
                    ["PAT", "STYLE", "BUZZ"],
                    [0.0, 0.0, 0.0],
                    ["2 BAR", "GO"],
                )
            },
        }
    }

    fn first_bank_filter_info(self) -> EffectInfo {
        match self {
            Self::SvfFilter => EffectInfo {
                neutral_amount: 0.5,
                ..plain(
                    "SVF Filter",
                    ["LP", "RES", "HP"],
                    [1.0, 0.498, 0.0],
                    ["CLUB", "TYPE B"],
                )
            },
            Self::LadderLfo | Self::SvfLfo => EffectInfo {
                neutral_amount: 0.498,
                group_mix: false,
                ..plain(
                    if self == Self::LadderLfo {
                        "Ladder LFO"
                    } else {
                        "SVF LFO"
                    },
                    ["RATE", "RES", "SHAPE"],
                    [0.635, 0.502, 0.502],
                    ["FR.R", "UP"],
                )
            },
            Self::LadderPulse | Self::SvfPulse => plain(
                if self == Self::LadderPulse {
                    "Ladder Pulse"
                } else {
                    "SVF Pulse"
                },
                ["AMNT", "RES", "SOFT"],
                [1.0, 0.6, 0.51],
                ["P.MD", "P.SN"],
            ),
            Self::TransposeStretch => EffectInfo {
                neutral_amount: 0.502,
                group_mix: false,
                ..plain(
                    "Transpose Stretch",
                    ["KEY", "GRAIN", "STRETCH"],
                    [0.498, 1.0, 0.0],
                    ["ST.2", "GRN"],
                )
            },
            Self::FormantFilter => EffectInfo {
                neutral_amount: 0.502,
                group_mix: false,
                ..plain(
                    "Formant Filter",
                    ["TALK", "SIZE", "SHARP"],
                    [0.502, 0.498, 0.502],
                    ["TYP", ""],
                )
            },
            _ => EffectInfo {
                neutral_amount: 0.498,
                group_mix: false,
                ..plain(
                    "Peak Filter",
                    ["FREQ", "EDGE", "PUMP"],
                    [0.498, 0.0, 0.0],
                    ["KILL", ""],
                )
            },
        }
    }

    fn first_bank_rack_info(self) -> EffectInfo {
        match self {
            Self::BarberPole => EffectInfo {
                neutral_amount: 0.5,
                ..plain(
                    "Barber Pole",
                    ["MOD", "RNG", "BAWD"],
                    [0.75, 0.5, 0.5],
                    ["FAST", ""],
                )
            },
            Self::StepStutter => EffectInfo {
                neutral_amount: 0.498,
                group_mix: false,
                ..plain(
                    "Step Stutter",
                    ["SPEED", "FILTER", "TRANSP"],
                    [0.498, 1.0, 0.502],
                    ["x2", "HOLD"],
                )
            },
            Self::AutoStepStutter => EffectInfo {
                neutral_amount: 0.502,
                group_mix: false,
                ..plain(
                    "Auto Step Stutter",
                    ["PATTERN", "BEND", "TRANSP"],
                    [0.0, 0.502, 0.502],
                    ["ALT", "x2"],
                )
            },
            Self::Drive => plain(
                "Drive",
                ["DRIVE", "FEEDB", "TONE"],
                [0.502, 0.502, 0.498],
                ["FB-", ""],
            ),
            Self::ReverseGrain => EffectInfo {
                neutral_amount: 1.0,
                group_mix: false,
                ..plain(
                    "Reverse Grain",
                    ["SPEED", "GRAIN", "PITCH"],
                    [0.498, 1.0, 0.502],
                    ["REV", "INV"],
                )
            },
            _ => EffectInfo {
                neutral_amount: 0.506,
                group_mix: false,
                ..plain(
                    "Ramp Delay",
                    ["RATE", "DURATION", "FILTER"],
                    [0.506, 0.690, 0.502],
                    ["FB+", "FRZ"],
                )
            },
        }
    }

    fn second_bank_info(self) -> EffectInfo {
        match self {
            Self::ShepardSaw => plain(
                "Shepard Saw",
                ["BEAT", "ECHO", "MOD"],
                [5.0 / 9.0, 0.5, 0.5],
                ["REVERSE", "FREEZE"],
            ),
            Self::ShepardTri => plain(
                "Shepard Tri",
                ["BEAT", "ECHO", "MOD"],
                [5.0 / 9.0, 0.5, 0.5],
                ["REVERSE", "FREEZE"],
            ),
            Self::ShepardJet => plain(
                "Shepard Jet",
                ["BEAT", "FEEDB", "MOD"],
                [5.0 / 9.0, 0.5, 0.5],
                ["REVERSE", "FREEZE"],
            ),
            Self::VinylBrake => plain(
                "Vinyl Brake",
                ["BEAT", "REPEAT", "ECHO"],
                [5.0 / 9.0, 0.5, 0.5],
                ["REPEAT ON", "BPF"],
            ),
            Self::ReleaseEcho => EffectInfo {
                neutral_amount: 0.5,
                group_mix: false,
                ..plain("Release Echo", ["TIME", "", ""], [0.5, 0.0, 0.0], ["", ""])
            },
            Self::ReleaseBrake | Self::BackSpin => EffectInfo {
                neutral_amount: 0.6,
                group_mix: false,
                ..plain(
                    if self == Self::BackSpin {
                        "Back Spin"
                    } else {
                        "Release Brake"
                    },
                    ["TIME", "", ""],
                    [0.6, 0.0, 0.0],
                    ["", ""],
                )
            },
            Self::RisingEcho => plain(
                "Rising Echo",
                ["BEAT", "TRIM", "SPEED"],
                [3.0 / 9.0, 0.5, 0.5],
                ["LOW CUT", "HI CUT"],
            ),
            Self::BeatDelay => plain(
                "Beat Delay",
                ["BEAT", "L/R", "PAN"],
                [3.0 / 9.0, 0.5, 0.5],
                ["LOW CUT", "HI CUT"],
            ),
            Self::ReverseDelay => plain(
                "Reverse Delay",
                ["BEAT", "L/R", "PAN"],
                [3.0 / 9.0, 0.5, 0.5],
                ["LOW CUT", "HI CUT"],
            ),
            Self::BeatPitch => plain(
                "Pitch",
                ["PITCH", "", ""],
                [0.5, 0.0, 0.0],
                ["LOW CUT", "HI CUT"],
            ),
            Self::MultiTapDelay => plain(
                "Multi Tap Delay",
                ["BEAT", "SWING", "TAPS"],
                [5.0 / 9.0, 0.5, 0.5],
                ["LOW CUT", "HI CUT"],
            ),
            _ => plain(
                match self {
                    Self::Roll => "Roll",
                    Self::SlipRoll => "Slip Roll",
                    _ => "Reverse Roll",
                },
                ["BEAT", "DUTY", "SWING"],
                [3.0 / 9.0, 0.5, 0.5],
                ["LOW CUT", "HI CUT"],
            ),
        }
    }

    fn second_bank_lfo_info(self) -> EffectInfo {
        match self {
            Self::BeatReverb => plain(
                "Beat Reverb",
                ["BEAT", "ROOM", "DECAY"],
                [5.0 / 9.0, 0.5, 0.5],
                ["LOW CUT", "HI CUT"],
            ),
            Self::Stretch => plain(
                "Stretch",
                ["BEAT", "LO/HI CUT", ""],
                [5.0 / 9.0, 0.5, 0.0],
                ["REPITCH", "BPF"],
            ),
            Self::Trans => plain(
                "Trans",
                ["BEAT", "DUTY", "SWING"],
                [5.0 / 9.0, 0.5, 0.5],
                ["LOW CUT", "HI CUT"],
            ),
            Self::Pan => plain(
                "Pan",
                ["BEAT", "RATE", "SWING"],
                [5.0 / 9.0, 0.5, 0.5],
                ["MID THRU", "LO+HI THRU"],
            ),
            Self::BeatFilter => plain(
                "Beat Filter",
                ["BEAT", "RES", "LFO AMP"],
                [5.0 / 9.0, 0.5, 0.5],
                ["HPF", "PHASE"],
            ),
            Self::BeatFlanger => plain(
                "Beat Flanger",
                ["BEAT", "FEEDB", "LFO AMP"],
                [5.0 / 9.0, 0.5, 0.5],
                ["REV FB", "PHASE"],
            ),
            _ => plain(
                "Beat Phaser",
                ["BEAT", "FEEDB", "STAGE"],
                [5.0 / 9.0, 0.5, 0.5],
                ["REV FB", "PHASE"],
            ),
        }
    }
}

impl EffectKind {
    fn macro_info(self) -> EffectInfo {
        let name = match self {
            Self::MacroTunnel => "Macro: Tunnel",
            Self::MacroZapCut => "Macro: Zap Cut",
            Self::MacroGrainSweep => "Macro: Grain Sweep",
            Self::MacroBassMachine => "Macro: Bass Machine",
            Self::MacroStretchFast => "Macro: Stretch Fast",
            Self::MacroStretchSlow => "Macro: Stretch Slow",
            Self::MacroPolar => "Macro: Polar",
            Self::MacroHorizon => "Macro: Horizon",
            Self::MacroZap => "Macro: Zap",
            Self::MacroDark => "Macro: Dark",
            Self::MacroFlight => "Macro: Flight",
            _ => "Macro",
        };
        EffectInfo {
            neutral_amount: 0.5,
            ..plain(name, ["DEPTH", "", ""], [0.5, 0.0, 0.0], ["", ""])
        }
    }
}

fn plain(
    name: &'static str,
    params: [&'static str; 3],
    defaults: [f32; 3],
    [b0, b1]: [&'static str; 2],
) -> EffectInfo {
    EffectInfo {
        name,
        params,
        defaults,
        buttons: [b0, b1, ""],
        neutral_amount: 0.0,
        group_mix: true,
    }
}

pub(crate) fn mixer_engine(kind: EffectKind, sample_rate: u32) -> Box<dyn Effect> {
    let rate = f64::from(sample_rate);
    match kind {
        EffectKind::Delay => Box::new(delay::Delay::new(rate)),
        EffectKind::Reverb => Box::new(reverb::Reverb::new(rate)),
        EffectKind::Gater => Box::new(gater::Gater::new(rate)),
        EffectKind::Flanger => Box::new(modulation::Flanger::new(rate)),
        EffectKind::Phaser => Box::new(modulation::Phaser::new(rate)),
        EffectKind::LoFi => Box::new(lofi::LoFi::new(rate)),
        other => create_engine(other, sample_rate),
    }
}

pub fn create(kind: EffectKind, sample_rate: u32) -> Box<dyn Effect> {
    let effect = create_engine(kind, sample_rate);
    if kind.mirrored() {
        Box::new(Mirrored(effect))
    } else {
        effect
    }
}

#[derive(Debug)]
struct Mirrored(Box<dyn Effect>);

impl Effect for Mirrored {
    fn kind(&self) -> EffectKind {
        self.0.kind()
    }
    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        self.0.process(buf, ctx);
    }
    fn set_amount(&mut self, amount: f32) {
        self.0.set_amount(amount);
    }
    fn set_param(&mut self, index: usize, value: f32) {
        if index < 3 {
            self.0.set_param(2 - index, value);
        }
    }
    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            1 => self.0.set_button(1, on),
            2 => self.0.set_button(0, on),
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.0.reset();
    }
    fn has_tail(&self) -> bool {
        self.0.has_tail()
    }
    fn set_decay(&mut self, decay: f32) {
        self.0.set_decay(decay);
    }
    fn set_active(&mut self, on: bool) {
        self.0.set_active(on);
    }
    fn listens(&self) -> bool {
        self.0.listens()
    }
    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.0.set_dry_wet(dry_wet)
    }
}

pub(crate) fn create_engine(kind: EffectKind, sample_rate: u32) -> Box<dyn Effect> {
    let rate = f64::from(sample_rate);
    match kind {
        EffectKind::None => Box::new(Bypass),
        EffectKind::Delay => Box::new(delay_classic::Delay::new(rate)),
        EffectKind::Reverb => Box::new(metaverb::Metaverb::new(rate)),
        EffectKind::Filter => Box::new(filter_family::FilterFx::new(
            filter_family::Family::Ladder,
            filter_family::Drive::Hand,
            rate,
        )),
        EffectKind::Flanger => Box::new(sync_lfo::Flanger::new(rate)),
        EffectKind::Phaser => Box::new(sync_lfo::Phaser::new(rate)),
        EffectKind::Gater => Box::new(gate::Gater::new(rate)),
        EffectKind::BeatChopper => Box::new(beat_chopper::BeatChopper::new(rate)),
        EffectKind::LoFi => Box::new(digital_lofi::LoFi::new(rate)),
        EffectKind::Ringmod => Box::new(digital_lofi::Ringmod::new(rate)),
        EffectKind::Echo => Box::new(echo::Echo::new(echo::Variant::Echo, rate)),
        EffectKind::PingPong => Box::new(echo::Echo::new(echo::Variant::PingPong, rate)),
        EffectKind::PitchEcho => Box::new(echo::Echo::new(echo::Variant::PitchEcho, rate)),
        EffectKind::CoilEcho => Box::new(echo::Echo::new(echo::Variant::CoilEcho, rate)),
        EffectKind::LowCutEcho => Box::new(echo::Echo::new(echo::Variant::LowCutEcho, rate)),
        EffectKind::LowCutPitchEcho => {
            Box::new(echo::Echo::new(echo::Variant::LowCutPitchEcho, rate))
        }
        EffectKind::DelayT3 => Box::new(delay_t3::DelayT3::new(rate)),
        EffectKind::TapeDelay => Box::new(tape_delay::TapeDelay::new(rate)),
        EffectKind::ReverbT3 => Box::new(reverb_t3::ReverbT3::new(rate)),
        EffectKind::GlassReverb => Box::new(glass_reverb::GlassReverb::new(rate)),
        EffectKind::Turntable => Box::new(turntable::Turntable::new(rate)),
        EffectKind::Slicer => Box::new(slicer::Slicer::new(rate)),
        EffectKind::BeatDelay => Box::new(beat_delay::BeatDelay::new(false, rate)),
        EffectKind::Roll => Box::new(roll::Roll::new(roll::Variant::Roll, rate)),
        EffectKind::SlipRoll => Box::new(roll::Roll::new(roll::Variant::SlipRoll, rate)),
        EffectKind::ReverseRoll => Box::new(roll::Roll::new(roll::Variant::ReverseRoll, rate)),
        EffectKind::ReverseDelay => Box::new(beat_delay::BeatDelay::new(true, rate)),
        EffectKind::MultiTapDelay => Box::new(multi_tap::MultiTap::new(rate)),
        EffectKind::BeatPitch => Box::new(beat_pitch::BeatPitch::new(rate)),
        EffectKind::RisingEcho => Box::new(rising_echo::RisingEcho::new(rate)),
        EffectKind::VinylBrake => Box::new(brake::Brake::new(brake::Kind::VinylBrake, rate)),
        EffectKind::ReleaseBrake => Box::new(brake::Brake::new(brake::Kind::ReleaseBrake, rate)),
        EffectKind::BackSpin => Box::new(brake::BackSpin::new(rate)),
        EffectKind::ReleaseEcho => Box::new(release_echo::ReleaseEcho::new(rate)),
        EffectKind::ShepardSaw => Box::new(shepard::Shepard::new(shepard::Wave::Saw, rate)),
        EffectKind::ShepardTri => Box::new(shepard::Shepard::new(shepard::Wave::Triangle, rate)),
        EffectKind::ShepardJet => Box::new(shepard::ShepardJet::new(rate)),
        EffectKind::RampDelay => Box::new(ramp_delay::RampDelay::new(rate)),
        EffectKind::ReverseGrain => Box::new(reverse_grain::ReverseGrain::new(rate)),
        EffectKind::Drive => Box::new(drive::Drive::new(rate)),
        EffectKind::StepStutter => Box::new(step_stutter::StepStutter::new(rate)),
        EffectKind::AutoStepStutter => Box::new(step_stutter::AutoStepStutter::new(rate)),
        EffectKind::BarberPole => Box::new(barber_pole::BarberPole::new(rate)),
        EffectKind::FormantFilter => Box::new(formant::Formant::new(rate)),
        EffectKind::PeakFilter => Box::new(formant::PeakFilter::new(rate)),
        EffectKind::TransposeStretch => Box::new(transpose::Transpose::new(rate)),
        EffectKind::LadderLfo => Box::new(filter_family::FilterFx::new(
            filter_family::Family::Ladder,
            filter_family::Drive::Lfo,
            rate,
        )),
        EffectKind::LadderPulse => Box::new(filter_family::FilterFx::new(
            filter_family::Family::Ladder,
            filter_family::Drive::Pulse,
            rate,
        )),
        EffectKind::SvfFilter => Box::new(filter_family::FilterFx::new(
            filter_family::Family::Svf,
            filter_family::Drive::Hand,
            rate,
        )),
        EffectKind::SvfLfo => Box::new(filter_family::FilterFx::new(
            filter_family::Family::Svf,
            filter_family::Drive::Lfo,
            rate,
        )),
        EffectKind::SvfPulse => Box::new(filter_family::FilterFx::new(
            filter_family::Family::Svf,
            filter_family::Drive::Pulse,
            rate,
        )),
        EffectKind::FlangerPulse => Box::new(pulse_hold::Flanger::new(true, rate)),
        EffectKind::FlangerHold => Box::new(pulse_hold::Flanger::new(false, rate)),
        EffectKind::PhaserPulse => Box::new(pulse_hold::Phaser::new(true, rate)),
        EffectKind::PhaserHold => Box::new(pulse_hold::Phaser::new(false, rate)),
        EffectKind::Trans => Box::new(beat_lfo::BeatLfo::new(beat_lfo::Kind::Trans, rate)),
        EffectKind::Pan => Box::new(beat_lfo::BeatLfo::new(beat_lfo::Kind::Pan, rate)),
        EffectKind::BeatFilter => Box::new(beat_lfo::BeatLfo::new(beat_lfo::Kind::Filter, rate)),
        EffectKind::BeatFlanger => Box::new(beat_lfo::BeatLfo::new(beat_lfo::Kind::Flanger, rate)),
        EffectKind::BeatPhaser => Box::new(beat_lfo::BeatLfo::new(beat_lfo::Kind::Phaser, rate)),
        EffectKind::BeatReverb => Box::new(beat_reverb::BeatReverb::new(rate)),
        EffectKind::Stretch => Box::new(beat_reverb::Stretch::new(rate)),
        EffectKind::ModFilter | EffectKind::ModDecay => Box::new(Modifier(kind)),
        _ => macros::create(kind, sample_rate),
    }
}

#[derive(Debug)]
pub struct Bypass;

impl Effect for Bypass {
    fn kind(&self) -> EffectKind {
        EffectKind::None
    }
    fn process(&mut self, _buf: &mut [f32], _ctx: &Context) {}
    fn set_amount(&mut self, _amount: f32) {}
    fn set_param(&mut self, _index: usize, _value: f32) {}
    fn set_button(&mut self, _index: usize, _on: bool) {}
    fn reset(&mut self) {}
}

#[derive(Debug)]
pub struct Modifier(EffectKind);

impl Effect for Modifier {
    fn kind(&self) -> EffectKind {
        self.0
    }
    fn process(&mut self, _buf: &mut [f32], _ctx: &Context) {}
    fn set_amount(&mut self, _amount: f32) {}
    fn set_param(&mut self, _index: usize, _value: f32) {}
    fn set_button(&mut self, _index: usize, _on: bool) {}
    fn reset(&mut self) {}
}

pub(crate) fn stretch(value: f32, decay: f32, short: f32, long: f32) -> f32 {
    let d = decay.clamp(-1.0, 1.0);
    if d < 0.0 {
        value + (short - value) * -d
    } else {
        value + (long - value) * d
    }
}

pub(crate) fn note_hz(note: f64) -> f64 {
    8.175_8 * 2f64.powf(note / 12.0)
}

pub(crate) fn delay_mix(v: f32) -> (f32, f32) {
    let v = v.clamp(0.0, 1.0);
    ((2.0 * (1.0 - v)).min(1.0).powi(2), v * (2.0 - v))
}

pub(crate) fn reverb_mix(v: f32) -> (f32, f32) {
    let v = v.clamp(0.0, 1.0);
    ((2.0 * (1.0 - v)).min(1.0), (2.0 * v).min(1.0))
}

pub(crate) const NOTE_BEATS: [f64; 9] = [0.0625, 0.125, 0.1875, 0.25, 0.375, 0.5, 0.75, 1.0, 2.0];
pub(crate) const LFO_BEATS: [f64; 7] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0];

pub(crate) fn pick<T: Copy>(table: &[T], value: f32) -> T {
    let i = (value.clamp(0.0, 1.0) * (table.len() - 1) as f32).round() as usize;
    table[i]
}

pub(crate) fn nearest_beat(table: &[f64], beats: f64) -> f64 {
    let far = |b: f64| (b / beats.max(1e-6)).ln().abs();
    table
        .iter()
        .copied()
        .min_by(|a, b| far(*a).total_cmp(&far(*b)))
        .unwrap_or(beats)
}

pub(crate) fn beats_per_sample(ctx: &Context, rate: f64) -> f64 {
    ctx.bpm / 60.0 / rate
}

const LFO_RESET_LATE_S: f64 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq)]
enum LfoAsk {
    Start,
    Reset,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LfoStart {
    origin: f64,
    next: Option<f64>,
    ask: Option<LfoAsk>,
}

impl LfoStart {
    pub fn start(&mut self) {
        self.ask = Some(LfoAsk::Start);
    }

    pub fn reset(&mut self) {
        self.ask = Some(LfoAsk::Reset);
    }

    pub fn begin(&mut self, ctx: &Context, period_beats: f64, window: f64) -> bool {
        let half = (ctx.beat * 2.0).floor() / 2.0;
        match self.ask.take() {
            None => false,
            Some(LfoAsk::Start) => {
                self.origin = half;
                self.next = None;
                false
            }
            Some(LfoAsk::Reset) => {
                let into = (ctx.beat - half) * 2.0;
                let late_s = (ctx.beat - half) * 60.0 / ctx.bpm.max(1.0);
                if into < (period_beats).min(window) && late_s < LFO_RESET_LATE_S {
                    self.origin = half;
                    self.next = None;
                } else {
                    self.next = Some(half + 0.5);
                }
                true
            }
        }
    }

    pub fn beat(&mut self, beat: f64) -> f64 {
        if let Some(n) = self.next
            && beat >= n
        {
            self.origin = n;
            self.next = None;
        }
        beat - self.origin
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use crate::Context;

    pub const RATE: u32 = 48_000;

    pub fn ctx() -> Context {
        Context {
            bpm: 120.0,
            beat: 0.0,
            beat_end: 0.0,
        }
    }

    pub fn impulse(frames: usize) -> Vec<f32> {
        let mut v = vec![0.0; frames * 2];
        v[0] = 1.0;
        v[1] = 1.0;
        v
    }

    pub fn sine(freq: f64, frames: usize, amp: f32) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let v = (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(RATE)).sin()
                    as f32
                    * amp;
                [v, v]
            })
            .collect()
    }

    pub fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    pub fn run(effect: &mut dyn crate::Effect, input: &[f32], ctx: Context) -> Vec<f32> {
        let mut out = input.to_vec();
        let mut c = ctx;
        for block in out.chunks_mut(512) {
            effect.process(block, &c);
            c.beat += (block.len() / 2) as f64 * c.bpm / 60.0 / f64::from(RATE);
        }
        out
    }
}

#[cfg(test)]
mod lfo_start_tests {
    use super::LfoStart;
    use crate::Context;

    fn at(beat: f64) -> Context {
        Context {
            bpm: 120.0,
            beat,
            beat_end: beat,
        }
    }

    #[test]
    fn a_reset_just_after_a_half_beat_restarts_on_it() {
        let mut s = LfoStart::default();
        s.reset();
        assert!(s.begin(&at(2.6), 4.0, f64::INFINITY));
        assert!((s.beat(2.6) - 0.1).abs() < 1e-9);
    }

    #[test]
    fn a_late_reset_waits_for_the_next_half_beat() {
        let mut s = LfoStart::default();
        s.reset();
        s.begin(&at(2.9), 4.0, f64::INFINITY);
        assert!((s.beat(2.95) - 2.95).abs() < 1e-9, "old cycle goes on");
        assert!(s.beat(3.0).abs() < 1e-9);
        assert!((s.beat(3.25) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn the_gater_window_is_half_of_a_half_beat_at_most() {
        let mut s = LfoStart::default();
        s.reset();
        s.begin(&at(2.8), 1.0, 0.5);
        assert!((s.beat(2.9) - 2.9).abs() < 1e-9);
    }

    #[test]
    fn switching_on_counts_from_the_half_beat_just_passed() {
        let mut s = LfoStart::default();
        s.start();
        assert!(!s.begin(&at(5.7), 4.0, f64::INFINITY));
        assert!((s.beat(5.7) - 0.2).abs() < 1e-9);
    }
}
