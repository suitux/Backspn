use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Input;
use crate::modifier::Modifiers;

const R: f32 = 127.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Encoder {
    #[default]
    TwosComplement,
    Offset,
    Absolute,
    Button,
    Rotary,
}

impl Encoder {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn change(self, v: u8, previous: u8) -> f32 {
        let v = f32::from(v);
        let half = f32::midpoint(R, 1.0);
        match self {
            Self::TwosComplement if v < half => v,
            Self::TwosComplement => v - 1.0 - R,
            Self::Offset => v - half,
            Self::Absolute => {
                let mut d = v - f32::from(previous);
                if d < -0.75 * R {
                    d += R;
                }
                if d > 0.75 * R {
                    d -= R;
                }
                d
            }
            Self::Button => f32::from(u8::from(v > 0.0)),
            Self::Rotary => match v {
                1.0..=30.0 => v,
                34.0..=63.0 | 65.0..=94.0 => v - half,
                98.0..=127.0 => v - R - 1.0,
                _ => v,
            },
        }
    }
}

pub fn sensitivity(percent: f32) -> f32 {
    let s = percent / 100.0;
    0.99975 * s * s + 0.00025
}

pub fn accelerate(x: f32, a: f32) -> f32 {
    let y = (x.abs() + 1.0).powf(1.0 + 4.0 * a) - 1.0;
    if x < 0.0 { -y } else { y }
}

pub fn steps(d: f32, percent: f32, acceleration: f32, stepped: Option<&mut f32>) -> f32 {
    let k = sensitivity(percent);
    match stepped {
        Some(rest) => {
            let v = k * accelerate(d, acceleration) + *rest;
            let n = v.round();
            *rest = v - n;
            n
        }
        None => k * accelerate(d / R, acceleration) * R,
    }
}

pub fn takes_over(value: f32, old: f32, new: f32) -> bool {
    let half = (new - old).abs() / 2.0;
    old.min(new) - half <= value && value <= old.max(new) + half
}

#[derive(Debug, Clone, Default)]
pub struct Source {
    pub modifiers: Modifiers,
    pub(crate) previous: HashMap<Input, u8>,
    pub(crate) rest: HashMap<usize, f32>,
    pub(crate) held: Vec<Held>,
    pub(crate) last_midi: Option<(crate::MidiId, u8)>,
    pub port: Option<String>,
}

impl Source {
    pub fn for_port(port: &str) -> Self {
        Self {
            port: Some(port.to_owned()),
            ..Self::default()
        }
    }

    pub fn repeating(&self) -> bool {
        !self.held.is_empty()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Held {
    pub mapping: usize,
    pub event: crate::InputEvent,
    pub ticks: u32,
}

pub const REPEAT_PERIOD: std::time::Duration = std::time::Duration::from_millis(100);
pub const REPEAT_DELAY: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_modes() {
        let e = Encoder::TwosComplement;
        assert_eq!(
            (e.change(1, 0), e.change(127, 0), e.change(63, 0)),
            (1.0, -1.0, 63.0)
        );
        assert_eq!(e.change(64, 0), -64.0);
        let o = Encoder::Offset;
        assert_eq!(
            (o.change(65, 0), o.change(63, 0), o.change(64, 0)),
            (1.0, -1.0, 0.0)
        );
        let a = Encoder::Absolute;
        assert_eq!(a.change(70, 64), 6.0);
        assert_eq!(a.change(64, 0), 64.0);
        assert_eq!(a.change(2, 127), 2.0);
        assert_eq!(a.change(125, 0), -2.0);
        let r = Encoder::Rotary;
        assert_eq!(
            (r.change(3, 0), r.change(66, 0), r.change(62, 0)),
            (3.0, 2.0, -2.0)
        );
        assert_eq!((r.change(126, 0), r.change(0, 0)), (-2.0, 0.0));
    }

    #[test]
    fn sensitivity_and_acceleration() {
        assert!((sensitivity(100.0) - 1.0).abs() < 1e-6);
        assert!((sensitivity(50.0) - 0.25).abs() < 1e-3);
        assert_eq!(accelerate(3.0, 0.0), 3.0);
        assert_eq!(accelerate(-3.0, 0.0), -3.0);
        assert!((accelerate(1.0, 0.25) - 3.0).abs() < 1e-5);
        assert!((steps(2.0, 100.0, 0.0, None) - 2.0).abs() < 1e-5);
        assert!((steps(2.0, 50.0, 0.0, None) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn soft_takeover_needs_the_fader_to_pass_the_value() {
        assert!(!takes_over(0.8, 0.2, 0.3));
        assert!(takes_over(0.8, 0.7, 0.85));
        assert!(takes_over(0.8, 0.9, 0.7));
        assert!(takes_over(0.49, 0.2, 0.4));
        assert!(!takes_over(0.52, 0.2, 0.4));
    }

    #[test]
    fn stepped_controls_keep_the_rest() {
        let mut rest = 0.0;
        let clicks: Vec<f32> = (0..8)
            .map(|_| steps(1.0, 50.0, 0.0, Some(&mut rest)))
            .collect();
        assert_eq!(clicks.iter().sum::<f32>(), 2.0);
        assert_eq!(steps(-1.0, 100.0, 0.0, Some(&mut 0.0)), -1.0);
    }
}
