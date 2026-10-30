use super::screen::Screens;
use super::{
    Decoder, Device, JogRing, Layout, Leds, METER_REPORT_LEN, Meters, RING_STEPS, RingMode, S4,
    colour, motor,
};
use crate::controller::{Controller, EventSender, ScreenSink, Waker};
use crate::feedback::{
    DECKS, DeckFeedback, Feedback, HotcueKind, STEMS, SampleFeedback, StemFeedback,
};
use crate::{
    Action, Input, InputEvent, Interaction, Mapping, MappingSet, SamplePart, SampleSlotPart,
    StemPart, Trigger, trigger,
};

pub struct S4Mk3 {
    layout: &'static Layout,
    device: Device,
    decoder: Decoder,
    screens: Option<Result<Screens, String>>,
    state: S4State,
    leds: Option<Leds>,
    meters: Option<Meters>,
    rings: Option<[JogRing; 2]>,
    motors: Option<motor::Settings>,
    displays: Option<Vec<Vec<u8>>>,
}

impl std::fmt::Debug for S4Mk3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S4Mk3")
            .field("device", &self.device)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

pub fn start(events: EventSender, wake: Waker) -> Result<Box<dyn Controller>, String> {
    start_with(&S4, events, wake)
}

pub fn start_with(
    layout: &'static Layout,
    events: EventSender,
    wake: Waker,
) -> Result<Box<dyn Controller>, String> {
    let device = Device::start(layout, events, wake)?;
    Ok(Box::new(S4Mk3 {
        layout,
        device,
        decoder: Decoder::new(layout),
        screens: layout.screens.then(Screens::start),
        state: S4State::for_layout(layout),
        leds: None,
        meters: None,
        rings: None,
        motors: None,
        displays: None,
    }))
}

impl Controller for S4Mk3 {
    fn name(&self) -> &'static str {
        self.layout.name
    }

    fn source(&self) -> &'static str {
        self.layout.source
    }

    fn connected(&self) -> bool {
        self.device.connected()
    }

    fn default_mappings(&self) -> Vec<Mapping> {
        (self.layout.default_mappings)()
    }

    fn upgrade_mapping(&self, mapping: &Mapping) -> Option<Mapping> {
        self.layout.upgrade.and_then(|upgrade| upgrade(mapping))
    }

    fn tick(&mut self) -> Vec<(Action, Trigger)> {
        click_due(&mut self.state, std::time::Instant::now())
    }

    fn busy(&self) -> bool {
        self.state.click.iter().any(Option::is_some)
    }

    fn held_stem(&self, deck: usize) -> Option<usize> {
        self.state.active_stem.get(deck).copied().flatten()
    }

    fn held_slot(&self, deck: usize) -> Option<(usize, bool)> {
        let slot = self.state.active_slot.get(deck).copied().flatten()?;
        Some((
            slot,
            self.state.browsing[deck].on(std::time::Instant::now()),
        ))
    }

    fn owns_input(&self, input: &Input) -> bool {
        matches!(input, Input::Hid(name) if self.layout.has(name))
    }

    fn translate(&mut self, set: &MappingSet, event: &InputEvent) -> Vec<(Action, Trigger)> {
        let mut event = event.clone();
        if matches!(&event.input, Input::Hid(name) if name.ends_with(".Jog.Turn")) {
            event.value /= self.layout.jog_steps_per_turn as f32;
        }
        translate(&mut self.state, set, &event)
    }

    fn show(&mut self, feedback: &Feedback) {
        self.state.shown = *feedback;
        self.state.stems = feedback.decks.map(|d| d.stems);
        self.state.sample_deck = feedback.decks.map(|d| d.sample_deck);
        self.state.blink = blink_on(std::time::Instant::now());
        self.state.turntable = feedback.decks.map(|d| d.turntable);
        self.state.tempo_relative = feedback.tempo_fader_relative;
        self.state.mixer_fx_on = std::array::from_fn(|c| feedback.channels[c].mixer_fx_on);
        self.state.drop_unsupported_modes();
        let rings: [JogRing; 2] = std::array::from_fn(|i| {
            let deck = self.state.layer[i];
            let colour = if deck >= 2 {
                colour::ORANGE
            } else {
                colour::BLUE
            };
            let platter = (self.state.touched[i], self.state.platter[i]);
            jog_ring(&feedback.decks[deck], colour + colour::BRIGHT, platter)
        });
        if self.layout.rings && self.rings != Some(rings) {
            self.device.send_rings(rings);
            self.rings = Some(rings);
        }
        if self.layout.motors {
            let motors = motor_settings(&self.state.layer, feedback);
            if self.motors != Some(motors) {
                self.device.send_motors(motors);
                self.motors = Some(motors);
            }
        }
        let mut leds = leds(&self.decoder, &self.state, feedback);
        if let Some(wheels) = self.layout.wheel_leds {
            for (i, ring) in rings.iter().enumerate() {
                let deck = &feedback.decks[self.state.layer[i]];
                wheel_leds(&mut leds, &wheels[i], *ring, deck, self.state.blink);
            }
        }
        if let Some(strips) = self.layout.strip_leds {
            for (i, strip) in strips.iter().enumerate() {
                let deck = &feedback.decks[self.state.layer[i]];
                strip_leds(&mut leds, strip, deck, self.state.blink);
            }
        }
        if let Some(meter_leds) = self.layout.meter_leds {
            meter_leds(feedback, &self.decoder, &mut leds);
        }
        if let Some(deck_leds) = self.layout.deck_leds {
            deck_leds(feedback, &self.decoder, self.state.layer, &mut leds);
        }
        if let Some(hooks) = self.layout.modifier_hooks {
            (hooks.leds)(
                feedback,
                &self.decoder,
                &self.state.source.modifiers,
                &mut leds,
            );
        }
        if let Some(bar) = self.layout.touch_strip.and_then(|s| s.bar) {
            let levels = self
                .state
                .strip
                .bar(feedback, self.state.layer, bar.positions);
            crate::strip::write_bar(&levels, bar, &mut leds.0);
        }
        if self.leds.as_ref() != Some(&leds) {
            self.device.send_leds(leds.clone());
            self.leds = Some(leds);
        }
        if let Some(displays) = self.layout.displays {
            let reports = displays(feedback, self.state.layer, &self.state.source.modifiers);
            if self.displays.as_ref() != Some(&reports) {
                self.device.send_reports(reports.clone());
                self.displays = Some(reports);
            }
        }
        if let Some(meters) = self.layout.meters {
            let meters = meters(feedback);
            if self.meters.as_ref() != Some(&meters) {
                self.device.send_meters(meters.clone());
                self.meters = Some(meters);
            }
        }
    }

    fn deck_layers(&self) -> Option<[usize; 2]> {
        Some(self.state.layer)
    }

    fn screen_decks(&self) -> Option<[usize; 2]> {
        Some(screen_decks(self.state.layer, &self.state.active_slot))
    }

    fn motor_jogs(&self) -> bool {
        self.layout.motors
    }

    fn calibration_groups(&self) -> &'static [crate::calibration::Group] {
        self.layout.calibration
    }

    fn calibrate(&self, group: usize, step: crate::calibration::Step) -> bool {
        self.layout
            .calibration
            .get(group)
            .is_some_and(|g| self.device.calibrate(g, step))
    }

    fn audio(&self) -> Option<crate::profile::Audio> {
        self.layout.sound_card.map(|c| crate::profile::Audio {
            device: c.name.to_owned(),
            master: c.master,
            monitor: c.monitor,
            buffer_frames: Some(c.buffer_frames),
            inputs: c.inputs,
        })
    }

    fn screens(&self) -> Option<&dyn ScreenSink> {
        self.screens
            .as_ref()
            .and_then(|s| s.as_ref().ok())
            .map(|s| s as &dyn ScreenSink)
    }

    fn status(&self) -> Vec<String> {
        match &self.screens {
            None => Vec::new(),
            Some(Ok(s)) if s.connected() => vec!["Screens: showing the decks.".into()],
            Some(Ok(s)) => s
                .problem()
                .map(|p| vec![format!("Screens: {p}.")])
                .unwrap_or_default(),
            Some(Err(e)) => vec![format!("Screens: {e}.")],
        }
    }
}

fn screen_decks(layer: [usize; 2], active_slot: &[Option<usize>; DECKS]) -> [usize; 2] {
    std::array::from_fn(|side| {
        let (top, bottom) = (side, side + 2);
        if active_slot[bottom].is_some() {
            bottom
        } else if active_slot[top].is_some() {
            top
        } else {
            layer[side]
        }
    })
}

fn strip_leds(leds: &mut Leds, strip: &[usize; 6], deck: &DeckFeedback, blink: bool) {
    let value = if deck.playing && deck.loop_active {
        colour::GREEN + colour::BRIGHT
    } else if deck.playing && deck.end_warning {
        if blink {
            colour::RED + colour::BRIGHT
        } else {
            colour::OFF
        }
    } else {
        colour::BLUE + colour::BRIGHT
    };
    for &led in strip {
        if let Some(byte) = leds.0.get_mut(led) {
            *byte = value;
        }
    }
}

fn wheel_leds(
    leds: &mut Leds,
    wheel: &[usize; 8],
    ring: JogRing,
    deck: &DeckFeedback,
    blink: bool,
) {
    let all = match ring.mode {
        RingMode::Off => false,
        RingMode::Flash => blink,
        RingMode::Spot => deck.playing && deck.loop_active,
    };
    let spot = usize::from(ring.position) * wheel.len() / usize::from(RING_STEPS);
    for (i, &led) in wheel.iter().enumerate() {
        let on = all || (ring.mode == RingMode::Spot && i == spot);
        if let Some(byte) = leds.0.get_mut(led) {
            *byte = if on { ring.colour } else { colour::OFF };
        }
    }
}

pub(crate) fn upgraded(mapping: &Mapping) -> Option<Mapping> {
    let Input::Hid(name) = &mapping.input else {
        return None;
    };
    if let Action::LoopActive { deck } = mapping.action
        && name.ends_with(".Loop.Push")
    {
        return Some(Mapping {
            action: Action::FastLoop { deck },
            ..mapping.clone()
        });
    }
    if matches!(mapping.action, Action::MixerFxOn { .. })
        && mapping.interaction == Interaction::Trigger
    {
        return Some(Mapping {
            interaction: Interaction::Hold,
            ..mapping.clone()
        });
    }
    if !name.ends_with(".Move.Turn") || mapping.interaction != Interaction::Relative {
        return None;
    }
    let Action::BeatJump { deck, beats } = mapping.action else {
        return None;
    };
    ((beats - 4.0).abs() < f64::EPSILON).then(|| Mapping {
        action: Action::LoopMove { deck },
        ..mapping.clone()
    })
}

fn motor_settings(layer: &[usize; 2], feedback: &Feedback) -> motor::Settings {
    motor::Settings {
        sides: std::array::from_fn(|i| {
            let deck = &feedback.decks[layer[i]];
            if deck.turntable {
                motor::Side {
                    mode: motor::Mode::Turntable,
                    playing: deck.platter.motor_on,
                    speed_rpm: deck.platter.rpm,
                    motor_off: deck.platter.motor_off,
                }
            } else {
                motor::Side {
                    mode: motor::Mode::Jog,
                    playing: deck.playing,
                    ..motor::Side::default()
                }
            }
        }),
        tension: feedback.jog_tension,
    }
}

fn leds(d: &Decoder, state: &S4State, feedback: &Feedback) -> Leds {
    let mut leds = Leds::for_layout(d.layout());
    for (i, side) in ["Left", "Right"].into_iter().enumerate() {
        let deck = state.layer[i];
        let s = &feedback.decks[deck];
        let n = |c: &str| format!("{side}.{c}");
        let (upper, lower) = if i == 0 { ("A", "C") } else { ("B", "D") };
        let lower_on = deck >= 2;
        let deck_colour = if lower_on {
            colour::ORANGE
        } else {
            colour::BLUE
        };
        leds.set(d, &n(&format!("Deck.{upper}")), colour::BLUE, !lower_on);
        leds.set(d, &n(&format!("Deck.{lower}")), colour::ORANGE, lower_on);
        leds.set(d, &n("Shift"), colour::WHITE, state.shift[i]);
        leds.set(d, &n("Jog Mode.Grid"), deck_colour, state.grid[i]);
        leds.set(d, &n("Jog Mode.TT"), colour::WHITE, s.turntable);
        leds.set(d, &n("Jog Mode.Jog"), colour::WHITE, !s.turntable);
        leds.set(d, &n("Play"), colour::GREEN, s.playing);
        leds.set(d, &n("Cue"), colour::ORANGE, s.loaded && !s.playing);
        tempo_leds(&mut leds, d, (i, side), state.shift[i], s, deck_colour);
        leds.set(d, &n("Master"), colour::WHITE, s.master);
        leds.set(d, &n("Keylock"), deck_colour, s.keylock);
        leds.set(d, &n("Quant"), colour::WHITE, feedback.quantize);
        leds.set(d, &n("Loop.In"), colour::GREEN, s.loop_active);
        leds.set(d, &n("Loop.Out"), colour::GREEN, s.loop_active);
        leds.set(d, &n("Display.Key Lock"), colour::WHITE, s.keylock);
        leds.set(d, &n("Display.Master"), colour::WHITE, s.master);
        leds.set(d, &n("Display.On Air"), colour::WHITE, s.playing);
        leds.set(d, &n("Display.Deck A/B"), colour::BLUE, !lower_on);
        leds.set(d, &n("Display.Deck C/D"), colour::ORANGE, lower_on);
        leds.set(d, &n("Load"), colour::WHITE, s.loaded);
        pad_leds(&mut leds, d, state, feedback, (i, side), deck_colour);
        leds.set(d, &n("Transport Mode.Flux"), deck_colour, s.slip);
        leds.set(d, &n("Transport Mode.Reverse"), deck_colour, s.reverse);
        let b = &feedback.browser;
        leds.set(d, &n("Browse.View"), deck_colour, b.maximized);
        leds.set(d, &n("Browse.Favorite"), deck_colour, b.favorites);
        leds.set(d, &n("Browse.Preview"), deck_colour, b.preview);
        let unit = &feedback.fx[i];
        leds.set(d, &n("FX.Button 1"), colour::ORANGE, unit.on);
        leds.set(d, &n("FX.Mode"), colour::ORANGE, unit.mode != 0);
        for (b, on) in unit.buttons.iter().enumerate() {
            leds.set(d, &n(&format!("FX.Button {}", b + 2)), colour::ORANGE, *on);
        }
    }
    for (ch, name) in ["A", "B", "C", "D"].iter().enumerate() {
        let c = &feedback.channels[ch];
        let cue_lit = if state.mode_held {
            feedback.decks[ch].playing
        } else {
            c.cue
        };
        leds.set(d, &format!("Mixer.{name}.Cue"), colour::BLUE, cue_lit);
        leds.set(
            d,
            &format!("Mixer.{name}.Ext"),
            colour::WHITE,
            feedback.decks[ch].live,
        );
        for unit in 0..2 {
            let button = format!("Mixer.{name}.FX Assign {}", unit + 1);
            leds.set(d, &button, colour::ORANGE, c.fx_assign[unit]);
        }
        let fx_colour = colour::MIXER_FX[usize::from(c.mixer_fx.min(4))];
        leds.set(
            d,
            &format!("Mixer.{name}.Mixer FX.On"),
            fx_colour,
            c.mixer_fx_on,
        );
    }
    for (slot, name) in ["Filter", "1", "2", "3", "4"].into_iter().enumerate() {
        let used = feedback
            .channels
            .iter()
            .any(|c| usize::from(c.mixer_fx) == slot);
        let button = format!("Mixer.Mixer FX Select.{name}");
        leds.set(d, &button, colour::MIXER_FX[slot], used);
    }
    leds.set(d, "Mixer.Quant", colour::WHITE, feedback.quantize);
    leds.set(d, "Mixer.Mic", colour::WHITE, feedback.microphone);
    for (ch, name) in ["A", "B", "C", "D"].iter().enumerate() {
        leds.set(
            d,
            &format!("Mixer.{name}.Loaded"),
            colour::WHITE,
            feedback.decks[ch].loaded,
        );
    }
    leds
}

fn tempo_leds(
    leds: &mut Leds,
    d: &Decoder,
    (i, side): (usize, &str),
    shift: bool,
    s: &DeckFeedback,
    deck_colour: u8,
) {
    let sync = format!("{side}.Sync");
    if shift {
        let enabled = !s.synced && !s.turntable;
        let c = if enabled { colour::WHITE } else { colour::OFF };
        leds.set(d, &sync, c, s.tempo_locked);
    } else {
        leds.set(d, &sync, colour::SKY, s.synced);
    }
    let Some(tempo_led) = d.layout().tempo_leds else {
        return;
    };
    let at_one = (s.tempo - 1.0).abs() <= 0.0005;
    leds.0[tempo_led[i]] = if s.loaded && (s.tempo_locked || at_one) {
        let c = if s.tempo_locked {
            colour::RED
        } else {
            deck_colour
        };
        c + colour::BRIGHT
    } else {
        colour::OFF
    };
}

pub(crate) const TEMPO_LED: [usize; 2] = [11, 34];

const RING_TURNS_PER_SECOND: f64 = 100.0 / 3.0 / 60.0;

const METER_ON: u8 = 0xE4;
const CHANNEL_METER: usize = 15;
const MASTER_METER: [usize; 2] = [60, 69];
const MASTER_DB: [f32; 8] = [-15.0, -10.0, -7.0, -5.0, -3.0, -2.0, -1.0, 0.0];

pub(crate) fn meters(feedback: &Feedback) -> Meters {
    let mut m = vec![0u8; METER_REPORT_LEN];
    for (ch, c) in feedback.channels.iter().enumerate() {
        for i in 0..CHANNEL_METER {
            let threshold = if i == 0 {
                f32::EPSILON
            } else {
                i as f32 / CHANNEL_METER as f32
            };
            if c.level >= threshold {
                m[CHANNEL_METER * ch + i] = METER_ON;
            }
        }
    }
    for (side, &first) in MASTER_METER.iter().enumerate() {
        let level = feedback.master_level[side];
        let db = 20.0 * level.max(1e-9).log10();
        for (i, &at) in MASTER_DB.iter().enumerate() {
            if db >= at {
                m[first + i] = METER_ON;
            }
        }
        if level > 1.0 {
            m[first + MASTER_DB.len()] = METER_ON;
        }
    }
    Meters(m)
}

fn jog_ring(deck: &DeckFeedback, colour: u8, (touched, platter): (bool, u16)) -> JogRing {
    if !deck.loaded {
        return JogRing::default();
    }
    if deck.playing && deck.end_warning {
        return JogRing {
            mode: RingMode::Flash,
            position: 0,
            colour,
        };
    }
    let position = if deck.playing && !deck.turntable && !touched {
        let turns = deck.position_s.max(0.0) * RING_TURNS_PER_SECOND;
        (turns.fract() * f64::from(RING_STEPS)) as u16
    } else {
        platter
    };
    JogRing {
        mode: RingMode::Spot,
        position,
        colour,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PadsMode {
    #[default]
    Hotcues,
    Stems,
    SampleDeck,
    Pattern(u8),
    Loops,
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
struct S4State {
    layer: [usize; 2],
    shift: [bool; 2],
    grid: [bool; 2],
    pads: [PadsMode; DECKS],
    active_stem: [Option<usize>; DECKS],
    stems: [Option<[StemFeedback; STEMS]>; DECKS],
    sample_deck: [Option<SampleFeedback>; DECKS],
    active_slot: [Option<usize>; DECKS],
    mute: [bool; 2],
    browsing: [Browsing; DECKS],
    blink: bool,
    touched: [bool; 2],
    turntable: [bool; DECKS],
    tempo_relative: bool,
    tempo_prev: [Option<f32>; 2],
    click: [Option<PendingClick>; 2],
    platter: [u16; 2],
    shifted: super::ShiftFn,
    modifier_hooks: Option<super::ModifierHooks>,
    jog_toggle: bool,
    scratch_on_touch: [bool; 2],
    samples_is_stems: bool,
    samples_toggles: bool,
    fx_select_toggle: bool,
    fx_selected: Option<usize>,
    mixer_fx_on: [bool; DECKS],
    mx2: bool,
    shared_shift: bool,
    mode_held: bool,
    keylock_held: [bool; 2],
    keylock_engaged: [bool; 2],
    sync_engaged: [bool; 2],
    master_engaged: [bool; 2],
    stutter: [Vec<u8>; DECKS],
    source: crate::relative::Source,
    shown: Feedback,
    has_strip: bool,
    strip: crate::strip::Strip,
    /// A press the strip took, whose release it swallows too.
    strip_took: Option<Input>,
}

impl Default for S4State {
    fn default() -> Self {
        Self {
            layer: [0, 1],
            shift: [false; 2],
            grid: [false; 2],
            pads: [PadsMode::Hotcues; DECKS],
            active_stem: [None; DECKS],
            stems: [None; DECKS],
            sample_deck: [None; DECKS],
            active_slot: [None; DECKS],
            mute: [false; 2],
            browsing: [Browsing::Off; DECKS],
            blink: false,
            touched: [false; 2],
            platter: [0; 2],
            turntable: [false; DECKS],
            tempo_relative: true,
            tempo_prev: [None; 2],
            click: [None, None],
            shifted: super::shifted,
            modifier_hooks: None,
            jog_toggle: false,
            scratch_on_touch: [true; 2],
            samples_is_stems: false,
            samples_toggles: false,
            fx_select_toggle: false,
            fx_selected: None,
            mixer_fx_on: [false; DECKS],
            mx2: false,
            shared_shift: false,
            mode_held: false,
            keylock_held: [false; 2],
            keylock_engaged: [false; 2],
            sync_engaged: [false; 2],
            master_engaged: [false; 2],
            stutter: std::array::from_fn(|_| Vec::new()),
            source: crate::relative::Source::default(),
            shown: Feedback::default(),
            has_strip: false,
            strip: crate::strip::Strip::default(),
            strip_took: None,
        }
    }
}

impl S4State {
    fn for_layout(layout: &Layout) -> Self {
        Self {
            shifted: layout.shifted,
            modifier_hooks: layout.modifier_hooks,
            jog_toggle: layout.jog_button_toggles_touch,
            samples_is_stems: layout.samples_button_stems,
            samples_toggles: layout.samples_button_toggles,
            fx_select_toggle: layout.fx_select_toggles,
            mx2: layout.mx2_transport,
            shared_shift: layout.shared_shift,
            has_strip: layout.touch_strip.is_some(),
            ..Self::default()
        }
    }

    fn pads_mode(&self, deck: usize) -> PadsMode {
        match self.pads[deck] {
            PadsMode::Stems if self.stems[deck].is_some() => PadsMode::Stems,
            PadsMode::SampleDeck if self.sample_deck(deck).is_some() => PadsMode::SampleDeck,
            PadsMode::Pattern(page) if self.drum_machine(deck) => PadsMode::Pattern(page),
            PadsMode::Loops if !self.shown.decks[deck].live => PadsMode::Loops,
            _ => PadsMode::Hotcues,
        }
    }

    fn drop_unsupported_modes(&mut self) {
        for deck in 0..DECKS {
            let drop = match self.pads[deck] {
                PadsMode::Pattern(_) => !self.drum_machine(deck),
                PadsMode::Loops => self.shown.decks[deck].live,
                _ => false,
            };
            if drop {
                self.pads[deck] = PadsMode::Hotcues;
            }
        }
    }

    fn drum_machine(&self, deck: usize) -> bool {
        self.shown.fx.get(deck).is_some_and(|f| f.mode == 2)
    }

    fn sample_deck(&self, deck: usize) -> Option<usize> {
        let other = deck ^ 2;
        [deck, other]
            .into_iter()
            .find(|&d| self.sample_deck.get(d).is_some_and(Option::is_some))
    }
}

const STEM_ENCODER_STEP: f32 = 0.05;

fn stem_pads(
    state: &mut S4State,
    side: usize,
    control: &str,
    event: &InputEvent,
) -> Option<Vec<(Action, Trigger)>> {
    let deck = state.layer[side];
    let pressed = event.pressed == Some(true);
    match control {
        "Pad Mode.Hotcues" => {
            if pressed {
                state.pads[deck] = PadsMode::Hotcues;
            }
            return Some(Vec::new());
        }
        "Pad Mode.Stems" => {
            if pressed && state.stems[deck].is_some() {
                state.pads[deck] = PadsMode::Stems;
            }
            return Some(Vec::new());
        }
        "Pad Mode.Samples" if state.samples_is_stems && state.stems[deck].is_some() => {
            if pressed {
                state.pads[deck] = PadsMode::Stems;
            }
            return Some(Vec::new());
        }
        _ => {}
    }
    if state.pads_mode(deck) != PadsMode::Stems {
        state.active_stem[deck] = None;
        return None;
    }
    let stem = |part, trigger| {
        (
            Action::Stem {
                deck,
                stem: 0,
                part,
            },
            trigger,
        )
    };
    if let Some(pad) = control.strip_prefix("Pads.") {
        let pad = pad.parse::<usize>().ok()?.checked_sub(1)?;
        if pad < STEMS {
            return Some(if pressed {
                vec![with_stem(stem(StemPart::Mute, Trigger::Press), pad)]
            } else {
                Vec::new()
            });
        }
        state.active_stem[deck] = pressed.then_some(pad - STEMS);
        return Some(Vec::new());
    }
    let selected = state.active_stem[deck]?;
    if !matches!(
        control,
        "Loop.Turn" | "Loop.Push" | "Move.Turn" | "Move.Push"
    ) {
        return None;
    }
    if state.shift[side] {
        return Some(Vec::new());
    }
    let filter_on = state.stems[deck].is_some_and(|s| s[selected].filter_on);
    let steps = Trigger::Delta(event.value * STEM_ENCODER_STEP / crate::RELATIVE_STEP);
    let triggers = match control {
        "Move.Turn" => vec![stem(StemPart::Volume, steps)],
        "Move.Push" if pressed => vec![stem(StemPart::Volume, Trigger::Set(1.0))],
        "Loop.Turn" => vec![
            stem(StemPart::Filter, steps),
            stem(StemPart::FilterOn, Trigger::Set(1.0)),
        ],
        "Loop.Push" if pressed => vec![if filter_on {
            stem(StemPart::Filter, Trigger::Set(0.5))
        } else {
            stem(StemPart::FilterOn, Trigger::Set(1.0))
        }],
        _ => Vec::new(),
    };
    Some(
        triggers
            .into_iter()
            .map(|t| with_stem(t, selected))
            .collect(),
    )
}

fn z1_mode(state: &mut S4State, name: &str, event: &InputEvent) -> Option<Vec<(Action, Trigger)>> {
    let pressed = event.pressed == Some(true);
    if name == "Mixer.Mode" {
        state.mode_held = pressed;
        return Some(Vec::new());
    }
    if !state.mode_held {
        return None;
    }
    let channel = name.strip_prefix("Mixer.")?.strip_suffix(".Cue")?;
    let deck = usize::from(channel.as_bytes().first()?.checked_sub(b'A')?);
    Some(if pressed {
        vec![(Action::PlayPause { deck }, Trigger::Press)]
    } else {
        Vec::new()
    })
}

fn side_functions(
    state: &mut S4State,
    side: usize,
    control: &str,
    event: &InputEvent,
) -> Option<Vec<(Action, Trigger)>> {
    mx2_transport(state, side, control, event)
        .or_else(|| pattern_and_loop_pads(state, side, control, event))
        .or_else(|| stem_pads(state, side, control, event))
        .or_else(|| sample_pads(state, side, control, event))
}

fn stutter_beats(pad: u8) -> f64 {
    2f64.powi(i32::from(pad) - 5)
}

fn pattern_and_loop_pads(
    state: &mut S4State,
    side: usize,
    control: &str,
    event: &InputEvent,
) -> Option<Vec<(Action, Trigger)>> {
    let deck = state.layer[side];
    let pressed = event.pressed == Some(true);
    match control {
        "Pad Mode.Pattern" => {
            if !state.drum_machine(deck) {
                return Some(Vec::new());
            }
            if state.shift[side] {
                return None;
            }
            if pressed {
                state.pads[deck] = match state.pads[deck] {
                    PadsMode::Pattern(0) => PadsMode::Pattern(1),
                    _ => PadsMode::Pattern(0),
                };
            }
            return Some(Vec::new());
        }
        "Pad Mode.Loops" => {
            if pressed && !state.shown.decks[deck].live {
                state.pads[deck] = PadsMode::Loops;
            }
            return Some(Vec::new());
        }
        _ => {}
    }
    let pad = control
        .strip_prefix("Pads.")?
        .parse::<u8>()
        .ok()?
        .checked_sub(1)?;
    match state.pads_mode(deck) {
        PadsMode::Pattern(page) => Some(if pressed {
            let step = usize::from(page) * 8 + usize::from(pad);
            vec![(Action::PatternStep { unit: deck, step }, Trigger::Press)]
        } else {
            Vec::new()
        }),
        PadsMode::Loops => {
            let held = &mut state.stutter[deck];
            let before = held.last().copied();
            held.retain(|&p| p != pad);
            if pressed {
                held.push(pad);
            }
            let after = held.last().copied();
            Some(match (before, after) {
                (_, Some(top)) if before != after => vec![(
                    Action::SlipRoll {
                        deck,
                        beats: stutter_beats(top),
                    },
                    Trigger::Press,
                )],
                (Some(top), None) => vec![(
                    Action::SlipRoll {
                        deck,
                        beats: stutter_beats(top),
                    },
                    Trigger::Release,
                )],
                _ => Vec::new(),
            })
        }
        _ => None,
    }
}

fn mx2_transport(
    state: &mut S4State,
    side: usize,
    control: &str,
    event: &InputEvent,
) -> Option<Vec<(Action, Trigger)>> {
    if !state.mx2 {
        return None;
    }
    let deck = state.layer[side];
    let pressed = event.pressed == Some(true);
    if state.shift[side]
        && let Some(button) = control.strip_prefix("FX.Button ")
    {
        let mode = state.shown.fx.get(deck).map_or(0, |f| f.mode);
        return Some(if pressed {
            mx2_shift_fx(deck, mode, button).into_iter().collect()
        } else {
            Vec::new()
        });
    }
    let press = |action| vec![(action, Trigger::Press)];
    Some(match control {
        "Keylock" => {
            if pressed {
                state.keylock_held[side] = true;
                state.keylock_engaged[side] = true;
                Vec::new()
            } else {
                state.keylock_held[side] = false;
                if state.keylock_engaged[side] {
                    press(Action::Keylock { deck })
                } else {
                    Vec::new()
                }
            }
        }
        "Loop.Turn" | "Move.Turn" | "Loop.Push" | "Move.Push" if state.keylock_held[side] => {
            let turned = control.ends_with(".Turn");
            if turned || pressed {
                state.keylock_engaged[side] = false;
            }
            if !state.shown.decks[deck].keylock {
                return Some(Vec::new());
            }
            match (turned, pressed) {
                (true, _) => vec![(
                    Action::KeyAdjust { deck },
                    Trigger::Delta(event.value.round()),
                )],
                (false, true) => vec![(Action::KeyAdjust { deck }, Trigger::Set(0.5))],
                _ => Vec::new(),
            }
        }
        "Sync" | "Master" => {
            let sync = control == "Sync";
            let (mine, other) = if sync {
                (
                    &mut state.sync_engaged[side],
                    &mut state.master_engaged[side],
                )
            } else {
                (
                    &mut state.master_engaged[side],
                    &mut state.sync_engaged[side],
                )
            };
            if pressed {
                if *other {
                    *other = false;
                    press(Action::TempoReset { deck })
                } else {
                    *mine = true;
                    Vec::new()
                }
            } else if !std::mem::take(mine) {
                Vec::new()
            } else if sync {
                press(Action::Sync { deck })
            } else if state.shown.decks[deck].master {
                Vec::new()
            } else {
                press(Action::Master { deck })
            }
        }
        _ => return None,
    })
}

fn mx2_shift_fx(unit: usize, mode: u8, button: &str) -> Option<(Action, Trigger)> {
    let step = |action, by: f32| Some((action, Trigger::Delta(by)));
    match (mode, button) {
        (_, "1") => step(Action::FxMode { unit }, 1.0),
        (1, "2") => step(Action::FxSelect { unit, slot: 0 }, -1.0),
        (1, "3") | (0, "2") => step(Action::FxSelect { unit, slot: 0 }, 1.0),
        (0, "3") => step(Action::FxSelect { unit, slot: 1 }, 1.0),
        (0, "4") => step(Action::FxSelect { unit, slot: 2 }, 1.0),
        (2, "2") => step(Action::PatternKit { unit }, -1.0),
        (2, "3") => step(Action::PatternKit { unit }, 1.0),
        _ => None,
    }
}

fn pad_leds(
    leds: &mut Leds,
    d: &Decoder,
    state: &S4State,
    feedback: &Feedback,
    (i, side): (usize, &str),
    deck_colour: u8,
) {
    let deck = state.layer[i];
    let s = &feedback.decks[deck];
    let n = |c: &str| format!("{side}.{c}");
    let mode = state.pads_mode(deck);
    leds.set(
        d,
        &n("Pad Mode.Hotcues"),
        deck_colour,
        mode == PadsMode::Hotcues,
    );
    if s.stems.is_some() {
        leds.set(
            d,
            &n("Pad Mode.Stems"),
            deck_colour,
            mode == PadsMode::Stems,
        );
    }
    if state.sample_deck(deck).is_some() {
        leds.set(
            d,
            &n("Pad Mode.Samples"),
            deck_colour,
            mode == PadsMode::SampleDeck,
        );
    }
    if state.drum_machine(deck) {
        let pattern_colour = if mode == PadsMode::Pattern(1) {
            colour::WHITE
        } else {
            colour::BLUE
        };
        let on = matches!(mode, PadsMode::Pattern(_));
        leds.set(d, &n("Pad Mode.Pattern"), pattern_colour, on);
    }
    if !s.live {
        leds.set(
            d,
            &n("Pad Mode.Loops"),
            colour::BLUE,
            mode == PadsMode::Loops,
        );
    }
    let sample_deck = state
        .sample_deck(deck)
        .and_then(|t| Some((t, feedback.decks[t].sample_deck?)));
    match (mode, s.stems) {
        (PadsMode::Pattern(page), _) => {
            let fx = feedback.fx.get(deck).copied().unwrap_or_default();
            let playing = fx.playing_sixteenth();
            for k in 0..8u8 {
                let step = page * 8 + k;
                let colour = if playing == Some(step) {
                    colour::ORANGE
                } else {
                    colour::MINT
                };
                let on = fx.pattern_steps & (1 << step) != 0;
                leds.set(d, &n(&format!("Pads.{}", k + 1)), colour, on);
            }
        }
        (PadsMode::Loops, _) => {
            let top = state.stutter[deck].last().copied();
            for k in 0..8u8 {
                leds.set(
                    d,
                    &n(&format!("Pads.{}", k + 1)),
                    colour::GREEN,
                    top == Some(k),
                );
            }
        }
        (PadsMode::SampleDeck, _) if sample_deck.is_some() => {
            if let Some((target, r)) = sample_deck {
                sample_pad_leds(leds, d, state, (i, side), target, &r);
                leds.set(d, &n("Pad Mode.Record"), deck_colour, r.recording);
            }
        }
        (PadsMode::Stems, Some(stems)) => {
            for (k, stem) in stems.iter().enumerate() {
                let colour = colour::nearest(stem.colour);
                leds.set(d, &n(&format!("Pads.{}", k + 1)), colour, !stem.muted);
                let held = state.active_stem[deck] == Some(k);
                leds.set(d, &n(&format!("Pads.{}", k + 5)), colour, held);
            }
        }
        _ => {
            for (slot, &kind) in s.hotcues.iter().enumerate() {
                let pad = n(&format!("Pads.{}", slot + 1));
                leds.set(d, &pad, hotcue_colour(kind), kind != HotcueKind::Empty);
            }
        }
    }
}

const BROWSING_MS: u64 = 500;
const TURN_BROWSING_MS: u64 = 500 + BROWSING_MS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Browsing {
    Off,
    Held,
    Until(std::time::Instant),
}

impl Browsing {
    fn on(self, now: std::time::Instant) -> bool {
        match self {
            Self::Off => false,
            Self::Held => true,
            Self::Until(t) => now < t,
        }
    }
}

const BLINK_MS: u128 = 150;

fn blink_on(now: std::time::Instant) -> bool {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = *START.get_or_init(|| now);
    now.saturating_duration_since(start).as_millis() / BLINK_MS % 2 == 1
}

const SLOT_ENCODER_STEP: f32 = 0.025;

fn sample_pads(
    state: &mut S4State,
    side: usize,
    control: &str,
    event: &InputEvent,
) -> Option<Vec<(Action, Trigger)>> {
    let deck = state.layer[side];
    let pressed = event.pressed == Some(true);
    match control {
        "Pad Mode.Samples" => {
            if pressed {
                if state.samples_toggles && state.pads[deck] == PadsMode::SampleDeck {
                    state.pads[deck] = PadsMode::Hotcues;
                } else if state.sample_deck(deck).is_some() {
                    state.pads[deck] = PadsMode::SampleDeck;
                }
            }
            return Some(Vec::new());
        }
        "Pad Mode.Mute" => {
            state.mute[side] = pressed;
            return Some(Vec::new());
        }
        _ => {}
    }
    if state.pads_mode(deck) != PadsMode::SampleDeck {
        state.active_slot[deck] = None;
        return None;
    }
    let target = state.sample_deck(deck)?;
    let shift = state.shift[side];
    let slot_action = |slot, part, trigger| {
        (
            Action::SampleSlot {
                deck: target,
                slot,
                part,
            },
            trigger,
        )
    };
    let mixer = |slot, part, trigger| {
        (
            Action::Stem {
                deck: target,
                stem: slot,
                part,
            },
            trigger,
        )
    };
    if let Some(pad) = control.strip_prefix("Pads.") {
        let pad = pad.parse::<usize>().ok()?.checked_sub(1)?;
        if pad >= 4 {
            state.active_slot[target] = pressed.then_some(pad - 4);
            return Some(Vec::new());
        }
        let trigger = if pressed {
            Trigger::Press
        } else {
            Trigger::Release
        };
        return Some(if state.mute[side] {
            if pressed {
                vec![mixer(pad, StemPart::Mute, Trigger::Press)]
            } else {
                Vec::new()
            }
        } else if shift {
            if pressed {
                vec![
                    slot_action(pad, SampleSlotPart::StopSelected, Trigger::Press),
                    slot_action(pad, SampleSlotPart::DeletePattern, Trigger::Press),
                ]
            } else {
                Vec::new()
            }
        } else {
            vec![slot_action(pad, SampleSlotPart::TriggerSelected, trigger)]
        });
    }
    if control == "Pad Mode.Record" && !shift {
        let deck_action = |part, trigger| (Action::SampleDeck { deck: target, part }, trigger);
        return Some(if pressed {
            vec![
                deck_action(SamplePart::SequencerRecord, Trigger::Press),
                deck_action(SamplePart::SequencerOn, Trigger::Set(1.0)),
            ]
        } else {
            Vec::new()
        });
    }
    sample_encoders(state, side, target, control, event)
}

fn sample_encoders(
    state: &mut S4State,
    side: usize,
    target: usize,
    control: &str,
    event: &InputEvent,
) -> Option<Vec<(Action, Trigger)>> {
    let pressed = event.pressed == Some(true);
    let shift = state.shift[side];
    let slot_action = |slot, part, trigger| {
        (
            Action::SampleSlot {
                deck: target,
                slot,
                part,
            },
            trigger,
        )
    };
    let mixer = |slot, part, trigger| {
        (
            Action::Stem {
                deck: target,
                stem: slot,
                part,
            },
            trigger,
        )
    };
    let encoders = ["Loop.Turn", "Loop.Push", "Move.Turn", "Move.Push"];
    let Some(slot) = state.active_slot[target] else {
        if !shift || !control.starts_with("Loop.") {
            return None;
        }
        let deck_action = |part, trigger| (Action::SampleDeck { deck: target, part }, trigger);
        return Some(match control {
            "Loop.Push" if pressed => vec![deck_action(SamplePart::QuantizeOn, Trigger::Press)],
            "Loop.Turn" => vec![deck_action(
                SamplePart::QuantizeSelector,
                Trigger::Delta(event.value),
            )],
            _ => Vec::new(),
        });
    };
    if !encoders.contains(&control) && !control.starts_with("Browse.") {
        return None;
    }
    let later =
        |ms| Browsing::Until(std::time::Instant::now() + std::time::Duration::from_millis(ms));
    match control {
        "Browse.Encoder.Push" if pressed => state.browsing[target] = Browsing::Held,
        "Browse.Encoder.Push" => state.browsing[target] = later(BROWSING_MS),
        "Browse.Encoder.Turn" if state.browsing[target] != Browsing::Held => {
            state.browsing[target] = later(TURN_BROWSING_MS);
        }
        _ => {}
    }
    if shift {
        return Some(Vec::new());
    }
    let filter_on = state.sample_deck[target].is_some_and(|r| r.slots[slot].filter_on);
    let steps = Trigger::Delta(event.value * SLOT_ENCODER_STEP / crate::RELATIVE_STEP);
    Some(match control {
        "Move.Turn" => vec![mixer(slot, StemPart::Volume, steps)],
        "Move.Push" if pressed => vec![mixer(slot, StemPart::Volume, Trigger::Set(1.0))],
        "Loop.Turn" => vec![
            mixer(slot, StemPart::Filter, steps),
            mixer(slot, StemPart::FilterOn, Trigger::Set(1.0)),
        ],
        "Loop.Push" if pressed => vec![if filter_on {
            mixer(slot, StemPart::Filter, Trigger::Set(0.5))
        } else {
            mixer(slot, StemPart::FilterOn, Trigger::Set(1.0))
        }],
        "Browse.Encoder.Turn" => vec![slot_action(
            slot,
            SampleSlotPart::SelectedCell,
            Trigger::Delta(event.value),
        )],
        _ => Vec::new(),
    })
}

fn sample_pad_leds(
    leds: &mut Leds,
    d: &Decoder,
    state: &S4State,
    (side, name): (usize, &str),
    target: usize,
    sample_deck: &SampleFeedback,
) {
    let n = |pad: usize| format!("{name}.Pads.{pad}");
    let deck_colour = if target >= 2 {
        colour::ORANGE
    } else {
        colour::BLUE
    };
    for (k, slot) in sample_deck.slots.iter().enumerate() {
        if state.mute[side] {
            leds.set(d, &n(k + 1), colour::WHITE, !slot.muted);
        } else {
            let colour = slot.selected_colour.map_or(deck_colour, colour::nearest);
            match slot.cells[usize::from(slot.selected) % 16] {
                0 => leds.set(d, &n(k + 1), colour::OFF, false),
                2 => leds.set(d, &n(k + 1), colour, true),
                3 => leds.set(d, &n(k + 1), colour, state.blink),
                _ => leds.set(d, &n(k + 1), colour, false),
            }
        }
        let held = state.active_slot[target] == Some(k);
        leds.set(d, &n(k + 5), colour::WHITE, held);
    }
}

fn with_stem((action, trigger): (Action, Trigger), n: usize) -> (Action, Trigger) {
    match action {
        Action::Stem { deck, part, .. } => (
            Action::Stem {
                deck,
                stem: n,
                part,
            },
            trigger,
        ),
        other => (other, trigger),
    }
}

fn map_table(state: &mut S4State, set: &MappingSet, event: &InputEvent) -> Vec<(Action, Trigger)> {
    set.map_source(event, &mut state.source, &state.shown, &|_| None)
}

fn translate(state: &mut S4State, set: &MappingSet, event: &InputEvent) -> Vec<(Action, Trigger)> {
    if !state.has_strip {
        return translate_controls(state, set, event);
    }
    if let Some(triggers) = strip_input(state, set, event) {
        return triggers;
    }
    let triggers = translate_controls(state, set, event);
    strip_takes(state, event, triggers)
}

/// The strip's own inputs, and the release of a press it took.
fn strip_input(
    state: &mut S4State,
    set: &MappingSet,
    event: &InputEvent,
) -> Option<Vec<(Action, Trigger)>> {
    use crate::s4mk3::{
        STRIP_ANY_POSITION, STRIP_ANY_TOUCH, STRIP_HALF_POSITION, STRIP_TOUCH, STRIP_TURN,
    };
    if event.pressed == Some(false) && state.strip_took.as_ref() == Some(&event.input) {
        state.strip_took = None;
        return Some(Vec::new());
    }
    let Input::Hid(name) = &event.input else {
        return None;
    };
    if set.overrides(&event.input) {
        return None;
    }
    let name = name.as_str();
    let side_of = |names: [&str; 2]| names.iter().position(|&n| n == name);
    let mapped_deck = |state: &mut S4State| {
        map_table(state, set, event)
            .into_iter()
            .find_map(|(mut a, _)| a.deck_mut().map(|d| *d))
    };
    if name == STRIP_ANY_TOUCH {
        Some(state.strip.touch(event.pressed == Some(true)))
    } else if name == STRIP_ANY_POSITION {
        Some(state.strip.position(event.value))
    } else if let Some(side) = side_of(STRIP_HALF_POSITION) {
        Some(state.strip.half_position(side, event.value))
    } else if let Some(side) = side_of(STRIP_TOUCH) {
        let Some(deck) = mapped_deck(state) else {
            return Some(Vec::new());
        };
        let playing = state.shown.decks.get(deck).is_some_and(|d| d.playing);
        let down = event.pressed == Some(true);
        Some(
            state
                .strip
                .half_touch(side, deck, down, state.shift[side], playing),
        )
    } else if let Some(side) = side_of(STRIP_TURN) {
        let Some(deck) = mapped_deck(state) else {
            return Some(Vec::new());
        };
        Some(state.strip.half_turn(side, deck, event.value))
    } else {
        None
    }
}

/// A press of a loop or effect control that hands the strip over or back.
fn strip_takes(
    state: &mut S4State,
    event: &InputEvent,
    triggers: Vec<(Action, Trigger)>,
) -> Vec<(Action, Trigger)> {
    use crate::strip::Target;
    if event.pressed != Some(true) || state.shift.contains(&true) {
        return triggers;
    }
    let target = triggers.iter().find_map(|&(a, _)| match a {
        Action::FastLoop { deck }
        | Action::LoopActive { deck }
        | Action::FastLoopSize { deck, .. }
        | Action::LoopSize { deck, .. } => Some(Target::Loop { deck }),
        Action::FxButton { unit, button } => Some(Target::Fx { unit, slot: button }),
        _ => None,
    });
    match target.and_then(|t| state.strip.press(t)) {
        Some(out) => {
            state.strip_took = Some(event.input.clone());
            out
        }
        None => triggers,
    }
}

#[allow(clippy::too_many_lines)]
fn translate_controls(
    state: &mut S4State,
    set: &MappingSet,
    event: &InputEvent,
) -> Vec<(Action, Trigger)> {
    let Input::Hid(name) = &event.input else {
        return map_table(state, set, event);
    };
    if set.overrides(&event.input) {
        return map_table(state, set, event);
    }
    let side = if name.starts_with("Left.") {
        Some(0)
    } else if name.starts_with("Right.") {
        Some(1)
    } else {
        None
    };
    if let Some(side) = side {
        let control = &name[name.find('.').map_or(0, |i| i + 1)..];
        match control {
            "Shift" => {
                let held = event.pressed == Some(true);
                if state.shared_shift {
                    state.shift = [held; 2];
                } else {
                    state.shift[side] = held;
                }
                return Vec::new();
            }
            "Jog Mode.Grid" => {
                state.grid[side] = event.pressed == Some(true);
                return Vec::new();
            }
            "Jog Mode.Jog" if state.jog_toggle => {
                if event.pressed == Some(true) {
                    state.scratch_on_touch[side] = !state.scratch_on_touch[side];
                }
                return Vec::new();
            }
            "Jog.Touch" => {
                state.touched[side] = event.pressed == Some(true);
                if !state.scratch_on_touch[side] {
                    return Vec::new();
                }
            }
            "Jog.Turn" => {
                let steps = i32::from(RING_STEPS);
                let at = (i32::from(state.platter[side]) + event.value as i32).rem_euclid(steps);
                state.platter[side] = u16::try_from(at).unwrap_or(0);
            }
            _ => {}
        }
        if let Some(triggers) = side_functions(state, side, control, event) {
            return triggers;
        }
    }
    if let Some(triggers) = z1_mode(state, name, event) {
        return triggers;
    }
    if state.fx_select_toggle
        && let Some(slot) = name.strip_prefix("Mixer.Mixer FX Select.")
    {
        return fx_select_toggle(state, slot, event.pressed == Some(true));
    }
    let shift_held = match side {
        Some(side) => state.shift[side],
        None => state.shift.contains(&true),
    };
    let grid = side.is_some_and(|side| state.grid[side]) && name.ends_with(".Jog.Turn");
    let replaced = if grid {
        Some(Some((
            Action::GridAdjust {
                deck: side.unwrap_or(0),
            },
            Interaction::Relative,
        )))
    } else if shift_held {
        match state.modifier_hooks {
            Some(hooks) => (hooks.shifted)(name, &state.source.modifiers),
            None => (state.shifted)(name),
        }
    } else {
        None
    };
    let triggers = match replaced {
        None => map_table(state, set, event),
        Some(None) => Vec::new(),
        Some(Some((action, interaction))) => {
            let mapping = Mapping::new(event.input.clone(), action, interaction);
            trigger(&mapping, event)
                .map(|t| (action, t))
                .into_iter()
                .collect()
        }
    };
    let triggers = match side {
        Some(side) if name.ends_with(".Tempo") && !shift_held => tempo_fader(state, side, triggers),
        Some(side) if name.ends_with(".Tempo") => tempo_fader_shifted(state, set, event, side),
        _ => triggers,
    };
    let mut triggers = apply_layer(&mut state.layer, event, triggers);
    if let Some(side) = side
        && name.ends_with(".Browse.Encoder.Push")
        && !shift_held
    {
        let now = std::time::Instant::now();
        return browse_click(state, side, event.pressed == Some(true), triggers, now);
    }
    triggers.retain(|(a, _)| match a {
        Action::TempoLock { deck } => !state.turntable.get(*deck).copied().unwrap_or(false),
        _ => true,
    });
    triggers
}

fn fx_select_toggle(state: &mut S4State, slot: &str, pressed: bool) -> Vec<(Action, Trigger)> {
    let slot = match slot {
        "Filter" => 0,
        n => match n.parse::<usize>() {
            Ok(n) if (1..=4).contains(&n) => n,
            _ => return Vec::new(),
        },
    };
    if !pressed {
        return Vec::new();
    }
    let mut out = Vec::new();
    let on_off = |out: &mut Vec<(Action, Trigger)>, channel| {
        out.push((Action::MixerFxOn { channel }, Trigger::Press));
        out.push((Action::MixerFxOn { channel }, Trigger::Release));
    };
    if state.fx_selected == Some(slot) {
        state.fx_selected = None;
        for channel in 0..2 {
            if state.mixer_fx_on[channel] {
                on_off(&mut out, channel);
            }
        }
        return out;
    }
    state.fx_selected = Some(slot);
    out.push((Action::MixerFxSelect { slot }, Trigger::Press));
    out.push((Action::MixerFxSelect { slot }, Trigger::Release));
    for channel in 0..2 {
        if !state.mixer_fx_on[channel] {
            on_off(&mut out, channel);
        }
    }
    out
}

type PendingClick = (std::time::Instant, Vec<(Action, Trigger)>);

const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(401);

fn browse_click(
    state: &mut S4State,
    side: usize,
    pressed: bool,
    alone: Vec<(Action, Trigger)>,
    now: std::time::Instant,
) -> Vec<(Action, Trigger)> {
    if !pressed {
        return Vec::new();
    }
    match state.click[side].take() {
        Some((at, _)) if now.duration_since(at) <= DOUBLE_CLICK => {
            let deck = state.layer[side];
            let mode = crate::LoadMode::Alternative;
            vec![(Action::LoadSelectedAs { deck, mode }, Trigger::Press)]
        }
        _ => {
            state.click[side] = Some((now, alone));
            Vec::new()
        }
    }
}

fn click_due(state: &mut S4State, now: std::time::Instant) -> Vec<(Action, Trigger)> {
    let mut out = Vec::new();
    for click in &mut state.click {
        if click
            .as_ref()
            .is_some_and(|(at, _)| now.duration_since(*at) > DOUBLE_CLICK)
            && let Some((_, triggers)) = click.take()
        {
            out.extend(triggers);
        }
    }
    out
}

fn tempo_fader(
    state: &mut S4State,
    side: usize,
    triggers: Vec<(Action, Trigger)>,
) -> Vec<(Action, Trigger)> {
    triggers
        .into_iter()
        .filter_map(|(action, t)| match (action, t) {
            (Action::Tempo { deck }, Trigger::Set(v)) if state.tempo_relative => {
                let before = state.tempo_prev[side].replace(v)?;
                let moved = v - before;
                (moved != 0.0).then_some((
                    Action::TempoRelative { deck },
                    Trigger::Delta(moved * 200.0),
                ))
            }
            (Action::Tempo { .. }, Trigger::Set(v)) => {
                state.tempo_prev[side] = None;
                Some((action, Trigger::Set(dead_zone(v))))
            }
            other => Some(other),
        })
        .collect()
}

fn tempo_fader_shifted(
    state: &mut S4State,
    set: &MappingSet,
    event: &InputEvent,
    side: usize,
) -> Vec<(Action, Trigger)> {
    for (action, t) in map_table(state, set, event) {
        if let (Action::Tempo { .. }, Trigger::Set(v)) = (action, t)
            && state.tempo_relative
        {
            state.tempo_prev[side] = Some(v);
        }
    }
    Vec::new()
}

fn dead_zone(v: f32) -> f32 {
    const LOW: f32 = 0.495;
    const HIGH: f32 = 0.505;
    if v < LOW {
        (v / LOW * 0.5).max(0.0)
    } else if v > HIGH {
        (0.5 + (v - HIGH) / (1.0 - HIGH) * 0.5).min(1.0)
    } else {
        0.5
    }
}

fn apply_layer(
    layer: &mut [usize; 2],
    event: &InputEvent,
    mut triggers: Vec<(Action, Trigger)>,
) -> Vec<(Action, Trigger)> {
    let Input::Hid(name) = &event.input else {
        return triggers;
    };
    let select = match name.as_str() {
        "Left.Deck.A" => Some((0, 0)),
        "Left.Deck.C" => Some((0, 2)),
        "Right.Deck.B" => Some((1, 1)),
        "Right.Deck.D" => Some((1, 3)),
        "Left.Deck Select" => Some((0, layer[0] ^ 2)),
        "Right.Deck Select" => Some((1, layer[1] ^ 2)),
        _ => None,
    };
    if let Some((side, deck)) = select {
        if event.pressed == Some(true) {
            layer[side] = deck;
        }
        return Vec::new();
    }
    let side = if name.starts_with("Left.") {
        0
    } else if name.starts_with("Right.") {
        1
    } else {
        return triggers;
    };
    for (action, _) in &mut triggers {
        if let Some(deck) = action.deck_mut()
            && *deck == side
        {
            *deck = layer[side];
        }
    }
    triggers
}

fn hotcue_colour(kind: HotcueKind) -> u8 {
    match kind {
        HotcueKind::Empty => colour::OFF,
        HotcueKind::Cue => colour::BLUE,
        HotcueKind::FadeIn | HotcueKind::FadeOut => colour::ORANGE,
        HotcueKind::Load => colour::YELLOW,
        HotcueKind::Grid => colour::WHITE,
        HotcueKind::Loop => colour::GREEN,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn meters_light_from_the_bottom_with_the_level() {
        let mut f = Feedback::default();
        assert!(meters(&f).0.iter().all(|&b| b == 0), "silence: all off");
        f.channels[1].level = 0.3;
        let m = meters(&f).0;
        let lit: Vec<usize> = (0..METER_REPORT_LEN).filter(|&i| m[i] != 0).collect();
        assert_eq!(lit, (15..20).collect::<Vec<_>>());
        assert!(lit.iter().all(|&i| m[i] == METER_ON));
        f.channels[3].level = 1.0;
        assert!(meters(&f).0[45..60].iter().all(|&b| b == METER_ON));
        f.master_level = [0.5, 1.2];
        let m = meters(&f).0;
        assert_eq!(
            &m[60..69],
            &[METER_ON, METER_ON, METER_ON, 0, 0, 0, 0, 0, 0]
        );
        assert!(m[69..78].iter().all(|&b| b == METER_ON));
    }

    use super::*;

    fn hid(name: &str, pressed: Option<bool>) -> InputEvent {
        InputEvent {
            input: Input::Hid(name.into()),
            value: 1.0,
            raw: 0,
            pressed,
        }
    }

    fn s4_set() -> MappingSet {
        MappingSet {
            mappings: crate::s4mk3::default_mappings(),
            midi_inputs: Vec::new(),
            ..MappingSet::default()
        }
    }

    fn turn(name: &str, steps: f32) -> InputEvent {
        InputEvent {
            input: Input::Hid(name.into()),
            value: steps,
            raw: 0,
            pressed: None,
        }
    }

    fn fader(name: &str, v: f32) -> InputEvent {
        InputEvent {
            input: Input::Hid(name.into()),
            value: v,
            raw: 0,
            pressed: None,
        }
    }

    #[test]
    fn tempo_fader_is_relative_or_absolute_with_a_dead_zone() {
        let set = s4_set();
        let mut st = S4State::default();
        let go = |st: &mut S4State, v| translate(st, &set, &fader("Left.Tempo", v));
        assert_eq!(go(&mut st, 0.3), []);
        match go(&mut st, 0.35)[..] {
            [(Action::TempoRelative { deck: 0 }, Trigger::Delta(d))] => {
                assert!((d.abs() - 10.0).abs() < 1e-3, "{d}");
            }
            ref other => panic!("{other:?}"),
        }
        st.tempo_relative = false;
        match go(&mut st, 0.5)[..] {
            [(Action::Tempo { deck: 0 }, Trigger::Set(v))] => assert!((v - 0.5).abs() < 1e-6),
            ref other => panic!("{other:?}"),
        }
        assert!((dead_zone(0.503) - 0.5).abs() < 1e-6);
        assert!((dead_zone(0.0)).abs() < 1e-6);
        assert!((dead_zone(1.0) - 1.0).abs() < 1e-6);
        assert!((dead_zone(0.2475) - 0.25).abs() < 1e-4);
    }

    #[test]
    fn shift_moves_the_tempo_fader_without_changing_the_tempo() {
        let set = s4_set();
        let mut st = S4State::default();
        let go = |st: &mut S4State, e: InputEvent| translate(st, &set, &e);
        go(&mut st, fader("Left.Tempo", 0.5));
        assert_eq!(go(&mut st, hid("Left.Shift", Some(true))), []);
        assert_eq!(go(&mut st, fader("Left.Tempo", 0.8)), []);
        assert_eq!(go(&mut st, fader("Left.Tempo", 1.0)), []);
        go(&mut st, hid("Left.Shift", Some(false)));
        match go(&mut st, fader("Left.Tempo", 0.95))[..] {
            [(Action::TempoRelative { deck: 0 }, Trigger::Delta(d))] => {
                assert!((d.abs() - 10.0).abs() < 1e-3, "{d}");
            }
            ref other => panic!("{other:?}"),
        }
        st.tempo_relative = false;
        let before = st.tempo_prev[0];
        go(&mut st, hid("Left.Shift", Some(true)));
        assert_eq!(go(&mut st, fader("Left.Tempo", 0.2)), []);
        assert_eq!(st.tempo_prev[0], before);
    }

    #[test]
    fn browse_push_waits_for_a_double_click() {
        let mut st = S4State::default();
        let t = std::time::Instant::now();
        let ms = |n| t + std::time::Duration::from_millis(n);
        let load = vec![(Action::LoadSelected { deck: 0 }, Trigger::Press)];
        assert_eq!(browse_click(&mut st, 0, true, load.clone(), t), []);
        assert_eq!(browse_click(&mut st, 0, false, Vec::new(), ms(80)), []);
        assert_eq!(click_due(&mut st, ms(300)), []);
        assert_eq!(click_due(&mut st, ms(402)), load);
        browse_click(&mut st, 0, true, load.clone(), ms(1000));
        assert_eq!(
            browse_click(&mut st, 0, true, load.clone(), ms(1250)),
            [(
                Action::LoadSelectedAs {
                    deck: 0,
                    mode: crate::LoadMode::Alternative
                },
                Trigger::Press
            )]
        );
        assert_eq!(click_due(&mut st, ms(2000)), []);
    }

    #[test]
    fn tempo_led_lights_at_zero_and_red_when_locked() {
        let d = Decoder::default();
        let st = S4State::default();
        let mut f = Feedback::default();
        f.decks[0].loaded = true;
        f.decks[0].tempo = 1.0;
        assert_eq!(
            leds(&d, &st, &f).0[TEMPO_LED[0]],
            colour::BLUE + colour::BRIGHT
        );
        f.decks[0].tempo = 1.02;
        assert_eq!(leds(&d, &st, &f).0[TEMPO_LED[0]], colour::OFF);
        f.decks[0].tempo_locked = true;
        assert_eq!(
            leds(&d, &st, &f).0[TEMPO_LED[0]],
            colour::RED + colour::BRIGHT
        );
    }

    #[test]
    fn shift_switches_to_shifted_functions_on_its_side() {
        let set = s4_set();
        let mut st = S4State::default();
        let go = |st: &mut S4State, e: InputEvent| translate(st, &set, &e);
        assert_eq!(
            go(&mut st, hid("Left.Pads.3", Some(true))),
            [(Action::Hotcue { deck: 0, slot: 2 }, Trigger::Press)]
        );
        assert_eq!(go(&mut st, hid("Left.Shift", Some(true))), []);
        assert_eq!(
            go(&mut st, hid("Left.Pads.3", Some(true))),
            [(Action::DeleteHotcue { deck: 0, slot: 2 }, Trigger::Press)]
        );
        assert_eq!(
            go(&mut st, hid("Left.Cue", Some(true))),
            [(Action::ReturnToStart { deck: 0 }, Trigger::Press)]
        );
        assert_eq!(go(&mut st, hid("Left.Cue", Some(false))), []);
        assert_eq!(
            go(&mut st, hid("Left.Sync", Some(true))),
            [(Action::TempoLock { deck: 0 }, Trigger::Press)],
            "tempo lock"
        );
        st.turntable[0] = true;
        assert_eq!(
            go(&mut st, hid("Left.Sync", Some(true))),
            [],
            "no tempo lock in TT mode"
        );
        st.turntable[0] = false;
        assert_eq!(
            go(&mut st, turn("Left.Move.Turn", -2.0)),
            [(
                Action::BeatJump {
                    deck: 0,
                    beats: 1.0
                },
                Trigger::Delta(-2.0)
            )]
        );
        assert_eq!(
            go(&mut st, turn("Left.Browse.Encoder.Turn", 1.0)),
            [(Action::BrowserTree, Trigger::Delta(1.0))]
        );
        assert_eq!(
            go(&mut st, hid("Mixer.Quant", Some(true))),
            [(Action::Snap, Trigger::Press)],
            "either SHIFT shifts the mixer"
        );
        assert_eq!(
            go(&mut st, hid("Right.Cue", Some(true))),
            [(Action::Cue { deck: 1 }, Trigger::Press)]
        );
        go(&mut st, hid("Left.Deck.C", Some(true)));
        assert_eq!(
            go(&mut st, hid("Left.Master", Some(true))),
            [(Action::TempoReset { deck: 2 }, Trigger::Press)]
        );
        go(&mut st, hid("Left.Shift", Some(false)));
        assert_eq!(
            go(&mut st, hid("Mixer.Quant", Some(true))),
            [(Action::Quantize, Trigger::Press)]
        );
    }

    fn override_with(set: &mut MappingSet, name: &str, action: Action) -> Mapping {
        let mut m = Mapping::new(Input::Hid(name.into()), action, Interaction::Trigger);
        m.override_factory = true;
        set.mappings.retain(|d| d.input != m.input);
        set.mappings.push(m.clone());
        m
    }

    #[test]
    fn override_factory_map_skips_the_built_in_function_of_that_control() {
        let mut set = s4_set();
        override_with(&mut set, "Left.Cue", Action::Keylock { deck: 0 });
        let mut st = S4State::default();
        let go = |st: &mut S4State, e: InputEvent| translate(st, &set, &e);
        assert_eq!(go(&mut st, hid("Left.Shift", Some(true))), []);
        assert_eq!(
            go(&mut st, hid("Left.Cue", Some(true))),
            [(Action::Keylock { deck: 0 }, Trigger::Press)]
        );
        assert_eq!(
            go(&mut st, hid("Left.Pads.3", Some(true))),
            [(Action::DeleteHotcue { deck: 0, slot: 2 }, Trigger::Press)]
        );
    }

    #[test]
    fn an_overridden_shift_is_a_plain_control_with_its_own_modifier() {
        let mut set = s4_set();
        let mut shift = override_with(&mut set, "Left.Shift", Action::Modifier { index: 0 });
        shift.interaction = Interaction::Hold;
        shift.gate.value = Some(1);
        *set.mappings.last_mut().unwrap() = shift;
        let mut cue = override_with(&mut set, "Left.Cue", Action::Keylock { deck: 0 });
        cue.gate.conditions.push(crate::modifier::Condition {
            modifier: 0,
            value: 1,
        });
        *set.mappings.last_mut().unwrap() = cue;
        let mut st = S4State::default();
        let go = |st: &mut S4State, e: InputEvent| translate(st, &set, &e);
        assert_eq!(go(&mut st, hid("Left.Cue", Some(true))), [], "M1 = 0");
        go(&mut st, hid("Left.Cue", Some(false)));
        assert_eq!(go(&mut st, hid("Left.Shift", Some(true))), []);
        assert!(!st.shift[0], "SHIFT is the user's, not the driver's");
        assert_eq!(
            go(&mut st, hid("Left.Cue", Some(true))),
            [(Action::Keylock { deck: 0 }, Trigger::Press)]
        );
        go(&mut st, hid("Left.Cue", Some(false)));
        go(&mut st, hid("Left.Shift", Some(false)));
        assert_eq!(go(&mut st, hid("Left.Cue", Some(true))), [], "M1 back to 0");
    }

    #[test]
    fn an_overridden_pad_ignores_the_pad_modes() {
        let mut set = s4_set();
        override_with(&mut set, "Left.Pads.2", Action::Keylock { deck: 0 });
        let mut st = S4State::default();
        stem_deck(&mut st, 0, false);
        translate(&mut st, &set, &hid("Left.Pad Mode.Stems", Some(true)));
        assert_eq!(st.pads_mode(0), PadsMode::Stems);
        assert_eq!(
            translate(&mut st, &set, &hid("Left.Pads.2", Some(true))),
            [(Action::Keylock { deck: 0 }, Trigger::Press)]
        );
        assert_eq!(
            translate(&mut st, &set, &hid("Left.Pads.3", Some(true))),
            [(stem(0, 2, StemPart::Mute), Trigger::Press)],
            "the other pads are still stem pads"
        );
    }

    #[test]
    fn user_mappings_follow_the_focused_deck_shown() {
        let mut set = s4_set();
        let mut m = override_with(&mut set, "Left.Cue", Action::Keylock { deck: 0 });
        m.focus = true;
        *set.mappings.last_mut().unwrap() = m;
        let mut st = S4State::default();
        st.shown.focus = 2;
        assert_eq!(
            translate(&mut st, &set, &hid("Left.Cue", Some(true))),
            [(Action::Keylock { deck: 2 }, Trigger::Press)]
        );
    }

    #[test]
    fn turntable_mode_drives_the_platter_and_shift_play_runs_it_down() {
        let mut f = Feedback::default();
        f.decks[0].turntable = true;
        f.decks[0].playing = true;
        f.decks[0].platter = crate::feedback::PlatterFeedback {
            motor_on: true,
            rpm: 36.0,
            motor_off: 2,
        };
        f.decks[1].playing = true;
        let m = motor_settings(&[0, 1], &f);
        assert_eq!(m.sides[0].mode, motor::Mode::Turntable);
        assert!(m.sides[0].playing);
        assert!((m.sides[0].speed_rpm - 36.0).abs() < 1e-6);
        assert_eq!(m.sides[0].motor_off, 2);
        assert_eq!(m.sides[1].mode, motor::Mode::Jog);
        assert_eq!(
            crate::s4mk3::shifted("Left.Play"),
            Some(Some((Action::MotorOff { deck: 0 }, Interaction::Trigger)))
        );
    }

    #[test]
    fn jog_ring_follows_the_track_or_the_platter_and_warns_at_the_end() {
        let mut d = DeckFeedback::default();
        let free = (false, 100);
        assert_eq!(jog_ring(&d, 46, free).mode, RingMode::Off, "empty deck");
        d.loaded = true;
        d.duration_s = 300.0;
        d.position_s = 0.45;
        let ring = jog_ring(&d, 46, free);
        assert_eq!(
            (ring.mode, ring.colour, ring.position),
            (RingMode::Spot, 46, 100)
        );
        d.playing = true;
        assert_eq!(jog_ring(&d, 46, free).position, 720);
        assert_eq!(jog_ring(&d, 46, (true, 100)).position, 100, "touched");
        d.turntable = true;
        assert_eq!(jog_ring(&d, 46, free).position, 100, "TT: the platter");
        d.end_warning = true;
        assert_eq!(jog_ring(&d, 46, free).mode, RingMode::Flash);
        d.playing = false;
        assert_eq!(jog_ring(&d, 46, free).mode, RingMode::Spot);
    }

    #[test]
    fn grid_held_turns_the_jog_into_grid_adjust() {
        let set = s4_set();
        let mut st = S4State::default();
        assert_eq!(
            translate(&mut st, &set, &turn("Right.Jog.Turn", 5.0)),
            [(Action::JogTurn { deck: 1 }, Trigger::Delta(5.0))]
        );
        translate(&mut st, &set, &hid("Right.Jog Mode.Grid", Some(true)));
        assert_eq!(
            translate(&mut st, &set, &turn("Right.Jog.Turn", 5.0)),
            [(Action::GridAdjust { deck: 1 }, Trigger::Delta(5.0))]
        );
        translate(&mut st, &set, &hid("Right.Jog Mode.Grid", Some(false)));
        assert_eq!(
            translate(&mut st, &set, &turn("Right.Jog.Turn", 5.0))[0].0,
            Action::JogTurn { deck: 1 }
        );
    }

    #[test]
    fn deck_buttons_move_each_side_to_its_lower_deck() {
        let mut layer = [0, 1];
        let play = vec![(Action::PlayPause { deck: 0 }, Trigger::Press)];
        let ev = hid("Left.Play", Some(true));
        assert_eq!(
            apply_layer(&mut layer, &ev, play.clone())[0].0,
            Action::PlayPause { deck: 0 }
        );
        let consumed = apply_layer(&mut layer, &hid("Left.Deck.C", Some(true)), play.clone());
        assert_eq!(consumed, [], "deck buttons only switch layers");
        assert_eq!(layer, [2, 1]);
        assert_eq!(
            apply_layer(&mut layer, &ev, play)[0].0,
            Action::PlayPause { deck: 2 }
        );
        apply_layer(&mut layer, &hid("Left.Deck.A", Some(false)), Vec::new());
        assert_eq!(layer, [2, 1]);
        apply_layer(&mut layer, &hid("Right.Deck.D", Some(true)), Vec::new());
        let cue = vec![(Action::Cue { deck: 1 }, Trigger::Press)];
        assert_eq!(
            apply_layer(&mut layer, &hid("Right.Cue", Some(true)), cue)[0].0,
            Action::Cue { deck: 3 }
        );
    }

    #[test]
    fn layers_leave_mixer_fx_and_other_inputs_alone() {
        let mut layer = [2, 3];
        let fx = vec![(Action::FxOn { unit: 0 }, Trigger::Press)];
        assert_eq!(
            apply_layer(&mut layer, &hid("Left.FX.Button 1", Some(true)), fx)[0].0,
            Action::FxOn { unit: 0 }
        );
        let vol = vec![(Action::Volume { channel: 0 }, Trigger::Set(0.5))];
        assert_eq!(
            apply_layer(&mut layer, &hid("Mixer.A.Volume", None), vol)[0].0,
            Action::Volume { channel: 0 }
        );
        let key = InputEvent {
            input: Input::Key("W".into()),
            value: 1.0,
            raw: 0,
            pressed: Some(true),
        };
        let play = vec![(Action::PlayPause { deck: 0 }, Trigger::Press)];
        assert_eq!(
            apply_layer(&mut layer, &key, play)[0].0,
            Action::PlayPause { deck: 0 }
        );
    }

    #[test]
    fn jog_turns_reach_the_app_in_turns() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut s4 = S4Mk3 {
            layout: &S4,
            device: Device::start(&S4, tx, Box::new(|| {})).unwrap(),
            decoder: Decoder::default(),
            screens: Some(Err("test".into())),
            state: S4State::default(),
            leds: None,
            meters: None,
            rings: None,
            motors: None,
            displays: None,
        };
        let set = s4_set();
        let half = turn(
            "Left.Jog.Turn",
            (crate::s4mk3::JOG_STEPS_PER_TURN / 2.0) as f32,
        );
        assert_eq!(
            s4.translate(&set, &half),
            [(Action::JogTurn { deck: 0 }, Trigger::Delta(0.5))]
        );
    }

    #[test]
    fn the_old_move_default_becomes_the_loop_move() {
        let old = Mapping::new(
            Input::Hid("Left.Move.Turn".into()),
            Action::BeatJump {
                deck: 0,
                beats: 4.0,
            },
            Interaction::Relative,
        );
        assert_eq!(
            upgraded(&old).map(|m| m.action),
            Some(Action::LoopMove { deck: 0 })
        );
        let own = Mapping {
            action: Action::BeatJump {
                deck: 0,
                beats: 8.0,
            },
            ..old.clone()
        };
        assert_eq!(upgraded(&own), None, "a changed mapping stays");
    }

    #[test]
    fn the_old_loop_push_default_sets_a_new_loop() {
        let old = Mapping::new(
            Input::Hid("Right.Loop.Push".into()),
            Action::LoopActive { deck: 1 },
            Interaction::Trigger,
        );
        assert_eq!(
            upgraded(&old).map(|m| m.action),
            Some(Action::FastLoop { deck: 1 })
        );
    }

    fn stem_deck(st: &mut S4State, deck: usize, filter_on: bool) {
        st.stems[deck] = Some(std::array::from_fn(|k| StemFeedback {
            colour: [255, 0, 0],
            muted: k == 1,
            filter_on,
        }));
    }

    fn stem(deck: usize, stem: usize, part: StemPart) -> Action {
        Action::Stem { deck, stem, part }
    }

    #[test]
    fn stem_pads_mute_select_and_drive_encoders() {
        let set = s4_set();
        let mut st = S4State::default();
        let go = |st: &mut S4State, e: InputEvent| translate(st, &set, &e);
        go(&mut st, hid("Left.Pad Mode.Stems", Some(true)));
        assert_eq!(st.pads_mode(0), PadsMode::Hotcues);
        stem_deck(&mut st, 0, false);
        go(&mut st, hid("Left.Pad Mode.Stems", Some(true)));
        assert_eq!(st.pads_mode(0), PadsMode::Stems);

        assert_eq!(
            go(&mut st, hid("Left.Pads.2", Some(true))),
            [(stem(0, 1, StemPart::Mute), Trigger::Press)]
        );
        assert_eq!(go(&mut st, hid("Left.Pads.2", Some(false))), []);

        assert_eq!(
            go(&mut st, turn("Left.Move.Turn", 1.0)),
            [(Action::LoopMove { deck: 0 }, Trigger::Delta(1.0))]
        );
        assert_eq!(go(&mut st, hid("Left.Pads.7", Some(true))), []);
        let steps = 0.05 / crate::RELATIVE_STEP;
        assert_eq!(
            go(&mut st, turn("Left.Move.Turn", 2.0)),
            [(stem(0, 2, StemPart::Volume), Trigger::Delta(2.0 * steps))]
        );
        assert_eq!(
            go(&mut st, turn("Left.Loop.Turn", -1.0)),
            [
                (stem(0, 2, StemPart::Filter), Trigger::Delta(-steps)),
                (stem(0, 2, StemPart::FilterOn), Trigger::Set(1.0)),
            ],
            "turning the filter switches it on"
        );
        assert_eq!(
            go(&mut st, hid("Left.Loop.Push", Some(true))),
            [(stem(0, 2, StemPart::FilterOn), Trigger::Set(1.0))]
        );
        stem_deck(&mut st, 0, true);
        assert_eq!(
            go(&mut st, hid("Left.Loop.Push", Some(true))),
            [(stem(0, 2, StemPart::Filter), Trigger::Set(0.5))],
            "a pushed filter that is on returns to the centre"
        );
        assert_eq!(
            go(&mut st, hid("Left.Move.Push", Some(true))),
            [(stem(0, 2, StemPart::Volume), Trigger::Set(1.0))]
        );
        go(&mut st, hid("Left.Shift", Some(true)));
        assert_eq!(
            go(&mut st, turn("Left.Move.Turn", 1.0)),
            [],
            "not with SHIFT"
        );
        go(&mut st, hid("Left.Shift", Some(false)));
        go(&mut st, hid("Left.Pads.7", Some(false)));
        assert_eq!(st.active_stem[0], None);

        st.stems[0] = None;
        assert_eq!(
            go(&mut st, hid("Left.Pads.2", Some(true))),
            [(Action::Hotcue { deck: 0, slot: 1 }, Trigger::Press)]
        );
        stem_deck(&mut st, 0, false);
        assert_eq!(st.pads_mode(0), PadsMode::Stems);
        go(&mut st, hid("Left.Pad Mode.Hotcues", Some(true)));
        assert_eq!(st.pads_mode(0), PadsMode::Hotcues);
    }

    #[test]
    fn stem_pads_light_in_the_stem_colours() {
        let d = Decoder::default();
        let mut st = S4State::default();
        stem_deck(&mut st, 0, false);
        st.pads[0] = PadsMode::Stems;
        st.active_stem[0] = Some(3);
        let mut f = Feedback::default();
        f.decks[0].stems = st.stems[0];
        let leds = leds(&d, &st, &f);
        let at = |name: &str| leds.0[d.led(name).unwrap()];
        let red = colour::nearest([255, 0, 0]);
        assert_eq!(red, colour::RED);
        assert_eq!(at("Left.Pads.1"), red + colour::BRIGHT, "playing");
        assert_eq!(at("Left.Pads.2"), red, "muted: dim");
        assert_eq!(at("Left.Pads.8"), red + colour::BRIGHT, "held");
        assert_eq!(at("Left.Pads.5"), red);
        assert_eq!(at("Left.Pad Mode.Stems"), colour::BLUE + colour::BRIGHT);
        assert_eq!(at("Left.Pad Mode.Hotcues"), colour::BLUE);
        assert_eq!(at("Right.Pad Mode.Stems"), 0);
    }

    fn sample_deck(st: &mut S4State, deck: usize) -> SampleFeedback {
        let mut r = SampleFeedback::default();
        r.slots[0].cells[0] = 2;
        r.slots[1].cells[3] = 1;
        r.slots[1].selected = 3;
        r.slots[1].selected_colour = Some([255, 0, 0]);
        r.slots[2].cells[0] = 3;
        r.slots[3].muted = true;
        st.sample_deck[deck] = Some(r);
        r
    }

    fn slot(deck: usize, slot: usize, part: SampleSlotPart) -> Action {
        Action::SampleSlot { deck, slot, part }
    }

    #[test]
    fn sample_pads_trigger_stop_and_mute_the_selected_cells() {
        let set = s4_set();
        let mut st = S4State::default();
        let go = |st: &mut S4State, e: InputEvent| translate(st, &set, &e);
        go(&mut st, hid("Left.Pad Mode.Samples", Some(true)));
        assert_eq!(st.pads_mode(0), PadsMode::Hotcues);
        sample_deck(&mut st, 2);
        go(&mut st, hid("Left.Pad Mode.Samples", Some(true)));
        assert_eq!(st.pads_mode(0), PadsMode::SampleDeck);
        let trigger = slot(2, 1, SampleSlotPart::TriggerSelected);
        assert_eq!(
            go(&mut st, hid("Left.Pads.2", Some(true))),
            [(trigger, Trigger::Press)]
        );
        assert_eq!(
            go(&mut st, hid("Left.Pads.2", Some(false))),
            [(trigger, Trigger::Release)],
            "the release too: gated cells stop"
        );
        go(&mut st, hid("Left.Shift", Some(true)));
        assert_eq!(
            go(&mut st, hid("Left.Pads.1", Some(true))),
            [
                (slot(2, 0, SampleSlotPart::StopSelected), Trigger::Press),
                (slot(2, 0, SampleSlotPart::DeletePattern), Trigger::Press)
            ]
        );
        go(&mut st, hid("Left.Shift", Some(false)));
        let sample_deck = |part| Action::SampleDeck { deck: 2, part };
        assert_eq!(
            go(&mut st, hid("Left.Pad Mode.Record", Some(true))),
            [
                (sample_deck(SamplePart::SequencerRecord), Trigger::Press),
                (sample_deck(SamplePart::SequencerOn), Trigger::Set(1.0))
            ],
            "RECORD: recording, and the sequencers on"
        );
        assert_eq!(go(&mut st, hid("Left.Pad Mode.Record", Some(false))), []);
        go(&mut st, hid("Left.Pad Mode.Mute", Some(true)));
        assert_eq!(
            go(&mut st, hid("Left.Pads.4", Some(true))),
            [(stem(2, 3, StemPart::Mute), Trigger::Press)]
        );
        go(&mut st, hid("Left.Pad Mode.Mute", Some(false)));
        st.sample_deck[2] = None;
        assert_eq!(st.pads_mode(0), PadsMode::Hotcues);
    }

    #[test]
    fn a_held_slot_shows_its_sample_deck_on_the_screen() {
        let set = s4_set();
        let mut st = S4State::default();
        let go = |st: &mut S4State, e: InputEvent| translate(st, &set, &e);
        st.sample_deck[2] = Some(SampleFeedback::default());
        go(&mut st, hid("Left.Pad Mode.Samples", Some(true)));
        assert_eq!(screen_decks(st.layer, &st.active_slot), [0, 1]);
        go(&mut st, hid("Left.Pads.6", Some(true)));
        assert_eq!(st.active_slot[2], Some(1));
        assert_eq!(screen_decks(st.layer, &st.active_slot), [2, 1]);
        go(&mut st, hid("Left.Pads.6", Some(false)));
        assert_eq!(screen_decks(st.layer, &st.active_slot), [0, 1]);
        let held = [Some(0), None, Some(1), None];
        assert_eq!(screen_decks([0, 1], &held), [2, 1]);
        let held = [None, Some(0), None, None];
        assert_eq!(screen_decks([0, 3], &held), [0, 1]);
    }

    #[test]
    fn a_held_slot_takes_the_encoders_and_the_browser() {
        let set = s4_set();
        let mut st = S4State::default();
        let go = |st: &mut S4State, e: InputEvent| translate(st, &set, &e);
        sample_deck(&mut st, 0);
        go(&mut st, hid("Left.Pad Mode.Samples", Some(true)));
        go(&mut st, hid("Left.Shift", Some(true)));
        assert_eq!(
            go(&mut st, turn("Left.Loop.Turn", 1.0)),
            [(
                Action::SampleDeck {
                    deck: 0,
                    part: SamplePart::QuantizeSelector
                },
                Trigger::Delta(1.0)
            )]
        );
        go(&mut st, hid("Left.Shift", Some(false)));
        assert_eq!(go(&mut st, hid("Left.Pads.6", Some(true))), []);
        let steps = 0.025 / crate::RELATIVE_STEP;
        assert_eq!(
            go(&mut st, turn("Left.Move.Turn", 1.0)),
            [(stem(0, 1, StemPart::Volume), Trigger::Delta(steps))]
        );
        assert_eq!(
            go(&mut st, turn("Left.Loop.Turn", -1.0)),
            [
                (stem(0, 1, StemPart::Filter), Trigger::Delta(-steps)),
                (stem(0, 1, StemPart::FilterOn), Trigger::Set(1.0)),
            ]
        );
        assert_eq!(
            go(&mut st, turn("Left.Browse.Encoder.Turn", -1.0)),
            [(
                slot(0, 1, SampleSlotPart::SelectedCell),
                Trigger::Delta(-1.0)
            )]
        );
        assert_eq!(go(&mut st, hid("Left.Browse.Encoder.Push", Some(true))), []);
        go(&mut st, hid("Left.Pads.6", Some(false)));
        assert_eq!(st.active_slot[0], None);
    }

    #[test]
    fn sample_pads_light_by_their_selected_cell() {
        let d = Decoder::default();
        let mut st = S4State::default();
        let r = sample_deck(&mut st, 0);
        st.pads[0] = PadsMode::SampleDeck;
        st.active_slot[0] = Some(2);
        let mut f = Feedback::default();
        f.decks[0].sample_deck = Some(r);
        let at = |st: &S4State, name: &str| leds(&d, st, &f).0[d.led(name).unwrap()];
        assert_eq!(
            at(&st, "Left.Pads.1"),
            colour::BLUE + colour::BRIGHT,
            "playing"
        );
        assert_eq!(
            at(&st, "Left.Pads.2"),
            colour::RED,
            "loaded: dim, its colour"
        );
        assert_eq!(at(&st, "Left.Pads.4"), colour::OFF, "empty");
        assert_eq!(at(&st, "Left.Pads.3"), colour::BLUE, "queued: blinks");
        st.blink = true;
        assert_eq!(at(&st, "Left.Pads.3"), colour::BLUE + colour::BRIGHT);
        assert_eq!(
            at(&st, "Left.Pads.7"),
            colour::WHITE + colour::BRIGHT,
            "held"
        );
        assert_eq!(at(&st, "Left.Pads.5"), colour::WHITE);
        assert_eq!(
            at(&st, "Left.Pad Mode.Samples"),
            colour::BLUE + colour::BRIGHT
        );
        st.mute[0] = true;
        assert_eq!(at(&st, "Left.Pads.1"), colour::WHITE + colour::BRIGHT);
        assert_eq!(at(&st, "Left.Pads.4"), colour::WHITE, "muted: dim");
        assert_eq!(at(&st, "Right.Pad Mode.Samples"), 0);
    }

    fn mx2() -> (S4State, MappingSet) {
        let layout = &crate::kontrol::mx2::LAYOUT;
        let set = MappingSet {
            mappings: (layout.default_mappings)(),
            ..MappingSet::default()
        };
        (S4State::for_layout(layout), set)
    }

    #[test]
    fn mx2_pattern_pads_toggle_drum_machine_steps() {
        let (mut st, set) = mx2();
        let go = |st: &mut S4State, name, down| translate(st, &set, &hid(name, Some(down)));
        go(&mut st, "Left.Pad Mode.Pattern", true);
        assert_eq!(st.pads_mode(0), PadsMode::Hotcues);
        st.shown.fx[0].mode = 2;
        go(&mut st, "Left.Pad Mode.Pattern", true);
        assert_eq!(st.pads_mode(0), PadsMode::Pattern(0));
        go(&mut st, "Left.Pad Mode.Pattern", true);
        assert_eq!(st.pads_mode(0), PadsMode::Pattern(1));
        assert_eq!(
            go(&mut st, "Left.Pads.3", true),
            [(Action::PatternStep { unit: 0, step: 10 }, Trigger::Press)]
        );
        assert_eq!(go(&mut st, "Left.Pads.3", false), []);
        go(&mut st, "Left.Pad Mode.Pattern", true);
        assert_eq!(st.pads_mode(0), PadsMode::Pattern(0));
        st.shown.fx[0].mode = 0;
        st.drop_unsupported_modes();
        assert_eq!(st.pads[0], PadsMode::Hotcues);
    }

    #[test]
    fn mx2_loop_pads_stack_their_rolls() {
        let (mut st, set) = mx2();
        let go = |st: &mut S4State, name, down| translate(st, &set, &hid(name, Some(down)));
        go(&mut st, "Right.Pad Mode.Loops", true);
        assert_eq!(st.pads_mode(1), PadsMode::Loops);
        let roll = |beats, t| (Action::SlipRoll { deck: 1, beats }, t);
        assert_eq!(
            go(&mut st, "Right.Pads.1", true),
            [roll(1.0 / 32.0, Trigger::Press)]
        );
        assert_eq!(
            go(&mut st, "Right.Pads.8", true),
            [roll(4.0, Trigger::Press)]
        );
        assert_eq!(
            go(&mut st, "Right.Pads.8", false),
            [roll(1.0 / 32.0, Trigger::Press)],
            "back to the one still held"
        );
        assert_eq!(
            go(&mut st, "Right.Pads.1", false),
            [roll(1.0 / 32.0, Trigger::Release)]
        );
        st.shown.decks[1].live = true;
        st.drop_unsupported_modes();
        assert_eq!(st.pads_mode(1), PadsMode::Hotcues);
    }

    #[test]
    fn mx2_keylock_toggles_on_release_or_adjusts_the_key() {
        let (mut st, set) = mx2();
        let press = |st: &mut S4State, down| translate(st, &set, &hid("Left.Keylock", Some(down)));
        assert_eq!(press(&mut st, true), []);
        assert_eq!(
            press(&mut st, false),
            [(Action::Keylock { deck: 0 }, Trigger::Press)]
        );
        st.shown.decks[0].keylock = true;
        press(&mut st, true);
        assert_eq!(
            translate(&mut st, &set, &turn("Left.Loop.Turn", -2.0)),
            [(Action::KeyAdjust { deck: 0 }, Trigger::Delta(-2.0))]
        );
        assert_eq!(
            translate(&mut st, &set, &hid("Left.Move.Push", Some(true))),
            [(Action::KeyAdjust { deck: 0 }, Trigger::Set(0.5))]
        );
        assert_eq!(press(&mut st, false), [], "turned: no toggle");
        assert!(
            translate(&mut st, &set, &turn("Left.Loop.Turn", 1.0))
                .iter()
                .all(|(a, _)| !matches!(a, Action::KeyAdjust { .. }))
        );
    }

    #[test]
    fn mx2_sync_and_master_act_on_release_or_reset_the_tempo() {
        let (mut st, set) = mx2();
        let go = |st: &mut S4State, name, down| translate(st, &set, &hid(name, Some(down)));
        assert_eq!(go(&mut st, "Left.Master", true), []);
        assert_eq!(
            go(&mut st, "Left.Sync", true),
            [(Action::TempoReset { deck: 0 }, Trigger::Press)]
        );
        assert_eq!(go(&mut st, "Left.Sync", false), []);
        assert_eq!(go(&mut st, "Left.Master", false), []);
        go(&mut st, "Left.Sync", true);
        assert_eq!(
            go(&mut st, "Left.Sync", false),
            [(Action::Sync { deck: 0 }, Trigger::Press)]
        );
        assert_eq!(go(&mut st, "Left.Master", true), []);
        assert_eq!(
            go(&mut st, "Left.Master", false),
            [(Action::Master { deck: 0 }, Trigger::Press)]
        );
    }

    #[test]
    fn a_shared_shift_reaches_both_sides() {
        let layout = &crate::kontrol::x1mk2::LAYOUT;
        let set = MappingSet {
            mappings: (layout.default_mappings)(),
            ..MappingSet::default()
        };
        let mut st = S4State::for_layout(layout);
        let go = |st: &mut S4State, name, down| translate(st, &set, &hid(name, Some(down)));
        go(&mut st, "Left.Shift", true);
        assert_eq!(
            go(&mut st, "Right.Cue", true),
            [(Action::ReturnToStart { deck: 1 }, Trigger::Press)]
        );
        go(&mut st, "Right.Cue", false);
        go(&mut st, "Left.Shift", false);
        assert!(st.shift.iter().all(|&held| !held));
    }

    fn x1mk2() -> (S4State, MappingSet) {
        let layout = &crate::kontrol::x1mk2::LAYOUT;
        let set = MappingSet {
            mappings: (layout.default_mappings)(),
            ..MappingSet::default()
        };
        (S4State::for_layout(layout), set)
    }

    fn at(name: &str, value: f32) -> InputEvent {
        InputEvent {
            input: Input::Hid(name.into()),
            value,
            raw: 0,
            pressed: None,
        }
    }

    #[test]
    fn the_strip_follows_the_deck_and_shift() {
        let (mut st, set) = x1mk2();
        let go = |st: &mut S4State, e: InputEvent| translate(st, &set, &e);
        st.shown.decks[0].playing = true;
        assert_eq!(go(&mut st, hid("Left.Jog.Touch", Some(true))), []);
        assert_eq!(
            go(&mut st, at("Left.Jog.Turn", 0.1)),
            [(Action::JogTurn { deck: 0 }, Trigger::Delta(0.1))],
            "a playing deck bends"
        );
        go(&mut st, hid("Left.Jog.Touch", Some(false)));
        go(&mut st, hid("Left.Shift", Some(true)));
        assert_eq!(
            go(&mut st, hid("Left.Jog.Touch", Some(true))),
            [(Action::JogTouch { deck: 0 }, Trigger::Press)],
            "SHIFT scratches a playing deck"
        );
        assert_eq!(
            go(&mut st, at("Left.Jog.Turn", 0.1)),
            [(Action::JogTurn { deck: 0 }, Trigger::Delta(0.1))],
            "not a search"
        );
        go(&mut st, hid("Left.Jog.Touch", Some(false)));
        go(&mut st, hid("Right.Jog.Touch", Some(true)));
        assert_eq!(
            go(&mut st, at("Right.Strip.Position", 0.75)),
            [(Action::Seek { deck: 1 }, Trigger::Set(0.75))],
            "SHIFT seeks a stopped deck"
        );
    }

    #[test]
    fn a_loop_push_with_a_finger_on_the_strip_takes_it() {
        let (mut st, set) = x1mk2();
        let go = |st: &mut S4State, e: InputEvent| translate(st, &set, &e);
        assert_eq!(
            go(&mut st, hid("Right.Loop.Push", Some(true))),
            [(Action::FastLoop { deck: 1 }, Trigger::Press)],
            "without a finger it is the encoder's"
        );
        go(&mut st, hid("Right.Loop.Push", Some(false)));
        go(&mut st, hid("Strip.Touch", Some(true)));
        assert_eq!(go(&mut st, hid("Right.Loop.Push", Some(true))), []);
        assert_eq!(go(&mut st, hid("Right.Loop.Push", Some(false))), []);
        assert_eq!(
            go(&mut st, at("Strip.Position", 0.5)),
            [(
                Action::LoopSize {
                    deck: 1,
                    beats: 1.0
                },
                Trigger::Press
            )]
        );
        assert_eq!(go(&mut st, at("Left.Jog.Turn", 0.1)), [], "the halves wait");
        assert_eq!(
            go(&mut st, hid("Strip.Touch", Some(false))),
            [(Action::LoopActive { deck: 1 }, Trigger::Press)]
        );
        assert_eq!(go(&mut st, hid("Right.Loop.Push", Some(true))), []);
        assert_eq!(st.strip.mode(), crate::strip::Mode::Transport);
    }

    #[test]
    fn an_effect_button_with_a_finger_on_the_strip_takes_it() {
        let (mut st, set) = x1mk2();
        let go = |st: &mut S4State, e: InputEvent| translate(st, &set, &e);
        go(&mut st, hid("Strip.Touch", Some(true)));
        assert_eq!(
            go(&mut st, hid("Left.FX.Button 3", Some(true))),
            [(Action::FxButton { unit: 0, button: 1 }, Trigger::Set(1.0))]
        );
        assert_eq!(
            go(&mut st, at("Strip.Position", 0.5)),
            [(Action::FxKnob { unit: 0, knob: 1 }, Trigger::Set(0.5))]
        );
        let mut leds = Leds::for_layout(&crate::kontrol::x1mk2::LAYOUT);
        let bar = crate::kontrol::x1mk2::LAYOUT
            .touch_strip
            .and_then(|s| s.bar)
            .unwrap();
        let levels = st.strip.bar(&st.shown, st.layer, bar.positions);
        crate::strip::write_bar(&levels, bar, &mut leds.0);
        assert_ne!(leds.0[bar.first], 0, "the bar shows the value");
    }

    #[test]
    fn z1_mode_and_cue_play_the_deck() {
        let layout = &crate::kontrol::z1::LAYOUT;
        let set = MappingSet {
            mappings: (layout.default_mappings)(),
            ..MappingSet::default()
        };
        let mut st = S4State::for_layout(layout);
        let go = |st: &mut S4State, name, down| translate(st, &set, &hid(name, Some(down)));
        assert_eq!(
            go(&mut st, "Mixer.B.Cue", true),
            [(Action::MonitorCue { channel: 1 }, Trigger::Press)]
        );
        go(&mut st, "Mixer.B.Cue", false);
        assert_eq!(go(&mut st, "Mixer.Mode", true), []);
        assert_eq!(
            go(&mut st, "Mixer.B.Cue", true),
            [(Action::PlayPause { deck: 1 }, Trigger::Press)]
        );
        let d = Decoder::new(layout);
        let mut f = Feedback::default();
        f.decks[1].playing = true;
        let lit = leds(&d, &st, &f);
        assert_eq!(lit.0[d.led("Mixer.B.Cue").expect("led")], 0x7F, "playing");
        go(&mut st, "Mixer.Mode", false);
        let lit = leds(&d, &st, &f);
        assert_eq!(lit.0[d.led("Mixer.B.Cue").expect("led")], 0x0A, "cue off");
    }

    #[test]
    fn mx2_shift_fx_buttons_follow_the_unit_type() {
        let (mut st, set) = mx2();
        st.shift[1] = true;
        let go = |st: &mut S4State, name| translate(st, &set, &hid(name, Some(true)));
        assert_eq!(
            go(&mut st, "Right.FX.Button 1"),
            [(Action::FxMode { unit: 1 }, Trigger::Delta(1.0))]
        );
        assert_eq!(
            go(&mut st, "Right.FX.Button 4"),
            [(Action::FxSelect { unit: 1, slot: 2 }, Trigger::Delta(1.0))]
        );
        st.shown.fx[1].mode = 1;
        assert_eq!(
            go(&mut st, "Right.FX.Button 2"),
            [(Action::FxSelect { unit: 1, slot: 0 }, Trigger::Delta(-1.0))]
        );
        assert_eq!(go(&mut st, "Right.FX.Button 4"), []);
        st.shown.fx[1].mode = 2;
        assert_eq!(
            go(&mut st, "Right.FX.Button 3"),
            [(Action::PatternKit { unit: 1 }, Trigger::Delta(1.0))]
        );
    }

    #[test]
    fn mx2_pattern_and_loop_leds() {
        let (mut st, _) = mx2();
        let d = Decoder::new(&crate::kontrol::mx2::LAYOUT);
        let mut f = Feedback::default();
        f.fx[0].mode = 2;
        f.fx[0].pattern_steps = 0b0000_0001_0000_0010;
        f.fx[0].pattern_cells = 16;
        f.fx[0].pattern_step = Some(8);
        st.shown = f;
        st.pads[0] = PadsMode::Pattern(1);
        let lit = leds(&d, &st, &f);
        let at = |name: &str| lit.0[d.led(name).expect(name)];
        assert_eq!(at("Left.Pad Mode.Pattern"), colour::WHITE + colour::BRIGHT);
        assert_eq!(
            at("Left.Pads.1"),
            colour::ORANGE + colour::BRIGHT,
            "step 9 plays"
        );
        assert_eq!(at("Left.Pads.2"), colour::MINT, "step 10 off");
        st.pads[1] = PadsMode::Loops;
        st.stutter[1] = vec![2];
        let lit = leds(&d, &st, &f);
        let at = |name: &str| lit.0[d.led(name).expect(name)];
        assert_eq!(at("Right.Pads.3"), colour::GREEN + colour::BRIGHT);
        assert_eq!(at("Right.Pads.1"), colour::GREEN);
        assert_eq!(at("Right.Pad Mode.Pattern"), 0, "no Drum Machine");
    }
}
