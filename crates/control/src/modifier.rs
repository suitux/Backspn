use serde::{Deserialize, Serialize};

use crate::{Interaction, Trigger};

pub const COUNT: usize = 8;
pub const PER_PORT: usize = 16;
pub const MAX: u8 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers(pub [u8; PER_PORT]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    pub modifier: usize,
    pub value: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Gate {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Condition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<crate::state::StateCondition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<u8>,
}

impl Gate {
    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty() && self.states.is_empty() && self.value.is_none()
    }
}

impl Modifiers {
    pub fn hold(&self, conditions: &[Condition]) -> bool {
        conditions
            .iter()
            .all(|c| self.0.get(c.modifier).is_some_and(|&v| v == c.value))
    }

    pub fn apply(&mut self, index: usize, interaction: Interaction, value: Option<u8>, t: Trigger) {
        let Some(current) = self.0.get_mut(index) else {
            return;
        };
        let set = value.map(|v| v.min(MAX));
        *current = match (interaction, t) {
            (Interaction::Trigger, Trigger::Press) => MAX,
            (Interaction::Reset, Trigger::Reset) => 0,
            (Interaction::Toggle, Trigger::Press) => {
                if *current == 0 {
                    MAX
                } else {
                    0
                }
            }
            (Interaction::Hold, Trigger::Press) => set.unwrap_or(MAX),
            (Interaction::Hold, Trigger::Release) => match set {
                Some(v) if *current != v => *current,
                _ => 0,
            },
            (Interaction::Direct, Trigger::Set(x)) => {
                set.unwrap_or_else(|| (x.clamp(0.0, 1.0) * f32::from(MAX)).round() as u8)
            }
            (Interaction::Relative | Interaction::Direct, Trigger::Delta(d)) => {
                (f32::from(*current) + d.round()).clamp(0.0, f32::from(MAX)) as u8
            }
            _ => *current,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(m: &mut Modifiers, i: Interaction, value: Option<u8>, t: Trigger) -> u8 {
        m.apply(0, i, value, t);
        m.0[0]
    }

    #[test]
    fn the_rules() {
        let mut m = Modifiers::default();
        assert_eq!(
            applied(&mut m, Interaction::Trigger, None, Trigger::Press),
            7
        );
        assert_eq!(
            applied(&mut m, Interaction::Toggle, Some(3), Trigger::Press),
            0
        );
        assert_eq!(
            applied(&mut m, Interaction::Toggle, Some(3), Trigger::Press),
            7
        );
        let mut m = Modifiers::default();
        assert_eq!(applied(&mut m, Interaction::Hold, None, Trigger::Press), 7);
        assert_eq!(
            applied(&mut m, Interaction::Hold, None, Trigger::Release),
            0
        );
        assert_eq!(
            applied(&mut m, Interaction::Hold, Some(2), Trigger::Press),
            2
        );
        assert_eq!(
            applied(&mut m, Interaction::Hold, Some(2), Trigger::Release),
            0
        );
        applied(&mut m, Interaction::Hold, Some(2), Trigger::Press);
        m.0[0] = 5;
        assert_eq!(
            applied(&mut m, Interaction::Hold, Some(2), Trigger::Release),
            5
        );
        assert_eq!(
            applied(&mut m, Interaction::Direct, Some(4), Trigger::Set(1.0)),
            4
        );
        assert_eq!(
            applied(&mut m, Interaction::Direct, None, Trigger::Set(0.5)),
            4
        );
        assert_eq!(
            applied(&mut m, Interaction::Relative, None, Trigger::Delta(5.0)),
            7
        );
        assert_eq!(
            applied(&mut m, Interaction::Relative, None, Trigger::Delta(-9.0)),
            0
        );
    }

    #[test]
    fn conditions_must_all_hold() {
        let mut m = Modifiers::default();
        m.0[1] = 3;
        let c = |modifier, value| Condition { modifier, value };
        assert!(m.hold(&[]));
        assert!(m.hold(&[c(1, 3)]));
        assert!(m.hold(&[c(1, 3), c(0, 0)]));
        assert!(!m.hold(&[c(1, 3), c(0, 1)]));
        assert!(!m.hold(&[c(PER_PORT, 0)]));
    }

    #[test]
    fn a_port_keeps_more_than_one_devices_eight() {
        let mut m = Modifiers::default();
        m.apply(12, Interaction::Toggle, None, Trigger::Press);
        assert_eq!(m.0[12], MAX);
        assert!(m.hold(&[Condition {
            modifier: 12,
            value: MAX
        }]));
        m.apply(PER_PORT, Interaction::Toggle, None, Trigger::Press);
        assert_eq!(m.0.len(), PER_PORT);
    }
}
