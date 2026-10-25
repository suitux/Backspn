pub mod block_ramp;
pub mod classic_eq;
pub mod classic_limiter;
pub mod club_filter;
pub mod curves;
pub mod four_band;
pub mod hilbert;
mod rbj;
pub mod stem_master;
pub mod stem_strip;
pub mod sweeping_shelves_eq;
pub mod three_centre_eq;
pub mod transparent_limiter;
pub mod wide_bands_eq;
pub mod wide_isolator;
pub mod z_filter;
use backspn_dsp::{Isolator, Limiter, Smoothed, Svf};
use serde::{Deserialize, Serialize};

use classic_eq::ClassicEq;
use classic_limiter::ClassicLimiter;
use club_filter::{ClubFilter, club_setting};
use four_band::FourBandEq;
use sweeping_shelves_eq::SweepingShelvesEq;
use three_centre_eq::ThreeCentreEq;
use transparent_limiter::TransparentLimiter;
use wide_bands_eq::WideBandsEq;
use wide_isolator::WideIsolator;
use z_filter::ZFilter;

pub use curves::{
    FilterSetting, balance_gains, crossfader_gains, eq_gain, fader_gain, filter_setting,
};

pub const CHANNELS: usize = 4;
pub const LOW_CROSSOVER_HZ: f64 = 250.0;
pub const HIGH_CROSSOVER_HZ: f64 = 2_500.0;
const FILTER_Q: f64 = 1.0;
const CONTROL_BLOCK: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Low = 0,
    Mid = 1,
    High = 2,
    MidLow = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Assign {
    #[default]
    Thru,
    A,
    B,
}

const BALANCE_RAMP_S: f64 = 64.0 / 44_100.0;

pub const DEFAULT_HEADROOM_DB: f32 = -3.0;

pub const MAX_AUTO_GAIN_DB: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Gain(usize, f32),
    AutoGain(usize, f32),
    Balance(usize, f32),
    Mute(usize, bool),
    MasterBalance(f32),
    MasterMono(bool),
    MonitorMono(bool),
    Headroom(f32),
    Eq(usize, Band, f32),
    Filter(usize, f32),
    Fader(usize, f32),
    Cue(usize, bool),
    Assign(usize, Assign),
    Crossfader(f32),
    CrossfaderCurve(f32),
    MasterVolume(f32),
    CueMix(f32),
    CueVolume(f32),
    FilterModel(FilterModel),
    EqModel(EqModel),
    FilterResonance(f32),
    Limiter(bool),
    LimiterModel(LimiterModel),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelState {
    pub gain_db: f32,
    pub auto_gain_db: f32,
    pub balance: f32,
    pub mute: bool,
    pub eq: [f32; 4],
    pub filter: f32,
    pub fader: f32,
    pub cue: bool,
    pub assign: Assign,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            auto_gain_db: 0.0,
            balance: 0.0,
            mute: false,
            eq: [0.5; 4],
            filter: 0.0,
            fader: 1.0,
            cue: false,
            assign: Assign::Thru,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EqModel {
    #[default]
    Isolator,
    FourBand,
    WideIsolator,
    Classic,
    #[serde(alias = "P600")]
    ThreeCentre,
    #[serde(alias = "Nuo4")]
    SweepingShelves,
    #[serde(alias = "P800")]
    WideBands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FilterModel {
    #[default]
    Standard,
    Club,
    Ladder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LimiterModel {
    #[default]
    Standard,
    Transparent,
    Classic,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerState {
    pub channels: [ChannelState; CHANNELS],
    pub crossfader: f32,
    pub crossfader_curve: f32,
    pub master_volume: f32,
    pub master_balance: f32,
    pub master_mono: bool,
    pub monitor_mono: bool,
    pub headroom_db: f32,
    pub cue_mix: f32,
    pub cue_volume: f32,
    pub limiter: bool,
    pub limiter_model: LimiterModel,
    pub filter_model: FilterModel,
    pub eq_model: EqModel,
    pub filter_resonance: f32,
    pub channel_peaks: [f32; CHANNELS],
    pub master_peak: [f32; 2],
    pub limiter_db: f32,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            channels: [ChannelState::default(); CHANNELS],
            crossfader: 0.5,
            crossfader_curve: 0.0,
            master_volume: 0.8,
            master_balance: 0.0,
            master_mono: false,
            monitor_mono: false,
            headroom_db: DEFAULT_HEADROOM_DB,
            cue_mix: 0.0,
            cue_volume: 0.8,
            limiter: true,
            limiter_model: LimiterModel::Standard,
            filter_model: FilterModel::Standard,
            eq_model: EqModel::Isolator,
            filter_resonance: 0.5,
            channel_peaks: [0.0; CHANNELS],
            master_peak: [0.0; 2],
            limiter_db: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChannelFilter {
    filter: Svf,
    club: ClubFilter,
    ladder: ZFilter,
    wet: Smoothed,
    knob: Smoothed,
    high_pass: bool,
}

impl ChannelFilter {
    pub fn new(rate: f64) -> Self {
        Self {
            filter: Svf::default(),
            club: ClubFilter::default(),
            ladder: ZFilter::new(rate),
            wet: Smoothed::new(0.0, 0.01, rate),
            knob: Smoothed::new(0.0, 0.02, rate / CONTROL_BLOCK as f64),
            high_pass: false,
        }
    }

    pub fn set(&mut self, knob: f32) {
        let knob = knob.clamp(-1.0, 1.0);
        self.knob.set(knob);
        self.ladder.set(knob);
    }

    fn start_block(&mut self, model: FilterModel, resonance: f32, rate: f64) {
        let knob = self.knob.tick();
        self.high_pass = false;
        match model {
            FilterModel::Standard => match filter_setting(knob) {
                FilterSetting::Off => self.wet.set(0.0),
                setting @ (FilterSetting::LowPass(hz) | FilterSetting::HighPass(hz)) => {
                    self.filter.set(hz, FILTER_Q, rate);
                    self.wet.set(1.0);
                    self.high_pass = matches!(setting, FilterSetting::HighPass(_));
                }
            },
            FilterModel::Ladder => self.wet.set(0.0),
            FilterModel::Club => {
                let setting = club_setting(knob, resonance);
                if setting.bypass {
                    self.wet.set(0.0);
                } else {
                    self.club.set(&setting, rate);
                    self.wet.set(1.0);
                }
            }
        }
    }

    #[inline]
    fn tick(&mut self, frame: &mut [f32; 2], model: FilterModel) {
        let wet = self.wet.tick();
        if model == FilterModel::Ladder {
            self.ladder.tick(frame);
            return;
        }
        for (ch, x) in frame.iter_mut().enumerate() {
            let dry = *x;
            let filtered = match model {
                FilterModel::Standard => {
                    let (lp, hp) = self.filter.tick(ch, dry);
                    if self.high_pass { hp } else { lp }
                }
                FilterModel::Club => self.club.tick(ch, dry),
                FilterModel::Ladder => dry,
            };
            *x = dry + (filtered - dry) * wet;
        }
    }

    fn end_block(&mut self) {
        if self.wet.value() == 0.0 {
            self.filter.reset();
            self.club.reset();
        }
    }

    pub fn process(&mut self, audio: &mut [f32], model: FilterModel, resonance: f32, rate: f64) {
        let (frames, _) = audio.as_chunks_mut::<2>();
        for block in frames.chunks_mut(CONTROL_BLOCK) {
            self.start_block(model, resonance, rate);
            for frame in block.iter_mut() {
                self.tick(frame, model);
            }
            self.end_block();
        }
    }
}

#[derive(Debug, Clone)]
struct Strip {
    isolator: Isolator,
    filter: ChannelFilter,
    trim: Smoothed,
    eq: [Smoothed; 3],
    four_band: FourBandEq,
    wide: WideIsolator,
    classic: ClassicEq,
    three_centre: ThreeCentreEq,
    sweeping_shelves: SweepingShelvesEq,
    wide_bands: WideBandsEq,
    level: Smoothed,
    balance: [Smoothed; 2],
    cue: Smoothed,
}

impl Strip {
    fn new(rate: f64) -> Self {
        let smooth = |v| Smoothed::new(v, 0.01, rate);
        Self {
            isolator: Isolator::new(LOW_CROSSOVER_HZ, HIGH_CROSSOVER_HZ, rate),
            filter: ChannelFilter::new(rate),
            trim: smooth(1.0),
            eq: [smooth(1.0); 3],
            four_band: FourBandEq::new(rate),
            wide: WideIsolator::new(rate),
            classic: ClassicEq::new(rate),
            three_centre: ThreeCentreEq::new(rate),
            sweeping_shelves: SweepingShelvesEq::new(rate),
            wide_bands: WideBandsEq::new(rate),
            level: smooth(1.0),
            balance: [Smoothed::new(1.0, BALANCE_RAMP_S, rate); 2],
            cue: smooth(0.0),
        }
    }
}

#[derive(Debug)]
pub struct Mixer {
    rate: f64,
    state: MixerState,
    strips: [Strip; CHANNELS],
    master_volume: Smoothed,
    master_balance: [Smoothed; 2],
    headroom: f32,
    cue_mix: Smoothed,
    cue_volume: Smoothed,
    limiter: Limiter,
    transparent: TransparentLimiter,
    classic_limiter: ClassicLimiter,
    decay_per_frame: f32,
}

impl Mixer {
    pub fn new(sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate);
        let state = MixerState::default();
        let smooth = |v| Smoothed::new(v, 0.01, rate);
        let mut mixer = Self {
            rate,
            state,
            strips: std::array::from_fn(|_| Strip::new(rate)),
            master_volume: smooth(fader_gain(state.master_volume)),
            master_balance: [Smoothed::new(1.0, BALANCE_RAMP_S, rate); 2],
            headroom: db_to_gain(DEFAULT_HEADROOM_DB),
            cue_mix: smooth(state.cue_mix),
            cue_volume: smooth(fader_gain(state.cue_volume)),
            limiter: Limiter::new(rate),
            transparent: TransparentLimiter::new(rate),
            classic_limiter: ClassicLimiter::new(rate),
            decay_per_frame: 10f32.powf(-20.0 / 20.0 / sample_rate as f32),
        };
        for i in 0..CHANNELS {
            mixer.update_level(i);
        }
        mixer
    }

    pub fn handle(&mut self, command: Command) {
        let s = &mut self.state;
        match command {
            Command::Gain(i, db) if i < CHANNELS => {
                s.channels[i].gain_db = db.min(12.0);
                self.update_trim(i);
            }
            Command::AutoGain(i, db) if i < CHANNELS && db.is_finite() => {
                s.channels[i].auto_gain_db = db.clamp(-MAX_AUTO_GAIN_DB, MAX_AUTO_GAIN_DB);
                self.update_trim(i);
            }
            Command::Balance(i, b) if i < CHANNELS && b.is_finite() => {
                s.channels[i].balance = b.clamp(-1.0, 1.0);
                let gains = balance_gains(b);
                for (g, smoothed) in gains.into_iter().zip(&mut self.strips[i].balance) {
                    smoothed.set(g);
                }
            }
            Command::Mute(i, on) if i < CHANNELS => {
                s.channels[i].mute = on;
                self.update_level(i);
            }
            Command::MasterMono(on) => s.master_mono = on,
            Command::Headroom(db) if db.is_finite() => {
                s.headroom_db = db.min(0.0);
                self.headroom = db_to_gain(s.headroom_db);
            }
            Command::MonitorMono(on) => s.monitor_mono = on,
            Command::MasterBalance(b) if b.is_finite() => {
                s.master_balance = b.clamp(-1.0, 1.0);
                for (g, smoothed) in balance_gains(b).into_iter().zip(&mut self.master_balance) {
                    smoothed.set(g);
                }
            }
            Command::Eq(i, band, value) if i < CHANNELS => {
                let value = value.clamp(0.0, 1.0);
                s.channels[i].eq[band as usize] = value;
                if let Some(gain) = self.strips[i].eq.get_mut(band as usize) {
                    gain.set(eq_gain(value));
                }
                let [low, mid, high, mid_low] = s.channels[i].eq;
                self.strips[i].four_band.set([low, mid_low, mid, high]);
                self.strips[i].wide.set([low, mid, high]);
                self.strips[i].classic.set([low, mid, high]);
                self.strips[i].three_centre.set([low, mid, high]);
                self.strips[i].sweeping_shelves.set([low, mid, high]);
                self.strips[i].wide_bands.set([low, mid, high]);
            }
            Command::Filter(i, value) if i < CHANNELS => {
                let value = value.clamp(-1.0, 1.0);
                s.channels[i].filter = value;
                self.strips[i].filter.set(value);
            }
            Command::Fader(i, value) if i < CHANNELS => {
                s.channels[i].fader = value.clamp(0.0, 1.0);
                self.update_level(i);
            }
            Command::Cue(i, on) if i < CHANNELS => {
                s.channels[i].cue = on;
                self.strips[i].cue.set(if on { 1.0 } else { 0.0 });
            }
            Command::Assign(i, assign) if i < CHANNELS => {
                s.channels[i].assign = assign;
                self.update_level(i);
            }
            Command::Crossfader(value) => {
                s.crossfader = value.clamp(0.0, 1.0);
                self.update_all_levels();
            }
            Command::CrossfaderCurve(value) => {
                s.crossfader_curve = value.clamp(0.0, 1.0);
                self.update_all_levels();
            }
            Command::MasterVolume(value) => {
                s.master_volume = value.clamp(0.0, 1.0);
                self.master_volume.set(fader_gain(s.master_volume));
            }
            Command::CueMix(value) => {
                s.cue_mix = value.clamp(0.0, 1.0);
                self.cue_mix.set(s.cue_mix);
            }
            Command::CueVolume(value) => {
                s.cue_volume = value.clamp(0.0, 1.0);
                self.cue_volume.set(fader_gain(s.cue_volume));
            }
            Command::Limiter(on) => s.limiter = on,
            Command::LimiterModel(model) => s.limiter_model = model,
            Command::FilterModel(model) => s.filter_model = model,
            Command::EqModel(model) => {
                s.eq_model = model;
                for strip in &mut self.strips {
                    strip.four_band.reset();
                    strip.three_centre.reset();
                    strip.sweeping_shelves.reset();
                    strip.wide_bands.reset();
                }
            }
            Command::FilterResonance(value) => s.filter_resonance = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    pub fn filter_settings(&self) -> (FilterModel, f32) {
        (self.state.filter_model, self.state.filter_resonance)
    }

    pub fn state(&self) -> MixerState {
        self.state
    }

    fn update_all_levels(&mut self) {
        for i in 0..CHANNELS {
            self.update_level(i);
        }
    }

    fn update_trim(&mut self, i: usize) {
        let ch = self.state.channels[i];
        self.strips[i]
            .trim
            .set(db_to_gain(ch.gain_db) * db_to_gain(ch.auto_gain_db));
    }

    fn update_level(&mut self, i: usize) {
        let ch = self.state.channels[i];
        let (a, b) = crossfader_gains(self.state.crossfader, self.state.crossfader_curve);
        let xf = match ch.assign {
            Assign::Thru => 1.0,
            Assign::A => a,
            Assign::B => b,
        };
        let level = if ch.mute {
            0.0
        } else {
            fader_gain(ch.fader) * xf
        };
        self.strips[i].level.set(level);
    }

    pub fn process(&mut self, inputs: &mut [&mut [f32]], master: &mut [f32], cue: &mut [f32]) {
        self.process_channels(inputs, cue);
        Self::sum(inputs, master);
        self.finish(master, cue);
    }

    pub fn process_channels(&mut self, inputs: &mut [&mut [f32]], cue: &mut [f32]) {
        self.pre_channels(inputs);
        self.post_channels(inputs, cue);
    }

    pub fn pre_channels(&mut self, inputs: &mut [&mut [f32]]) {
        for (i, input) in inputs.iter_mut().enumerate().take(CHANNELS) {
            self.pre_strip(i, input);
        }
    }

    pub fn post_channels(&mut self, inputs: &mut [&mut [f32]], cue: &mut [f32]) {
        cue.fill(0.0);
        let frames = cue.len() / 2;
        for (i, input) in inputs.iter_mut().enumerate().take(CHANNELS) {
            let peak = self.post_strip(i, &mut input[..2 * frames], cue);
            let held = self.state.channel_peaks[i] * self.decay_per_frame.powi(frames as i32);
            self.state.channel_peaks[i] = held.max(peak);
        }
    }

    pub fn sum(inputs: &[&mut [f32]], master: &mut [f32]) {
        master.fill(0.0);
        let n = master.len();
        for input in inputs.iter().take(CHANNELS) {
            for (m, x) in master.iter_mut().zip(&input[..n]) {
                *m += x;
            }
        }
    }

    fn pre_strip(&mut self, i: usize, input: &mut [f32]) {
        let rate = self.rate;
        let (model, resonance) = (self.state.filter_model, self.state.filter_resonance);
        let eq_model = self.state.eq_model;
        let strip = &mut self.strips[i];
        let (frames, _) = input.as_chunks_mut::<2>();
        for block in frames.chunks_mut(CONTROL_BLOCK) {
            strip.filter.start_block(model, resonance, rate);
            for frame in block.iter_mut() {
                let trim = strip.trim.tick();
                let eq = [strip.eq[0].tick(), strip.eq[1].tick(), strip.eq[2].tick()];
                let mut eqd_frame = [frame[0] * trim, frame[1] * trim];
                match eq_model {
                    EqModel::FourBand => strip.four_band.tick(&mut eqd_frame),
                    EqModel::WideIsolator => strip.wide.tick(&mut eqd_frame),
                    EqModel::Classic => strip.classic.tick(&mut eqd_frame),
                    EqModel::ThreeCentre => strip.three_centre.tick(&mut eqd_frame),
                    EqModel::SweepingShelves => strip.sweeping_shelves.tick(&mut eqd_frame),
                    EqModel::WideBands => strip.wide_bands.tick(&mut eqd_frame),
                    EqModel::Isolator => {
                        for (ch, x) in eqd_frame.iter_mut().enumerate() {
                            let [lo, mid, hi] = strip.isolator.split(ch, *x);
                            *x = lo * eq[0] + mid * eq[1] + hi * eq[2];
                        }
                    }
                }
                strip.filter.tick(&mut eqd_frame, model);
                *frame = eqd_frame;
            }
            strip.filter.end_block();
        }
    }

    fn post_strip(&mut self, i: usize, input: &mut [f32], cue: &mut [f32]) -> f32 {
        let strip = &mut self.strips[i];
        let mut peak = 0.0f32;
        let (frames, _) = input.as_chunks_mut::<2>();
        let (cue, _) = cue.as_chunks_mut::<2>();
        for (frame, c) in frames.iter_mut().zip(cue.iter_mut()) {
            let level = strip.level.tick();
            let balance = [strip.balance[0].tick(), strip.balance[1].tick()];
            let cue_gain = strip.cue.tick();
            for ch in 0..2 {
                let y = frame[ch];
                peak = peak.max(y.abs());
                frame[ch] = y * level * balance[ch];
                c[ch] += y * cue_gain;
            }
        }
        peak
    }

    pub fn finish(&mut self, master: &mut [f32], cue: &mut [f32]) {
        let (master, _) = master.as_chunks_mut::<2>();
        let (cue, _) = cue.as_chunks_mut::<2>();
        let mut peak = [0.0f32; 2];
        self.limiter.min_gain = 1.0;
        self.transparent.min_gain = 1.0;
        self.classic_limiter.min_gain = 1.0;
        for (m, c) in master.iter_mut().zip(cue.iter_mut()) {
            let volume = self.master_volume.tick();
            let balance = [self.master_balance[0].tick(), self.master_balance[1].tick()];
            let gain = volume * self.headroom;
            let pre = [m[0] * gain * balance[0], m[1] * gain * balance[1]];
            peak = [peak[0].max(pre[0].abs()), peak[1].max(pre[1].abs())];
            let out = match (self.state.limiter, self.state.limiter_model) {
                (false, _) => pre.map(|s| s.clamp(-1.0, 1.0)),
                (true, LimiterModel::Standard) => self.limiter.tick(pre),
                (true, LimiterModel::Transparent) => self.transparent.tick(pre),
                (true, LimiterModel::Classic) => self.classic_limiter.tick(pre),
            };
            let out = if self.state.master_mono {
                mono(out)
            } else {
                out
            };
            *m = out;
            let mix = self.cue_mix.tick();
            let phones = self.cue_volume.tick();
            let cue = [c[0] * self.headroom, c[1] * self.headroom];
            let phones_out = [0, 1]
                .map(|ch| ((cue[ch] * (1.0 - mix) + out[ch] * mix) * phones).clamp(-1.0, 1.0));
            *c = if self.state.monitor_mono {
                mono(phones_out)
            } else {
                phones_out
            };
        }
        let decay = self.decay_per_frame.powi(master.len() as i32);
        for (held, new) in self.state.master_peak.iter_mut().zip(peak) {
            *held = (*held * decay).max(new);
        }
        let min_gain = match self.state.limiter_model {
            LimiterModel::Standard => self.limiter.min_gain,
            LimiterModel::Transparent => self.transparent.min_gain,
            LimiterModel::Classic => self.classic_limiter.min_gain,
        };
        self.state.limiter_db = 20.0 * min_gain.max(1e-6).log10();
    }

    pub fn finish_external(&mut self, outputs: &mut [&mut [f32]]) {
        let frames = outputs.first().map_or(0, |o| o.len() / 2);
        let mut peak = [0.0f32; 2];
        for i in 0..frames {
            let gain = self.master_volume.tick() * self.headroom;
            for out in outputs.iter_mut() {
                for ch in 0..2 {
                    let y = out[2 * i + ch] * gain;
                    peak[ch] = peak[ch].max(y.abs());
                    out[2 * i + ch] = y.clamp(-1.0, 1.0);
                }
            }
        }
        let decay = self.decay_per_frame.powi(frames as i32);
        for (held, new) in self.state.master_peak.iter_mut().zip(peak) {
            *held = (*held * decay).max(new);
        }
        self.state.limiter_db = 0.0;
    }
}

fn mono([l, r]: [f32; 2]) -> [f32; 2] {
    let m = f32::midpoint(l, r);
    [m, m]
}

fn db_to_gain(db: f32) -> f32 {
    if db <= -96.0 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

#[cfg(test)]
mod tests;
