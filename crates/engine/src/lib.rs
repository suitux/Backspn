use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use backspn_deck::Deck;
pub use backspn_deck::{
    Command as DeckCommand, CueKind, DeckState, FreezeView, Grid, HOTCUES, HotcuePoint,
    KeylockRange, LoopEdge, MAX_KEY_ADJUST, MAX_TEMPO, MIN_TEMPO, Material, STEMS, Stems, Track,
    freeze::{SLICE_SIZES, SLICES},
};
use backspn_fx::Rack;
pub use backspn_fx::mixer_fx::{DEFAULT_SLOTS as MIXER_FX_SLOTS, MixerFx};
pub use backspn_fx::{
    CHANNELS as FX_CHANNELS, Command as FxCommand, DrumMachine, EffectInfo, EffectKind,
    Mode as FxMode, PATTERN_KNOBS, RackState, Routing as FxRouting, SLOTS as FX_SLOTS,
    UNITS as FX_UNITS, create as create_effect, pad_fx, pad_host, pattern as fx_pattern,
};
use backspn_mixer::Mixer;
pub use backspn_mixer::stem_master::{
    CompressorSettings, StemLimiterSettings, StemMaster, StemMasterSettings,
};
pub use backspn_mixer::stem_strip::StemControls;
use backspn_mixer::stem_strip::StemStrip;
pub use backspn_mixer::{
    Assign, Band, ChannelState, Command as MixerCommand, DEFAULT_HEADROOM_DB, EqModel, FilterModel,
    LimiterModel, MAX_AUTO_GAIN_DB, MixerState, crossfader_gains, eq_gain, fader_gain,
};
pub use backspn_sampler::{
    Behaviour as SampleBehaviour, CELLS as SAMPLE_CELLS, CellAudio, CellParams, CellState,
    Command as SampleCommand, PAGES as SAMPLE_PAGES, PlayType, PlayerState, QUANTIZE_BEATS,
    SIZE_STEPS as SAMPLE_SIZE_STEPS, SLOTS as SAMPLE_SLOTS, SampleDeck, SampleState, SlotState,
    TriggerType, size_factor as sample_size_factor,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, ErrorKind, SampleFormat, StreamConfig, SupportedBufferSize};
pub mod input;
pub mod loop_recorder;
mod metronome;
mod midi_clock;
mod record;
use backspn_sync::Sync;
use backspn_sync::midi_clock::ClockSender;
use backspn_sync::midi_clock_in::ClockReceiver;
pub use backspn_sync::{
    Command as SyncCommand, DEFAULT_CLOCK_BPM, MAX_CLOCK_BPM, MIN_CLOCK_BPM, SyncState,
};
use metronome::Metronome;
pub use midi_clock::{
    ClockInput, ClockMessage, MidiClockFeed, MidiClockInFeed, MidiClockSink, MidiClockTap,
    midi_clock_input, midi_clock_tap,
};
pub use record::{RecordFeed, RecordTap, record_tap};

pub const DECKS: usize = 4;
pub const DEFAULT_BUFFER_FRAMES: u32 = 256;

const COMMAND_CAPACITY: usize = 1024;
const GARBAGE_CAPACITY: usize = 16;
const MAX_BLOCK_FRAMES: usize = 4096;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no audio output device available")]
    NoDevice,
    #[error("output device {0:?} not found")]
    DeviceNotFound(String),
    #[error("device only offers {0} samples; f32 output is required")]
    UnsupportedFormat(SampleFormat),
    #[error("audio backend error: {0}")]
    Backend(#[from] cpal::Error),
    #[error("the device cannot run at {0} Hz")]
    RateUnavailable(u32),
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub device: Option<String>,
    pub buffer_frames: Option<u32>,
    pub sample_rate: Option<u32>,
    pub monitor: Option<(u16, u16)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecordSource {
    #[default]
    Master,
    Input(Option<(u16, u16)>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Outputs {
    #[default]
    Internal,
    External(ExternalOutputs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExternalOutputs {
    pub decks: [Option<(u16, u16)>; DECKS],
    pub preview: Option<(u16, u16)>,
    pub fx_return: Option<(u16, u16)>,
}

fn moving(from: f64, to: f64) -> bool {
    (to - from).abs() > f64::EPSILON
}

#[derive(Debug)]
pub enum Command {
    Deck(usize, DeckCommand),
    Preview(DeckCommand),
    Mixer(MixerCommand),
    Sync(SyncCommand),
    Fx(FxCommand),
    DeckTick(usize, bool),
    MasterTick(bool),
    RecordTap(Option<RecordTap>),
    Stem(usize, StemCommand),
    LinkTimeline(Option<backspn_link::Timeline>),
    LiveInput(usize, Option<(u16, u16)>),
    Microphone(Option<((u16, u16), f32)>),
    SendInput(Option<(u16, u16)>),
    RecordSource(RecordSource),
    LoopRecorder(loop_recorder::Command),
    Outputs(Outputs),
    LoopRecorderCopy(Box<loop_recorder::LoopCopy>),
    MidiClock(MidiClockCommand),
    SampleDeck(usize, SampleDeckCommand),
}

#[derive(Debug)]
pub enum SampleDeckCommand {
    Install(Box<SampleDeck>),
    Remove,
    Deck(SampleCommand),
}

#[derive(Debug)]
pub enum MidiClockCommand {
    Tap(Option<MidiClockTap>),
    Send(bool),
    Resync,
    OffsetMs(f64),
    Input(Option<MidiClockInFeed>),
}

#[derive(Debug)]
pub enum StemCommand {
    Controls(usize, StemControls),
    Mastering(Option<Box<StemMaster>>),
}

#[derive(Debug)]
pub enum Garbage {
    Material(Material),
    Effects(backspn_fx::Retired),
    RecordTap(RecordTap),
    StemMaster(Box<StemMaster>),
    MidiClockTap(MidiClockTap),
    MidiClockInput(MidiClockInFeed),
    SampleDeck(Box<SampleDeck>),
    LoopCopy(Box<loop_recorder::LoopCopy>),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EngineState {
    pub decks: [DeckState; DECKS],
    pub preview: DeckState,
    pub mixer: MixerState,
    pub sync: SyncState,
    pub fx: RackState,
    pub ticks: [bool; DECKS],
    pub master_tick: bool,
    pub record_dropped: u32,
    pub midi_clock_running: bool,
    pub sample_deck: [Option<SampleState>; DECKS],
    pub loop_recorder: loop_recorder::LoopRecorderState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_frames: Option<u32>,
    pub monitor: Option<(u16, u16)>,
    pub sample_rates: Vec<u32>,
    pub buffer_range: Option<(u32, u32)>,
    pub inputs: u16,
}

impl StreamInfo {
    pub fn buffer_latency(&self) -> Option<Duration> {
        self.buffer_frames
            .map(|f| Duration::from_secs_f64(f64::from(f) / f64::from(self.sample_rate)))
    }
}

#[derive(Debug, Default)]
pub struct Stats {
    callbacks: AtomicU64,
    xruns: AtomicU64,
    overruns: AtomicU64,
    max_callback_ns: AtomicU64,
    output_latency_ns: AtomicU64,
    load_permille: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatsSnapshot {
    pub callbacks: u64,
    pub xruns: u64,
    pub overruns: u64,
    pub max_callback: Duration,
    pub output_latency: Duration,
    pub load_permille: u64,
}

impl Stats {
    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            callbacks: self.callbacks.load(Ordering::Relaxed),
            xruns: self.xruns.load(Ordering::Relaxed),
            overruns: self.overruns.load(Ordering::Relaxed),
            max_callback: Duration::from_nanos(self.max_callback_ns.load(Ordering::Relaxed)),
            output_latency: Duration::from_nanos(self.output_latency_ns.load(Ordering::Relaxed)),
            load_permille: self.load_permille.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug)]
pub struct Renderer {
    decks: [Deck; DECKS],
    preview: Deck,
    preview_buffer: Box<[f32]>,
    mixer: Mixer,
    sync: Sync,
    fx: Rack,
    commands: rtrb::Consumer<Command>,
    garbage: rtrb::Producer<Garbage>,
    state: triple_buffer::Input<EngineState>,
    stats: Arc<Stats>,
    sample_rate: u32,
    deck_buffers: [Box<[f32]>; DECKS],
    master: Box<[f32]>,
    cue: Box<[f32]>,
    send_bus: Box<[f32]>,
    send_dry: Box<[f32]>,
    ticks: [bool; DECKS],
    deck_clicks: [Metronome; DECKS],
    master_tick: bool,
    master_click: Metronome,
    record: Option<RecordTap>,
    record_dropped: u32,
    stem_strips: [StemStrip; DECKS],
    stem_masters: [Option<Box<StemMaster>>; DECKS],
    stem_buffers: [Box<[f32]>; STEMS],
    stem_bypass: Box<[f32]>,
    midi_clock: ClockSender,
    midi_clock_tap: Option<MidiClockTap>,
    midi_clock_in: ClockReceiver,
    midi_clock_input: Option<MidiClockInFeed>,
    sample_deck: [Option<Box<SampleDeck>>; DECKS],
    block_start: Instant,
    epoch: Instant,
    link_timeline: Option<backspn_link::Timeline>,
    output_latency_us: i64,
    input: Option<input::InputFeed>,
    live: [Option<(u16, u16)>; DECKS],
    microphone: Option<((u16, u16), f32)>,
    send_input: Option<(u16, u16)>,
    record_source: RecordSource,
    loop_recorder: loop_recorder::LoopRecorder,
    loop_source: Box<[f32]>,
    outputs: Outputs,
    fx_return: Box<[f32]>,
}

#[derive(Debug)]
pub struct Controller {
    commands: rtrb::Producer<Command>,
    garbage: rtrb::Consumer<Garbage>,
    state: triple_buffer::Output<EngineState>,
    stats: Arc<Stats>,
    loop_copy: Option<Box<loop_recorder::LoopCopy>>,
}

pub fn channel(sample_rate: u32) -> (Renderer, Controller) {
    let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(COMMAND_CAPACITY);
    let (gc_tx, gc_rx) = rtrb::RingBuffer::new(GARBAGE_CAPACITY);
    let (state_in, state_out) = triple_buffer::TripleBuffer::new(&EngineState::default()).split();
    let stats = Arc::new(Stats::default());
    let buffer = || vec![0.0f32; MAX_BLOCK_FRAMES * 2].into_boxed_slice();
    let renderer = Renderer {
        decks: std::array::from_fn(|_| Deck::with_stems(sample_rate)),
        preview: Deck::new(sample_rate),
        preview_buffer: buffer(),
        mixer: Mixer::new(sample_rate),
        sync: Sync::default(),
        fx: Rack::new(sample_rate),
        commands: cmd_rx,
        garbage: gc_tx,
        state: state_in,
        stats: Arc::clone(&stats),
        sample_rate,
        deck_buffers: std::array::from_fn(|_| buffer()),
        master: buffer(),
        cue: buffer(),
        send_bus: buffer(),
        send_dry: buffer(),
        ticks: [false; DECKS],
        deck_clicks: std::array::from_fn(|_| Metronome::new(sample_rate)),
        master_tick: false,
        master_click: Metronome::new(sample_rate),
        record: None,
        record_dropped: 0,
        stem_strips: std::array::from_fn(|_| StemStrip::new(f64::from(sample_rate))),
        stem_masters: std::array::from_fn(|_| None),
        stem_buffers: std::array::from_fn(|_| buffer()),
        stem_bypass: buffer(),
        midi_clock: ClockSender::default(),
        midi_clock_tap: None,
        midi_clock_in: ClockReceiver::default(),
        midi_clock_input: None,
        sample_deck: std::array::from_fn(|_| None),
        block_start: Instant::now(),
        link_timeline: None,
        output_latency_us: 0,
        input: None,
        live: [None; DECKS],
        microphone: None,
        send_input: None,
        record_source: RecordSource::Master,
        loop_recorder: loop_recorder::LoopRecorder::new(sample_rate),
        loop_source: buffer(),
        epoch: Instant::now(),
        outputs: Outputs::Internal,
        fx_return: buffer(),
    };
    let controller = Controller {
        commands: cmd_tx,
        garbage: gc_rx,
        state: state_out,
        stats,
        loop_copy: None,
    };
    (renderer, controller)
}

impl Renderer {
    pub fn render(&mut self, out: &mut [f32], channels: usize, monitor: Option<(usize, usize)>) {
        let started = Instant::now();
        self.drain_commands();
        if channels == 0 {
            return;
        }
        let mut done = 0;
        for block in out.chunks_mut(MAX_BLOCK_FRAMES * channels) {
            let frames = block.len() / channels;
            self.block_start =
                started + Duration::from_secs_f64(done as f64 / f64::from(self.sample_rate));
            done += frames;
            self.render_block(frames);
            if let Outputs::External(pairs) = self.outputs {
                self.write_external(block, channels, &pairs);
                continue;
            }
            let master = &self.master[..2 * frames];
            let cue = &self.cue[..2 * frames];
            for (i, frame) in block.chunks_exact_mut(channels).enumerate() {
                frame.fill(0.0);
                let (l, r) = (master[2 * i], master[2 * i + 1]);
                if channels == 1 {
                    frame[0] = l.midpoint(r);
                    continue;
                }
                frame[0] = l;
                frame[1] = r;
                if let Some((ml, mr)) = monitor
                    && ml < channels
                    && mr < channels
                {
                    frame[ml] = cue[2 * i];
                    frame[mr] = cue[2 * i + 1];
                }
            }
        }
        self.publish();
        self.record_timing(started.elapsed(), out.len() / channels);
    }

    fn write_external(&self, block: &mut [f32], channels: usize, pairs: &ExternalOutputs) {
        let frames = block.len() / channels;
        let sources = pairs
            .decks
            .iter()
            .zip(&self.deck_buffers)
            .map(|(p, b)| (*p, &b[..2 * frames]))
            .chain([
                (pairs.preview, &self.preview_buffer[..2 * frames]),
                (pairs.fx_return, &self.fx_return[..2 * frames]),
            ]);
        block.fill(0.0);
        for (pair, audio) in sources {
            let Some((l, r)) = pair.map(|(l, r)| (usize::from(l), usize::from(r))) else {
                continue;
            };
            if l >= channels || r >= channels {
                continue;
            }
            for (frame, x) in block
                .chunks_exact_mut(channels)
                .zip(audio.as_chunks::<2>().0)
            {
                frame[l] += x[0];
                frame[r] += x[1];
            }
        }
        for x in block.iter_mut() {
            *x = x.clamp(-1.0, 1.0);
        }
    }

    fn drain_commands(&mut self) {
        while let Ok(command) = self.commands.pop() {
            match command {
                Command::Deck(i, command) => {
                    if let Some(deck) = self.decks.get_mut(i)
                        && let Some(old) = deck.handle(command)
                    {
                        drop(self.garbage.push(Garbage::Material(old)));
                    }
                }
                Command::Preview(command) => {
                    if let Some(old) = self.preview.handle(command) {
                        drop(self.garbage.push(Garbage::Material(old)));
                    }
                }
                Command::Mixer(command) => self.mixer.handle(command),
                Command::Sync(command) => self.sync.handle(command),
                Command::Fx(command) => {
                    if let Some(old) = self.fx.handle(command) {
                        drop(self.garbage.push(Garbage::Effects(old)));
                    }
                }
                Command::DeckTick(i, on) => {
                    if let Some(tick) = self.ticks.get_mut(i) {
                        *tick = on;
                    }
                }
                Command::MasterTick(on) => self.master_tick = on,
                Command::LinkTimeline(tl) => self.link_timeline = tl,
                Command::LiveInput(ch, routing) => {
                    if let Some(live) = self.live.get_mut(ch) {
                        *live = routing;
                    }
                }
                Command::Microphone(mic) => self.microphone = mic,
                Command::SendInput(send) => self.send_input = send,
                Command::RecordSource(source) => self.record_source = source,
                Command::Outputs(outputs) => self.outputs = outputs,
                Command::LoopRecorder(c) => {
                    let s = self.sync.state();
                    self.loop_recorder.handle(
                        c,
                        loop_recorder::Clock {
                            bpm: s.clock_bpm,
                            beat: s.clock_beat,
                        },
                    );
                }
                Command::LoopRecorderCopy(mut copy) => {
                    self.loop_recorder.copy_into(&mut copy);
                    drop(self.garbage.push(Garbage::LoopCopy(copy)));
                }
                Command::Stem(i, command) => self.stem_command(i, command),
                Command::RecordTap(tap) => {
                    self.record_dropped = 0;
                    if let Some(old) = std::mem::replace(&mut self.record, tap) {
                        drop(self.garbage.push(Garbage::RecordTap(old)));
                    }
                }
                Command::MidiClock(command) => self.midi_clock_command(command),
                Command::SampleDeck(i, command) => self.sample_command(i, command),
            }
        }
    }

    fn midi_clock_command(&mut self, command: MidiClockCommand) {
        let (tap, at) = (&mut self.midi_clock_tap, self.block_start);
        let rate = self.sample_rate;
        let emit = |offset: f64, byte: u8| send_clock(tap, at, rate, offset, byte);
        match command {
            MidiClockCommand::Tap(new) => {
                if let Some(old) = std::mem::replace(&mut self.midi_clock_tap, new) {
                    drop(self.garbage.push(Garbage::MidiClockTap(old)));
                }
            }
            MidiClockCommand::Send(on) => self.midi_clock.set_send(on, emit),
            MidiClockCommand::Resync => self.midi_clock.resync(emit),
            MidiClockCommand::OffsetMs(ms) => self.midi_clock.set_offset_ms(ms),
            MidiClockCommand::Input(new) => {
                if let Some(old) = std::mem::replace(&mut self.midi_clock_input, new) {
                    drop(self.garbage.push(Garbage::MidiClockInput(old)));
                }
            }
        }
    }

    fn micros(&self, at: Instant) -> i64 {
        i64::try_from(at.saturating_duration_since(self.epoch).as_micros()).unwrap_or(i64::MAX)
    }

    fn follow_link(&mut self, frames: usize) {
        let Some(tl) = self.link_timeline else {
            return;
        };
        let block_us = (frames as f64 * 1e6 / f64::from(self.sample_rate)) as i64;
        let t = backspn_link::micros_at(self.block_start) + block_us + self.output_latency_us;
        self.sync.follow_link(backspn_link::phase_at(&tl, t, 1.0));
    }

    pub fn set_input(&mut self, feed: Option<input::InputFeed>) {
        self.input = feed;
    }

    pub fn set_output_latency(&mut self, latency: Duration) {
        self.output_latency_us = i64::try_from(latency.as_micros()).unwrap_or(0);
    }

    fn receive_midi_clock(&mut self) {
        while let Some(m) = self
            .midi_clock_input
            .as_mut()
            .and_then(MidiClockInFeed::pop)
        {
            let at = self.micros(m.at);
            self.midi_clock_in.handle(m.status, m.spp, at);
        }
        let now = self.micros(self.block_start);
        let beat = self.midi_clock_in.beat_at(now);
        self.sync.follow_external(self.midi_clock_in.tempo(), beat);
    }

    fn send_midi_clock(&mut self, beat: f64, bpm: f64, frames: usize) {
        let (tap, at) = (&mut self.midi_clock_tap, self.block_start);
        let rate = self.sample_rate;
        self.midi_clock
            .block(bpm, beat, frames, rate, |offset, byte| {
                send_clock(tap, at, rate, offset, byte);
            });
    }

    fn sample_command(&mut self, ch: usize, command: SampleDeckCommand) {
        if ch >= DECKS {
            return;
        }
        match command {
            SampleDeckCommand::Install(deck) => {
                if let Some(old) = self.sample_deck[ch].replace(deck) {
                    drop(self.garbage.push(Garbage::SampleDeck(old)));
                }
            }
            SampleDeckCommand::Remove => {
                if let Some(old) = self.sample_deck[ch].take() {
                    drop(self.garbage.push(Garbage::SampleDeck(old)));
                }
            }
            SampleDeckCommand::Deck(command) => {
                if let Some(deck) = &mut self.sample_deck[ch]
                    && let Some(old) = deck.handle(command)
                {
                    drop(self.garbage.push(Garbage::Material(old)));
                }
            }
        }
    }

    fn render_sample_deck(&mut self, ch: usize, len: usize) {
        let [a, b, c, d] = &mut self.stem_buffers;
        let mut slots: [&mut [f32]; STEMS] =
            [&mut a[..len], &mut b[..len], &mut c[..len], &mut d[..len]];
        if let Some(deck) = &mut self.sample_deck[ch] {
            deck.process(&mut slots);
        }
        let filter = self.mixer.filter_settings();
        self.stem_strips[ch].process(
            &mut slots,
            &mut self.deck_buffers[ch][..len],
            &mut self.stem_bypass[..len],
            filter,
        );
    }

    fn render_sample_channel(
        &mut self,
        ch: usize,
        len: usize,
        clock: &SyncState,
        context: &mut backspn_fx::Context,
    ) {
        let bend = self.sync.bend(ch);
        let Some(sample_deck) = &mut self.sample_deck[ch] else {
            return;
        };
        if clock.master == Some(ch) {
            sample_deck.set_bend(bend);
        } else {
            sample_deck.follow_clock(clock.synced[ch], clock.clock_bpm, clock.clock_beat, bend);
        }
        let beat = sample_deck.beat_position();
        self.render_sample_deck(ch, len);
        if let Some(sample_deck) = &self.sample_deck[ch]
            && moving(beat, sample_deck.beat_position())
        {
            *context = backspn_fx::Context {
                bpm: sample_deck.effective_bpm(),
                beat,
                beat_end: sample_deck.beat_position(),
            };
        }
        self.fx
            .process(ch, &mut self.deck_buffers[ch][..len], context);
        self.finish_stems(ch, len);
    }

    fn stem_command(&mut self, deck: usize, command: StemCommand) {
        if deck >= DECKS {
            return;
        }
        match command {
            StemCommand::Controls(stem, controls) => self.stem_strips[deck].set(stem, controls),
            StemCommand::Mastering(master) => {
                if let Some(old) = std::mem::replace(&mut self.stem_masters[deck], master) {
                    drop(self.garbage.push(Garbage::StemMaster(old)));
                }
            }
        }
    }

    fn render_stems(&mut self, ch: usize, len: usize) {
        let [a, b, c, d] = &mut self.stem_buffers;
        let mut stems: [&mut [f32]; STEMS] =
            [&mut a[..len], &mut b[..len], &mut c[..len], &mut d[..len]];
        self.decks[ch].process_layers(&mut stems);
        let filter = self.mixer.filter_settings();
        self.stem_strips[ch].process(
            &mut stems,
            &mut self.deck_buffers[ch][..len],
            &mut self.stem_bypass[..len],
            filter,
        );
    }

    fn finish_stems(&mut self, ch: usize, len: usize) {
        let out = &mut self.deck_buffers[ch][..len];
        for (x, y) in out.iter_mut().zip(&self.stem_bypass[..len]) {
            *x += y;
        }
        if let Some(master) = &mut self.stem_masters[ch] {
            let (frames, _) = out.as_chunks_mut::<2>();
            for frame in frames {
                *frame = master.tick(*frame);
            }
        }
    }

    fn update_sync(&mut self, frames: usize) {
        let mixer = self.mixer.state();
        let on_air: [bool; DECKS] =
            std::array::from_fn(|i| on_air(&mixer.channels[i], mixer.crossfader));
        self.receive_midi_clock();
        self.follow_link(frames);
        let beat = self.sync.state().clock_beat;
        let leads: [Option<backspn_sync::Lead>; DECKS] = std::array::from_fn(|i| {
            self.sample_deck[i].as_ref().map(|r| backspn_sync::Lead {
                loaded: r.has_audio(),
                playing: r.is_playing(),
                bpm: r.effective_bpm(),
                beat: r.beat_position(),
            })
        });
        self.sync
            .update_with(&mut self.decks, &leads, &on_air, frames, self.sample_rate);
        self.send_midi_clock(beat, self.sync.state().clock_bpm, frames);
    }

    #[allow(clippy::too_many_lines)]
    fn render_block(&mut self, frames: usize) {
        let clock_from = self.sync.state().clock_beat;
        let beats_from: [Option<f64>; DECKS] =
            std::array::from_fn(|i| self.decks[i].beat_position());
        self.update_sync(frames);
        let clock = self.sync.state();
        let clock_ctx = backspn_fx::Context {
            bpm: clock.clock_bpm,
            beat: clock_from,
            beat_end: clock.clock_beat,
        };
        let mut contexts = [clock_ctx; DECKS];
        let len = 2 * frames;
        if let Some(feed) = &mut self.input {
            feed.pull(frames);
        }
        for ch in 0..DECKS {
            if let Some(routing) = self.live[ch] {
                let buf = &mut self.deck_buffers[ch][..len];
                match &self.input {
                    Some(feed) => feed.stereo(routing, buf),
                    None => buf.fill(0.0),
                }
                self.fx.process(ch, buf, &contexts[ch]);
                continue;
            }
            if self.sample_deck[ch].is_some() {
                self.render_sample_channel(ch, len, &clock, &mut contexts[ch]);
                continue;
            }
            let bpm = self.decks[ch].state().effective_bpm();
            let from = self.decks[ch].beat_position();
            let stems = self.decks[ch].has_stems();
            if stems {
                self.render_stems(ch, len);
            } else {
                self.decks[ch].process(&mut self.deck_buffers[ch][..len]);
            }
            if let (Some(bpm), Some(beat), Some(beat_end)) =
                (bpm, from, self.decks[ch].beat_position())
                && moving(beat, beat_end)
            {
                contexts[ch] = backspn_fx::Context {
                    bpm,
                    beat,
                    beat_end,
                };
            }
            self.fx
                .process(ch, &mut self.deck_buffers[ch][..len], &contexts[ch]);
            if stems {
                self.finish_stems(ch, len);
            }
        }
        let [a, b, c, d] = &mut self.deck_buffers;
        let mut inputs: [&mut [f32]; DECKS] = [
            &mut a[..2 * frames],
            &mut b[..2 * frames],
            &mut c[..2 * frames],
            &mut d[..2 * frames],
        ];
        let len = 2 * frames;
        self.mixer.pre_channels(&mut inputs);
        for (ch, input) in inputs.iter_mut().enumerate() {
            self.fx.process_mixer_fx(ch, input, &contexts[ch]);
        }
        self.mixer.post_channels(&mut inputs, &mut self.cue[..len]);
        let external = matches!(self.outputs, Outputs::External(_));
        for (i, deck) in self.decks.iter().enumerate() {
            if self.ticks[i] && !external {
                self.deck_clicks[i].render(
                    beats_from[i],
                    deck.beat_position(),
                    &mut self.cue[..len],
                );
            }
        }
        if self.master_tick && !external {
            self.master_click.render(
                Some(clock_from),
                Some(clock.clock_beat),
                &mut self.cue[..len],
            );
        }
        for (ch, input) in inputs.iter_mut().enumerate() {
            self.fx.process_post_fader(ch, input, &contexts[ch]);
        }
        let master = &mut self.master[..len];
        Mixer::sum(&inputs, master);
        if external {
            let fx_return = &mut self.fx_return[..len];
            match (&self.input, self.send_input) {
                (Some(feed), Some((left, right))) => {
                    for (i, frame) in fx_return.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                        frame[0] = feed.sample(i, left);
                        frame[1] = feed.sample(i, right);
                    }
                }
                _ => fx_return.fill(0.0),
            }
            for unit in 0..FX_UNITS {
                if self.fx.routing(unit) == FxRouting::Send {
                    self.fx.process_send(unit, fx_return, &clock_ctx);
                }
            }
        }
        for unit in 0..FX_UNITS {
            if external || self.fx.routing(unit) != FxRouting::Send {
                continue;
            }
            let bus = &mut self.send_bus[..len];
            bus.fill(0.0);
            for (ch, input) in inputs.iter().enumerate() {
                if self.fx.assigned(ch, unit) {
                    for (b, x) in bus.iter_mut().zip(input.iter()) {
                        *b += x;
                    }
                }
            }
            self.send_dry[..len].copy_from_slice(bus);
            self.fx.process_send(unit, bus, &clock_ctx);
            for ((m, wet), dry) in master.iter_mut().zip(bus.iter()).zip(&self.send_dry[..len]) {
                *m += wet - dry;
            }
        }
        let aux = &mut self.loop_source[..len];
        aux.fill(0.0);
        if let (Some(feed), Some(((l, r), gain))) = (&self.input, self.microphone) {
            for (i, frame) in aux.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                frame[0] = feed.sample(i, l) * gain;
                frame[1] = feed.sample(i, r) * gain;
            }
            if !external {
                for (m, x) in master.iter_mut().zip(aux.iter()) {
                    *m += x;
                }
            }
        }
        let preview = &mut self.preview_buffer[..len];
        self.preview.process(preview);
        for (c, p) in self.cue[..len].iter_mut().zip(preview.iter()) {
            *c += p;
        }
        let source = &mut self.loop_source[..len];
        match self.loop_recorder.state.source {
            loop_recorder::Source::Main => source.copy_from_slice(master),
            loop_recorder::Source::Cue => source.copy_from_slice(&self.cue[..len]),
            loop_recorder::Source::Ext => match (&self.input, self.send_input) {
                (Some(feed), Some((left, right))) => {
                    for (i, frame) in source.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                        frame[0] = feed.sample(i, left);
                        frame[1] = feed.sample(i, right);
                    }
                }
                _ => source.fill(0.0),
            },
            loop_recorder::Source::Aux => {}
        }
        self.loop_recorder.process(
            source,
            if external {
                &mut self.fx_return[..len]
            } else {
                master
            },
            loop_recorder::Clock {
                bpm: clock.clock_bpm,
                beat: clock_from,
            },
            external,
        );
        if external {
            self.mixer.finish_external(&mut inputs);
            master.fill(0.0);
        } else {
            self.mixer.finish(master, &mut self.cue[..len]);
        }
        let recorded: &[f32] = match self.record_source {
            RecordSource::Master => master,
            RecordSource::Input(pair) => {
                let buf = &mut self.send_dry[..len];
                match (&self.input, pair) {
                    (Some(feed), Some(pair)) => feed.stereo(pair, buf),
                    _ => buf.fill(0.0),
                }
                buf
            }
        };
        if let Some(tap) = &mut self.record
            && !tap.push(recorded)
        {
            self.record_dropped = self.record_dropped.saturating_add(1);
        }
        if external {
            self.external_clicks(&beats_from, clock_from, clock.clock_beat, len);
        }
    }

    fn external_clicks(
        &mut self,
        beats_from: &[Option<f64>; DECKS],
        clock_from: f64,
        clock_to: f64,
        len: usize,
    ) {
        for (i, deck) in self.decks.iter().enumerate() {
            if self.ticks[i] {
                self.deck_clicks[i].render(
                    beats_from[i],
                    deck.beat_position(),
                    &mut self.deck_buffers[i][..len],
                );
            }
        }
        if !self.master_tick {
            return;
        }
        let click = &mut self.send_dry[..len];
        click.fill(0.0);
        self.master_click
            .render(Some(clock_from), Some(clock_to), click);
        let cues = self.mixer.state().channels.map(|c| c.cue);
        for (buffer, _) in self
            .deck_buffers
            .iter_mut()
            .zip(cues)
            .filter(|(_, cue)| *cue)
        {
            for (x, c) in buffer[..len].iter_mut().zip(click.iter()) {
                *x += c;
            }
        }
    }

    fn publish(&mut self) {
        let state = EngineState {
            decks: std::array::from_fn(|i| self.decks[i].state()),
            preview: self.preview.state(),
            mixer: self.mixer.state(),
            sync: self.sync.state(),
            fx: self.fx.state(),
            ticks: self.ticks,
            master_tick: self.master_tick,
            record_dropped: self.record_dropped,
            midi_clock_running: self.midi_clock.running(),
            sample_deck: std::array::from_fn(|i| self.sample_deck[i].as_ref().map(|d| d.state())),
            loop_recorder: self.loop_recorder.state,
        };
        self.state.write(state);
    }

    fn record_timing(&self, elapsed: Duration, frames: usize) {
        let stats = &self.stats;
        stats.callbacks.fetch_add(1, Ordering::Relaxed);
        let nanos = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        stats.max_callback_ns.fetch_max(nanos, Ordering::Relaxed);
        let budget_ns = frames as f64 * 1e9 / f64::from(self.sample_rate);
        if budget_ns > 0.0 {
            let load = (nanos as f64 / budget_ns * 1000.0) as u64;
            stats.load_permille.store(load, Ordering::Relaxed);
            if load > 1000 {
                stats.overruns.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

impl Controller {
    #[allow(clippy::result_large_err)]
    pub fn send(&mut self, command: Command) -> Result<(), Command> {
        self.commands.push(command).map_err(|e| match e {
            rtrb::PushError::Full(command) => command,
        })
    }

    pub fn state(&mut self) -> EngineState {
        *self.state.read()
    }

    pub fn stats(&self) -> StatsSnapshot {
        self.stats.snapshot()
    }

    pub fn collect_garbage(&mut self) {
        while let Ok(g) = self.garbage.pop() {
            if let Garbage::LoopCopy(copy) = g {
                self.loop_copy = Some(copy);
            }
        }
    }

    pub fn take_loop_copy(&mut self) -> Option<Box<loop_recorder::LoopCopy>> {
        self.loop_copy.take()
    }
}

pub struct Engine {
    #[allow(dead_code)]
    stream: Option<cpal::Stream>,
    renderer: Option<Arc<Mutex<Renderer>>>,
    controller: Controller,
    info: StreamInfo,
    lost: Arc<AtomicBool>,
    #[allow(dead_code)]
    input: Option<cpal::Stream>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

struct Opened {
    device: cpal::Device,
    config: StreamConfig,
    info: StreamInfo,
}

fn open_device(config: &Config, rate: Option<u32>) -> Result<Opened, Error> {
    let host = cpal::default_host();
    let device = match &config.device {
        None => host.default_output_device().ok_or(Error::NoDevice)?,
        Some(name) => host
            .output_devices()?
            .find(|d| d.description().is_ok_and(|desc| desc.name() == name))
            .ok_or_else(|| Error::DeviceNotFound(name.clone()))?,
    };
    let device_name = device
        .description()
        .map_or_else(|_| "unknown".to_owned(), |d| d.name().to_owned());
    let current = device.default_output_config()?;
    if current.sample_format() != SampleFormat::F32 {
        return Err(Error::UnsupportedFormat(current.sample_format()));
    }
    let ranges: Vec<_> = device
        .supported_output_configs()
        .map(|c| {
            c.filter(|c| {
                c.sample_format() == SampleFormat::F32 && c.channels() == current.channels()
            })
            .collect()
        })
        .unwrap_or_default();
    let sample_rates = offered_rates(
        &ranges
            .iter()
            .map(|c| (c.min_sample_rate(), c.max_sample_rate()))
            .collect::<Vec<_>>(),
    );
    let supported = rate
        .or(config.sample_rate)
        .filter(|r| *r != current.sample_rate())
        .and_then(|r| ranges.into_iter().find_map(|c| c.try_with_sample_rate(r)))
        .unwrap_or(current);
    let requested = config.buffer_frames.unwrap_or(DEFAULT_BUFFER_FRAMES);
    let buffer_range = match supported.buffer_size() {
        SupportedBufferSize::Range { min, max } => Some((*min, *max)),
        SupportedBufferSize::Unknown => None,
    };
    let buffer_size = buffer_range.map_or(BufferSize::Default, |(min, max)| {
        BufferSize::Fixed(requested.clamp(min, max))
    });
    let stream_config = StreamConfig {
        buffer_size,
        ..supported.config()
    };
    let info = StreamInfo {
        device: device_name,
        sample_rate: stream_config.sample_rate,
        channels: stream_config.channels,
        buffer_frames: match buffer_size {
            BufferSize::Fixed(n) => Some(n),
            BufferSize::Default => None,
        },
        monitor: monitor_channels(config.monitor, stream_config.channels),
        sample_rates,
        buffer_range,
        inputs: 0,
    };
    Ok(Opened {
        device,
        config: stream_config,
        info,
    })
}

fn play(
    opened: &mut Opened,
    renderer: &Arc<Mutex<Renderer>>,
    stats: &Arc<Stats>,
    lost: &Arc<AtomicBool>,
) -> Result<cpal::Stream, Error> {
    let (stats, error_stats) = (Arc::clone(stats), Arc::clone(stats));
    let (renderer, lost) = (Arc::clone(renderer), Arc::clone(lost));
    let channel_count = usize::from(opened.info.channels);
    let monitor_channels = opened
        .info
        .monitor
        .map(|(l, r)| (usize::from(l), usize::from(r)));
    let stream = opened.device.build_output_stream::<f32, _, _>(
        opened.config,
        move |data, info| {
            let ts = info.timestamp();
            if let Some(latency) = ts.playback.checked_duration_since(ts.callback) {
                let nanos = u64::try_from(latency.as_nanos()).unwrap_or(u64::MAX);
                stats.output_latency_ns.store(nanos, Ordering::Relaxed);
            }
            let latency = ts.playback.checked_duration_since(ts.callback);
            match renderer.try_lock() {
                Ok(mut r) => {
                    if let Some(latency) = latency {
                        r.set_output_latency(latency);
                    }
                    r.render(data, channel_count, monitor_channels);
                }
                Err(_) => data.fill(0.0),
            }
        },
        move |err| match err.kind() {
            ErrorKind::Xrun => {
                error_stats.xruns.fetch_add(1, Ordering::Relaxed);
            }
            ErrorKind::DeviceNotAvailable => {
                log::warn!("audio device lost: {err}");
                lost.store(true, Ordering::Relaxed);
            }
            _ => log::error!("audio stream error: {err}"),
        },
        None,
    )?;
    stream.play()?;
    if let Ok(n) = stream.buffer_size() {
        opened.info.buffer_frames = Some(n);
    }
    Ok(stream)
}

fn open_input(info: &mut StreamInfo, renderer: &Arc<Mutex<Renderer>>) -> Option<cpal::Stream> {
    let (stream, feed, channels) = input::open(&info.device, info.sample_rate, MAX_BLOCK_FRAMES)?;
    info.inputs = channels;
    if let Ok(mut r) = renderer.lock() {
        r.set_input(Some(feed));
    }
    Some(stream)
}

impl Engine {
    pub fn start(config: &Config) -> Result<Self, Error> {
        let mut opened = open_device(config, None)?;
        let (renderer, controller) = channel(opened.info.sample_rate);
        let renderer = Arc::new(Mutex::new(renderer));
        let lost = Arc::new(AtomicBool::new(false));
        let stream = play(&mut opened, &renderer, &controller.stats, &lost)?;
        let input = open_input(&mut opened.info, &renderer);
        log::info!("audio started: {:?}", opened.info);
        Ok(Self {
            stream: Some(stream),
            renderer: Some(renderer),
            controller,
            info: opened.info,
            lost,
            input,
        })
    }

    pub fn lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    pub fn reopen(&mut self, config: &Config) -> Result<(), Error> {
        let Some(renderer) = self.renderer.clone() else {
            return Ok(());
        };
        self.stream = None;
        let rate = self.info.sample_rate;
        let mut opened = open_device(config, Some(rate))?;
        if opened.info.sample_rate != rate {
            return Err(Error::RateUnavailable(rate));
        }
        self.lost.store(false, Ordering::Relaxed);
        self.input = None;
        let stream = play(&mut opened, &renderer, &self.controller.stats, &self.lost)?;
        self.input = open_input(&mut opened.info, &renderer);
        log::info!("audio reopened: {:?}", opened.info);
        self.stream = Some(stream);
        self.info = opened.info;
        Ok(())
    }

    pub fn offline(sample_rate: u32) -> (Self, Renderer) {
        let (renderer, controller) = channel(sample_rate);
        let info = StreamInfo {
            device: "offline".to_owned(),
            sample_rate,
            channels: 2,
            buffer_frames: None,
            monitor: None,
            sample_rates: vec![sample_rate],
            buffer_range: None,
            inputs: 0,
        };
        (
            Self {
                stream: None,
                renderer: None,
                controller,
                info,
                lost: Arc::new(AtomicBool::new(false)),
                input: None,
            },
            renderer,
        )
    }

    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    pub fn controller(&mut self) -> &mut Controller {
        &mut self.controller
    }
}

fn monitor_channels(requested: Option<(u16, u16)>, channels: u16) -> Option<(u16, u16)> {
    match requested {
        Some((l, r)) => (l < channels && r < channels).then_some((l, r)),
        None => (channels >= 4).then_some((2, 3)),
    }
}

fn send_clock(tap: &mut Option<MidiClockTap>, start: Instant, rate: u32, offset: f64, byte: u8) {
    if let Some(tap) = tap {
        let at = start + Duration::from_secs_f64(offset.max(0.0) / f64::from(rate));
        tap.push(ClockMessage { at, byte });
    }
}

fn on_air(channel: &ChannelState, crossfader: f32) -> bool {
    channel.fader != 0.0
        && match channel.assign {
            Assign::Thru => true,
            Assign::A => crossfader < 1.0,
            Assign::B => crossfader > 0.0,
        }
}

const COMMON_RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

fn offered_rates(ranges: &[(u32, u32)]) -> Vec<u32> {
    let mut rates: Vec<u32> = ranges
        .iter()
        .flat_map(|&(min, max)| {
            let common = COMMON_RATES
                .iter()
                .copied()
                .filter(move |r| (min..=max).contains(r));
            (min == max).then_some(min).into_iter().chain(common)
        })
        .collect();
    rates.sort_unstable();
    rates.dedup();
    rates
}

impl Error {
    pub fn device_missing(&self) -> bool {
        matches!(self, Self::NoDevice | Self::DeviceNotFound(_))
    }
}

pub fn device_present(wanted: Option<&str>, available: &[String]) -> bool {
    match wanted {
        Some(name) => available.iter().any(|d| d == name),
        None => !available.is_empty(),
    }
}

pub fn output_devices() -> Vec<String> {
    cpal::default_host()
        .output_devices()
        .map(|devices| {
            devices
                .filter_map(|d| d.description().ok().map(|desc| desc.name().to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    #[test]
    fn a_device_is_present_by_name_or_any_for_the_default() {
        let devices = vec!["Speakers".to_owned(), "S4".to_owned()];
        assert!(device_present(Some("S4"), &devices));
        assert!(!device_present(Some("X1"), &devices));
        assert!(device_present(None, &devices));
        assert!(!device_present(None, &[]));
        assert!(Error::DeviceNotFound("S4".into()).device_missing());
        assert!(!Error::RateUnavailable(44_100).device_missing());
    }

    fn tone_track() -> Arc<Track> {
        Arc::new(Track::from_stereo(
            RATE,
            [0.5, -0.5].repeat(RATE as usize * 4),
        ))
    }

    fn playing(controller: &mut Controller, deck: usize) {
        controller
            .send(Command::Deck(deck, DeckCommand::Load(tone_track())))
            .unwrap();
        controller
            .send(Command::Deck(deck, DeckCommand::Play))
            .unwrap();
        controller
            .send(Command::Mixer(MixerCommand::MasterVolume(1.0)))
            .unwrap();
        controller
            .send(Command::Mixer(MixerCommand::Limiter(false)))
            .unwrap();
        controller
            .send(Command::Mixer(MixerCommand::Headroom(0.0)))
            .unwrap();
    }

    fn stems() -> Arc<Stems> {
        Arc::new(Stems(std::array::from_fn(|k| {
            Track::from_stereo(RATE, [0.1 * (k + 1) as f32; 2].repeat(RATE as usize * 4))
        })))
    }

    fn stem_deck(controller: &mut Controller) {
        controller
            .send(Command::Deck(0, DeckCommand::LoadStems(stems())))
            .unwrap();
        controller
            .send(Command::Deck(0, DeckCommand::Play))
            .unwrap();
        for c in [
            MixerCommand::MasterVolume(1.0),
            MixerCommand::Limiter(false),
            MixerCommand::Headroom(0.0),
        ] {
            controller.send(Command::Mixer(c)).unwrap();
        }
    }

    #[test]
    fn a_stem_deck_plays_the_stems_through_their_controls() {
        let (mut renderer, mut controller) = channel(RATE);
        stem_deck(&mut controller);
        let mut out = vec![0.0; 4_800 * 2];
        renderer.render(&mut out, 2, None);
        assert!(controller.state().decks[0].stems);
        let last = out[out.len() - 2];
        assert!((last - 1.0).abs() < 0.01, "all four: {last}");
        let mute = StemControls {
            mute: true,
            ..StemControls::default()
        };
        let half = StemControls {
            volume: 0.5,
            ..StemControls::default()
        };
        for (stem, c) in [(3, mute), (1, half)] {
            controller
                .send(Command::Stem(0, StemCommand::Controls(stem, c)))
                .unwrap();
        }
        renderer.render(&mut out, 2, None);
        let last = out[out.len() - 2];
        assert!((last - (0.1 + 0.1 + 0.3)).abs() < 0.01, "{last}");
    }

    #[test]
    fn the_stems_mastering_runs_after_them() {
        let (mut renderer, mut controller) = channel(RATE);
        stem_deck(&mut controller);
        let mut settings = StemMasterSettings::default();
        settings.compressor.enabled = true;
        settings.compressor.input_gain_db = 6.0;
        let master = Box::new(StemMaster::new(&settings, f64::from(RATE)));
        controller
            .send(Command::Stem(0, StemCommand::Mastering(Some(master))))
            .unwrap();
        let mute = StemControls {
            mute: true,
            ..StemControls::default()
        };
        for stem in 1..STEMS {
            controller
                .send(Command::Stem(0, StemCommand::Controls(stem, mute)))
                .unwrap();
        }
        let mut out = vec![0.0; 4_800 * 2];
        renderer.render(&mut out, 2, None);
        let want = 0.1 * (0.5 + 0.5 * 10f32.powf(6.0 / 20.0) * 10f32.powf(0.117_27));
        let last = out[out.len() - 2];
        assert!((last - want).abs() < 0.01, "{last} vs {want}");
        controller
            .send(Command::Stem(0, StemCommand::Mastering(None)))
            .unwrap();
        renderer.render(&mut out, 2, None);
        assert!(matches!(
            controller.garbage.pop(),
            Ok(Garbage::StemMaster(_))
        ));
    }

    #[test]
    fn renders_decks_through_mixer_and_publishes_state() {
        let (mut renderer, mut controller) = channel(RATE);
        playing(&mut controller, 1);
        let mut out = vec![0.0; 4_800 * 2];
        renderer.render(&mut out, 2, None);
        let state = controller.state();
        assert!(state.decks[1].loaded && state.decks[1].playing);
        assert!(!state.decks[0].loaded);
        assert!((state.decks[1].position_s - 0.1).abs() < 1e-3);
        assert!(
            (out[out.len() - 2] - 0.5).abs() < 0.01,
            "{}",
            out[out.len() - 2]
        );
        assert!(state.mixer.channel_peaks[1] > 0.4);
        assert_eq!(controller.stats().callbacks, 1);
    }

    #[test]
    fn a_live_input_deck_plays_its_input_through_the_channel() {
        let (mut renderer, mut controller) = channel(RATE);
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(65_536);
        for _ in 0..4_800 {
            for v in [0.0, 0.0, 0.25, 0.25] {
                producer.push(v).unwrap();
            }
        }
        renderer.set_input(Some(input::InputFeed::new(consumer, 4, MAX_BLOCK_FRAMES)));
        controller
            .send(Command::LiveInput(2, Some((2, 3))))
            .unwrap();
        let mut out = vec![0.0; 1_024 * 2];
        renderer.render(&mut out, 2, None);
        let state = controller.state();
        assert!(
            state.mixer.channel_peaks[2] > 0.2,
            "{:?}",
            state.mixer.channel_peaks
        );
        assert!(out[out.len() - 2].abs() > 0.1);
        controller.send(Command::LiveInput(2, None)).unwrap();
        renderer.render(&mut out, 2, None);
        renderer.render(&mut out, 2, None);
        assert!(out[out.len() - 2].abs() < 1e-6);
    }

    #[test]
    fn the_mix_recorder_can_record_an_input_instead_of_the_master() {
        let (mut renderer, mut controller) = channel(RATE);
        playing(&mut controller, 0);
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(65_536);
        for _ in 0..4_800 {
            for v in [0.0, 0.0, 0.25, 0.25] {
                producer.push(v).unwrap();
            }
        }
        renderer.set_input(Some(input::InputFeed::new(consumer, 4, MAX_BLOCK_FRAMES)));
        let (tap, mut feed) = record_tap(RATE, 1.0);
        controller.send(Command::RecordTap(Some(tap))).unwrap();
        controller
            .send(Command::RecordSource(RecordSource::Input(Some((2, 3)))))
            .unwrap();
        let mut out = vec![0.0; 2 * 512];
        renderer.render(&mut out, 2, None);
        let mut recorded = Vec::new();
        feed.drain(&mut recorded);
        assert!(
            recorded.iter().all(|x| (x - 0.25).abs() < 1e-6),
            "the input, not deck A"
        );
        controller
            .send(Command::RecordSource(RecordSource::Input(None)))
            .unwrap();
        renderer.render(&mut out, 2, None);
        recorded.clear();
        feed.drain(&mut recorded);
        assert!(!recorded.is_empty() && recorded.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn routes_cue_to_monitor_channels() {
        let (mut renderer, mut controller) = channel(RATE);
        playing(&mut controller, 0);
        controller
            .send(Command::Mixer(MixerCommand::Fader(0, 0.0)))
            .unwrap();
        controller
            .send(Command::Mixer(MixerCommand::Cue(0, true)))
            .unwrap();
        controller
            .send(Command::Mixer(MixerCommand::CueVolume(1.0)))
            .unwrap();
        let mut out = vec![9.0; 4 * 4_800];
        renderer.render(&mut out, 4, Some((2, 3)));
        let last = &out[out.len() - 4..];
        assert!(
            last[0].abs() < 1e-3 && last[1].abs() < 1e-3,
            "master silent: {last:?}"
        );
        assert!(
            (last[2] - 0.5).abs() < 0.01 && (last[3] + 0.5).abs() < 0.01,
            "cue: {last:?}"
        );
    }

    #[test]
    fn an_external_mixer_gets_each_deck_and_the_preview_on_their_pairs() {
        let (mut renderer, mut controller) = channel(RATE);
        playing(&mut controller, 0);
        controller
            .send(Command::Preview(DeckCommand::Load(tone_track())))
            .unwrap();
        controller
            .send(Command::Preview(DeckCommand::Play))
            .unwrap();
        let mut pairs = ExternalOutputs::default();
        pairs.decks[0] = Some((2, 3));
        pairs.decks[1] = Some((4, 5));
        pairs.preview = Some((6, 7));
        controller
            .send(Command::Outputs(Outputs::External(pairs)))
            .unwrap();
        let mut out = vec![9.0; 8 * 4_800];
        renderer.render(&mut out, 8, Some((0, 1)));
        let last = &out[out.len() - 8..];
        assert!(last[..2].iter().all(|x| x.abs() < 1e-6), "{last:?}");
        assert!(
            (last[2] - 0.5).abs() < 0.01 && (last[3] + 0.5).abs() < 0.01,
            "{last:?}"
        );
        assert!(last[4..6].iter().all(|x| x.abs() < 1e-6), "deck B stopped");
        assert!((last[6] - 0.5).abs() < 0.01, "preview: {last:?}");
        playing(&mut controller, 1);
        pairs.decks[1] = Some((2, 3));
        controller
            .send(Command::Outputs(Outputs::External(pairs)))
            .unwrap();
        renderer.render(&mut out, 8, None);
        let last = &out[out.len() - 8..];
        assert!((last[2] - 1.0).abs() < 0.02, "{last:?}");
    }

    #[test]
    fn an_offline_engine_renders_when_asked() {
        let (mut engine, mut renderer) = Engine::offline(RATE);
        assert_eq!(engine.info().device, "offline");
        playing(engine.controller(), 0);
        let mut out = vec![0.0; 2 * 4_800];
        renderer.render(&mut out, 2, None);
        assert!(engine.controller().state().decks[0].playing);
        assert!(out.iter().any(|s| s.abs() > 0.01), "the deck is heard");
    }

    #[test]
    fn preview_is_heard_on_the_cue_output_only() {
        let (mut renderer, mut controller) = channel(48_000);
        controller
            .send(Command::Preview(DeckCommand::Load(tone_track())))
            .unwrap();
        controller
            .send(Command::Preview(DeckCommand::Play))
            .unwrap();
        controller
            .send(Command::Mixer(MixerCommand::CueVolume(1.0)))
            .unwrap();
        let mut out = vec![0.0f32; 4 * 4096];
        renderer.render(&mut out, 4, Some((2, 3)));
        renderer.render(&mut out, 4, Some((2, 3)));
        let peak = |ch: usize| out.chunks(4).map(|f| f[ch].abs()).fold(0.0f32, f32::max);
        assert!(peak(2) > 0.01, "cue output carries the preview");
        assert!(peak(0) < 1e-6, "the master does not");
        assert!(controller.state().preview.playing);
    }

    #[test]
    fn a_channel_is_on_air_unless_its_fader_or_the_crossfader_shuts_it() {
        let channel = |fader, assign| ChannelState {
            fader,
            assign,
            ..ChannelState::default()
        };
        assert!(on_air(&channel(0.5, Assign::Thru), 1.0));
        assert!(!on_air(&channel(0.0, Assign::Thru), 0.5));
        assert!(on_air(&channel(1.0, Assign::A), 0.99));
        assert!(!on_air(&channel(1.0, Assign::A), 1.0));
        assert!(!on_air(&channel(1.0, Assign::B), 0.0));
        assert!(on_air(&channel(1.0, Assign::B), 0.01));
    }

    #[test]
    fn offered_rates_list_fixed_rates_and_common_ones_in_ranges() {
        assert_eq!(
            offered_rates(&[(48_000, 48_000), (44_100, 44_100), (48_000, 48_000)]),
            [44_100, 48_000]
        );
        assert_eq!(
            offered_rates(&[(8_000, 96_000), (192_000, 192_000)]),
            [44_100, 48_000, 88_200, 96_000, 192_000]
        );
        assert_eq!(offered_rates(&[]), Vec::<u32>::new());
    }

    #[test]
    fn monitor_channel_choice() {
        assert_eq!(monitor_channels(None, 2), None);
        assert_eq!(monitor_channels(None, 4), Some((2, 3)));
        assert_eq!(monitor_channels(Some((4, 5)), 6), Some((4, 5)));
        assert_eq!(
            monitor_channels(Some((4, 5)), 4),
            None,
            "missing pair: no cue"
        );
        assert_eq!(monitor_channels(Some((u16::MAX, u16::MAX)), 8), None);
    }

    #[test]
    fn mono_device_gets_mid() {
        let (mut renderer, mut controller) = channel(RATE);
        playing(&mut controller, 0);
        let mut out = vec![9.0; 4_800];
        renderer.render(&mut out, 1, None);
        assert!(out[out.len() - 1].abs() < 1e-3);
    }

    #[test]
    fn splits_large_callbacks() {
        let (mut renderer, mut controller) = channel(RATE);
        playing(&mut controller, 0);
        let mut out = vec![0.0; (MAX_BLOCK_FRAMES * 2 + 10) * 2];
        renderer.render(&mut out, 2, None);
        let expected = (MAX_BLOCK_FRAMES * 2 + 10) as f64 / f64::from(RATE);
        assert!((controller.state().decks[0].position_s - expected).abs() < 1e-6);
    }

    #[test]
    fn replaced_tracks_come_back_for_freeing() {
        let (mut renderer, mut controller) = channel(RATE);
        let first = tone_track();
        controller
            .send(Command::Deck(0, DeckCommand::Load(Arc::clone(&first))))
            .unwrap();
        controller
            .send(Command::Deck(0, DeckCommand::Load(tone_track())))
            .unwrap();
        renderer.render(&mut [0.0; 2], 2, None);
        assert_eq!(
            Arc::strong_count(&first),
            2,
            "old track parked in the garbage queue"
        );
        controller.collect_garbage();
        assert_eq!(Arc::strong_count(&first), 1);
    }

    #[test]
    fn the_record_tap_gets_the_master_as_heard() {
        let (mut renderer, mut controller) = channel(RATE);
        let sound = fx_pattern::Sound {
            name: "kick".into(),
            samples: vec![0.5; 4000].into(),
            rate: RATE as f32,
            level: 1.0,
            patterns: [fx_pattern::Steps {
                bits: u64::MAX,
                per_beat: 4,
            }; fx_pattern::PATTERNS],
        };
        let (tap, mut feed) = record_tap(RATE, 1.0);
        for c in [
            FxCommand::SetMode(0, FxMode::DrumMachine),
            FxCommand::SetKit {
                unit: 0,
                players: std::array::from_fn(|_| {
                    Box::new(DrumMachine::new(std::slice::from_ref(&sound), RATE))
                }),
            },
            FxCommand::Assign {
                channel: 0,
                unit: 0,
                on: true,
            },
            FxCommand::SetDryWet(0, 1.0),
            FxCommand::SetOn(0, true),
        ] {
            controller.send(Command::Fx(c)).unwrap();
        }
        controller.send(Command::RecordTap(Some(tap))).unwrap();
        let mut out = vec![0.0; 2 * 512];
        for _ in 0..40 {
            renderer.render(&mut out, 2, None);
        }
        let mut recorded = Vec::new();
        feed.drain(&mut recorded);
        assert_eq!(recorded.len(), 40 * 1024, "every block");
        let peak = recorded.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(
            peak > 0.1,
            "the kit on an empty deck reaches the master: {peak}"
        );
    }

    #[test]
    fn the_drum_machine_runs_on_the_clock_beside_a_stopped_deck() {
        let (mut renderer, mut controller) = channel(RATE);
        let sound = fx_pattern::Sound {
            name: "kick".into(),
            samples: vec![0.5; 4000].into(),
            rate: RATE as f32,
            level: 1.0,
            patterns: [fx_pattern::Steps {
                bits: u64::MAX,
                per_beat: 4,
            }; fx_pattern::PATTERNS],
        };
        let silent = Arc::new(Track::from_stereo(RATE, vec![0.0; 2 * RATE as usize * 4]));
        controller
            .send(Command::Deck(0, DeckCommand::Load(silent)))
            .unwrap();
        controller
            .send(Command::Deck(
                0,
                DeckCommand::SetGrid(Some(Grid::new(120.0, 0.0))),
            ))
            .unwrap();
        for c in [
            FxCommand::SetMode(0, FxMode::DrumMachine),
            FxCommand::SetKit {
                unit: 0,
                players: std::array::from_fn(|_| {
                    Box::new(DrumMachine::new(std::slice::from_ref(&sound), RATE))
                }),
            },
            FxCommand::Assign {
                channel: 0,
                unit: 0,
                on: true,
            },
            FxCommand::SetDryWet(0, 1.0),
            FxCommand::SetOn(0, true),
        ] {
            controller.send(Command::Fx(c)).unwrap();
        }
        let mut out = vec![0.0; 2 * 512];
        let mut peak = 0.0f32;
        for _ in 0..40 {
            renderer.render(&mut out, 2, None);
            peak = out.iter().fold(peak, |m, x| m.max(x.abs()));
        }
        assert!(peak > 0.1, "the kit is heard: {peak}");
    }

    #[test]
    fn the_midi_clock_starts_on_a_beat_and_ticks_24_a_beat() {
        let (mut renderer, mut controller) = channel(RATE);
        let (tap, mut feed) = midi_clock_tap();
        controller
            .send(Command::MidiClock(MidiClockCommand::Tap(Some(tap))))
            .unwrap();
        controller
            .send(Command::MidiClock(MidiClockCommand::Send(true)))
            .unwrap();
        let mut out = vec![0.0; 2 * 256];
        let mut sent = Vec::new();
        for _ in 0..(2 * RATE as usize / 256) {
            renderer.render(&mut out, 2, None);
            while let Some(m) = feed.pop() {
                sent.push(m);
            }
        }
        assert!(controller.state().midi_clock_running);
        assert_eq!(sent.first().map(|m| m.byte), Some(0xFA));
        let ticks = sent.iter().filter(|m| m.byte == 0xF8).count();
        assert!((72..=73).contains(&ticks), "{ticks} ticks");
        controller
            .send(Command::MidiClock(MidiClockCommand::Send(false)))
            .unwrap();
        renderer.render(&mut out, 2, None);
        assert_eq!(feed.pop().map(|m| m.byte), Some(0xFC));
        assert_eq!(feed.pop(), None);
    }

    #[test]
    fn in_ext_the_clock_follows_the_midi_clock_received() {
        let (mut renderer, mut controller) = channel(RATE);
        let (sink, feed) = midi_clock_input();
        controller
            .send(Command::MidiClock(MidiClockCommand::Input(Some(feed))))
            .unwrap();
        controller
            .send(Command::Sync(SyncCommand::SetExternal(true)))
            .unwrap();
        let base = Instant::now();
        let tick = |k: u64| base + Duration::from_micros(17_857 * k);
        sink.push(ClockInput {
            at: base,
            status: 0xFA,
            spp: 0,
        });
        for k in 0..200 {
            sink.push(ClockInput {
                at: tick(k),
                status: 0xF8,
                spp: 0,
            });
        }
        let mut out = vec![0.0; 2 * 256];
        for _ in 0..2_000 {
            renderer.render(&mut out, 2, None);
        }
        let sync = controller.state().sync;
        assert!(sync.external && sync.external_playing);
        assert!(sync.clock_bpm > 125.0, "{}", sync.clock_bpm);
    }

    #[test]
    fn sync_commands_reach_the_sync_engine() {
        let (mut renderer, mut controller) = channel(RATE);
        controller
            .send(Command::Sync(SyncCommand::SetSync(2, true)))
            .unwrap();
        renderer.render(&mut [0.0; 64], 2, None);
        assert!(controller.state().sync.synced[2]);
    }

    #[test]
    fn a_sample_deck_follows_its_channel_sync() {
        let (mut renderer, mut controller) = channel(RATE);
        let mut deck = SampleDeck::new(RATE);
        deck.handle(SampleCommand::SetBaseBpm(100.0));
        deck.handle(SampleCommand::Play(true));
        controller
            .send(Command::SampleDeck(
                2,
                SampleDeckCommand::Install(Box::new(deck)),
            ))
            .unwrap();
        let bpm = |renderer: &mut Renderer, controller: &mut Controller| {
            renderer.render(&mut [0.0; 64], 2, None);
            controller.state().sample_deck[2].as_ref().unwrap().bpm
        };
        assert!((bpm(&mut renderer, &mut controller) - 100.0).abs() < 1e-9);
        controller
            .send(Command::Sync(SyncCommand::SetSync(2, true)))
            .unwrap();
        let clock = controller.state().sync.clock_bpm;
        assert!((bpm(&mut renderer, &mut controller) - clock).abs() < 1e-9);
    }

    #[test]
    fn a_synced_sample_deck_follows_a_playing_track_master() {
        let (mut renderer, mut controller) = channel(RATE);
        playing(&mut controller, 0);
        controller
            .send(Command::Deck(
                0,
                DeckCommand::SetGrid(Some(Grid::new(128.0, 0.0))),
            ))
            .unwrap();
        let mut deck = SampleDeck::new(RATE);
        deck.handle(SampleCommand::SetQuantize {
            on: false,
            beats: 1.0,
        });
        deck.handle(SampleCommand::LoadCell {
            slot: 0,
            cell: 0,
            audio: CellAudio {
                track: tone_track(),
                params: CellParams {
                    bpm: 100.0,
                    play_type: PlayType::Loop,
                    synced: true,
                    ..CellParams::default()
                },
            },
        });
        deck.handle(SampleCommand::Play(true));
        deck.handle(SampleCommand::Trigger {
            slot: 0,
            cell: 0,
            pressed: true,
        });
        controller
            .send(Command::SampleDeck(
                2,
                SampleDeckCommand::Install(Box::new(deck)),
            ))
            .unwrap();
        controller
            .send(Command::Sync(SyncCommand::SetSync(2, true)))
            .unwrap();
        let mut out = vec![0.0; 2 * 512];
        for _ in 0..200 {
            renderer.render(&mut out, 2, None);
        }
        let state = controller.state();
        let s = state.sample_deck[2].as_ref().unwrap();
        eprintln!(
            "master {:?} clock {} sample bpm {} base {}",
            state.sync.master, state.sync.clock_bpm, s.bpm, s.base_bpm
        );
        assert_eq!(state.sync.master, Some(0));
        assert!((s.bpm - 128.0).abs() < 1e-6, "{}", s.bpm);
    }

    #[test]
    fn a_quantized_loop_triggered_mid_beat_starts_on_the_masters_beat() {
        let (mut renderer, mut controller) = channel(RATE);
        playing(&mut controller, 0);
        controller
            .send(Command::Deck(
                0,
                DeckCommand::SetGrid(Some(Grid::new(128.0, 0.0))),
            ))
            .unwrap();
        let mut deck = SampleDeck::new(RATE);
        deck.handle(SampleCommand::SetQuantize {
            on: true,
            beats: 1.0,
        });
        deck.handle(SampleCommand::LoadCell {
            slot: 0,
            cell: 0,
            audio: CellAudio {
                track: tone_track(),
                params: CellParams {
                    bpm: 100.0,
                    play_type: PlayType::Loop,
                    synced: true,
                    ..CellParams::default()
                },
            },
        });
        deck.handle(SampleCommand::Play(true));
        controller
            .send(Command::SampleDeck(
                2,
                SampleDeckCommand::Install(Box::new(deck)),
            ))
            .unwrap();
        controller
            .send(Command::Sync(SyncCommand::SetSync(2, true)))
            .unwrap();
        let mut out = vec![0.0; 2 * 64];
        let beat = |c: &mut Controller| c.state().sample_deck[2].as_ref().unwrap().beat;
        for _ in 0..400 {
            renderer.render(&mut out, 2, None);
        }
        let state = controller.state();
        let clock = state.sync.clock_beat;
        let phase = (beat(&mut controller) - clock).rem_euclid(1.0);
        assert!(phase.min(1.0 - phase) < 0.01, "phase {phase}");
        while (beat(&mut controller).fract() - 0.5).abs() > 0.05 {
            renderer.render(&mut out, 2, None);
        }
        controller
            .send(Command::SampleDeck(
                2,
                SampleDeckCommand::Deck(SampleCommand::Trigger {
                    slot: 0,
                    cell: 0,
                    pressed: true,
                }),
            ))
            .unwrap();
        renderer.render(&mut out, 2, None);
        let slot = |c: &mut Controller| c.state().sample_deck[2].as_ref().unwrap().slots[0];
        assert_ne!(slot(&mut controller).player, PlayerState::Playing, "waits");
        let target = beat(&mut controller).ceil();
        while beat(&mut controller) < target + 0.25 {
            renderer.render(&mut out, 2, None);
        }
        let s = slot(&mut controller);
        assert_eq!(s.player, PlayerState::Playing);
        let into = (beat(&mut controller) - target) * 60.0 / 100.0;
        assert!(
            (s.position_s - into).abs() < 0.01,
            "{} vs {into}",
            s.position_s
        );
    }
}
