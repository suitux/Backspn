use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use backspn_control::controller::{self, Controller};
use backspn_control::display::Frame;
use backspn_control::feedback::{
    BrowserFeedback, Feedback, HotcueKind, MixerKnobs, STEMS, StemFeedback,
};
use backspn_control::{Action, Input, InputEvent, Learn, MappingSet, Trigger, midi};
use backspn_engine::{CueKind, DECKS, EngineState, FxMode};
use eframe::egui::{self, RichText, Ui};

use crate::i18n::tr;
use crate::keys;

mod calibrate;
mod leds;
mod manager;
mod profiles;
mod tables;

fn mappings_path() -> PathBuf {
    if let Some(path) = std::env::var_os("BACKSPN_MAPPINGS") {
        return PathBuf::from(path);
    }
    crate::dirs::data().join("mappings.json")
}

pub struct Controls {
    pub set: MappingSet,
    path: PathBuf,
    ports: Vec<midi::Port>,
    devices: Vec<Box<dyn Controller>>,
    merged: Vec<bool>,
    tx: Sender<(String, InputEvent)>,
    rx: Receiver<(String, InputEvent)>,
    pub learn: Option<Learn>,
    message: Option<String>,
    manager: manager::Manager,
    clock: Option<crate::midi_clock::ClockHandlers>,
    sources: std::collections::HashMap<String, backspn_control::relative::Source>,
    leds: Vec<leds::Led>,
    pulses: backspn_control::output::Pulses,
    learn_output: bool,
    shown: Feedback,
    set_since: Vec<(Action, f32)>,
    repeat_at: std::time::Instant,
    ports_checked: std::time::Instant,
    audio_offer: Option<backspn_control::profile::Audio>,
    seen_inputs: Vec<String>,
    profiles: Vec<backspn_control::profile::Profile>,
    cards_offered: Vec<&'static str>,
    calibration: Option<calibrate::Calibration>,
    calibration_outcome: Option<calibrate::Outcome>,
}

impl Controls {
    pub fn open(ctx: &egui::Context, clock: crate::midi_clock::ClockHandlers) -> Self {
        let path = mappings_path();
        let set = match MappingSet::load(&path) {
            Ok((set, skipped)) => {
                if skipped > 0 {
                    log::warn!("mappings: skipped {skipped} unreadable entries");
                    crate::prefs::keep_backup(&path);
                }
                set
            }
            Err(e) => {
                if path.exists() {
                    log::warn!("mappings: cannot read {}: {e}", path.display());
                    crate::prefs::keep_backup(&path);
                }
                MappingSet {
                    mappings: MappingSet::default_keyboard(),
                    midi_inputs: Vec::new(),
                    ..MappingSet::default()
                }
            }
        };
        let (tx, rx) = mpsc::channel();
        let mut controls = Self {
            set,
            path,
            ports: Vec::new(),
            devices: Vec::new(),
            merged: Vec::new(),
            tx,
            rx,
            learn: None,
            message: None,
            manager: manager::Manager::default(),
            clock: Some(clock),
            sources: std::collections::HashMap::new(),
            leds: Vec::new(),
            pulses: backspn_control::output::Pulses::default(),
            learn_output: false,
            shown: Feedback::default(),
            set_since: Vec::new(),
            repeat_at: std::time::Instant::now(),
            ports_checked: std::time::Instant::now(),
            audio_offer: None,
            seen_inputs: Vec::new(),
            profiles: backspn_control::profile::all(),
            cards_offered: Vec::new(),
            calibration: None,
            calibration_outcome: None,
        };
        let (user, errors) = backspn_control::profile::read_dir(&profiles::user_dir());
        controls.profiles = backspn_control::profile::with_user(user);
        if !errors.is_empty() {
            controls.message = Some(format!("Cannot read profiles: {}", errors.join("; ")));
        }
        for name in controls.set.midi_inputs.clone() {
            controls.connect(&name, ctx);
        }
        for name in controls.set.midi_outputs.clone() {
            controls.connect_output(&name);
        }
        controls.watch_devices(ctx);
        controls
    }

    #[cfg(test)]
    pub fn detached() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            set: MappingSet {
                mappings: MappingSet::default_keyboard(),
                midi_inputs: Vec::new(),
                ..MappingSet::default()
            },
            path: PathBuf::new(),
            ports: Vec::new(),
            devices: Vec::new(),
            merged: Vec::new(),
            tx,
            rx,
            learn: None,
            message: None,
            manager: manager::Manager::default(),
            clock: None,
            sources: std::collections::HashMap::new(),
            leds: Vec::new(),
            pulses: backspn_control::output::Pulses::default(),
            learn_output: false,
            shown: Feedback::default(),
            set_since: Vec::new(),
            repeat_at: std::time::Instant::now(),
            ports_checked: std::time::Instant::now(),
            audio_offer: None,
            seen_inputs: Vec::new(),
            profiles: backspn_control::profile::all(),
            cards_offered: Vec::new(),
            calibration: None,
            calibration_outcome: None,
        }
    }

    fn watch_devices(&mut self, ctx: &egui::Context) {
        for model in controller::models() {
            let wake = ctx.clone();
            match (model.start)(self.tx.clone(), Box::new(move || wake.request_repaint())) {
                Ok(device) => {
                    self.devices.push(device);
                    self.merged.push(false);
                }
                Err(e) => self.message = Some(format!("{}: {e}", model.name)),
            }
        }
    }

    fn merge_default_mappings(&mut self) {
        let mut changed = false;
        for (device, merged) in self.devices.iter().zip(&mut self.merged) {
            if *merged || !device.connected() {
                continue;
            }
            *merged = true;
            for mapping in &mut self.set.mappings {
                if let Some(new) = device.upgrade_mapping(mapping) {
                    *mapping = new;
                    changed = true;
                }
            }
            let missing: Vec<_> = device
                .default_mappings()
                .into_iter()
                .filter(|d| !self.set.mappings.iter().any(|m| m.input == d.input))
                .collect();
            changed |= !missing.is_empty();
            self.set.mappings.extend(missing);
        }
        if changed {
            self.save();
        }
    }

    fn screen_device(&self) -> Option<&dyn Controller> {
        self.devices
            .iter()
            .find(|d| d.screens().is_some())
            .map(AsRef::as_ref)
    }

    pub fn held_stem(&self, deck: usize) -> Option<usize> {
        self.screen_device()?.held_stem(deck)
    }

    pub fn held_slot(&self, deck: usize) -> Option<(usize, bool)> {
        self.screen_device()?.held_slot(deck)
    }

    pub fn screens(&self) -> Option<((usize, usize), Vec<usize>)> {
        let device = self.screen_device()?;
        let screens = device.screens()?;
        if !screens.connected() {
            return None;
        }
        let decks = device
            .screen_decks()
            .map_or_else(|| (0..screens.count()).collect(), |l| l.to_vec());
        Some((screens.size(), decks))
    }

    pub fn motor_decks(&self) -> [bool; DECKS] {
        let mut decks = [false; DECKS];
        let layers = self
            .devices
            .iter()
            .filter(|d| d.connected() && d.motor_jogs())
            .filter_map(|d| d.deck_layers());
        for deck in layers.flatten() {
            if let Some(d) = decks.get_mut(deck) {
                *d = true;
            }
        }
        decks
    }

    pub fn send_screens(&self, frames: Vec<Frame>) {
        if let Some(screens) = self.screen_device().and_then(|d| d.screens()) {
            screens.send(frames);
        }
    }

    pub fn show(
        &mut self,
        state: &EngineState,
        quantize: bool,
        turntable: [bool; DECKS],
        app: &AppFeedback,
    ) {
        self.merge_default_mappings();
        let mut feedback = Feedback {
            jog_tension: app.jog_tension,
            tempo_fader_relative: app.tempo_fader_relative,
            browser: app.browser,
            knobs: app.knobs,
            focus: app.focus,
            ..feedback(state, quantize, turntable)
        };
        for ((((deck, stems), range), platter), warning) in feedback
            .decks
            .iter_mut()
            .zip(app.stems)
            .zip(app.tempo_ranges)
            .zip(app.platters)
            .zip(app.end_warnings)
        {
            deck.stems = stems;
            deck.tempo_range = range;
            deck.platter = platter;
            deck.end_warning = warning;
        }
        for ((deck, locked), live) in feedback
            .decks
            .iter_mut()
            .zip(app.tempo_locked)
            .zip(app.live)
        {
            deck.tempo_locked = locked;
            deck.live = live;
        }
        feedback.microphone = app.microphone;
        for (unit, (steps, cells)) in feedback.fx.iter_mut().zip(app.pattern_steps) {
            unit.pattern_steps = steps;
            unit.pattern_cells = cells;
        }
        for ((deck, pad_fx), loop_size) in feedback
            .decks
            .iter_mut()
            .zip(app.pad_fx)
            .zip(app.loop_sizes)
        {
            deck.pad_fx = pad_fx;
            deck.loop_size = loop_size;
        }
        for (deck, sample_deck) in feedback.decks.iter_mut().zip(app.sample_deck) {
            deck.sample_deck = sample_deck;
        }
        for (c, (fx, on)) in feedback.channels.iter_mut().zip(app.mixer_fx) {
            c.mixer_fx = fx;
            c.mixer_fx_on = on;
        }
        for device in &mut self.devices {
            device.show(&feedback);
        }
        self.send_leds(&feedback);
        self.shown = feedback;
        self.set_since.clear();
    }

    fn save(&mut self) {
        if self.path.as_os_str().is_empty() {
            return;
        }
        if let Err(e) = self.set.save(&self.path) {
            self.message = Some(format!("Cannot save mappings: {e}"));
        }
    }

    fn connect(&mut self, name: &str, ctx: &egui::Context) {
        if !midi::input_ports().iter().any(|p| p == name) {
            log::info!("MIDI input {name:?} not present yet");
            return;
        }
        let wake = ctx.clone();
        let clock = self.clock.as_ref().map(|handlers| handlers());
        match midi::Port::open(name, self.tx.clone(), move || wake.request_repaint(), clock) {
            Ok(port) => self.ports.push(port),
            Err(e) => self.message = Some(e),
        }
    }

    fn watch_ports(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(PORT_CHECK);
        if self.ports_checked.elapsed() < PORT_CHECK {
            return;
        }
        self.ports_checked = std::time::Instant::now();
        let available = midi::input_ports();
        let appeared: Vec<String> = available
            .iter()
            .filter(|p| !self.seen_inputs.contains(p))
            .cloned()
            .collect();
        self.seen_inputs.clone_from(&available);
        self.recognize(&appeared, ctx);
        let open: Vec<String> = self.ports.iter().map(|p| p.name.clone()).collect();
        let (add, gone) = port_changes(&self.set.midi_inputs, &open, &available);
        self.ports.retain(|p| !gone.contains(&p.name));
        for name in add {
            self.connect(&name, ctx);
        }
        self.watch_outputs();
    }

    pub fn events(&mut self, ctx: &egui::Context) -> Vec<(String, InputEvent)> {
        self.watch_ports(ctx);
        let mut events = Vec::new();
        if !ctx.text_edit_focused() {
            ctx.input(|i| {
                for e in &i.events {
                    if let egui::Event::Key {
                        key,
                        physical_key,
                        pressed,
                        repeat: false,
                        modifiers,
                    } = e
                    {
                        let name = keys::name(physical_key.unwrap_or(*key), *modifiers);
                        events.push((
                            KEYBOARD.to_owned(),
                            InputEvent {
                                input: Input::Key(name),
                                value: if *pressed { 1.0 } else { 0.0 },
                                raw: 0,
                                pressed: Some(*pressed),
                            },
                        ));
                    }
                }
            });
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space));
        }
        while let Ok(event) = self.rx.try_recv() {
            events.push(event);
        }
        events
    }

    pub fn map(&mut self, source: &str, event: &InputEvent) -> Vec<(Action, Trigger)> {
        if let Some(device) = self.devices.iter_mut().find(|d| d.source() == source) {
            return device.translate(&self.set, event);
        }
        let state = self
            .sources
            .entry(source.to_owned())
            .or_insert_with(|| backspn_control::relative::Source::for_port(source));
        let (shown, set_since) = (&self.shown, &self.set_since);
        let current = |action: Action| {
            set_since
                .iter()
                .rev()
                .find(|(a, _)| *a == action)
                .map(|&(_, v)| v)
                .or_else(|| backspn_control::output::position(action, shown))
        };
        let triggers = self.set.map_source(event, state, shown, &current);
        for &(action, t) in &triggers {
            if let Trigger::Set(v) = t {
                self.set_since.push((action, v));
            }
        }
        triggers
    }

    pub fn repeats(&mut self, ctx: &egui::Context) -> Vec<(Action, Trigger)> {
        use backspn_control::relative::REPEAT_PERIOD;
        let now = std::time::Instant::now();
        let behind = now.saturating_duration_since(self.repeat_at);
        if behind >= REPEAT_PERIOD {
            let periods = behind.as_nanos() / REPEAT_PERIOD.as_nanos();
            self.repeat_at += REPEAT_PERIOD * u32::try_from(periods).unwrap_or(u32::MAX);
        }
        let mut out = Vec::new();
        for device in &mut self.devices {
            out.extend(device.tick());
            if device.busy() {
                ctx.request_repaint_after(std::time::Duration::from_millis(20));
            }
        }
        while self.repeat_at <= now {
            self.repeat_at += REPEAT_PERIOD;
            for source in self.sources.values_mut() {
                out.extend(self.set.repeat_tick(source, &self.shown));
            }
        }
        if self
            .sources
            .values()
            .any(backspn_control::relative::Source::repeating)
        {
            ctx.request_repaint_after(self.repeat_at - now);
        }
        out
    }

    pub fn native_connected(&self, name: &str) -> bool {
        self.devices
            .iter()
            .any(|d| d.name() == name && d.connected())
    }

    pub fn device_section(&mut self, ui: &mut Ui, name: &str) {
        let Some(device) = self.devices.iter().find(|d| d.name() == name) else {
            ui.label(RichText::new(tr("Not available on this system.")).weak());
            return;
        };
        if device.connected() {
            ui.label(tr("Connected."));
        } else {
            ui.label(RichText::new(tr("Not found (plug it in any time).")).weak());
        }
        for line in device.status() {
            ui.label(RichText::new(line).weak());
        }
        if ui
            .button(tr("Restore default mapping"))
            .on_hover_text(tr(
                "Uses the stock mapping; your own mappings stay in the Controller Manager's Mapping list",
            ))
            .clicked()
        {
            self.sync_tables();
            self.use_stock_of(&backspn_control::table::Owner::Native(name.to_owned()));
        }
    }

    fn import_tsi(&mut self, _ctx: &egui::Context) {
        let documents = crate::dirs::home().join(backspn_formats::interop::DATA_DIR);
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Settings / mappings", &["tsi"])
            .set_directory(&documents)
            .pick_file()
        else {
            return;
        };
        let tsi = match backspn_formats::tsi::Tsi::from_path(&path) {
            Ok(tsi) => tsi,
            Err(e) => {
                self.message = Some(format!("Cannot read {}: {e}", path.display()));
                return;
            }
        };
        let mut imported = 0;
        let mut leds = 0;
        let mut skipped = 0;
        let mut tables = Vec::new();
        let mut results: Vec<_> = tsi
            .devices
            .iter()
            .map(backspn_control::tsi_import::import)
            .collect();
        let modifiers_apart = backspn_control::tsi_import::separate_modifiers(&mut results);
        for (device, result) in tsi.devices.iter().zip(results) {
            let Some(d) = result.device else {
                tables.push(backspn_control::table::from_tsi(device));
                continue;
            };
            imported += result.mappings.len();
            skipped += result.skipped;
            for m in result.mappings {
                self.set.upsert(m);
            }
            for r in result.pair_ranges {
                self.set.set_pair_range(r);
            }
            leds += result.outputs.len();
            self.import_outputs(result.outputs, &device.out_port);
            self.set.add_device(d);
            self.manager.selected = Some(manager::Selected::Device(self.set.devices.len() - 1));
            self.manager.row = None;
            if !device.in_port.is_empty()
                && device.in_port != "None"
                && !self.set.midi_inputs.contains(&device.in_port)
                && midi::input_ports().contains(&device.in_port)
            {
                self.set.midi_inputs.push(device.in_port.clone());
            }
        }
        self.sync_tables();
        for file in tables {
            let owner = match &file.target {
                backspn_control::table::For::Native(name) => {
                    backspn_control::table::Owner::Native(name.clone())
                }
                _ => match self
                    .set
                    .devices
                    .iter()
                    .find(|d| d.kind == backspn_control::device::Kind::Keyboard)
                {
                    Some(k) => backspn_control::table::Owner::Device(k.id),
                    None => continue,
                },
            };
            imported += file.mappings.len();
            skipped += file.skipped;
            self.add_imported(&owner, vec![file]);
        }
        let mut message = format!(
            "Imported {imported} mappings and {leds} LED outputs ({skipped} unsupported skipped)"
        );
        if !modifiers_apart {
            message.push_str(". Devices on one port share some modifiers (more than eight in all)");
        }
        self.message = Some(message);
        self.save();
    }
}

#[derive(Clone, Copy)]
pub struct AppFeedback {
    pub jog_tension: u8,
    pub mixer_fx: [(u8, bool); 4],
    pub browser: BrowserFeedback,
    pub knobs: MixerKnobs,
    pub focus: usize,
    pub platters: [backspn_control::feedback::PlatterFeedback; DECKS],
    pub tempo_ranges: [u8; DECKS],
    pub loop_sizes: [u8; DECKS],
    pub microphone: bool,
    pub pattern_steps: [(u16, u8); backspn_control::feedback::FX_UNITS],
    pub stems: [Option<[StemFeedback; STEMS]>; DECKS],
    pub sample_deck: [Option<backspn_control::feedback::SampleFeedback>; DECKS],
    pub end_warnings: [bool; DECKS],
    pub tempo_locked: [bool; DECKS],
    pub live: [bool; DECKS],
    pub pad_fx: [[u16; 2]; DECKS],
    pub tempo_fader_relative: bool,
}

const PORT_CHECK: std::time::Duration = std::time::Duration::from_secs(2);

fn port_changes(
    wanted: &[String],
    open: &[String],
    available: &[String],
) -> (Vec<String>, Vec<String>) {
    let add = wanted
        .iter()
        .filter(|n| available.contains(n) && !open.contains(n))
        .cloned()
        .collect();
    let gone = open
        .iter()
        .filter(|n| !available.contains(n))
        .cloned()
        .collect();
    (add, gone)
}

pub(crate) const KEYBOARD: &str = "keyboard";

fn hotcue_kind(kind: CueKind) -> HotcueKind {
    match kind {
        CueKind::FadeIn => HotcueKind::FadeIn,
        CueKind::FadeOut => HotcueKind::FadeOut,
        CueKind::Load => HotcueKind::Load,
        CueKind::Grid => HotcueKind::Grid,
        CueKind::Loop => HotcueKind::Loop,
        CueKind::Cue | CueKind::Other(_) => HotcueKind::Cue,
    }
}

#[allow(clippy::cast_possible_truncation)]
fn feedback(state: &EngineState, quantize: bool, turntable: [bool; DECKS]) -> Feedback {
    let r = &state.loop_recorder;
    let mut f = Feedback {
        quantize,
        loop_recorder: backspn_control::feedback::LoopRecorderFeedback {
            state: r.state.code(),
            undo: r.undo,
            size: u8::try_from(r.size).unwrap_or(0),
            source: u8::try_from(r.source.index()).unwrap_or(0),
            dry_wet: r.dry_wet,
            position: r.position,
        },
        ..Feedback::default()
    };
    for (deck, d) in f.decks.iter_mut().enumerate().take(DECKS) {
        let s = &state.decks[deck];
        d.loaded = s.loaded;
        d.playing = s.playing;
        d.tempo = s.tempo;
        d.synced = state.sync.synced[deck];
        d.master = state.sync.master == Some(deck);
        d.slip = s.slip;
        d.reverse = s.reverse;
        d.keylock = s.keylock;
        d.loop_active = s.loop_active;
        d.in_loop = s.loop_active
            && s.loop_start_s
                .zip(s.loop_end_s)
                .is_some_and(|(a, b)| (a..b).contains(&s.position_s));
        d.turntable = turntable[deck];
        d.position_s = s.position_s;
        d.duration_s = s.duration_s;
        d.hotcues = s
            .hotcues
            .map(|h| h.map_or(HotcueKind::Empty, |h| hotcue_kind(h.kind)));
    }
    for (ch, c) in f.channels.iter_mut().enumerate().take(DECKS) {
        c.cue = state.mixer.channels[ch].cue;
        c.fx_assign = state.fx.assign[ch];
        c.level = state.mixer.channel_peaks[ch];
        c.mute = state.mixer.channels[ch].mute;
    }
    f.master_level = state.mixer.master_peak;
    f.ticks = state.ticks;
    f.master_tick = state.master_tick;
    for (unit, fx) in f.fx.iter_mut().zip(&state.fx.units) {
        unit.on = fx.on;
        unit.mode = match fx.mode {
            FxMode::Group => 0,
            FxMode::Single => 1,
            FxMode::DrumMachine => 2,
        };
        unit.pattern_step = fx.pattern.step.map(|s| s as u8);
        unit.buttons = std::array::from_fn(|i| match fx.mode {
            FxMode::Group => fx.slots[i].on,
            FxMode::Single => fx.buttons[i],
            FxMode::DrumMachine => i == 2 && fx.pattern.duck,
        });
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    use backspn_engine::HotcuePoint;

    #[test]
    fn ports_plugged_in_open_and_unplugged_ones_close() {
        let s = |v: &[&str]| v.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        let (add, gone) = port_changes(&s(&["X1", "MPK"]), &s(&["MPK"]), &s(&["X1", "MPK"]));
        assert_eq!((add, gone), (s(&["X1"]), s(&[])));
        let (add, gone) = port_changes(&s(&["X1", "MPK"]), &s(&["X1", "MPK"]), &s(&["X1"]));
        assert_eq!((add, gone), (s(&[]), s(&["MPK"])));
        let (add, _) = port_changes(&s(&[]), &s(&[]), &s(&["X1"]));
        assert_eq!(add, Vec::<String>::new());
    }

    #[test]
    fn feedback_reflects_decks_mixer_and_fx() {
        let mut state = EngineState::default();
        state.decks[1].loaded = true;
        state.decks[1].playing = true;
        let hotcue = |kind| {
            Some(HotcuePoint {
                start_s: 1.0,
                kind,
                loop_s: None,
            })
        };
        state.decks[1].hotcues[0] = hotcue(CueKind::Cue);
        state.decks[1].hotcues[2] = hotcue(CueKind::Loop);
        state.decks[1].hotcues[3] = hotcue(CueKind::FadeOut);
        state.sync.master = Some(1);
        state.mixer.channels[3].cue = true;
        state.fx.assign[0][1] = true;
        state.fx.units[2].on = true;
        state.fx.units[2].mode = FxMode::Single;
        state.fx.units[2].buttons = [false, true, false];
        let f = feedback(&state, true, [false, true, false, false]);
        let d = &f.decks[1];
        assert!(d.loaded && d.playing && d.master && d.turntable);
        assert_eq!(
            d.hotcues[..4],
            [
                HotcueKind::Cue,
                HotcueKind::Empty,
                HotcueKind::Loop,
                HotcueKind::FadeOut
            ]
        );
        assert!(f.channels[3].cue && f.channels[0].fx_assign[1]);
        assert!(f.fx[2].on && f.fx[2].buttons == [false, true, false]);
        assert!(f.quantize && !f.decks[0].loaded);
        state.decks[0].keylock = true;
        state.mixer.channels[2].mute = true;
        state.ticks[3] = true;
        state.decks[0].loop_active = true;
        state.decks[0].loop_start_s = Some(1.0);
        state.decks[0].loop_end_s = Some(2.0);
        state.decks[0].position_s = 1.5;
        let f = feedback(&state, true, [false; DECKS]);
        assert!(f.decks[0].keylock && f.channels[2].mute && f.ticks[3]);
        assert!(f.decks[0].in_loop && f.fx[2].mode == 1);
        state.fx.units[1].mode = FxMode::DrumMachine;
        state.fx.units[1].pattern.duck = true;
        let f = feedback(&state, true, [false; DECKS]);
        assert_eq!(f.fx[1].buttons, [false, false, true]);
    }
}
