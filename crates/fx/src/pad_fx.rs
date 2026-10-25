#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadFxType {
    Delay,
    Echo,
    CoilEcho,
    Reverb,
    Trans,
    FilterLfo,
    Flanger,
    Phaser,
    Robot,
    SlipRoll,
    Roll,
    RevRoll,
    MtDelay,
    UpEcho,
    DownEcho,
    RevDelay,
    Pan,
    Crush,
    Space,
    DubEcho,
    Pitch,
    Hpf,
    Lpf,
    Sweep,
    Noise,
    GateComp,
    SlipLoop,
    VBrake,
    ReleaseEcho,
    Backspin,
    BpfEcho,
    RmxNoise,
    CoilUp,
    ReverbUp,
    HpfEcho,
    LpfEcho,
    CrushEcho,
    CoilDown,
    ReverbDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadFxGroup {
    Beat,
    Color,
    SlipLoop,
    Release,
    Rmx,
}

impl PadFxType {
    pub fn group(self) -> PadFxGroup {
        use PadFxType as T;
        match self {
            T::Crush
            | T::Space
            | T::DubEcho
            | T::Pitch
            | T::Hpf
            | T::Lpf
            | T::Sweep
            | T::Noise
            | T::GateComp => PadFxGroup::Color,
            T::SlipLoop => PadFxGroup::SlipLoop,
            T::VBrake | T::ReleaseEcho | T::Backspin => PadFxGroup::Release,
            T::BpfEcho
            | T::RmxNoise
            | T::CoilUp
            | T::ReverbUp
            | T::HpfEcho
            | T::LpfEcho
            | T::CrushEcho
            | T::CoilDown
            | T::ReverbDown => PadFxGroup::Rmx,
            _ => PadFxGroup::Beat,
        }
    }

    pub fn name(self) -> &'static str {
        use PadFxType as T;
        match self {
            T::Delay => "DELAY",
            T::Echo | T::ReleaseEcho => "ECHO",
            T::CoilEcho => "COIL ECHO",
            T::Reverb => "REVERB",
            T::Trans => "TRANS",
            T::FilterLfo => "FILTER LFO",
            T::Flanger => "FLANGER",
            T::Phaser => "PHASER",
            T::Robot => "ROBOT",
            T::SlipRoll => "SLIP ROLL",
            T::Roll => "ROLL",
            T::RevRoll => "REV ROLL",
            T::MtDelay => "MT DELAY",
            T::UpEcho => "UP ECHO",
            T::DownEcho => "DOWN ECHO",
            T::RevDelay => "REV DELAY",
            T::Pan => "PAN",
            T::Crush => "CRUSH",
            T::Space => "SPACE",
            T::DubEcho => "DUB ECHO",
            T::Pitch => "PITCH",
            T::Hpf => "HPF",
            T::Lpf => "LPF",
            T::Sweep => "SWEEP",
            T::Noise | T::RmxNoise => "NOISE",
            T::GateComp => "GATE COMP",
            T::SlipLoop => "SLIP LOOP",
            T::VBrake => "V.BRAKE",
            T::Backspin => "BACKSPIN",
            T::BpfEcho => "BPF ECHO",
            T::CoilUp => "COIL UP",
            T::ReverbUp => "REVERB UP",
            T::HpfEcho => "HPF ECHO",
            T::LpfEcho => "LPF ECHO",
            T::CrushEcho => "CRUSH ECHO",
            T::CoilDown => "COIL DOWN",
            T::ReverbDown => "REVERB DOWN",
        }
    }

    pub fn editable(self) -> bool {
        true
    }

    pub fn beat_editable(self) -> bool {
        match self.group() {
            PadFxGroup::Beat => !matches!(self, Self::Reverb | Self::Robot),
            PadFxGroup::SlipLoop | PadFxGroup::Release => true,
            PadFxGroup::Color | PadFxGroup::Rmx => false,
        }
    }

    pub fn quantized(self) -> bool {
        matches!(self.group(), PadFxGroup::Beat | PadFxGroup::Rmx)
    }

    pub fn tail(self) -> bool {
        use PadFxType as T;
        matches!(
            self,
            T::Echo
                | T::UpEcho
                | T::DownEcho
                | T::CoilEcho
                | T::Reverb
                | T::DubEcho
                | T::Space
                | T::BpfEcho
                | T::HpfEcho
                | T::LpfEcho
                | T::CrushEcho
                | T::RmxNoise
                | T::ReverbUp
                | T::ReverbDown
                | T::CoilUp
                | T::CoilDown
        )
    }
}

pub const SHIFT_BEATS: [f64; 11] = [
    0.0625, 0.125, 0.25, 0.5, 0.75, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0,
];

pub fn shift_beat(beat: f64, longer: bool) -> f64 {
    let Some(i) = SHIFT_BEATS.iter().position(|&b| (b - beat).abs() < 1e-9) else {
        return SHIFT_BEATS[0];
    };
    let j = if longer {
        (i + 1).min(SHIFT_BEATS.len() - 1)
    } else {
        i.saturating_sub(1)
    };
    SHIFT_BEATS[j]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadPreset {
    pub kind: PadFxType,
    pub beat: f64,
    pub level: u8,
    pub color: i8,
    pub room: u8,
    pub hold: bool,
}

impl PadPreset {
    const fn beat_fx(kind: PadFxType, beat: f64, level: u8) -> Self {
        Self {
            kind,
            beat,
            level,
            color: 0,
            room: 50,
            hold: false,
        }
    }

    const fn color_fx(kind: PadFxType, color: i8) -> Self {
        Self {
            kind,
            beat: 1.0,
            level: 50,
            color,
            room: 50,
            hold: false,
        }
    }

    pub fn level01(self) -> f32 {
        f32::from(self.level) * 0.01
    }

    pub fn room01(self) -> f32 {
        f32::from(self.room.max(1) - 1) / 99.0
    }

    pub fn color01(self) -> f32 {
        if matches!(self.kind, PadFxType::Hpf | PadFxType::Lpf) {
            f32::from(self.color) * 0.01
        } else {
            (f32::from(self.color) + 100.0) * 0.005
        }
    }
}

pub fn factory(bank: usize, pad: usize) -> Option<PadPreset> {
    use PadFxType as T;
    const B: fn(PadFxType, f64, u8) -> PadPreset = PadPreset::beat_fx;
    let reverb = PadPreset {
        room: 50,
        ..PadPreset::beat_fx(T::Reverb, 1.0, 30)
    };
    let release = |kind, beat| PadPreset {
        hold: false,
        ..PadPreset::beat_fx(kind, beat, 50)
    };
    let p = match (bank, pad) {
        (0, 0) => B(T::Roll, 0.5, 50),
        (0, 1) => PadPreset::color_fx(T::Sweep, 80),
        (0, 2) => B(T::Flanger, 16.0, 70),
        (0, 3) => release(T::VBrake, 0.75),
        (0, 4) => B(T::Echo, 0.25, 30),
        (0, 5) => B(T::Echo, 0.5, 30),
        (0, 6) => reverb,
        (0, 7) => release(T::ReleaseEcho, 0.5),
        (0, 8) => B(T::Trans, 0.5, 50),
        (0, 9) => PadPreset::color_fx(T::Crush, 40),
        (0, 10) => B(T::FilterLfo, 4.0, 100),
        (0, 11) => release(T::Backspin, 4.0),
        (0, 12) => B(T::MtDelay, 0.125, 30),
        (0, 13) => PadPreset::color_fx(T::DubEcho, 30),
        (0, 14) => PadPreset::color_fx(T::Space, 30),
        (0, 15) => release(T::ReleaseEcho, 1.0),
        (1, 0..16) => {
            let beat = [0.0625, 0.125, 0.25, 0.5][pad % 4];
            let kind = [T::SlipLoop, T::RevRoll, T::Trans, T::MtDelay][pad / 4];
            B(kind, beat, 50)
        }
        _ => return None,
    };
    Some(p)
}

pub const LAYERS: usize = 4;

pub type PadId = (u8, u8);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayerOp {
    Start(usize, PadPreset),
    Update(usize, PadPreset),
    Stop(usize),
    StartRelease(PadPreset),
    StopRelease,
}

#[derive(Debug, Clone, Copy)]
struct Layer {
    kind: PadFxType,
    owner: PadId,
    since: u64,
}

const MAX_HELD: usize = 32;

#[derive(Debug)]
pub struct PadFxPads {
    held: Vec<(PadId, PadPreset)>,
    layers: [Option<Layer>; LAYERS],
    loaded: [Option<PadFxType>; LAYERS],
    release: Option<(PadId, bool)>,
    presses: u64,
}

impl Default for PadFxPads {
    fn default() -> Self {
        Self {
            held: Vec::with_capacity(MAX_HELD),
            layers: [None; LAYERS],
            loaded: [None; LAYERS],
            release: None,
            presses: 0,
        }
    }
}

impl PadFxPads {
    pub fn lit(&self, pad: PadId) -> bool {
        self.release.is_some_and(|(p, _)| p == pad)
            || self.layers.iter().flatten().any(|l| l.owner == pad)
    }

    pub fn press(&mut self, pad: PadId, preset: PadPreset, mut op: impl FnMut(LayerOp)) {
        if preset.kind.group() == PadFxGroup::Release {
            self.press_release(pad, preset, &mut op);
            return;
        }
        if self.release.is_some() || self.held.len() >= MAX_HELD {
            return;
        }
        self.presses += 1;
        self.held.retain(|(p, _)| *p != pad);
        self.held.push((pad, preset));
        let kind = preset.kind;
        if let Some(i) = self.layer_of(kind) {
            if let Some(layer) = &mut self.layers[i] {
                layer.owner = pad;
            }
            if kind.editable() {
                op(LayerOp::Update(i, preset));
            } else {
                op(LayerOp::Stop(i));
                op(LayerOp::Start(i, preset));
            }
            return;
        }
        if self.layers.iter().all(Option::is_some) {
            let oldest = (0..LAYERS)
                .min_by_key(|&i| self.layers[i].map_or(u64::MAX, |l| l.since))
                .unwrap_or(0);
            self.layers[oldest] = None;
            op(LayerOp::Stop(oldest));
        }
        let free = |i: &usize| self.layers[*i].is_none();
        let Some(i) = (0..LAYERS)
            .filter(free)
            .find(|&i| self.loaded[i] == Some(kind))
            .or_else(|| (0..LAYERS).find(free))
        else {
            return;
        };
        self.layers[i] = Some(Layer {
            kind,
            owner: pad,
            since: self.presses,
        });
        self.loaded[i] = Some(kind);
        op(LayerOp::Start(i, preset));
    }

    pub fn release(&mut self, pad: PadId, mut op: impl FnMut(LayerOp)) {
        if let Some((p, hold)) = self.release
            && p == pad
        {
            if !hold {
                self.release = None;
                op(LayerOp::StopRelease);
            }
            return;
        }
        let Some(at) = self.held.iter().position(|(p, _)| *p == pad) else {
            return;
        };
        let (_, preset) = self.held.remove(at);
        let kind = preset.kind;
        if self.held[at..].iter().any(|(_, q)| q.kind == kind) {
            return;
        }
        let Some(i) = self.layer_of(kind) else {
            return;
        };
        if let Some(&(older, older_preset)) = self.held.iter().rev().find(|(_, q)| q.kind == kind) {
            if let Some(layer) = &mut self.layers[i] {
                layer.owner = older;
            }
            op(LayerOp::Update(i, older_preset));
        } else {
            self.layers[i] = None;
            op(LayerOp::Stop(i));
        }
    }

    fn press_release(&mut self, pad: PadId, preset: PadPreset, op: &mut impl FnMut(LayerOp)) {
        if let Some((p, _)) = self.release {
            self.release = None;
            op(LayerOp::StopRelease);
            if p == pad {
                return;
            }
        }
        for i in 0..LAYERS {
            if self.layers[i].take().is_some() {
                op(LayerOp::Stop(i));
            }
        }
        self.held.clear();
        self.release = Some((pad, preset.hold));
        op(LayerOp::StartRelease(preset));
    }

    pub fn shift_beat(&mut self, longer: bool, mut op: impl FnMut(LayerOp)) {
        for (pad, preset) in &mut self.held {
            if !preset.kind.beat_editable() {
                continue;
            }
            preset.beat = shift_beat(preset.beat, longer);
            let owner = self
                .layers
                .iter()
                .position(|l| l.is_some_and(|l| l.owner == *pad));
            if let Some(i) = owner {
                op(LayerOp::Update(i, *preset));
            }
        }
    }

    fn layer_of(&self, kind: PadFxType) -> Option<usize> {
        self.layers
            .iter()
            .position(|l| l.is_some_and(|l| l.kind == kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(f: impl FnOnce(&mut dyn FnMut(LayerOp))) -> Vec<LayerOp> {
        let mut v = Vec::new();
        f(&mut |o| v.push(o));
        v
    }

    fn preset(kind: PadFxType, beat: f64) -> PadPreset {
        PadPreset::beat_fx(kind, beat, 50)
    }

    #[test]
    fn the_factory_banks_are_right() {
        assert_eq!(
            factory(0, 0).map(|p| (p.kind, p.beat, p.level)),
            Some((PadFxType::Roll, 0.5, 50))
        );
        assert_eq!(
            factory(0, 1).map(|p| (p.kind, p.color)),
            Some((PadFxType::Sweep, 80))
        );
        assert_eq!(
            factory(0, 15).map(|p| (p.kind, p.beat, p.hold)),
            Some((PadFxType::ReleaseEcho, 1.0, false))
        );
        assert_eq!(
            factory(1, 5).map(|p| (p.kind, p.beat)),
            Some((PadFxType::RevRoll, 0.125))
        );
        assert_eq!(
            factory(1, 15).map(|p| (p.kind, p.beat)),
            Some((PadFxType::MtDelay, 0.5))
        );
        assert_eq!(factory(2, 0), None);
        assert_eq!(factory(0, 16), None);
    }

    #[test]
    fn parameters_scale_right() {
        let sweep = PadPreset::color_fx(PadFxType::Sweep, 80);
        assert!((sweep.color01() - 0.9).abs() < 1e-6);
        let hpf = PadPreset::color_fx(PadFxType::Hpf, 40);
        assert!((hpf.color01() - 0.4).abs() < 1e-6);
        let reverb = factory(0, 6).unwrap();
        assert!((reverb.room01() - 49.0 / 99.0).abs() < 1e-6);
        assert!((reverb.level01() - 0.3).abs() < 1e-6);
    }

    #[test]
    fn the_same_type_hands_over_without_a_cut_and_back() {
        let mut pads = PadFxPads::default();
        let half = preset(PadFxType::Echo, 0.5);
        let quarter = preset(PadFxType::Echo, 0.25);
        assert_eq!(
            ops(|o| pads.press((0, 5), half, o)),
            [LayerOp::Start(0, half)]
        );
        assert_eq!(
            ops(|o| pads.press((0, 4), quarter, o)),
            [LayerOp::Update(0, quarter)]
        );
        assert!(pads.lit((0, 4)) && !pads.lit((0, 5)));
        assert_eq!(ops(|o| pads.release((0, 4), o)), [LayerOp::Update(0, half)]);
        assert!(pads.lit((0, 5)));
        assert_eq!(ops(|o| pads.release((0, 5), o)), [LayerOp::Stop(0)]);
    }

    #[test]
    fn letting_go_an_older_pad_of_the_type_only_forgets_it() {
        let mut pads = PadFxPads::default();
        let a = preset(PadFxType::Echo, 0.5);
        let b = preset(PadFxType::Echo, 0.25);
        pads.press((0, 5), a, |_| {});
        pads.press((0, 4), b, |_| {});
        assert_eq!(ops(|o| pads.release((0, 5), o)), []);
        assert_eq!(ops(|o| pads.release((0, 4), o)), [LayerOp::Stop(0)]);
    }

    #[test]
    fn a_fifth_type_stops_the_oldest_and_reuses_a_loaded_layer() {
        let mut pads = PadFxPads::default();
        let kinds = [
            PadFxType::Echo,
            PadFxType::Roll,
            PadFxType::Trans,
            PadFxType::Flanger,
        ];
        for (n, k) in kinds.into_iter().enumerate() {
            pads.press((0, n as u8), preset(k, 1.0), |_| {});
        }
        let coil_echo = preset(PadFxType::CoilEcho, 1.0);
        assert_eq!(
            ops(|o| pads.press((0, 9), coil_echo, o)),
            [LayerOp::Stop(0), LayerOp::Start(0, coil_echo)]
        );
        pads.release((0, 1), |_| {});
        pads.release((0, 2), |_| {});
        let trans = preset(PadFxType::Trans, 0.5);
        assert_eq!(
            ops(|o| pads.press((0, 7), trans, o)),
            [LayerOp::Start(2, trans)]
        );
    }

    #[test]
    fn a_release_fx_switches_the_rest_off_and_holds_the_pads() {
        let mut pads = PadFxPads::default();
        let echo = preset(PadFxType::Echo, 0.5);
        pads.press((0, 5), echo, |_| {});
        let brake = factory(0, 3).unwrap();
        assert_eq!(
            ops(|o| pads.press((0, 3), brake, o)),
            [LayerOp::Stop(0), LayerOp::StartRelease(brake)]
        );
        assert_eq!(ops(|o| pads.press((0, 4), echo, o)), []);
        assert_eq!(ops(|o| pads.release((0, 3), o)), [LayerOp::StopRelease]);
        let held = PadPreset {
            hold: true,
            ..brake
        };
        pads.press((0, 3), held, |_| {});
        assert_eq!(ops(|o| pads.release((0, 3), o)), []);
        assert_eq!(ops(|o| pads.press((0, 3), held, o)), [LayerOp::StopRelease]);
    }

    #[test]
    fn parameter1_shifts_the_held_beats_until_let_go() {
        assert_eq!(shift_beat(0.5, true), 0.75);
        assert_eq!(shift_beat(0.0625, false), 0.0625);
        assert_eq!(shift_beat(32.0, true), 32.0);
        assert_eq!(shift_beat(3.0, true), 0.0625);
        let mut pads = PadFxPads::default();
        let echo = preset(PadFxType::Echo, 0.5);
        let sweep = PadPreset::color_fx(PadFxType::Sweep, 80);
        pads.press((0, 5), echo, |_| {});
        pads.press((0, 1), sweep, |_| {});
        let shorter = PadPreset { beat: 0.25, ..echo };
        assert_eq!(
            ops(|o| pads.shift_beat(false, o)),
            [LayerOp::Update(0, shorter)]
        );
        pads.release((0, 5), |_| {});
        assert_eq!(
            ops(|o| pads.press((0, 5), echo, o)),
            [LayerOp::Start(0, echo)]
        );
        assert!(!PadFxType::Reverb.beat_editable() && PadFxType::Backspin.beat_editable());
        assert!(PadFxType::Echo.tail() && !PadFxType::Delay.tail());
    }

    #[test]
    fn colour_fx_switch_at_once_and_beat_fx_on_the_beat() {
        assert!(PadFxType::Echo.quantized() && PadFxType::CoilUp.quantized());
        assert!(!PadFxType::Sweep.quantized());
        assert_eq!(PadFxType::Backspin.group(), PadFxGroup::Release);
    }
}
