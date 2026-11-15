//! Controllers whose pads change job with mode buttons, each mode sending
//! its own notes: which mode each side is in and what its pads then do.

use serde::{Deserialize, Serialize};

use crate::modifier::Modifiers;
use crate::{Action, Input, Mapping, MidiKind};

pub const PADS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PadModes {
    /// Per side of the controller, the modifier that holds its pad mode.
    #[serde(default)]
    pub modifiers: Vec<usize>,
    /// Per side, the MIDI channel its pads send on.
    #[serde(default)]
    pub channels: Vec<u8>,
    /// Mode `m` sends its first pad on note `m * stride`.
    #[serde(default)]
    pub stride: u8,
    /// The mode names, as printed on the controller.
    #[serde(default)]
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Panel {
    pub mode: usize,
    pub pads: [Option<Action>; PADS],
}

impl Panel {
    /// The deck the pads play on.
    pub fn deck(&self) -> Option<usize> {
        self.pads.iter().flatten().find_map(|&a| {
            let mut a = a;
            a.deck_mut().copied()
        })
    }
}

impl PadModes {
    /// The mode of one side and what its pads do, from the mappings on
    /// `port`.
    pub fn panel(
        &self,
        side: usize,
        modifiers: &Modifiers,
        mappings: &[Mapping],
        port: &str,
    ) -> Option<Panel> {
        let mode = self.mode(side, modifiers)?;
        self.pads(side, mode, mappings, port)
    }

    /// The mode one side of the controller is in.
    pub fn mode(&self, side: usize, modifiers: &Modifiers) -> Option<usize> {
        let modifier = *self.modifiers.get(side)?;
        Some(usize::from(*modifiers.0.get(modifier)?)).filter(|&m| m < self.names.len())
    }

    /// What the pads of one side do in `mode`.
    pub fn pads(
        &self,
        side: usize,
        mode: usize,
        mappings: &[Mapping],
        port: &str,
    ) -> Option<Panel> {
        let channel = *self.channels.get(side)?;
        if mode >= self.names.len() {
            return None;
        }
        let first = u8::try_from(mode).ok()?.checked_mul(self.stride)?;
        let pads = std::array::from_fn(|i| {
            let number = first.checked_add(u8::try_from(i).ok()?)?;
            let input = Input::Midi {
                channel,
                kind: MidiKind::Note,
                number,
            };
            mappings
                .iter()
                .find(|m| m.input == input && m.port.as_deref() == Some(port) && m.gate.is_empty())
                .map(|m| m.action)
        });
        Some(Panel { mode, pads })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MappingSet;

    #[test]
    fn each_side_shows_the_pads_of_its_mode() {
        let profile = crate::profile::for_port("DDJ-FLX4").unwrap();
        let modes = profile
            .pad_modes
            .clone()
            .expect("the FLX4 follows its pad modes");
        assert_eq!(modes.names.len(), 8);
        let mut set = MappingSet::default();
        set.add_profile(&profile, "DDJ-FLX4", "DDJ-FLX4");
        let mut modifiers = Modifiers::default();
        let hot_cue = modes
            .panel(0, &modifiers, &set.mappings, "DDJ-FLX4")
            .unwrap();
        assert_eq!(hot_cue.mode, 0);
        assert_eq!(hot_cue.pads[3], Some(Action::Hotcue { deck: 0, slot: 3 }));
        assert_eq!(hot_cue.deck(), Some(0));

        modifiers.0[modes.modifiers[0]] = 2;
        modifiers.0[modes.modifiers[1]] = 7;
        let jump = modes
            .panel(0, &modifiers, &set.mappings, "DDJ-FLX4")
            .unwrap();
        assert_eq!((jump.mode, &modes.names[jump.mode][..]), (2, "BEAT JUMP"));
        assert_eq!(jump.pads[0], Some(Action::BeatJumpPad { deck: 0, pad: 0 }));
        let key = modes
            .panel(1, &modifiers, &set.mappings, "DDJ-FLX4")
            .unwrap();
        assert_eq!(key.pads[7], Some(Action::KeyShiftPad { deck: 1, pad: 7 }));
        assert_eq!(key.deck(), Some(1));

        assert!(
            modes
                .panel(0, &modifiers, &set.mappings, "Other")
                .unwrap()
                .pads
                .iter()
                .all(Option::is_none)
        );
        assert_eq!(modes.panel(2, &modifiers, &set.mappings, "DDJ-FLX4"), None);
        let picked = modes.pads(0, 6, &set.mappings, "DDJ-FLX4").unwrap();
        assert_eq!(
            picked.pads[2],
            Some(Action::LoopSize {
                deck: 0,
                beats: 1.0
            }),
            "another mode, picked on screen"
        );
        assert_eq!(modes.pads(0, 8, &set.mappings, "DDJ-FLX4"), None);
    }
}
