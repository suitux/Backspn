//! Plays a planned set on its own: loads the next track on the free deck,
//! syncs it, and at the mix point plays the transition recipe beat by beat
//! (faders, EQ, filter, effects, loops), then eases the tempo back to the
//! new track's own.

use std::collections::HashMap;

use backspn_control::Action;
use backspn_remote::transition::{self, Band, Control, PatternSpec, Recipe, Side};

/// Decks the autopilot plays on.
pub const DECKS: [usize; 2] = [0, 1];
/// Seconds per beat when a deck has no beat grid.
const FALLBACK_BEAT_S: f64 = 60.0 / 124.0;
/// Beats kept between the end of a mix and the end of the outgoing track.
const END_MARGIN_BEATS: f64 = 2.0;
/// Real seconds before the end of a track at which it loops if the next
/// track is still not ready.
const HOLD_BEFORE_END_S: f64 = 12.0;
/// Real seconds to ease the tempo back after a mix.
const SETTLE_S: f64 = 30.0;
/// Seconds a deck may take to report that it plays after being started.
const START_GRACE_S: f64 = 2.0;

#[derive(Debug, Clone, PartialEq)]
pub struct AutoTrack {
    pub id: i64,
    /// Track seconds to play before mixing out; none plays to the end.
    pub play_s: Option<f64>,
    /// Share of the track (0–1) where it starts; none starts at its cue.
    pub start: Option<f64>,
    /// How this track comes in; none uses the set's default.
    pub transition: Option<Recipe>,
}

/// What the autopilot needs to know about a deck.
// Independent flags read from the engine each frame.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeckView {
    pub track: Option<i64>,
    pub loaded: bool,
    pub playing: bool,
    pub position_s: f64,
    pub duration_s: f64,
    /// Track seconds per beat at the play position.
    pub beat_s: Option<f64>,
    pub tempo: f64,
    pub synced: bool,
    pub keylock: bool,
    pub master: bool,
}

impl DeckView {
    fn beat_s(&self) -> f64 {
        self.beat_s.filter(|b| *b > 0.0).unwrap_or(FALLBACK_BEAT_S)
    }
}

/// A control on a real deck or effect unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    Volume(usize),
    Eq(usize, Band),
    Filter(usize),
    FxWet(usize),
    FxKnob(usize, usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Load {
        deck: usize,
        track: i64,
    },
    Seek {
        deck: usize,
        seconds: f64,
    },
    Play {
        deck: usize,
    },
    Stop {
        deck: usize,
    },
    Sync {
        deck: usize,
        on: bool,
    },
    Master {
        deck: usize,
    },
    Keylock {
        deck: usize,
        on: bool,
    },
    Tempo {
        deck: usize,
        ratio: f64,
    },
    Hold {
        deck: usize,
    },
    Set {
        target: Target,
        value: f32,
    },
    FxEffect {
        unit: usize,
        effect: String,
    },
    FxAssign {
        unit: usize,
        deck: usize,
        on: bool,
    },
    FxOn {
        unit: usize,
        on: bool,
    },
    Loop {
        deck: usize,
        beats: f64,
    },
    LoopOff {
        deck: usize,
    },
    BeatJump {
        deck: usize,
        beats: f64,
    },
    /// Sets up a drum pattern unit, through `deck`'s channel if given.
    Pattern {
        pattern: PatternSpec,
        deck: Option<usize>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pause {
    /// A deck the set uses was loaded or stopped by hand.
    DeckChanged,
    /// A control the set drives was moved by hand.
    Manual,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    Starting,
    Playing,
    Mixing { progress: f64 },
    Holding,
    Paused(Pause),
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Loading,
    Ready,
    Mixing {
        started: f64,
        beats_per_s: f64,
        last_beat: Option<f64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Incoming {
    deck: usize,
    index: usize,
    phase: Phase,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Current {
    deck: usize,
    index: usize,
    start_s: f64,
    started_at: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Settle {
    deck: usize,
    from: f64,
    started: f64,
}

#[derive(Debug, Clone)]
pub struct Autopilot {
    tracks: Vec<AutoTrack>,
    default: Recipe,
    keylock: bool,
    current: Option<Current>,
    incoming: Option<Incoming>,
    settle: Option<Settle>,
    holding: bool,
    pause: Option<Pause>,
    finished: bool,
    sent: HashMap<Target, f32>,
    tempo_sent: HashMap<usize, f64>,
}

fn other(deck: usize) -> usize {
    if deck == DECKS[0] { DECKS[1] } else { DECKS[0] }
}

impl Autopilot {
    pub fn new(tracks: Vec<AutoTrack>, default: Recipe, keylock: bool) -> Self {
        Self {
            tracks,
            default,
            keylock,
            current: None,
            incoming: None,
            settle: None,
            holding: false,
            pause: None,
            finished: false,
            sent: HashMap::new(),
            tempo_sent: HashMap::new(),
        }
    }

    pub fn tracks(&self) -> &[AutoTrack] {
        &self.tracks
    }

    /// Index of the track playing (or about to play) in the set.
    pub fn cursor(&self) -> usize {
        self.current.map_or(0, |c| c.index)
    }

    pub fn status(&self, now: f64) -> Status {
        if let Some(reason) = self.pause {
            return Status::Paused(reason);
        }
        if self.finished {
            return Status::Finished;
        }
        match (self.current, self.incoming) {
            (None, _) => Status::Starting,
            (
                _,
                Some(Incoming {
                    phase:
                        Phase::Mixing {
                            started,
                            beats_per_s,
                            ..
                        },
                    index,
                    ..
                }),
            ) => {
                let beats = self.recipe(index).beats;
                Status::Mixing {
                    progress: ((now - started) * beats_per_s / beats).clamp(0.0, 1.0),
                }
            }
            _ if self.holding => Status::Holding,
            _ => Status::Playing,
        }
    }

    pub fn running(&self) -> bool {
        self.pause.is_none() && !self.finished
    }

    pub fn pause(&mut self, reason: Pause) {
        if self.running() {
            self.pause = Some(reason);
        }
    }

    /// Picks up after a pause, taking the decks as they are.
    pub fn resume(&mut self) {
        self.pause = None;
        self.sent.clear();
        self.tempo_sent.clear();
    }

    /// Replaces what comes after the track playing. A track that is loaded
    /// but not yet mixing in is replaced too.
    pub fn replace_upcoming(&mut self, tracks: Vec<AutoTrack>) {
        let keep = if let Some(Incoming {
            phase: Phase::Mixing { .. },
            index,
            ..
        }) = self.incoming
        {
            index + 1
        } else {
            self.incoming = None;
            self.current.map_or(0, |c| c.index + 1)
        };
        self.tracks.truncate(keep.min(self.tracks.len()));
        self.tracks.extend(tracks);
        self.finished = false;
    }

    fn recipe(&self, index: usize) -> &Recipe {
        self.tracks[index]
            .transition
            .as_ref()
            .unwrap_or(&self.default)
    }

    fn set(&mut self, target: Target, value: f32, out: &mut Vec<Step>) {
        if self
            .sent
            .get(&target)
            .is_none_or(|s| (s - value).abs() > 1e-4)
        {
            self.sent.insert(target, value);
            out.push(Step::Set { target, value });
        }
    }

    fn tempo(&mut self, deck: usize, ratio: f64, out: &mut Vec<Step>) {
        if self
            .tempo_sent
            .get(&deck)
            .is_none_or(|s| (s - ratio).abs() > 1e-5)
        {
            self.tempo_sent.insert(deck, ratio);
            out.push(Step::Tempo { deck, ratio });
        }
    }

    /// Puts every mixer control of `deck` back to neutral, at `volume`.
    fn neutral(&mut self, deck: usize, volume: f32, out: &mut Vec<Step>) {
        self.set(Target::Volume(deck), volume, out);
        for band in [Band::Low, Band::Mid, Band::High] {
            self.set(Target::Eq(deck, band), 0.5, out);
        }
        self.set(Target::Filter(deck), 0.0, out);
    }

    fn target(control: Control, incoming: usize, outgoing: usize) -> Target {
        let deck = |side| match side {
            Side::In => incoming,
            Side::Out => outgoing,
        };
        match control {
            Control::Volume { side } => Target::Volume(deck(side)),
            Control::Eq { side, band } => Target::Eq(deck(side), band),
            Control::Filter { side } => Target::Filter(deck(side)),
            Control::FxWet { unit } => Target::FxWet(unit),
            Control::FxKnob { unit, knob } => Target::FxKnob(unit, knob),
        }
    }

    fn event(action: &transition::Action, incoming: usize, outgoing: usize) -> Step {
        use transition::Action as A;
        let deck = |side: Side| match side {
            Side::In => incoming,
            Side::Out => outgoing,
        };
        match action {
            A::Play { side } => Step::Play { deck: deck(*side) },
            A::Stop { side } => Step::Stop { deck: deck(*side) },
            A::FxEffect { unit, effect } => Step::FxEffect {
                unit: *unit,
                effect: effect.clone(),
            },
            A::FxAssign { unit, side, on } => Step::FxAssign {
                unit: *unit,
                deck: deck(*side),
                on: *on,
            },
            A::FxOn { unit, on } => Step::FxOn {
                unit: *unit,
                on: *on,
            },
            A::Loop { side, beats } => Step::Loop {
                deck: deck(*side),
                beats: *beats,
            },
            A::LoopOff { side } => Step::LoopOff { deck: deck(*side) },
            A::BeatJump { side, beats } => Step::BeatJump {
                deck: deck(*side),
                beats: *beats,
            },
            A::Pattern { side, pattern } => Step::Pattern {
                pattern: pattern.clone(),
                deck: side.map(deck),
            },
        }
    }

    /// Moves a loaded track to where the set starts it; returns that point.
    fn seek_start(&self, index: usize, view: &DeckView, deck: usize, out: &mut Vec<Step>) -> f64 {
        match self.tracks[index].start {
            Some(share) if view.duration_s > 0.0 => {
                let seconds = view.duration_s * share.clamp(0.0, 0.95);
                out.push(Step::Seek { deck, seconds });
                seconds
            }
            _ => view.position_s,
        }
    }

    fn load(&mut self, deck: usize, index: usize, out: &mut Vec<Step>) {
        out.push(Step::Load {
            deck,
            track: self.tracks[index].id,
        });
        self.set(Target::Volume(deck), 0.0, out);
        self.incoming = Some(Incoming {
            deck,
            index,
            phase: Phase::Loading,
        });
    }

    pub fn step(&mut self, decks: &[DeckView], now: f64) -> Vec<Step> {
        let mut out = Vec::new();
        if !self.running() || decks.len() <= DECKS[1] {
            return out;
        }
        match self.current {
            None => self.start(decks, now, &mut out),
            Some(current) => self.run(current, decks, now, &mut out),
        }
        out
    }

    fn start(&mut self, decks: &[DeckView], now: f64, out: &mut Vec<Step>) {
        let Some(first) = self.tracks.first().map(|t| t.id) else {
            self.finished = true;
            return;
        };
        let deck = DECKS[0];
        let Some(incoming) = self.incoming else {
            self.load(deck, 0, out);
            self.set(Target::Volume(other(deck)), 0.0, out);
            return;
        };
        let view = decks[incoming.deck];
        if view.track != Some(first) || !view.loaded {
            return;
        }
        if view.keylock != self.keylock {
            out.push(Step::Keylock {
                deck,
                on: self.keylock,
            });
        }
        if view.synced {
            out.push(Step::Sync { deck, on: false });
        }
        self.neutral(deck, 1.0, out);
        let start_s = self.seek_start(0, &view, deck, out);
        out.push(Step::Play { deck });
        out.push(Step::Master { deck });
        self.incoming = None;
        self.current = Some(Current {
            deck,
            index: 0,
            start_s,
            started_at: now,
        });
    }

    fn run(&mut self, current: Current, decks: &[DeckView], now: f64, out: &mut Vec<Step>) {
        let playing = decks[current.deck];
        let mixing = matches!(
            self.incoming,
            Some(Incoming {
                phase: Phase::Mixing { .. },
                ..
            })
        );
        if playing.track != Some(self.tracks[current.index].id) {
            self.pause = Some(Pause::DeckChanged);
            return;
        }
        if !mixing && !playing.playing && now - current.started_at > START_GRACE_S {
            let at_end = playing.position_s >= playing.duration_s - 0.5;
            if at_end && current.index + 1 >= self.tracks.len() {
                self.finished = true;
            } else {
                self.pause = Some(if at_end {
                    Pause::Stopped
                } else {
                    Pause::DeckChanged
                });
            }
            return;
        }
        if let Some(settle) = self.settle {
            let x = ((now - settle.started) / SETTLE_S).clamp(0.0, 1.0);
            self.tempo(settle.deck, settle.from + (1.0 - settle.from) * x, out);
            if x >= 1.0 {
                self.settle = None;
            }
        }
        let next = current.index + 1;
        if next >= self.tracks.len() {
            return;
        }
        let Some(mut incoming) = self.incoming else {
            self.load(other(current.deck), next, out);
            return;
        };
        let view = decks[incoming.deck];
        if view.track != Some(self.tracks[incoming.index].id) || !view.loaded {
            if incoming.phase == Phase::Loading {
                self.hold_if_late(current, &playing, out);
            } else {
                self.pause = Some(Pause::DeckChanged);
            }
            return;
        }
        if incoming.phase == Phase::Loading {
            self.prepare(current, &playing, incoming, &view, out);
            incoming.phase = Phase::Ready;
            self.incoming = Some(incoming);
        }
        if incoming.phase == Phase::Ready {
            let recipe_s = self.recipe(incoming.index).beats * playing.beat_s();
            let end = playing.duration_s - (END_MARGIN_BEATS * playing.beat_s());
            let out_at = self.tracks[current.index]
                .play_s
                .map_or(end - recipe_s, |p| current.start_s + p)
                .min(end - recipe_s);
            if playing.position_s < out_at && !self.holding {
                return;
            }
            incoming.phase = Phase::Mixing {
                started: now,
                beats_per_s: playing.tempo.max(0.01) / playing.beat_s(),
                last_beat: None,
            };
            self.incoming = Some(incoming);
        }
        self.mix(current, incoming, now, &view, out);
    }

    /// Readies the loaded track: keylock, sync to the playing deck and the
    /// incoming controls as the recipe starts them.
    fn prepare(
        &mut self,
        current: Current,
        playing: &DeckView,
        incoming: Incoming,
        view: &DeckView,
        out: &mut Vec<Step>,
    ) {
        let deck = incoming.deck;
        if view.keylock != self.keylock {
            out.push(Step::Keylock {
                deck,
                on: self.keylock,
            });
        }
        if !playing.master {
            out.push(Step::Master { deck: current.deck });
        }
        if !view.synced {
            out.push(Step::Sync { deck, on: true });
        }
        self.neutral(deck, 0.0, out);
        self.seek_start(incoming.index, view, deck, out);
        let start: Vec<(Control, f32)> = self.recipe(incoming.index).values_at(0.0).collect();
        for (control, value) in start {
            if matches!(
                control,
                Control::Volume { side: Side::In }
                    | Control::Eq { side: Side::In, .. }
                    | Control::Filter { side: Side::In }
            ) {
                self.set(Self::target(control, deck, current.deck), value, out);
            }
        }
    }

    fn hold_if_late(&mut self, current: Current, playing: &DeckView, out: &mut Vec<Step>) {
        let left = (playing.duration_s - playing.position_s) / playing.tempo.max(0.01);
        if !self.holding && left < HOLD_BEFORE_END_S {
            self.holding = true;
            out.push(Step::Hold { deck: current.deck });
        }
    }

    fn mix(
        &mut self,
        current: Current,
        mut incoming: Incoming,
        now: f64,
        view: &DeckView,
        out: &mut Vec<Step>,
    ) {
        let Phase::Mixing {
            started,
            beats_per_s,
            last_beat,
        } = incoming.phase
        else {
            return;
        };
        let (deck_in, deck_out) = (incoming.deck, current.deck);
        let recipe = self.recipe(incoming.index).clone();
        let beat = ((now - started) * beats_per_s).min(recipe.beats);
        if last_beat.is_none() && recipe.in_start() <= 0.0 {
            out.push(Step::Play { deck: deck_in });
        }
        for action in recipe.events_between(last_beat, beat) {
            out.push(Self::event(action, deck_in, deck_out));
        }
        for (control, value) in recipe.values_at(beat) {
            self.set(Self::target(control, deck_in, deck_out), value, out);
        }
        incoming.phase = Phase::Mixing {
            started,
            beats_per_s,
            last_beat: Some(beat),
        };
        self.incoming = Some(incoming);
        if beat < recipe.beats {
            return;
        }
        self.finish(current, incoming, &recipe, now, view, out);
    }

    fn finish(
        &mut self,
        current: Current,
        incoming: Incoming,
        recipe: &Recipe,
        now: f64,
        view: &DeckView,
        out: &mut Vec<Step>,
    ) {
        let (deck_in, deck_out) = (incoming.deck, current.deck);
        out.push(Step::Stop { deck: deck_out });
        out.push(Step::Sync {
            deck: deck_out,
            on: false,
        });
        self.neutral(deck_out, 0.0, out);
        self.neutral(deck_in, 1.0, out);
        for unit in recipe.fx_units() {
            out.push(Step::FxOn { unit, on: false });
            for deck in DECKS {
                out.push(Step::FxAssign {
                    unit,
                    deck,
                    on: false,
                });
            }
            self.sent.remove(&Target::FxWet(unit));
        }
        // Keep the tempo it was synced to, then ease back to its own.
        let from = view.tempo;
        self.tempo_sent.remove(&deck_in);
        self.tempo(deck_in, from, out);
        out.push(Step::Sync {
            deck: deck_in,
            on: false,
        });
        out.push(Step::Master { deck: deck_in });
        self.settle = Some(Settle {
            deck: deck_in,
            from,
            started: now,
        });
        self.holding = false;
        self.incoming = None;
        self.current = Some(Current {
            deck: deck_in,
            index: incoming.index,
            start_s: view.position_s - (recipe.beats - recipe.in_start()) * view.beat_s(),
            started_at: now,
        });
    }
}

/// Whether a control moved by hand takes over from the autopilot.
pub fn takes_over(action: Action) -> bool {
    let on_set_deck = |d: usize| DECKS.contains(&d);
    match action {
        Action::PlayPause { deck }
        | Action::Cue { deck }
        | Action::Cup { deck }
        | Action::Sync { deck }
        | Action::TempoSync { deck }
        | Action::Tempo { deck }
        | Action::TempoRelative { deck }
        | Action::LoadSelected { deck }
        | Action::LoadSelectedAs { deck, .. }
        | Action::LoadNext { deck }
        | Action::LoadPrevious { deck } => on_set_deck(deck),
        Action::Volume { channel } | Action::Eq { channel, .. } | Action::Filter { channel } => {
            on_set_deck(channel)
        }
        Action::Crossfader => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use backspn_library::set_plan::{Transition, TransitionStyle};

    use super::*;

    const A: usize = DECKS[0];
    const B: usize = DECKS[1];

    fn tracks(ids: &[i64]) -> Vec<AutoTrack> {
        ids.iter()
            .map(|&id| AutoTrack {
                id,
                play_s: None,
                start: None,
                transition: None,
            })
            .collect()
    }

    fn preset(style: TransitionStyle) -> Recipe {
        transition::preset(Transition { style, bars: 16 })
    }

    fn deck(track: i64, playing: bool, position_s: f64) -> DeckView {
        DeckView {
            track: Some(track),
            loaded: true,
            playing,
            position_s,
            duration_s: 300.0,
            beat_s: Some(0.5),
            tempo: 1.0,
            keylock: true,
            ..DeckView::default()
        }
    }

    fn has(steps: &[Step], step: &Step) -> bool {
        steps.contains(step)
    }

    fn volume(deck: usize, value: f32) -> Step {
        Step::Set {
            target: Target::Volume(deck),
            value,
        }
    }

    /// Runs the first track to playing on deck A, with the next one loaded
    /// and ready on deck B.
    fn ready(ids: &[i64], recipe: Recipe) -> Autopilot {
        let mut a = Autopilot::new(tracks(ids), recipe, true);
        let steps = a.step(&[DeckView::default(); 2], 0.0);
        assert!(has(
            &steps,
            &Step::Load {
                deck: A,
                track: ids[0]
            }
        ));
        let steps = a.step(&[deck(ids[0], false, 0.0), DeckView::default()], 0.1);
        assert!(has(&steps, &Step::Play { deck: A }));
        assert!(has(&steps, &volume(A, 1.0)));
        assert_eq!(a.status(0.1), Status::Playing);
        let steps = a.step(&[deck(ids[0], true, 10.0), DeckView::default()], 1.0);
        assert!(has(
            &steps,
            &Step::Load {
                deck: B,
                track: ids[1]
            }
        ));
        let steps = a.step(&[deck(ids[0], true, 11.0), deck(ids[1], false, 0.0)], 2.0);
        assert!(has(&steps, &Step::Sync { deck: B, on: true }));
        assert!(has(&steps, &Step::Master { deck: A }));
        assert!(
            !has(&steps, &Step::Play { deck: B }),
            "not yet at the mix point"
        );
        a
    }

    #[test]
    fn a_blend_starts_at_its_mix_point_and_swaps_the_bass() {
        let mut a = ready(&[1, 2], preset(TransitionStyle::Blend));
        // 64 beats of 0.5 s plus a 2-beat margin: the mix starts at 267 s.
        let steps = a.step(&[deck(1, true, 266.0), deck(2, false, 0.0)], 3.0);
        assert!(!has(&steps, &Step::Play { deck: B }));
        let steps = a.step(&[deck(1, true, 267.0), deck(2, false, 0.0)], 4.0);
        assert!(has(&steps, &Step::Play { deck: B }));
        let steps = a.step(&[deck(1, true, 283.0), deck(2, true, 16.0)], 20.0);
        assert!(has(&steps, &volume(B, 1.0)));
        let low = |deck, value| Step::Set {
            target: Target::Eq(deck, Band::Low),
            value,
        };
        assert!(has(&steps, &low(A, 0.0)));
        assert!(has(&steps, &low(B, 0.5)));
        assert!(matches!(a.status(20.0), Status::Mixing { .. }));
        let steps = a.step(&[deck(1, true, 299.0), deck(2, true, 32.0)], 36.0);
        assert!(has(&steps, &Step::Stop { deck: A }));
        assert!(has(&steps, &Step::Master { deck: B }));
        assert!(has(&steps, &Step::Sync { deck: B, on: false }));
        assert!(has(&steps, &low(A, 0.5)), "back to neutral");
        assert_eq!(a.cursor(), 1);
        assert_eq!(a.status(36.0), Status::Playing);
    }

    #[test]
    fn an_echo_out_drops_the_next_track_in_after_the_effect() {
        let mut a = ready(&[1, 2], preset(TransitionStyle::EchoOut));
        let steps = a.step(&[deck(1, true, 293.0), deck(2, false, 0.0)], 3.0);
        assert!(has(
            &steps,
            &Step::FxEffect {
                unit: 0,
                effect: "Echo".into()
            }
        ));
        assert!(has(
            &steps,
            &Step::FxAssign {
                unit: 0,
                deck: A,
                on: true
            }
        ));
        assert!(has(&steps, &Step::FxOn { unit: 0, on: true }));
        assert!(!has(&steps, &Step::Play { deck: B }), "drops on beat 4");
        let steps = a.step(&[deck(1, true, 295.0), deck(2, false, 0.0)], 5.0);
        assert!(has(&steps, &Step::Play { deck: B }));
        assert!(has(&steps, &volume(B, 1.0)));
        let steps = a.step(&[deck(1, true, 299.0), deck(2, true, 4.0)], 9.0);
        assert!(has(&steps, &Step::FxOn { unit: 0, on: false }));
        assert!(has(
            &steps,
            &Step::FxAssign {
                unit: 0,
                deck: A,
                on: false
            }
        ));
    }

    #[test]
    fn chops_cut_the_faders_on_the_beat() {
        let mut a = ready(&[1, 2], preset(TransitionStyle::Chop));
        a.step(&[deck(1, true, 267.0), deck(2, false, 0.0)], 3.0);
        let at = |a: &mut Autopilot, beat: f64| {
            a.step(
                &[deck(1, true, 280.0), deck(2, true, 10.0)],
                3.0 + beat * 0.5,
            )
        };
        assert!(has(&at(&mut a, 32.0), &volume(B, 1.0)));
        assert!(has(&at(&mut a, 32.5), &volume(B, 0.0)));
        assert!(has(&at(&mut a, 34.0), &volume(B, 1.0)));
    }

    #[test]
    fn eases_the_tempo_back_after_a_mix() {
        let mut a = ready(&[1, 2], preset(TransitionStyle::Cut));
        a.step(&[deck(1, true, 297.0), deck(2, false, 0.0)], 3.0);
        let synced = DeckView {
            tempo: 1.04,
            ..deck(2, true, 2.0)
        };
        let steps = a.step(&[deck(1, true, 299.0), synced], 5.0);
        assert!(has(
            &steps,
            &Step::Tempo {
                deck: B,
                ratio: 1.04
            }
        ));
        let steps = a.step(&[deck(1, false, 299.0), deck(2, true, 17.0)], 20.0);
        let Some(Step::Tempo { ratio, .. }) =
            steps.iter().find(|s| matches!(s, Step::Tempo { .. }))
        else {
            panic!("{steps:?}");
        };
        assert!((ratio - 1.02).abs() < 1e-9);
        let steps = a.step(&[deck(1, false, 299.0), deck(2, true, 40.0)], 40.0);
        assert!(has(
            &steps,
            &Step::Tempo {
                deck: B,
                ratio: 1.0
            }
        ));
    }

    #[test]
    fn a_late_track_holds_the_playing_one_in_a_loop() {
        let mut a = Autopilot::new(tracks(&[1, 2]), preset(TransitionStyle::Blend), true);
        a.step(&[DeckView::default(); 2], 0.0);
        a.step(&[deck(1, false, 0.0), DeckView::default()], 0.1);
        a.step(&[deck(1, true, 10.0), DeckView::default()], 1.0);
        let steps = a.step(&[deck(1, true, 280.0), DeckView::default()], 2.0);
        assert!(!has(&steps, &Step::Hold { deck: A }));
        let steps = a.step(&[deck(1, true, 290.0), DeckView::default()], 3.0);
        assert!(has(&steps, &Step::Hold { deck: A }));
        assert_eq!(a.status(3.0), Status::Holding);
        let steps = a.step(&[deck(1, true, 291.0), deck(2, false, 0.0)], 4.0);
        assert!(has(&steps, &Step::Sync { deck: B, on: true }));
        assert!(
            has(&steps, &Step::Play { deck: B }),
            "mixes in as soon as ready"
        );
    }

    #[test]
    fn play_time_moves_the_mix_point_earlier() {
        let mut set = tracks(&[1, 2]);
        set[0].play_s = Some(120.0);
        let mut a = Autopilot::new(set, preset(TransitionStyle::Blend), true);
        a.step(&[DeckView::default(); 2], 0.0);
        a.step(&[deck(1, false, 5.0), DeckView::default()], 0.1);
        a.step(&[deck(1, true, 10.0), DeckView::default()], 1.0);
        a.step(&[deck(1, true, 11.0), deck(2, false, 0.0)], 2.0);
        let steps = a.step(&[deck(1, true, 124.0), deck(2, false, 0.0)], 3.0);
        assert!(!has(&steps, &Step::Play { deck: B }));
        let steps = a.step(&[deck(1, true, 125.0), deck(2, false, 0.0)], 4.0);
        assert!(
            has(&steps, &Step::Play { deck: B }),
            "120 s after the cue at 5 s"
        );
    }

    #[test]
    fn a_track_can_bring_its_own_transition() {
        let mut set = tracks(&[1, 2]);
        set[1].transition = Some(preset(TransitionStyle::Brake));
        let mut a = Autopilot::new(set, preset(TransitionStyle::Blend), true);
        a.step(&[DeckView::default(); 2], 0.0);
        a.step(&[deck(1, false, 0.0), DeckView::default()], 0.1);
        a.step(&[deck(1, true, 10.0), DeckView::default()], 1.0);
        a.step(&[deck(1, true, 11.0), deck(2, false, 0.0)], 2.0);
        let steps = a.step(&[deck(1, true, 293.0), deck(2, false, 0.0)], 3.0);
        assert!(has(
            &steps,
            &Step::FxEffect {
                unit: 0,
                effect: "VinylBrake".into()
            }
        ));
    }

    #[test]
    fn a_hand_loaded_deck_pauses_and_the_end_finishes() {
        let mut a = ready(&[1, 2], preset(TransitionStyle::Blend));
        a.step(&[deck(9, true, 12.0), deck(2, false, 0.0)], 3.0);
        assert_eq!(a.status(3.0), Status::Paused(Pause::DeckChanged));
        assert_eq!(a.step(&[deck(1, true, 13.0), deck(2, false, 0.0)], 4.0), []);
        a.resume();
        assert!(a.running());

        let mut last = Autopilot::new(tracks(&[1]), preset(TransitionStyle::Blend), true);
        last.step(&[DeckView::default(); 2], 0.0);
        last.step(&[deck(1, false, 0.0), DeckView::default()], 0.1);
        last.step(&[deck(1, true, 100.0), DeckView::default()], 1.0);
        last.step(&[deck(1, false, 300.0), DeckView::default()], 10.0);
        assert_eq!(last.status(10.0), Status::Finished);
    }

    #[test]
    fn upcoming_tracks_can_be_replaced() {
        let mut a = ready(&[1, 2, 3], preset(TransitionStyle::Blend));
        a.replace_upcoming(tracks(&[7, 8]));
        assert_eq!(
            a.tracks().iter().map(|t| t.id).collect::<Vec<_>>(),
            [1, 7, 8]
        );
        let steps = a.step(&[deck(1, true, 12.0), deck(2, false, 0.0)], 3.0);
        assert!(has(&steps, &Step::Load { deck: B, track: 7 }));
    }

    #[test]
    fn a_recipe_pattern_plays_through_its_side_and_stops_at_the_end() {
        let mut recipe = preset(TransitionStyle::Cut);
        recipe.events.push(transition::Event {
            beat: 0.0,
            action: transition::Action::Pattern {
                side: Some(Side::In),
                pattern: PatternSpec {
                    unit: 2,
                    on: Some(true),
                    ..PatternSpec::default()
                },
            },
        });
        let mut a = ready(&[1, 2], recipe);
        let steps = a.step(&[deck(1, true, 297.0), deck(2, false, 0.0)], 3.0);
        assert!(steps.iter().any(|s| matches!(
            s,
            Step::Pattern {
                pattern: PatternSpec { unit: 2, .. },
                deck: Some(B)
            }
        )));
        let steps = a.step(&[deck(1, true, 299.0), deck(2, true, 2.0)], 5.0);
        assert!(has(&steps, &Step::FxOn { unit: 2, on: false }));
    }

    #[test]
    fn a_track_can_start_partway_in() {
        let mut set = tracks(&[1, 2]);
        set[0].start = Some(0.5);
        set[1].start = Some(0.25);
        let mut a = Autopilot::new(set, preset(TransitionStyle::Blend), true);
        a.step(&[DeckView::default(); 2], 0.0);
        let steps = a.step(&[deck(1, false, 0.0), DeckView::default()], 0.1);
        let seek = steps.iter().position(|s| {
            *s == Step::Seek {
                deck: A,
                seconds: 150.0,
            }
        });
        let play = steps.iter().position(|s| *s == Step::Play { deck: A });
        assert!(seek.unwrap() < play.unwrap(), "{steps:?}");
        a.step(&[deck(1, true, 151.0), DeckView::default()], 1.0);
        let steps = a.step(&[deck(1, true, 152.0), deck(2, false, 0.0)], 2.0);
        assert!(has(
            &steps,
            &Step::Seek {
                deck: B,
                seconds: 75.0
            }
        ));
    }

    #[test]
    fn hand_moves_on_set_decks_take_over() {
        assert!(takes_over(Action::Volume { channel: 0 }));
        assert!(takes_over(Action::PlayPause { deck: 1 }));
        assert!(takes_over(Action::Crossfader));
        assert!(!takes_over(Action::PlayPause { deck: 2 }));
        assert!(!takes_over(Action::MasterVolume));
    }
}
