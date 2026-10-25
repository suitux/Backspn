use serde::{Deserialize, Serialize};

use crate::deck_letter;
use crate::feedback::Feedback;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "control")]
pub enum Control {
    Playing,
    InActiveLoop,
    TempoRange,
    #[serde(alias = "DeckFlavor")]
    DeckType,
    HotcueType {
        slot: usize,
    },
    FxMode,
    PatternStep,
}

impl Control {
    pub fn from_id(id: u32) -> Option<Self> {
        Some(match id {
            100 => Self::Playing,
            203 => Self::InActiveLoop,
            19 => Self::TempoRange,
            2302 => Self::DeckType,
            2333..=2340 => Self::HotcueType {
                slot: (id - 2333) as usize,
            },
            335 | 2301 => Self::FxMode,
            381 => Self::PatternStep,
            _ => return None,
        })
    }

    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::Playing,
            Self::InActiveLoop,
            Self::TempoRange,
            Self::DeckType,
        ]
        .into_iter()
        .chain((0..8).map(|slot| Self::HotcueType { slot }))
        .chain([Self::FxMode, Self::PatternStep])
    }

    pub fn range(self) -> (i32, i32) {
        match self {
            Self::Playing | Self::InActiveLoop => (0, 1),
            Self::TempoRange => (0, 13),
            Self::DeckType | Self::FxMode => (0, 2),
            Self::HotcueType { .. } => (-1, 5),
            Self::PatternStep => (-1, 15),
        }
    }

    pub fn on_unit(self) -> bool {
        matches!(self, Self::FxMode | Self::PatternStep)
    }

    pub fn label(self) -> String {
        match self {
            Self::Playing => "Play".into(),
            Self::InActiveLoop => "In Loop".into(),
            Self::TempoRange => "Tempo Range".into(),
            Self::DeckType => "Deck Type".into(),
            Self::HotcueType { slot } => format!("Hotcue {} Type", slot + 1),
            Self::FxMode => "Mode".into(),
            Self::PatternStep => "Current Step".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateCondition {
    #[serde(flatten)]
    pub control: Control,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<usize>,
    pub value: i32,
}

impl StateCondition {
    pub fn current(&self, f: &Feedback) -> Option<i32> {
        self.current_on(f, f.focus)
    }

    pub fn current_on(&self, f: &Feedback, focus: usize) -> Option<i32> {
        let channel = self.channel.unwrap_or(focus);
        if self.control.on_unit() {
            let fx = f.fx.get(channel)?;
            return Some(match self.control {
                Control::FxMode => i32::from(fx.mode),
                _ => fx.pattern_step.map_or(-1, i32::from),
            });
        }
        let d = f.decks.get(channel)?;
        Some(match self.control {
            Control::Playing => i32::from(d.playing),
            Control::InActiveLoop => i32::from(d.in_loop),
            Control::TempoRange => i32::from(d.tempo_range),
            Control::DeckType => {
                if d.sample_deck.is_some() {
                    1
                } else if d.stems.is_some() {
                    2
                } else {
                    0
                }
            }
            Control::HotcueType { slot } => d.hotcues.get(slot)?.type_code(),
            Control::FxMode | Control::PatternStep => return None,
        })
    }

    pub fn label(&self) -> String {
        let channel = match (self.channel, self.control.on_unit()) {
            (None, _) => "Focus".to_owned(),
            (Some(u), true) => format!("FX {}", u + 1),
            (Some(d), false) => deck_letter(d).to_string(),
        };
        format!("{channel} {} = {}", self.control.label(), self.value)
    }
}

pub fn hold(conditions: &[StateCondition], f: &Feedback) -> bool {
    hold_on(conditions, f, f.focus)
}

pub fn hold_on(conditions: &[StateCondition], f: &Feedback, focus: usize) -> bool {
    conditions
        .iter()
        .all(|c| c.current_on(f, focus) == Some(c.value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::HotcueKind;

    #[test]
    fn conditions_read_decks_units_and_the_focus() {
        let mut f = Feedback::default();
        f.decks[1].playing = true;
        f.decks[1].hotcues[2] = HotcueKind::FadeOut;
        f.fx[3].mode = 2;
        f.fx[3].pattern_step = Some(5);
        let c = |control, channel, value| StateCondition {
            control,
            channel,
            value,
        };
        assert!(hold(&[c(Control::Playing, Some(1), 1)], &f));
        assert!(!hold(&[c(Control::Playing, None, 1)], &f), "focus on A");
        f.focus = 1;
        assert!(hold(
            &[
                c(Control::Playing, None, 1),
                c(Control::HotcueType { slot: 2 }, None, 2)
            ],
            &f
        ));
        assert!(hold(&[c(Control::HotcueType { slot: 0 }, None, -1)], &f));
        assert!(hold(&[c(Control::FxMode, Some(3), 2)], &f));
        assert!(hold(&[c(Control::PatternStep, Some(3), 5)], &f));
        assert!(hold(&[c(Control::PatternStep, Some(0), -1)], &f));
        assert!(!hold(&[c(Control::Playing, Some(7), 0)], &f), "no deck H");
        assert_eq!(
            Control::from_id(2335),
            Some(Control::HotcueType { slot: 2 })
        );
        assert_eq!(Control::from_id(736), None, "sample decks");
        assert_eq!(c(Control::FxMode, Some(0), 1).label(), "FX 1 Mode = 1");
        assert_eq!(c(Control::Playing, None, 1).label(), "Focus Play = 1");
        assert_eq!(Control::all().count(), 14);
        assert!(Control::all().all(|c| c.range().0 < c.range().1));
    }
}
