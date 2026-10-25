use backspn_dsp::Smoothed;

use super::EffectKind;
use crate::{Context, Effect};

type Shape = &'static [(f32, f32)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Knob(usize),
    DryWet,
    Button(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wave {
    Square,
    SawUp,
    SawDown,
    Triangle,
}

#[derive(Debug, Clone, Copy)]
struct Lfo {
    wave: Wave,
    beats: f64,
}

impl Lfo {
    fn at(self, beat: f64) -> f64 {
        let p = (beat / self.beats).rem_euclid(1.0);
        match self.wave {
            Wave::Square => {
                if p < 0.5 {
                    -1.0
                } else {
                    1.0
                }
            }
            Wave::SawUp => 2.0 * p - 1.0,
            Wave::SawDown => 1.0 - 2.0 * p,
            Wave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Path {
    target: Target,
    shape: Shape,
    lfos: &'static [(usize, Shape)],
}

#[derive(Debug, Clone, Copy)]
struct SubSpec {
    kind: EffectKind,
    switch: Option<Shape>,
    paths: &'static [Path],
}

fn eval(shape: Shape, k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    let Some(&(_, first)) = shape.first() else {
        return 0.0;
    };
    let mut prev = (0.0, first);
    for &(x, y) in shape {
        if k <= x {
            let span = x - prev.0;
            return if span <= 0.0 {
                y
            } else {
                prev.1 + (y - prev.1) * (k - prev.0) / span
            };
        }
        prev = (x, y);
    }
    prev.1
}

const GAP: f32 = 0.012;

fn in_gap(k: f32) -> bool {
    (k - 0.5).abs() < GAP
}

macro_rules! knob {
    ($i:expr, $shape:expr $(,)?) => {
        Path {
            target: Target::Knob($i),
            shape: $shape,
            lfos: &[],
        }
    };
}

macro_rules! dw {
    ($shape:expr $(,)?) => {
        Path {
            target: Target::DryWet,
            shape: $shape,
            lfos: &[],
        }
    };
}

macro_rules! button {
    ($i:expr, $shape:expr $(,)?) => {
        Path {
            target: Target::Button($i),
            shape: $shape,
            lfos: &[],
        }
    };
}

const ONE: Shape = &[(0.0, 1.0)];
const ZERO: Shape = &[(0.0, 0.0)];
const HALF: Shape = &[(0.0, 0.5)];
const OUT_OF_GAP: Shape = &[
    (0.0, 1.0),
    (0.488, 1.0),
    (0.489, 0.0),
    (0.511, 0.0),
    (0.512, 1.0),
];
const LEFT_HALF: Shape = &[(0.0, 1.0), (0.49, 1.0), (0.5, 0.0)];
const RIGHT_HALF: Shape = &[(0.0, 0.0), (0.5, 0.0), (0.51, 1.0)];
const EXTREMES: Shape = &[
    (0.0, 1.0),
    (0.01, 1.0),
    (0.011, 0.0),
    (0.989, 0.0),
    (0.99, 1.0),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recipe {
    Tunnel,
    ZapCut,
    GrainSweep,
    BassMachine,
    StretchFast,
    StretchSlow,
    Polar,
    Horizon,
    Zap,
    Dark,
    Flight,
}

#[allow(clippy::too_many_lines)]
fn recipe(r: Recipe) -> (&'static [SubSpec], &'static [Lfo]) {
    use EffectKind as K;
    match r {
        Recipe::Tunnel => (
            &[
                SubSpec {
                    kind: K::Delay,
                    switch: None,
                    paths: &[
                        knob!(0, &[(0.0, 0.75)]),
                        knob!(1, &[(0.0, 0.868)]),
                        knob!(2, &[(0.0, 0.31), (0.05, 1.0)]),
                        dw!(&[
                            (0.0, 0.76),
                            (0.38, 0.14),
                            (0.49, 0.76),
                            (0.51, 0.76),
                            (0.63, 0.14),
                            (1.0, 0.76)
                        ]),
                        button!(
                            0,
                            &[
                                (0.0, 0.0),
                                (0.01, 0.0),
                                (0.011, 1.0),
                                (0.989, 1.0),
                                (0.99, 0.0)
                            ]
                        ),
                        button!(1, EXTREMES),
                    ],
                },
                SubSpec {
                    kind: K::SvfFilter,
                    switch: Some(ONE),
                    paths: &[
                        button!(0, ONE),
                        knob!(
                            0,
                            &[
                                (0.0, 0.31),
                                (0.25, 0.4),
                                (0.5, 0.5),
                                (0.75, 0.75),
                                (1.0, 0.85),
                            ],
                        ),
                        knob!(1, &[(0.0, 0.75), (0.5, 0.5), (1.0, 0.75)]),
                        knob!(2, ONE),
                        dw!(OUT_OF_GAP),
                    ],
                },
                SubSpec {
                    kind: K::PhaserHold,
                    switch: Some(ONE),
                    paths: &[
                        knob!(0, &[(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)]),
                        knob!(1, &[(0.0, 0.37), (0.5, 0.08), (1.0, 0.37)]),
                        dw!(&[(0.0, 0.5), (0.38, 0.5), (0.5, 0.0), (0.62, 0.5), (1.0, 0.5)]),
                        button!(0, ONE),
                        button!(1, ONE),
                    ],
                },
            ],
            &[],
        ),
        Recipe::ZapCut => (
            &[
                SubSpec {
                    kind: K::Slicer,
                    switch: Some(&[
                        (0.0, 1.0),
                        (0.374, 1.0),
                        (0.375, 0.0),
                        (0.625, 0.0),
                        (0.626, 1.0),
                    ]),
                    paths: &[
                        knob!(0, &[(0.0, 0.8), (0.5, 0.9), (0.51, 1.0)]),
                        knob!(
                            1,
                            &[
                                (0.0, 0.25),
                                (0.125, 0.01),
                                (0.25, 0.25),
                                (0.5, 0.25),
                                (0.51, 0.9),
                            ],
                        ),
                        knob!(2, &[(0.0, 0.6), (0.2, 0.1)]),
                        dw!(&[
                            (0.0, 0.88),
                            (0.374, 0.88),
                            (0.375, 0.0),
                            (0.625, 0.0),
                            (0.626, 0.88),
                        ]),
                        button!(0, OUT_OF_GAP),
                    ],
                },
                SubSpec {
                    kind: K::FlangerPulse,
                    switch: None,
                    paths: &[
                        knob!(0, &[(0.0, 0.56), (0.5, 0.36), (1.0, 0.29)]),
                        knob!(1, &[(0.0, 0.6), (0.5, 0.5)]),
                        knob!(2, &[(0.0, 0.69), (0.375, 0.9)]),
                        dw!(&[(0.0, 0.84), (0.47, 0.84), (0.5, 0.0), (0.53, 0.84)]),
                        button!(0, ONE),
                        button!(1, LEFT_HALF),
                    ],
                },
                SubSpec {
                    kind: K::DelayT3,
                    switch: None,
                    paths: &[
                        knob!(0, &[(0.0, 0.53), (0.44, 0.53), (0.441, 0.63)]),
                        knob!(1, &[(0.0, 0.45)]),
                        knob!(2, &[(0.0, 0.8)]),
                        dw!(&[(0.0, 0.78), (0.2, 0.0), (0.8, 0.0), (1.0, 0.78)]),
                        button!(1, EXTREMES),
                    ],
                },
                SubSpec {
                    kind: K::SvfFilter,
                    switch: Some(&[
                        (0.0, 1.0),
                        (0.44, 1.0),
                        (0.441, 0.0),
                        (0.75, 0.0),
                        (0.751, 1.0),
                    ]),
                    paths: &[knob!(
                        2,
                        &[
                            (0.0, 0.84),
                            (0.489, 0.84),
                            (0.49, 0.0),
                            (0.51, 0.0),
                            (0.511, 0.84),
                        ],
                    )],
                },
            ],
            &[],
        ),
        Recipe::GrainSweep => (
            &[
                SubSpec {
                    kind: K::PhaserHold,
                    switch: Some(ONE),
                    paths: &[
                        knob!(
                            0,
                            &[
                                (0.0, 0.51),
                                (0.25, 0.16),
                                (0.5, 0.5),
                                (0.76, 0.83),
                                (1.0, 0.56),
                            ],
                        ),
                        knob!(
                            1,
                            &[
                                (0.0, 0.24),
                                (0.5, 0.0),
                                (0.62, 0.07),
                                (0.75, 0.4),
                                (0.87, 0.23),
                                (1.0, 0.3),
                            ],
                        ),
                        dw!(&[
                            (0.0, 0.1),
                            (0.38, 0.5),
                            (0.5, 0.0),
                            (0.76, 0.46),
                            (1.0, 0.0),
                        ]),
                        button!(0, ONE),
                        button!(1, LEFT_HALF),
                    ],
                },
                SubSpec {
                    kind: K::AutoStepStutter,
                    switch: Some(&[
                        (0.0, 1.0),
                        (0.374, 1.0),
                        (0.375, 0.0),
                        (0.625, 0.0),
                        (0.626, 1.0),
                    ]),
                    paths: &[
                        knob!(0, &[(0.0, 0.55), (0.5, 0.55), (0.51, 1.0), (0.6, 0.8)]),
                        knob!(1, &[(0.0, 0.88)]),
                        knob!(2, &[(0.0, 0.0), (0.25, 0.5)]),
                        dw!(&[
                            (0.0, 0.3),
                            (0.375, 0.23),
                            (0.376, 0.0),
                            (0.625, 0.0),
                            (0.626, 0.2),
                            (1.0, 0.29),
                        ]),
                        button!(0, LEFT_HALF),
                        button!(1, &[(0.0, 0.0), (0.88, 0.0), (0.881, 1.0)]),
                    ],
                },
                SubSpec {
                    kind: K::TapeDelay,
                    switch: None,
                    paths: &[
                        knob!(0, &[(0.0, 0.0), (0.499, 0.7), (0.5, 0.0), (1.0, 0.7)]),
                        knob!(1, &[(0.0, 0.47), (0.5, 0.0), (1.0, 0.7)]),
                        knob!(2, &[(0.0, 0.63), (0.88, 0.63), (0.881, 0.71)]),
                        dw!(&[(0.0, 0.16), (0.12, 0.0), (0.83, 0.0), (1.0, 0.18)]),
                    ],
                },
            ],
            &[],
        ),
        Recipe::BassMachine => (
            &[
                SubSpec {
                    kind: K::Flanger,
                    switch: None,
                    paths: &[
                        knob!(
                            0,
                            &[
                                (0.0, 0.15),
                                (0.3, 0.3),
                                (0.6, 0.15),
                                (0.88, 1.0),
                                (1.0, 0.3)
                            ]
                        ),
                        knob!(1, &[(0.0, 1.0), (0.3, 1.0), (0.5, 0.0), (1.0, 0.75)]),
                        knob!(2, &[(0.0, 1.0), (0.5, 0.25), (1.0, 0.86)]),
                        dw!(&[(0.0, 1.0), (0.38, 1.0), (0.5, 0.0), (0.62, 1.0)]),
                        button!(1, ONE),
                    ],
                },
                SubSpec {
                    kind: K::GlassReverb,
                    switch: None,
                    paths: &[
                        knob!(0, &[(0.0, 0.69)]),
                        knob!(1, &[(0.0, 0.9), (0.43, 0.15), (0.57, 0.15), (0.75, 0.57)]),
                        knob!(2, &[(0.0, 1.0), (0.02, 0.69)]),
                        dw!(&[(0.0, 0.97), (0.38, 0.5), (0.5, 0.0), (0.6, 0.4)]),
                        button!(0, &[(0.0, 1.0), (0.005, 1.0), (0.006, 0.0)]),
                    ],
                },
                SubSpec {
                    kind: K::AutoStepStutter,
                    switch: None,
                    paths: &[
                        knob!(0, &[(0.0, 1.0), (0.125, 0.8), (0.25, 1.0), (0.75, 0.8)]),
                        knob!(1, &[(0.0, 1.0), (0.02, 0.5)]),
                        knob!(2, &[(0.0, 0.5), (0.94, 0.5), (0.941, 0.0)]),
                        dw!(&[
                            (0.0, 1.0),
                            (0.2, 0.67),
                            (0.35, 1.0),
                            (0.488, 1.0),
                            (0.489, 0.0),
                            (0.511, 0.0),
                            (0.512, 1.0),
                        ]),
                        button!(
                            1,
                            &[
                                (0.0, 1.0),
                                (0.12, 1.0),
                                (0.121, 0.0),
                                (0.63, 0.0),
                                (0.631, 1.0),
                                (0.75, 1.0),
                                (0.751, 0.0),
                                (0.88, 0.0),
                                (0.881, 1.0),
                            ],
                        ),
                    ],
                },
            ],
            &[],
        ),
        Recipe::StretchFast | Recipe::StretchSlow => {
            let fast = r == Recipe::StretchFast;
            (
                if fast {
                    &[
                        SubSpec {
                            kind: K::ReverseGrain,
                            switch: None,
                            paths: &[
                                knob!(0, &[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)]),
                                knob!(
                                    1,
                                    &[(0.0, 0.0), (0.25, 1.0), (0.5, 0.5), (0.75, 1.0), (1.0, 0.0)],
                                ),
                                knob!(2, &[(0.0, 0.0), (0.18, 1.0), (0.82, 1.0), (1.0, 0.0)]),
                                dw!(OUT_OF_GAP),
                                button!(0, &[(0.0, 0.0), (0.49, 0.0), (0.491, 1.0)]),
                                button!(1, &[(0.0, 1.0), (0.49, 1.0), (0.491, 0.0)]),
                            ],
                        },
                        SubSpec {
                            kind: K::TapeDelay,
                            switch: None,
                            paths: &[
                                knob!(0, &[(0.0, 0.0), (0.375, 0.78), (0.75, 0.78), (1.0, 1.0)]),
                                knob!(1, &[(0.0, 1.0), (0.25, 0.4), (0.75, 0.4), (1.0, 1.0)]),
                                knob!(2, &[(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)]),
                                dw!(&[
                                    (0.0, 1.0),
                                    (0.125, 0.56),
                                    (0.25, 0.24),
                                    (0.45, 0.0),
                                    (0.55, 0.0),
                                    (0.75, 0.24),
                                    (0.875, 0.56),
                                    (1.0, 1.0),
                                ]),
                                button!(1, EXTREMES),
                            ],
                        },
                        SubSpec {
                            kind: K::PhaserHold,
                            switch: None,
                            paths: &[
                                knob!(0, &[(0.0, 1.0), (0.5, 0.5), (1.0, 1.0)]),
                                knob!(1, HALF),
                                dw!(&[
                                    (0.0, 0.77),
                                    (0.25, 0.0),
                                    (0.4, 0.7),
                                    (0.5, 0.0),
                                    (0.6, 0.7),
                                    (0.75, 0.0),
                                    (1.0, 0.77),
                                ]),
                                button!(0, ONE),
                                button!(1, ONE),
                            ],
                        },
                    ]
                } else {
                    &[
                        SubSpec {
                            kind: K::ReverseGrain,
                            switch: None,
                            paths: &[
                                knob!(0, &[(0.0, 0.0), (0.5, 0.5), (1.0, 0.0)]),
                                knob!(1, &[(0.0, 0.49), (0.5, 1.0), (0.51, 0.33), (1.0, 0.78)]),
                                knob!(2, ONE),
                                dw!(OUT_OF_GAP),
                                button!(0, RIGHT_HALF),
                                button!(1, LEFT_HALF),
                            ],
                        },
                        SubSpec {
                            kind: K::TapeDelay,
                            switch: None,
                            paths: &[
                                knob!(
                                    0,
                                    &[
                                        (0.0, 0.0),
                                        (0.38, 0.65),
                                        (0.5, 0.65),
                                        (0.51, 0.39),
                                        (0.75, 0.39),
                                        (1.0, 0.61),
                                    ],
                                ),
                                knob!(1, &[(0.0, 1.0), (0.25, 0.4), (0.75, 0.4), (1.0, 1.0)]),
                                knob!(2, &[(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)]),
                                dw!(&[(0.0, 0.4), (0.45, 0.0), (0.6, 0.0), (1.0, 0.6)]),
                            ],
                        },
                        SubSpec {
                            kind: K::PhaserHold,
                            switch: None,
                            paths: &[
                                knob!(0, &[(0.0, 1.0), (0.5, 0.5), (1.0, 1.0)]),
                                knob!(1, HALF),
                                dw!(&[(0.0, 0.38), (0.5, 0.0), (0.501, 0.38), (1.0, 0.0)]),
                            ],
                        },
                    ]
                },
                &[],
            )
        }
        Recipe::Polar => (
            &[
                SubSpec {
                    kind: K::PhaserHold,
                    switch: None,
                    paths: &[
                        Path {
                            target: Target::Knob(0),
                            shape: &[(0.0, 0.1), (0.125, 0.1), (0.875, 0.9)],
                            lfos: &[(0, &[(0.0, -0.1), (0.5, 0.0), (1.0, 0.1)])],
                        },
                        knob!(1, &[(0.0, 1.0), (0.18, 0.0), (0.875, 0.0), (0.876, 0.73)]),
                        Path {
                            target: Target::DryWet,
                            shape: OUT_OF_GAP_HALF,
                            lfos: &[(0, &[(0.0, -0.21), (0.5, 0.0), (1.0, 0.21)])],
                        },
                        button!(1, LEFT_HALF),
                        button!(0, RIGHT_HALF),
                    ],
                },
                SubSpec {
                    kind: K::ReverbT3,
                    switch: None,
                    paths: &[
                        Path {
                            target: Target::Knob(0),
                            shape: &[(0.0, 0.35), (0.3, 0.0), (0.7, 0.0), (1.0, 0.35)],
                            lfos: &[(1, &[(0.0, 0.1)])],
                        },
                        knob!(1, &[(0.0, 0.59), (0.3, 0.49), (0.7, 0.49), (1.0, 0.76)]),
                        knob!(2, &[(0.0, 0.27), (0.5, 0.5), (1.0, 0.73)]),
                        Path {
                            target: Target::DryWet,
                            shape: &[(0.0, 0.13), (0.5, 0.05), (1.0, 0.13)],
                            lfos: &[(1, &[(0.0, 0.0), (0.57, 0.0), (0.75, 0.25), (1.0, 0.47)])],
                        },
                    ],
                },
                SubSpec {
                    kind: K::FormantFilter,
                    switch: None,
                    paths: &[
                        Path {
                            target: Target::Knob(0),
                            shape: &[(0.0, 0.75)],
                            lfos: &[(0, &[(0.0, 0.26), (0.5, 0.26), (0.51, -0.26)])],
                        },
                        knob!(1, &[(0.0, 0.79), (0.5, 0.15), (1.0, 0.23)]),
                        knob!(2, &[(0.0, 1.0), (0.5, 1.0), (1.0, 0.15)]),
                        Path {
                            target: Target::DryWet,
                            shape: &[(0.0, 0.5), (0.3, 0.0), (0.7, 0.0), (1.0, 0.5)],
                            lfos: &[(1, &[(0.0, 0.35), (1.0, 0.0)])],
                        },
                        button!(0, LEFT_HALF),
                    ],
                },
            ],
            &[
                Lfo {
                    wave: Wave::SawDown,
                    beats: 4.0,
                },
                Lfo {
                    wave: Wave::SawUp,
                    beats: 1.0,
                },
            ],
        ),
        Recipe::Horizon => (
            &[
                SubSpec {
                    kind: K::SvfFilter,
                    switch: None,
                    paths: &[
                        Path {
                            target: Target::Knob(2),
                            shape: &[(0.0, 0.33)],
                            lfos: &[(0, &[(0.0, 0.32), (0.5, 0.32), (0.51, -0.32)])],
                        },
                        Path {
                            target: Target::Knob(1),
                            shape: &[(0.0, 1.0), (0.5, 0.6), (1.0, 1.0)],
                            lfos: &[(0, &[(0.0, 0.18), (0.5, 0.18), (0.51, -0.18)])],
                        },
                        Path {
                            target: Target::Knob(0),
                            shape: &[(0.0, 0.5), (0.5, 0.9), (1.0, 0.5)],
                            lfos: &[
                                (0, &[(0.0, 0.3), (0.5, 0.3), (0.51, -0.3)]),
                                (1, &[(0.0, 0.11), (0.5, 0.0), (1.0, -0.11)]),
                            ],
                        },
                        dw!(&[
                            (0.0, 1.0),
                            (0.38, 0.65),
                            (0.488, 0.0),
                            (0.512, 0.0),
                            (0.62, 0.65),
                            (1.0, 1.0),
                        ]),
                    ],
                },
                SubSpec {
                    kind: K::FlangerHold,
                    switch: None,
                    paths: &[
                        Path {
                            target: Target::Knob(0),
                            shape: &[(0.0, 0.5), (0.4, 0.0), (0.6, 0.0), (1.0, 0.5)],
                            lfos: &[(1, &[(0.0, 0.5), (0.5, 0.5), (0.51, -0.5)])],
                        },
                        knob!(1, &[(0.0, 0.6), (0.5, 0.0), (1.0, 0.79)]),
                        dw!(&[(0.0, 0.6), (0.5, 0.0), (1.0, 0.79)]),
                        button!(1, RIGHT_HALF),
                    ],
                },
                SubSpec {
                    kind: K::DelayT3,
                    switch: None,
                    paths: &[
                        knob!(0, &[(0.0, 0.7)]),
                        knob!(1, HALF),
                        knob!(
                            2,
                            &[
                                (0.0, 0.4),
                                (0.12, 0.67),
                                (0.5, 0.18),
                                (0.88, 0.67),
                                (1.0, 0.4),
                            ],
                        ),
                        dw!(&[(0.0, 0.32), (0.5, 0.0), (1.0, 0.32)]),
                    ],
                },
            ],
            &[
                Lfo {
                    wave: Wave::SawUp,
                    beats: 2.0,
                },
                Lfo {
                    wave: Wave::Triangle,
                    beats: 4.0,
                },
            ],
        ),
        Recipe::Zap => (
            &[
                SubSpec {
                    kind: K::FlangerPulse,
                    switch: Some(ONE),
                    paths: &[
                        knob!(0, &[(0.0, 0.9), (0.5, 0.51), (0.51, 0.1), (1.0, 0.45)]),
                        knob!(1, &[(0.0, 0.85), (0.5, 0.0), (1.0, 0.85)]),
                        knob!(2, &[(0.0, 0.51)]),
                        dw!(&[(0.0, 1.0), (0.38, 1.0), (0.5, 0.0), (0.62, 1.0)]),
                    ],
                },
                SubSpec {
                    kind: K::PhaserPulse,
                    switch: Some(ONE),
                    paths: &[
                        knob!(0, &[(0.0, 0.45), (0.5, 0.1), (0.51, 0.9), (1.0, 0.54)]),
                        knob!(1, &[(0.0, 0.59), (0.5, 0.0), (1.0, 0.59)]),
                        knob!(2, &[(0.0, 0.51)]),
                        dw!(&[
                            (0.0, 0.0),
                            (0.05, 0.0),
                            (0.051, 1.0),
                            (0.488, 1.0),
                            (0.5, 0.0),
                            (0.512, 1.0),
                        ]),
                    ],
                },
                SubSpec {
                    kind: K::GlassReverb,
                    switch: Some(ONE),
                    paths: &[
                        knob!(0, &[(0.0, 0.7)]),
                        knob!(1, &[(0.0, 0.88), (0.5, 0.25), (1.0, 0.88)]),
                        knob!(2, &[(0.0, 0.51)]),
                        dw!(&[(0.0, 0.32), (0.5, 0.0), (1.0, 0.26)]),
                    ],
                },
            ],
            &[],
        ),
        Recipe::Dark => (
            &[
                SubSpec {
                    kind: K::LoFi,
                    switch: None,
                    paths: &[
                        knob!(2, &[(0.0, 0.92), (0.5, 0.58), (0.51, 0.64), (1.0, 0.87)]),
                        knob!(1, &[(0.0, 0.33), (0.5, 0.71), (0.51, 0.44), (1.0, 0.06)]),
                        knob!(0, &[(0.0, 0.55), (0.5, 0.29), (0.51, 0.01), (1.0, 0.39)]),
                        dw!(&[(0.0, 0.56), (0.5, 0.0), (1.0, 0.65)]),
                        button!(0, &[(0.0, 0.0), (0.75, 0.0), (0.751, 1.0)]),
                    ],
                },
                SubSpec {
                    kind: K::SvfFilter,
                    switch: Some(&[
                        (0.0, 1.0),
                        (0.469, 1.0),
                        (0.47, 0.0),
                        (0.53, 0.0),
                        (0.531, 1.0),
                    ]),
                    paths: &[
                        Path {
                            target: Target::Knob(2),
                            shape: &[(0.0, 0.3), (0.4, 0.0), (0.6, 0.0), (1.0, 0.3)],
                            lfos: &[(1, &[(0.0, 0.39), (0.5, 0.2), (0.51, 0.04), (1.0, 0.16)])],
                        },
                        knob!(1, &[(0.0, 1.0), (0.5, 0.12), (0.51, 0.0), (1.0, 0.65)]),
                        Path {
                            target: Target::Knob(0),
                            shape: &[(0.0, 0.57), (0.5, 0.88), (0.51, 1.0)],
                            lfos: &[(0, &[(0.0, -0.03), (1.0, -0.19)])],
                        },
                        dw!(&[
                            (0.0, 0.64),
                            (0.25, 0.04),
                            (0.5, 0.56),
                            (0.75, 0.42),
                            (1.0, 0.77),
                        ]),
                    ],
                },
                SubSpec {
                    kind: K::DelayT3,
                    switch: None,
                    paths: &[
                        knob!(0, &[(0.0, 0.25), (0.5, 0.25), (0.51, 0.57)]),
                        knob!(1, &[(0.0, 0.82), (0.5, 0.17), (1.0, 0.82)]),
                        knob!(2, &[(0.0, 0.84), (0.5, 0.0), (1.0, 0.78)]),
                        dw!(&[(0.0, 0.7), (0.5, 0.1), (1.0, 0.7)]),
                    ],
                },
            ],
            &[
                Lfo {
                    wave: Wave::Square,
                    beats: 1.0,
                },
                Lfo {
                    wave: Wave::Triangle,
                    beats: 4.0,
                },
            ],
        ),
        Recipe::Flight => (
            &[
                SubSpec {
                    kind: K::PhaserPulse,
                    switch: None,
                    paths: &[
                        knob!(0, &[(0.0, 0.83), (0.5, 0.23), (0.51, 1.0), (1.0, 0.4)]),
                        knob!(1, &[(0.0, 0.66), (0.43, 0.0), (0.57, 0.0), (1.0, 0.66)]),
                        knob!(2, ZERO),
                        dw!(&[
                            (0.0, 0.5),
                            (0.37, 0.5),
                            (0.5, 0.0),
                            (0.63, 0.52),
                            (1.0, 0.86),
                        ]),
                    ],
                },
                SubSpec {
                    kind: K::DelayT3,
                    switch: None,
                    paths: &[
                        knob!(0, &[(0.0, 0.4), (0.5, 0.4), (0.51, 0.82)]),
                        knob!(1, HALF),
                        knob!(2, &[(0.0, 0.4), (0.5, 0.5)]),
                        dw!(&[(0.0, 0.46), (0.5, 0.0), (1.0, 0.47)]),
                    ],
                },
                SubSpec {
                    kind: K::FlangerHold,
                    switch: None,
                    paths: &[
                        knob!(0, &[(0.0, 0.44), (0.5, 0.98), (0.51, 0.21), (1.0, 0.75)]),
                        knob!(1, &[(0.0, 0.91), (0.375, 0.34), (1.0, 0.23)]),
                        dw!(&[(0.0, 0.5), (0.37, 0.0), (0.63, 0.0), (1.0, 0.5)]),
                    ],
                },
            ],
            &[],
        ),
    }
}

const OUT_OF_GAP_HALF: Shape = &[
    (0.0, 0.5),
    (0.488, 0.5),
    (0.489, 0.0),
    (0.511, 0.0),
    (0.512, 0.5),
];

struct Sub {
    effect: Box<dyn Effect>,
    spec: SubSpec,
    own_mix: bool,
    dry_wet: f32,
    gain: Smoothed,
    buffer: Vec<f32>,
}

impl std::fmt::Debug for Sub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sub")
            .field("kind", &self.spec.kind)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct Macro {
    recipe: Recipe,
    subs: Vec<Sub>,
    lfos: &'static [Lfo],
    knob: f32,
    active: bool,
}

impl Macro {
    pub fn new(recipe: Recipe, sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate);
        let (specs, lfos) = self::recipe(recipe);
        let subs = specs
            .iter()
            .map(|&spec| {
                let mut effect = super::create_engine(spec.kind, sample_rate);
                for i in 0..3 {
                    effect.set_param(i, 0.0);
                    effect.set_button(i, false);
                }
                let own_mix = !effect.set_dry_wet(0.0);
                Sub {
                    effect,
                    spec,
                    own_mix,
                    dry_wet: 0.0,
                    gain: Smoothed::new(0.0, 0.003, rate),
                    buffer: vec![0.0; 4 * crate::MAX_BLOCK_FRAMES],
                }
            })
            .collect();
        let mut m = Self {
            recipe,
            subs,
            lfos,
            knob: 0.5,
            active: false,
        };
        m.apply(0.0);
        m
    }

    fn apply(&mut self, beat: f64) {
        let k = self.knob;
        let mut lfo_values = [0.0; 2];
        for (v, l) in lfo_values.iter_mut().zip(self.lfos) {
            *v = l.at(beat);
        }
        for sub in &mut self.subs {
            let on = self.active
                && match sub.spec.switch {
                    Some(shape) => eval(shape, k) != 0.0,
                    None => !in_gap(k),
                };
            sub.effect.set_active(on);
            sub.gain.set(if on { 1.0 } else { 0.0 });
            for path in sub.spec.paths {
                let mut value = f64::from(eval(path.shape, k));
                for &(i, depth) in path.lfos {
                    value += f64::from(eval(depth, k)) * lfo_values.get(i).copied().unwrap_or(0.0);
                }
                let value = value.clamp(0.0, 1.0) as f32;
                match path.target {
                    Target::Knob(i) => sub.effect.set_param(i, value),
                    Target::Button(i) => sub.effect.set_button(i, value != 0.0),
                    Target::DryWet => {
                        sub.dry_wet = value;
                        if !sub.own_mix {
                            sub.effect.set_dry_wet(value);
                        }
                    }
                }
            }
        }
    }
}

pub fn create(kind: EffectKind, sample_rate: u32) -> Box<dyn Effect> {
    use EffectKind as K;
    let recipe = match kind {
        K::MacroTunnel => Recipe::Tunnel,
        K::MacroZapCut => Recipe::ZapCut,
        K::MacroGrainSweep => Recipe::GrainSweep,
        K::MacroBassMachine => Recipe::BassMachine,
        K::MacroStretchFast => Recipe::StretchFast,
        K::MacroStretchSlow => Recipe::StretchSlow,
        K::MacroPolar => Recipe::Polar,
        K::MacroHorizon => Recipe::Horizon,
        K::MacroZap => Recipe::Zap,
        K::MacroDark => Recipe::Dark,
        K::MacroFlight => Recipe::Flight,
        _ => return Box::new(super::Bypass),
    };
    Box::new(Macro::new(recipe, sample_rate))
}

impl Effect for Macro {
    fn kind(&self) -> EffectKind {
        match self.recipe {
            Recipe::Tunnel => EffectKind::MacroTunnel,
            Recipe::ZapCut => EffectKind::MacroZapCut,
            Recipe::GrainSweep => EffectKind::MacroGrainSweep,
            Recipe::BassMachine => EffectKind::MacroBassMachine,
            Recipe::StretchFast => EffectKind::MacroStretchFast,
            Recipe::StretchSlow => EffectKind::MacroStretchSlow,
            Recipe::Polar => EffectKind::MacroPolar,
            Recipe::Horizon => EffectKind::MacroHorizon,
            Recipe::Zap => EffectKind::MacroZap,
            Recipe::Dark => EffectKind::MacroDark,
            Recipe::Flight => EffectKind::MacroFlight,
        }
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        if !self.lfos.is_empty() {
            self.apply(ctx.beat);
        }
        let n = buf.len();
        for sub in &mut self.subs {
            let tail = sub.effect.has_tail();
            let (work, sent) = sub.buffer.split_at_mut(2 * crate::MAX_BLOCK_FRAMES);
            let (work, sent) = (&mut work[..n], &mut sent[..n]);
            work.copy_from_slice(buf);
            {
                let (frames, _) = work.as_chunks_mut::<2>();
                let (fed, _) = sent.as_chunks_mut::<2>();
                for (f, s) in frames.iter_mut().zip(fed.iter_mut()) {
                    let g = sub.gain.tick();
                    if tail {
                        f[0] *= g;
                        f[1] *= g;
                        *s = *f;
                    } else {
                        *s = [g, g];
                    }
                }
            }
            sub.effect.process(work, ctx);
            let mix = if sub.own_mix { sub.dry_wet } else { 1.0 };
            let (out, _) = work.as_chunks::<2>();
            let (fed, _) = sent.as_chunks::<2>();
            let (dry, _) = buf.as_chunks_mut::<2>();
            for ((o, s), d) in out.iter().zip(fed).zip(dry.iter_mut()) {
                if tail {
                    *d = [d[0] + (o[0] - s[0]) * mix, d[1] + (o[1] - s[1]) * mix];
                } else {
                    let g = s[0] * mix;
                    *d = [d[0] + (o[0] - d[0]) * g, d[1] + (o[1] - d[1]) * g];
                }
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.knob = amount;
        self.apply(0.0);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        if index == 0 {
            self.knob = value;
            self.apply(0.0);
        }
    }

    fn set_button(&mut self, _index: usize, _on: bool) {}

    fn set_active(&mut self, on: bool) {
        if on != self.active {
            self.active = on;
            self.apply(0.0);
        }
    }

    fn has_tail(&self) -> bool {
        true
    }

    fn listens(&self) -> bool {
        self.subs.iter().any(|s| s.effect.listens())
    }

    fn lfo_reset(&mut self) {
        for sub in &mut self.subs {
            sub.effect.lfo_reset();
        }
    }

    fn reset(&mut self) {
        for sub in &mut self.subs {
            sub.effect.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    const ALL: [Recipe; 11] = [
        Recipe::Tunnel,
        Recipe::ZapCut,
        Recipe::GrainSweep,
        Recipe::BassMachine,
        Recipe::StretchFast,
        Recipe::StretchSlow,
        Recipe::Polar,
        Recipe::Horizon,
        Recipe::Zap,
        Recipe::Dark,
        Recipe::Flight,
    ];

    #[test]
    fn curves_interpolate_and_hold_their_ends() {
        assert!((eval(&[(0.0, 0.0), (1.0, 1.0)], 0.25) - 0.25).abs() < 1e-6);
        assert!((eval(&[(0.2, 0.5), (0.8, 1.0)], 0.0) - 0.5).abs() < 1e-6);
        assert!((eval(&[(0.2, 0.5), (0.8, 1.0)], 1.0) - 1.0).abs() < 1e-6);
        assert_eq!(eval(OUT_OF_GAP, 0.5), 0.0);
        assert_eq!(eval(OUT_OF_GAP, 0.4), 1.0);
        assert!(
            (Lfo {
                wave: Wave::Triangle,
                beats: 4.0
            }
            .at(2.0)
                - 1.0)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn every_macro_is_neutral_at_the_centre_and_bounded_off_it() {
        let input = sine(330.0, RATE as usize, 0.4);
        for recipe in ALL {
            let mut m = Macro::new(recipe, RATE);
            m.set_active(true);
            m.set_amount(0.5);
            let out = run(&mut m, &input, ctx());
            let diff: Vec<f32> = out[2 * 24_000..]
                .iter()
                .zip(&input[2 * 24_000..])
                .map(|(a, b)| a - b)
                .collect();
            assert!(
                rms(&diff) < 0.05,
                "{recipe:?} at the centre: {}",
                rms(&diff)
            );
            for k in [0.0, 1.0] {
                let mut m = Macro::new(recipe, RATE);
                m.set_active(true);
                m.set_amount(k);
                let out = run(&mut m, &input, ctx());
                assert!(
                    out.iter().all(|v| v.is_finite() && v.abs() < 8.0),
                    "{recipe:?} {k}"
                );
            }
        }
    }
}
