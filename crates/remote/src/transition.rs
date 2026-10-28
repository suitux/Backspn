//! Transitions as recipes: lanes of control values and one-off events laid
//! out in beats from the start of the mix. Named styles build ready-made
//! recipes; an AI can also write its own.

use backspn_library::set_plan::{Transition, TransitionStyle};
use serde::{Deserialize, Serialize};

/// The two decks of a mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The track coming in.
    In,
    /// The track going out.
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Low,
    Mid,
    High,
}

/// A continuous control. Volume, EQ, FX amounts: 0–1 (EQ 0.5 is flat,
/// 0 kills the band). Filter: -1 (low-pass) to 1 (high-pass), 0 is off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "control")]
pub enum Control {
    Volume { side: Side },
    Eq { side: Side, band: Band },
    Filter { side: Side },
    FxWet { unit: usize },
    FxKnob { unit: usize, knob: usize },
}

impl Control {
    /// The value that leaves the sound untouched.
    pub fn neutral(self) -> f32 {
        match self {
            Self::Volume { .. } => 1.0,
            Self::Eq { .. } => 0.5,
            Self::Filter { .. } | Self::FxWet { .. } | Self::FxKnob { .. } => 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// Glides between points.
    #[default]
    Linear,
    /// Jumps to each point's value on its beat: fader cuts.
    Step,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub beat: f64,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lane {
    #[serde(flatten)]
    pub control: Control,
    #[serde(default)]
    pub shape: Shape,
    pub points: Vec<Point>,
}

impl Lane {
    fn new(control: Control, shape: Shape, points: &[(f64, f32)]) -> Self {
        Self {
            control,
            shape,
            points: points
                .iter()
                .map(|&(beat, value)| Point { beat, value })
                .collect(),
        }
    }

    /// The value at `beat`: the first value before the first point, the
    /// last after the last one.
    pub fn value_at(&self, beat: f64) -> Option<f32> {
        let first = self.points.first()?;
        if beat <= first.beat {
            return Some(first.value);
        }
        let after = self.points.iter().position(|p| p.beat > beat);
        let Some(i) = after else {
            return self.points.last().map(|p| p.value);
        };
        let (a, b) = (self.points[i - 1], self.points[i]);
        Some(match self.shape {
            Shape::Step => a.value,
            Shape::Linear => {
                let x = ((beat - a.beat) / (b.beat - a.beat)) as f32;
                a.value + (b.value - a.value) * x
            }
        })
    }
}

/// A one-off action. Effects are named as in the effect list
/// (`Echo`, `VinylBrake`…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "do")]
pub enum Action {
    /// Sets up a drum pattern unit, playing through `side`'s channel.
    Pattern {
        #[serde(default)]
        side: Option<Side>,
        #[serde(flatten)]
        pattern: PatternSpec,
    },
    Play {
        side: Side,
    },
    Stop {
        side: Side,
    },
    FxEffect {
        unit: usize,
        effect: String,
    },
    FxAssign {
        unit: usize,
        side: Side,
        on: bool,
    },
    FxOn {
        unit: usize,
        on: bool,
    },
    Loop {
        side: Side,
        beats: f64,
    },
    LoopOff {
        side: Side,
    },
    BeatJump {
        side: Side,
        beats: f64,
    },
}

/// Cells a pattern can have over its one-bar cycle.
pub const PATTERN_CELLS: [usize; 3] = [16, 32, 64];
/// Stored patterns in each kit.
pub const KIT_PATTERNS: usize = 8;

/// What a drum pattern unit plays. Every field but `unit` is optional and
/// leaves that part as it is.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PatternSpec {
    pub unit: usize,
    /// Kit name, as `library_overview` lists it.
    #[serde(default)]
    pub kit: Option<String>,
    /// One of the kit's stored patterns, 1–8.
    #[serde(default)]
    pub pattern: Option<usize>,
    /// Steps per sound name over one bar: 16, 32 or 64 cells of `x` (hit)
    /// and `.` (rest), e.g. `"x...x...x...x..."`.
    #[serde(default)]
    pub steps: std::collections::BTreeMap<String, String>,
    /// Silences the kit's sounds not named in `steps`.
    #[serde(default)]
    pub only_these: bool,
    #[serde(default)]
    pub on: Option<bool>,
    /// 0–1.
    #[serde(default)]
    pub volume: Option<f32>,
    /// 0–1.
    #[serde(default)]
    pub decay: Option<f32>,
    /// 0–1, 0.5 is the sound's own pitch.
    #[serde(default)]
    pub pitch: Option<f32>,
    /// Ducks the track under the pattern's hits.
    #[serde(default)]
    pub duck: Option<bool>,
    /// Repeats the current step while on.
    #[serde(default)]
    pub roll: Option<bool>,
}

impl PatternSpec {
    pub fn problems(&self, fx_units: usize) -> Vec<String> {
        let mut problems = Vec::new();
        if self.unit >= fx_units {
            problems.push(format!(
                "effect unit {} does not exist (0–{})",
                self.unit,
                fx_units - 1
            ));
        }
        if self
            .pattern
            .is_some_and(|p| !(1..=KIT_PATTERNS).contains(&p))
        {
            problems.push(format!("pattern must be 1–{KIT_PATTERNS}"));
        }
        for (sound, steps) in &self.steps {
            let cells = steps.chars().count();
            if !PATTERN_CELLS.contains(&cells) || steps.chars().any(|c| !"xX.-".contains(c)) {
                problems.push(format!(
                    "steps for {sound} must be 16, 32 or 64 cells of x and ."
                ));
            }
        }
        let unit_range = [
            ("volume", self.volume),
            ("decay", self.decay),
            ("pitch", self.pitch),
        ];
        for (name, value) in unit_range {
            if value.is_some_and(|v| !(0.0..=1.0).contains(&v)) {
                problems.push(format!("{name} must be 0–1"));
            }
        }
        problems
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub beat: f64,
    #[serde(flatten)]
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    /// Beats from the start of the mix to the end, when the outgoing deck
    /// stops and every control used goes back to neutral.
    pub beats: f64,
    #[serde(default)]
    pub lanes: Vec<Lane>,
    /// The incoming track starts on beat 0 unless an event plays it.
    #[serde(default)]
    pub events: Vec<Event>,
}

impl Recipe {
    /// Control values at `beat`.
    pub fn values_at(&self, beat: f64) -> impl Iterator<Item = (Control, f32)> + '_ {
        self.lanes
            .iter()
            .filter_map(move |l| l.value_at(beat).map(|v| (l.control, v)))
    }

    /// Events after `from` up to and including `to`, in order; `None`
    /// starts before the first beat.
    pub fn events_between(&self, from: Option<f64>, to: f64) -> Vec<&Action> {
        let mut events: Vec<&Event> = self
            .events
            .iter()
            .filter(|e| from.is_none_or(|f| e.beat > f) && e.beat <= to)
            .collect();
        events.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        events.into_iter().map(|e| &e.action).collect()
    }

    /// Beat at which the incoming track starts.
    pub fn in_start(&self) -> f64 {
        self.events
            .iter()
            .filter(|e| e.action == Action::Play { side: Side::In })
            .map(|e| e.beat)
            .fold(None, |m: Option<f64>, b| Some(m.map_or(b, |m| m.min(b))))
            .unwrap_or(0.0)
    }

    /// Effect units the recipe touches, to switch off at the end.
    pub fn fx_units(&self) -> Vec<usize> {
        let mut units: Vec<usize> = self
            .lanes
            .iter()
            .filter_map(|l| match l.control {
                Control::FxWet { unit } | Control::FxKnob { unit, .. } => Some(unit),
                _ => None,
            })
            .chain(self.events.iter().filter_map(|e| match e.action {
                Action::FxEffect { unit, .. }
                | Action::FxAssign { unit, .. }
                | Action::FxOn { unit, .. } => Some(unit),
                Action::Pattern { ref pattern, .. } => Some(pattern.unit),
                _ => None,
            }))
            .collect();
        units.sort_unstable();
        units.dedup();
        units
    }

    /// Problems that would stop the recipe from playing as written.
    pub fn check(&self, fx_units: usize) -> Vec<String> {
        let mut problems = Vec::new();
        if !(self.beats > 0.0 && self.beats <= 256.0) {
            problems.push(format!(
                "beats must be between 0 and 256, not {}",
                self.beats
            ));
        }
        for unit in self.fx_units() {
            if unit >= fx_units {
                problems.push(format!(
                    "effect unit {unit} does not exist (0–{})",
                    fx_units - 1
                ));
            }
        }
        for event in &self.events {
            if let Action::Pattern { pattern, .. } = &event.action {
                problems.extend(pattern.problems(fx_units));
            }
        }
        for lane in &self.lanes {
            if lane.points.is_empty() {
                problems.push(format!("{:?} has no points", lane.control));
            }
            if lane.points.windows(2).any(|w| w[1].beat < w[0].beat) {
                problems.push(format!("{:?} points are not in beat order", lane.control));
            }
        }
        problems
    }
}

const ECHO_UNIT: usize = 0;

fn volume(side: Side) -> Control {
    Control::Volume { side }
}

fn low(side: Side) -> Control {
    Control::Eq {
        side,
        band: Band::Low,
    }
}

fn filter(side: Side) -> Control {
    Control::Filter { side }
}

fn event(beat: f64, action: Action) -> Event {
    Event { beat, action }
}

/// Bass out on the incoming track until `at`, then swapped in one go.
fn bass_swap(at: f64) -> [Lane; 2] {
    [
        Lane::new(low(Side::In), Shape::Step, &[(0.0, 0.0), (at, 0.5)]),
        Lane::new(low(Side::Out), Shape::Step, &[(0.0, 0.5), (at, 0.0)]),
    ]
}

/// The outgoing track goes through an effect while the next drops in.
fn drop_in(effect: &str, drop: f64, beats: f64, wet: &[(f64, f32)]) -> Recipe {
    Recipe {
        beats,
        lanes: vec![
            Lane::new(volume(Side::In), Shape::Step, &[(0.0, 0.0), (drop, 1.0)]),
            Lane::new(
                volume(Side::Out),
                Shape::Linear,
                &[(drop, 1.0), (beats, 0.0)],
            ),
            Lane::new(Control::FxWet { unit: ECHO_UNIT }, Shape::Linear, wet),
        ],
        events: vec![
            event(
                0.0,
                Action::FxEffect {
                    unit: ECHO_UNIT,
                    effect: effect.into(),
                },
            ),
            event(
                0.0,
                Action::FxAssign {
                    unit: ECHO_UNIT,
                    side: Side::Out,
                    on: true,
                },
            ),
            event(
                0.0,
                Action::FxOn {
                    unit: ECHO_UNIT,
                    on: true,
                },
            ),
            event(drop, Action::Play { side: Side::In }),
        ],
    }
}

fn filter_sweep(beats: f64) -> Recipe {
    let swap = beats * 0.75;
    Recipe {
        beats,
        lanes: [
            Lane::new(
                volume(Side::In),
                Shape::Linear,
                &[(0.0, 0.0), (beats / 4.0, 1.0)],
            ),
            Lane::new(
                volume(Side::Out),
                Shape::Linear,
                &[(swap, 1.0), (beats, 0.0)],
            ),
            Lane::new(filter(Side::In), Shape::Linear, &[(0.0, -0.8), (swap, 0.0)]),
            Lane::new(
                filter(Side::Out),
                Shape::Linear,
                &[(0.0, 0.0), (beats, 0.8)],
            ),
        ]
        .into_iter()
        .chain(bass_swap(swap))
        .collect(),
        events: Vec::new(),
    }
}

fn chop(beats: f64) -> Recipe {
    let half = beats / 2.0;
    // The incoming track cuts in on off-beats, then on whole beats,
    // before the faders swap for good.
    let mut cuts_in = vec![(0.0, 0.0)];
    let mut cuts_out = vec![(0.0, 1.0)];
    let mut beat = half;
    while beat < beats - 1.0 {
        cuts_in.extend([(beat, 1.0), (beat + 0.5, 0.0)]);
        cuts_out.extend([(beat, 0.0), (beat + 0.5, 1.0)]);
        beat += if beat < beats * 0.75 { 2.0 } else { 1.0 };
    }
    cuts_in.push((beats - 1.0, 1.0));
    cuts_out.push((beats - 1.0, 0.0));
    Recipe {
        beats,
        lanes: [
            Lane::new(volume(Side::In), Shape::Step, &cuts_in),
            Lane::new(volume(Side::Out), Shape::Step, &cuts_out),
        ]
        .into_iter()
        .chain(bass_swap(beats - 1.0))
        .collect(),
        events: Vec::new(),
    }
}

/// The ready-made recipe for a transition style.
pub fn preset(transition: Transition) -> Recipe {
    let beats = transition.beats();
    let half = beats / 2.0;
    match transition.style {
        TransitionStyle::Blend => Recipe {
            beats,
            lanes: [
                Lane::new(volume(Side::In), Shape::Linear, &[(0.0, 0.0), (half, 1.0)]),
                Lane::new(
                    volume(Side::Out),
                    Shape::Linear,
                    &[(half, 1.0), (beats, 0.0)],
                ),
            ]
            .into_iter()
            .chain(bass_swap(half))
            .collect(),
            events: Vec::new(),
        },
        TransitionStyle::Fade => Recipe {
            beats,
            lanes: vec![
                Lane::new(volume(Side::In), Shape::Linear, &[(0.0, 0.0), (beats, 1.0)]),
                Lane::new(
                    volume(Side::Out),
                    Shape::Linear,
                    &[(0.0, 1.0), (beats, 0.0)],
                ),
            ],
            events: Vec::new(),
        },
        TransitionStyle::Cut => Recipe {
            beats,
            lanes: vec![
                Lane::new(volume(Side::In), Shape::Step, &[(0.0, 0.0), (beats, 1.0)]),
                Lane::new(volume(Side::Out), Shape::Step, &[(0.0, 1.0), (beats, 0.0)]),
            ],
            events: Vec::new(),
        },
        TransitionStyle::FilterSweep => filter_sweep(beats),
        TransitionStyle::Chop => chop(beats),
        TransitionStyle::LoopOut => Recipe {
            beats,
            lanes: [
                Lane::new(volume(Side::In), Shape::Linear, &[(0.0, 0.0), (half, 1.0)]),
                Lane::new(
                    volume(Side::Out),
                    Shape::Linear,
                    &[(half, 1.0), (beats, 0.0)],
                ),
                Lane::new(
                    filter(Side::Out),
                    Shape::Linear,
                    &[(half, 0.0), (beats, 0.6)],
                ),
            ]
            .into_iter()
            .chain(bass_swap(half))
            .collect(),
            events: vec![event(
                0.0,
                Action::Loop {
                    side: Side::Out,
                    beats: 4.0,
                },
            )],
        },
        TransitionStyle::EchoOut => drop_in("Echo", 4.0, beats, &[(0.0, 0.0), (3.0, 0.8)]),
        TransitionStyle::Brake => drop_in("VinylBrake", 4.0, beats, &[(0.0, 1.0)]),
        TransitionStyle::Backspin => drop_in("BackSpin", 4.0, beats, &[(0.0, 1.0)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLES: [TransitionStyle; 9] = [
        TransitionStyle::Blend,
        TransitionStyle::Cut,
        TransitionStyle::Fade,
        TransitionStyle::FilterSweep,
        TransitionStyle::Chop,
        TransitionStyle::EchoOut,
        TransitionStyle::LoopOut,
        TransitionStyle::Brake,
        TransitionStyle::Backspin,
    ];

    fn transition(style: TransitionStyle) -> Transition {
        Transition { style, bars: 16 }
    }

    fn value(recipe: &Recipe, control: Control, beat: f64) -> f32 {
        recipe
            .values_at(beat)
            .find(|(c, _)| *c == control)
            .map(|(_, v)| v)
            .unwrap()
    }

    #[test]
    fn lanes_glide_or_step_and_hold_their_ends() {
        let glide = Lane::new(volume(Side::In), Shape::Linear, &[(4.0, 0.0), (8.0, 1.0)]);
        assert_eq!(glide.value_at(0.0), Some(0.0));
        assert_eq!(glide.value_at(6.0), Some(0.5));
        assert_eq!(glide.value_at(20.0), Some(1.0));
        let cut = Lane::new(volume(Side::In), Shape::Step, &[(4.0, 0.0), (8.0, 1.0)]);
        assert_eq!(cut.value_at(7.9), Some(0.0));
        assert_eq!(cut.value_at(8.0), Some(1.0));
        assert_eq!(
            Lane::new(volume(Side::In), Shape::Step, &[]).value_at(1.0),
            None
        );
    }

    #[test]
    fn every_style_starts_with_the_old_track_and_ends_with_the_new() {
        for style in STYLES {
            let r = preset(transition(style));
            assert!(r.check(4).is_empty(), "{style:?}: {:?}", r.check(4));
            assert!(
                (r.beats - transition(style).beats()).abs() < 1e-9,
                "{style:?}"
            );
            assert_eq!(value(&r, volume(Side::In), 0.0), 0.0, "{style:?}");
            assert_eq!(value(&r, volume(Side::Out), 0.0), 1.0, "{style:?}");
            assert_eq!(value(&r, volume(Side::In), r.beats), 1.0, "{style:?}");
            assert_eq!(value(&r, volume(Side::Out), r.beats), 0.0, "{style:?}");
        }
    }

    #[test]
    fn blend_swaps_the_bass_halfway() {
        let r = preset(transition(TransitionStyle::Blend));
        assert_eq!(value(&r, low(Side::In), 31.9), 0.0);
        assert_eq!(value(&r, low(Side::In), 32.0), 0.5);
        assert_eq!(value(&r, low(Side::Out), 32.0), 0.0);
        assert_eq!(value(&r, volume(Side::In), 16.0), 0.5);
    }

    #[test]
    fn chop_cuts_on_the_beat() {
        let r = preset(transition(TransitionStyle::Chop));
        let cuts: Vec<f32> = (64..128)
            .map(|half_beats| value(&r, volume(Side::In), f64::from(half_beats) / 2.0))
            .collect();
        let changes = cuts
            .windows(2)
            .filter(|w| (w[0] - w[1]).abs() > 0.1)
            .count();
        assert!(changes > 20, "{changes}");
        assert_eq!(value(&r, volume(Side::In), 32.0), 1.0);
        assert_eq!(value(&r, volume(Side::In), 32.5), 0.0);
    }

    #[test]
    fn effect_styles_drop_the_new_track_in() {
        let r = preset(transition(TransitionStyle::EchoOut));
        assert_eq!(r.in_start(), 4.0);
        assert_eq!(r.fx_units(), [ECHO_UNIT]);
        let first = r.events_between(None, 0.0);
        assert!(first.contains(&&Action::FxOn {
            unit: ECHO_UNIT,
            on: true
        }));
        assert_eq!(r.events_between(Some(0.0), 3.9), Vec::<&Action>::new());
        assert_eq!(
            r.events_between(Some(3.9), 4.0),
            [&Action::Play { side: Side::In }]
        );
        assert_eq!(preset(transition(TransitionStyle::Blend)).in_start(), 0.0);
    }

    #[test]
    fn a_recipe_can_bring_in_a_drum_pattern() {
        let r: Recipe = serde_json::from_str(
            r#"{"beats": 32, "events": [
                {"beat": 0, "do": "pattern", "side": "out", "unit": 2, "kit": "808",
                 "steps": {"Kick": "x...x...x...x...", "Clap": "....x.......x..."},
                 "only_these": true, "volume": 0.7, "on": true},
                {"beat": 16, "do": "pattern", "unit": 2, "roll": true}]}"#,
        )
        .unwrap();
        let Action::Pattern { side, pattern } = &r.events[0].action else {
            panic!("{:?}", r.events[0]);
        };
        assert_eq!(*side, Some(Side::Out));
        assert_eq!(pattern.kit.as_deref(), Some("808"));
        assert_eq!(pattern.steps["Kick"], "x...x...x...x...");
        assert!(pattern.only_these);
        assert_eq!(r.fx_units(), [2]);
        assert!(r.check(4).is_empty(), "{:?}", r.check(4));
        let bad = PatternSpec {
            unit: 9,
            pattern: Some(0),
            steps: [("Kick".to_owned(), "x..x".to_owned())].into(),
            volume: Some(2.0),
            ..PatternSpec::default()
        };
        assert_eq!(bad.problems(4).len(), 4, "{:?}", bad.problems(4));
    }

    #[test]
    fn written_recipes_read_from_json_and_get_checked() {
        let r: Recipe = serde_json::from_str(
            r#"{"beats": 32,
                "lanes": [
                  {"control": "volume", "side": "in", "points": [{"beat": 0, "value": 0}, {"beat": 8, "value": 1}]},
                  {"control": "eq", "side": "out", "band": "low", "shape": "step",
                   "points": [{"beat": 16, "value": 0.5}, {"beat": 16, "value": 0}]},
                  {"control": "fx_wet", "unit": 7, "points": [{"beat": 4, "value": 1}, {"beat": 2, "value": 0}]}
                ],
                "events": [{"beat": 28, "do": "fx_effect", "unit": 1, "effect": "Echo"},
                           {"beat": 28, "do": "loop", "side": "out", "beats": 1}]}"#,
        )
        .unwrap();
        assert_eq!(r.lanes[1].shape, Shape::Step);
        assert_eq!(
            r.events[1].action,
            Action::Loop {
                side: Side::Out,
                beats: 1.0
            }
        );
        assert_eq!(r.fx_units(), [1, 7]);
        let problems = r.check(4);
        assert_eq!(problems.len(), 2, "{problems:?}");
    }
}
