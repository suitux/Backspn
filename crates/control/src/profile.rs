use serde::{Deserialize, Serialize};

use crate::output::OutMapping;
use crate::session::PortMessages;
use crate::{Mapping, MappingSet, PairRange};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub brand: String,
    pub model: String,
    pub ports: Vec<String>,
    #[serde(default)]
    pub mappings: Vec<Mapping>,
    #[serde(default)]
    pub outputs: Vec<OutMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pair_ranges: Vec<PairRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages: Option<PortMessages>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<Audio>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tested: bool,
}

const TESTED_NATIVE: &[&str] = &[crate::s4mk3::NAME];

pub fn native_tested(name: &str) -> bool {
    TESTED_NATIVE.contains(&name)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Audio {
    pub device: String,
    pub master: (u16, u16),
    pub monitor: (u16, u16),
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buffer_frames: Option<u32>,
}

pub fn card_named(name: &str, devices: &[String]) -> Option<String> {
    devices.iter().find(|d| d.contains(name)).cloned()
}

const FILES: &[&str] = &[
    include_str!("../profiles/alphatheta/ddj-flx2.json"),
    include_str!("../profiles/alphatheta/ddj-flx4.json"),
    include_str!("../profiles/alphatheta/ddj-grv6.json"),
    include_str!("../profiles/denon/mc7000.json"),
    include_str!("../profiles/hercules/djcontrol-inpulse-200.json"),
    include_str!("../profiles/hercules/djcontrol-inpulse-300.json"),
    include_str!("../profiles/hercules/djcontrol-inpulse-500.json"),
    include_str!("../profiles/numark/mixtrack-platinum-fx.json"),
    include_str!("../profiles/numark/mixtrack-pro-fx.json"),
    include_str!("../profiles/pioneer-dj/ddj-1000.json"),
    include_str!("../profiles/pioneer-dj/ddj-200.json"),
    include_str!("../profiles/pioneer-dj/ddj-400.json"),
    include_str!("../profiles/pioneer-dj/ddj-800.json"),
    include_str!("../profiles/pioneer-dj/ddj-ergo.json"),
    include_str!("../profiles/pioneer-dj/ddj-flx6.json"),
    include_str!("../profiles/pioneer-dj/ddj-rb.json"),
    include_str!("../profiles/pioneer-dj/ddj-rr.json"),
    include_str!("../profiles/pioneer-dj/ddj-rz.json"),
    include_str!("../profiles/pioneer-dj/ddj-rzx.json"),
    include_str!("../profiles/pioneer-dj/ddj-rev5.json"),
    include_str!("../profiles/pioneer-dj/ddj-rx.json"),
    include_str!("../profiles/pioneer-dj/ddj-sb.json"),
    include_str!("../profiles/pioneer-dj/ddj-sb2.json"),
    include_str!("../profiles/pioneer-dj/ddj-sp1.json"),
    include_str!("../profiles/pioneer-dj/ddj-sr.json"),
    include_str!("../profiles/pioneer-dj/ddj-sx.json"),
    include_str!("../profiles/pioneer-dj/ddj-sx2.json"),
    include_str!("../profiles/pioneer-dj/ddj-sz.json"),
    include_str!("../profiles/pioneer-dj/ddj-wego.json"),
    include_str!("../profiles/pioneer-dj/ddj-wego2.json"),
    include_str!("../profiles/pioneer-dj/ddj-wego3.json"),
    include_str!("../profiles/pioneer-dj/ddj-wego4.json"),
    include_str!("../profiles/pioneer-dj/ddj-xp1.json"),
    include_str!("../profiles/pioneer-dj/ddj-xp2.json"),
    include_str!("../profiles/reloop/buddy.json"),
    include_str!("../profiles/reloop/ready.json"),
];

pub fn all() -> Vec<Profile> {
    let mut all: Vec<Profile> = FILES
        .iter()
        .filter_map(|text| serde_json::from_str(text).ok())
        .collect();
    all.sort_by(|a, b| (&a.brand, &a.model).cmp(&(&b.brand, &b.model)));
    all
}

impl Profile {
    pub fn name(&self) -> String {
        format!("{} {}", self.brand, self.model)
    }

    pub fn owns(&self, port: &str) -> bool {
        self.ports.iter().any(|p| p == port)
    }
}

pub fn for_port(port: &str) -> Option<Profile> {
    all().into_iter().find(|p| p.owns(port))
}

pub fn read_dir(dir: &std::path::Path) -> (Vec<Profile>, Vec<String>) {
    let (mut found, mut errors) = (Vec::new(), Vec::new());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (found, errors);
    };
    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    for path in paths {
        let read = std::fs::read_to_string(&path).map_err(|e| e.to_string());
        match read.and_then(|t| serde_json::from_str::<Profile>(&t).map_err(|e| e.to_string())) {
            Ok(p) => found.push(p),
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }
    (found, errors)
}

pub fn with_user(user: Vec<Profile>) -> Vec<Profile> {
    let mut list: Vec<Profile> = all()
        .into_iter()
        .filter(|s| {
            !user
                .iter()
                .any(|u| u.brand == s.brand && u.model == s.model)
        })
        .collect();
    list.extend(user);
    list.sort_by(|a, b| (&a.brand, &a.model).cmp(&(&b.brand, &b.model)));
    list
}

impl MappingSet {
    pub fn add_profile(&mut self, profile: &Profile, input: &str, output: &str) {
        self.remove_ports(input, output);
        self.add_device(crate::device::Device::midi(
            profile.name(),
            Some(input.to_owned()),
            Some(output.to_owned()),
        ));
        for m in &profile.mappings {
            self.mappings.push(Mapping {
                port: Some(input.to_owned()),
                ..m.clone()
            });
        }
        for o in &profile.outputs {
            self.outputs.push(OutMapping {
                port: Some(output.to_owned()),
                ..o.clone()
            });
        }
        for r in &profile.pair_ranges {
            self.set_pair_range(*r);
        }
        if let Some(m) = &profile.messages {
            self.port_messages.push(PortMessages {
                port: output.to_owned(),
                ..m.clone()
            });
        }
        if !self.midi_inputs.iter().any(|p| p == input) {
            self.midi_inputs.push(input.to_owned());
        }
        if !self.midi_outputs.iter().any(|p| p == output) {
            self.midi_outputs.push(output.to_owned());
        }
    }

    pub fn remove_ports(&mut self, input: &str, output: &str) {
        self.mappings.retain(|m| m.port.as_deref() != Some(input));
        self.outputs.retain(|o| o.port.as_deref() != Some(output));
        self.port_messages.retain(|m| m.port != output);
    }

    pub fn has_port(&self, input: &str) -> bool {
        self.mappings
            .iter()
            .any(|m| m.port.as_deref() == Some(input))
    }
}

#[cfg(test)]
mod tests {
    use super::card_named;

    #[test]
    fn a_native_card_is_found_by_the_model_in_its_name() {
        let devices = vec![
            "Built-in Output".to_owned(),
            "Vendor Kontrol S4 MK3".to_owned(),
        ];
        assert_eq!(
            card_named("Kontrol S4 MK3", &devices).as_deref(),
            Some("Vendor Kontrol S4 MK3")
        );
        assert_eq!(card_named("Kontrol S2 MK3", &devices), None);
    }

    use super::*;
    use crate::{Action, Trigger};

    #[test]
    fn shipped_profiles_load_and_have_mappings() {
        for text in FILES {
            if let Err(e) = serde_json::from_str::<Profile>(text) {
                panic!("{e}");
            }
        }
        assert_eq!(all().len(), FILES.len());
        for p in all() {
            assert!(
                !p.mappings.is_empty() && !p.ports.is_empty(),
                "{}",
                p.name()
            );
            assert!(p.mappings.iter().all(|m| m.port.is_none()), "{}", p.name());
        }
    }

    fn flx4() -> (MappingSet, crate::relative::Source) {
        let mut set = MappingSet::default();
        set.add_profile(&for_port("DDJ-FLX4").unwrap(), "DDJ-FLX4", "DDJ-FLX4");
        (set, crate::relative::Source::for_port("DDJ-FLX4"))
    }

    fn send(
        set: &MappingSet,
        source: &mut crate::relative::Source,
        bytes: &[u8],
    ) -> Vec<(Action, Trigger)> {
        let event = crate::midi::parse(bytes).expect("MIDI");
        set.map_source(
            &event,
            source,
            &crate::feedback::Feedback::default(),
            &|_| None,
        )
    }

    #[test]
    fn ready_decks_and_pad_modes_keep_apart() {
        let mut set = MappingSet::default();
        let port = "Reloop Ready In";
        set.add_profile(&for_port(port).unwrap(), port, "Reloop Ready Out");
        let mut src = crate::relative::Source::for_port(port);
        assert_eq!(
            send(&set, &mut src, &[0x90, 0x0C, 0x7F]),
            [(Action::PlayPause { deck: 0 }, Trigger::Press)]
        );
        assert_eq!(
            send(&set, &mut src, &[0x91, 0x0C, 0x7F]),
            [(Action::PlayPause { deck: 1 }, Trigger::Press)]
        );
        send(&set, &mut src, &[0x94, 0x11, 0x7F]);
        let pad_b = send(&set, &mut src, &[0x95, 0x20, 0x7F]);
        assert!(
            pad_b.contains(&(Action::Hotcue { deck: 1, slot: 0 }, Trigger::Press)),
            "{pad_b:?}"
        );
        let pad_a = send(&set, &mut src, &[0x94, 0x20, 0x7F]);
        assert!(
            !pad_a
                .iter()
                .any(|(a, _)| matches!(a, Action::Hotcue { .. })),
            "{pad_a:?}"
        );
    }

    #[test]
    fn flx4_messages_become_actions() {
        let (set, mut src) = flx4();
        assert_eq!(
            send(&set, &mut src, &[0x91, 0x0B, 0x7F]),
            [(Action::PlayPause { deck: 1 }, Trigger::Press)]
        );
        match send(&set, &mut src, &[0xB0, 0x22, 0x42])[..] {
            [(Action::JogTurn { deck: 0 }, Trigger::Delta(d))] => {
                assert!((d - 2.0 / 720.0).abs() < 1e-6, "{d}");
            }
            ref other => panic!("{other:?}"),
        }
        assert_eq!(send(&set, &mut src, &[0xB0, 0x00, 0x40]), Vec::new());
        match send(&set, &mut src, &[0xB0, 0x20, 0x00])[..] {
            [(Action::Tempo { deck: 0 }, Trigger::Set(v))] => {
                assert!((v - 0.5).abs() < 1e-6, "{v}");
            }
            ref other => panic!("{other:?}"),
        }
        assert_eq!(
            send(&set, &mut src, &[0x97, 0x02, 0x7F]),
            [(Action::Hotcue { deck: 0, slot: 2 }, Trigger::Press)]
        );
        assert_eq!(
            send(&set, &mut src, &[0x98, 0x02, 0x7F]),
            [(Action::DeleteHotcue { deck: 0, slot: 2 }, Trigger::Press)]
        );
        assert_eq!(
            send(&set, &mut src, &[0x97, 0x35, 0x7F]),
            [(
                Action::SampleCell {
                    deck: 2,
                    slot: 1,
                    cell: 1
                },
                Trigger::Press
            )]
        );
        assert_eq!(
            send(&set, &mut src, &[0x98, 0x35, 0x7F]),
            [(
                Action::SampleCellStopLoad {
                    deck: 2,
                    slot: 1,
                    cell: 1
                },
                Trigger::Press
            )]
        );
        let mut other = crate::relative::Source::for_port("Other");
        assert_eq!(send(&set, &mut other, &[0x91, 0x0B, 0x7F]), Vec::new());
    }

    #[test]
    fn flx4_beat_fx_reaches_both_units_and_their_knobs() {
        let (set, mut src) = flx4();
        let knob = |unit, knob| Action::FxKnob { unit, knob };
        let assign = |channel, unit| Action::FxAssign { channel, unit };
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x4B, 0x7F]),
            [(knob(0, 0), Trigger::Delta(8.0))],
            "BEAT ▶ moves the first knob by 1/8"
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x6B, 0x7F]),
            Vec::new(),
            "SHIFT + BEAT ▶ picks the next knob"
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x4A, 0x7F]),
            [(knob(0, 1), Trigger::Delta(-8.0))]
        );
        send(&set, &mut src, &[0x94, 0x6B, 0x7F]);
        send(&set, &mut src, &[0x94, 0x6B, 0x7F]);
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x4A, 0x7F]),
            [(knob(0, 2), Trigger::Delta(-8.0))],
            "the last knob is as far as it goes"
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x11, 0x7F]),
            [
                (assign(0, 0), Trigger::Set(0.0)),
                (assign(1, 0), Trigger::Set(1.0))
            ],
            "channel switch on 2"
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x14, 0x7F]),
            [
                (assign(0, 0), Trigger::Set(1.0)),
                (assign(1, 0), Trigger::Set(1.0))
            ],
            "channel switch on 1&2"
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x47, 0x7F]),
            [(Action::FxOn { unit: 0 }, Trigger::Press)]
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x43, 0x7F]),
            Vec::new(),
            "SHIFT + ON/OFF switches to FX2"
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x4B, 0x7F]),
            [(knob(1, 2), Trigger::Delta(8.0))]
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x10, 0x7F]),
            [
                (assign(0, 1), Trigger::Set(1.0)),
                (assign(1, 1), Trigger::Set(0.0))
            ],
            "the switch assigns the unit in control"
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x47, 0x7F]),
            [(Action::FxOn { unit: 1 }, Trigger::Press)]
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x63, 0x7F]),
            [(Action::FxSelect { unit: 1, slot: 0 }, Trigger::Delta(1.0))]
        );
        send(&set, &mut src, &[0x94, 0x43, 0x7F]);
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x47, 0x7F]),
            [(Action::FxOn { unit: 0 }, Trigger::Press)],
            "and back to FX1"
        );
    }

    fn model(port: &str) -> (MappingSet, crate::relative::Source) {
        let mut set = MappingSet::default();
        set.add_profile(&for_port(port).unwrap(), port, port);
        (set, crate::relative::Source::for_port(port))
    }

    #[test]
    fn flx2_jogs_have_420_counts_and_it_keeps_alive() {
        let (set, mut src) = model("DDJ-FLX2");
        match send(&set, &mut src, &[0xB1, 0x22, 0x41])[..] {
            [(Action::JogTurn { deck: 1 }, Trigger::Delta(d))] => {
                assert!((d - 1.0 / 420.0).abs() < 1e-6, "{d}");
            }
            ref other => panic!("{other:?}"),
        }
        assert_eq!(
            send(&set, &mut src, &[0x97, 0x60, 0x7F]),
            [(
                Action::LoopSize {
                    deck: 0,
                    beats: 0.25
                },
                Trigger::Press
            )]
        );
        let m = &set.port_messages[0];
        assert_eq!(m.keep_alive[6..11], [0x04, 0x0D, 0x00, 0x50, 0x02]);
    }

    #[test]
    fn ddj400_loop_and_fx_controls() {
        let (set, mut src) = model("DDJ-400");
        assert_eq!(
            send(&set, &mut src, &[0x90, 0x4D, 0x7F]),
            [(Action::LoopActive { deck: 0 }, Trigger::Press)],
            "RELOOP/EXIT"
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x47, 0x7F]),
            [(Action::FxOn { unit: 0 }, Trigger::Press)]
        );
        assert_eq!(
            send(&set, &mut src, &[0x96, 0x47, 0x7F]),
            [(Action::LoadSelected { deck: 1 }, Trigger::Press)]
        );
        assert_eq!(set.port_messages[0].dump[6..8], [0x02, 0x06]);
    }

    #[test]
    fn ddj1000_jogs_and_faders_over_midi_and_its_pad_modes() {
        let (set, mut src) = model("DDJ-1000");
        let (action, d) = turns(&set, &mut src, &[0xB2, 0x22, 0x41]);
        assert_eq!(action, Action::JogTurn { deck: 2 });
        assert!((d - 1.0 / 12960.0).abs() < 1e-9, "{d}");
        send(&set, &mut src, &[0xB1, 0x13, 0x7F]);
        assert_eq!(
            send(&set, &mut src, &[0xB1, 0x33, 0x7F]),
            [(Action::Volume { channel: 1 }, Trigger::Set(1.0))]
        );
        send(&set, &mut src, &[0xB6, 0x1F, 0x00]);
        assert_eq!(
            send(&set, &mut src, &[0xB6, 0x3F, 0x00]),
            [(Action::Crossfader, Trigger::Set(0.0))]
        );
        assert_eq!(
            send(&set, &mut src, &[0x97, 0x00, 0x7F]),
            [(Action::Hotcue { deck: 0, slot: 0 }, Trigger::Press)]
        );
        assert_eq!(
            send(&set, &mut src, &[0x98, 0x19, 0x7F]),
            [(
                Action::PadFx {
                    deck: 0,
                    bank: 0,
                    pad: 9
                },
                Trigger::Press
            )]
        );
        assert_eq!(
            send(&set, &mut src, &[0x99, 0x60, 0x7F]),
            [(
                Action::LoopSize {
                    deck: 1,
                    beats: 0.25
                },
                Trigger::Press
            )]
        );
        assert_eq!(
            send(&set, &mut src, &[0x90, 0x26, 0x7F]),
            [(
                Action::BeatJumpPage {
                    deck: 0,
                    direction: -1.0,
                    far: false
                },
                Trigger::Press
            )]
        );
        assert_eq!(
            send(&set, &mut src, &[0x96, 0x49, 0x7F]),
            [(Action::LoadSelected { deck: 3 }, Trigger::Press)]
        );
        assert_eq!(send(&set, &mut src, &[0x94, 0x47, 0x7F]), Vec::new());
        let m = &set.port_messages[0];
        assert_eq!(m.dump[6..11], [0x02, 0x00, 0x00, 0x03, 0x01]);
        assert_eq!(m.keep_alive[6..11], [0x02, 0x00, 0x00, 0x50, 0x01]);
    }

    #[test]
    fn ddj800_is_the_1000_with_6480_counts() {
        let (set, mut src) = model("DDJ-800");
        let (action, d) = turns(&set, &mut src, &[0xB1, 0x22, 0x41]);
        assert_eq!(action, Action::JogTurn { deck: 1 });
        assert!((d - 1.0 / 6480.0).abs() < 1e-9, "{d}");
        assert_eq!(
            send(&set, &mut src, &[0x99, 0x08, 0x7F]),
            Vec::new(),
            "hot cue 9 is not the app's"
        );
        assert_eq!(
            send(&set, &mut src, &[0x96, 0x46, 0x7F]),
            [(Action::LoadSelected { deck: 0 }, Trigger::Press)]
        );
        assert_eq!(set.port_messages[0].dump[6..8], [0x02, 0x09]);
    }

    #[test]
    fn ddj200_jogs_over_midi_and_no_sound_card() {
        let (set, mut src) = model("DDJ-200");
        let (action, d) = turns(&set, &mut src, &[0xB1, 0x22, 0x41]);
        assert_eq!(action, Action::JogTurn { deck: 1 });
        assert!((d - 1.0 / 420.0).abs() < 1e-6, "{d}");
        assert_eq!(
            send(&set, &mut src, &[0x90, 0x36, 0x7F]),
            [(Action::JogTouch { deck: 0 }, Trigger::Press)]
        );
        assert_eq!(set.port_messages[0].dump[6..8], [0x02, 0x0A]);
        assert!(for_port("DDJ-200").unwrap().audio.is_none());
    }

    #[test]
    fn r_series_jogs_and_faders_over_midi() {
        for (port, counts, model_byte) in [
            ("DDJ-RB", 720.0, 0x0E),
            ("DDJ-RR", 720.0, 0x0D),
            ("DDJ-RZ", 3240.0, 0x04),
            ("DDJ-RZX", 3240.0, 0x14),
        ] {
            let (set, mut src) = model(port);
            let (action, d) = turns(&set, &mut src, &[0xB1, 0x22, 0x41]);
            assert_eq!(action, Action::JogTurn { deck: 1 }, "{port}");
            assert!((d - 1.0 / counts).abs() < 1e-6, "{port} {d}");
            send(&set, &mut src, &[0xB0, 0x13, 0x00]);
            assert_eq!(
                send(&set, &mut src, &[0xB0, 0x33, 0x00]),
                [(Action::Volume { channel: 0 }, Trigger::Set(0.0))],
                "{port}"
            );
            assert_eq!(
                send(&set, &mut src, &[0x96, 0x46, 0x7F]),
                [(Action::LoadSelected { deck: 0 }, Trigger::Press)],
                "{port}"
            );
            let m = &set.port_messages[0];
            assert_eq!(
                m.dump[6..11],
                [0x00, model_byte, 0x00, 0x03, 0x01],
                "{port}"
            );
        }
        let (rzx, _) = model("DDJ-RZX");
        assert_eq!(rzx.port_messages[0].quit[9..11], [0x50, 0x00]);
    }

    #[test]
    fn wego4_jogs_and_faders_over_midi() {
        let (set, mut src) = model("DDJ-WeGO4");
        let (action, d) = turns(&set, &mut src, &[0xB0, 0x22, 0x3F]);
        assert_eq!(action, Action::JogTurn { deck: 0 });
        assert!((d + 1.0 / 420.0).abs() < 1e-6, "{d}");
        send(&set, &mut src, &[0xB6, 0x15, 0x7F]);
        assert_eq!(
            send(&set, &mut src, &[0xB6, 0x35, 0x7F]),
            [(Action::Volume { channel: 1 }, Trigger::Set(1.0))]
        );
        assert_eq!(set.port_messages[0].dump[6..8], [0x01, 0x00]);
    }

    #[test]
    fn four_deck_models_drive_decks_3_and_4_and_follow_the_side() {
        let (set, mut src) = model("DDJ-FLX6");
        assert_eq!(
            send(&set, &mut src, &[0x92, 0x0B, 0x7F]),
            [(Action::PlayPause { deck: 2 }, Trigger::Press)]
        );
        assert_eq!(
            send(&set, &mut src, &[0x93, 0x3C, 0x7F]),
            [(Action::DeckFocus { deck: 3 }, Trigger::Press)]
        );
        assert!(
            set.mappings
                .iter()
                .filter(|m| matches!(m.action, Action::Tempo { .. }))
                .all(|m| m.soft_takeover),
            "tempo takes over"
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x48, 0x7F]),
            [(Action::FxButton { unit: 0, button: 1 }, Trigger::Press)]
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x1E, 0x7F]),
            [(
                Action::FxAssignHeld {
                    channel: 2,
                    unit: 0
                },
                Trigger::Press
            )]
        );
        match send(&set, &mut src, &[0xB3, 0x22, 0x41])[..] {
            [(Action::JogTurn { deck: 3 }, Trigger::Delta(d))] => {
                assert!((d - 1.0 / 6480.0).abs() < 1e-9, "{d}");
            }
            ref other => panic!("{other:?}"),
        }
        for port in ["DDJ-XP1", "DDJ-XP2", "Pioneer DDJ-SP1"] {
            let (set, _) = model(port);
            assert!(
                set.mappings
                    .iter()
                    .any(|m| matches!(m.action, Action::Hotcue { .. })),
                "{port}"
            );
        }
        let (sr, _) = model("PIONEER DDJ-SR");
        assert_eq!(sr.port_messages[0].connect, [vec![0x9B, 0x08, 0x00]]);
    }

    #[test]
    fn every_model_plays_and_lights_play_on_its_first_deck() {
        let decks = all().into_iter().filter(|p| {
            p.mappings
                .iter()
                .any(|m| matches!(m.action, Action::PlayPause { .. }))
        });
        for p in decks {
            let port = &p.ports[0];
            let (channel, note) = p
                .mappings
                .iter()
                .find_map(|m| match (m.action, &m.input) {
                    (
                        Action::PlayPause { deck: 0 },
                        crate::Input::Midi {
                            channel,
                            kind: crate::MidiKind::Note,
                            number,
                        },
                    ) if m.gate.conditions.iter().all(|c| c.value == 0) => {
                        Some((*channel, *number))
                    }
                    _ => None,
                })
                .unwrap_or((0, 0x0B));
            let status = 0x90 | channel;
            let (set, mut src) = model(port);
            assert_eq!(
                send(&set, &mut src, &[status, note, 0x7F]),
                [(Action::PlayPause { deck: 0 }, Trigger::Press)],
                "{port}"
            );
            let mut f = crate::feedback::Feedback::default();
            f.decks[0].playing = true;
            let out = crate::output::Sender::default().update(
                &set.outputs,
                &f,
                &crate::modifier::Modifiers::default(),
                &[],
                false,
            );
            assert!(out.contains(&[status, note, 0x7F]), "{port}");
        }
    }

    fn turns(set: &MappingSet, src: &mut crate::relative::Source, bytes: &[u8]) -> (Action, f32) {
        match send(set, src, bytes)[..] {
            [(action, Trigger::Delta(d))] => (action, d),
            ref other => panic!("{other:?}"),
        }
    }

    #[test]
    fn inpulse_300_jogs_pads_and_vinyl() {
        let (set, mut src) = model("DJControl Inpulse 300");
        let (a, d) = turns(&set, &mut src, &[0xB1, 0x0A, 0x01]);
        assert_eq!(a, Action::JogTurn { deck: 0 });
        assert!((d - 1.0 / 248.0).abs() < 1e-6, "{d}");
        let (a, d) = turns(&set, &mut src, &[0xB5, 0x0A, 0x7F]);
        assert_eq!(a, Action::JogTurn { deck: 1 });
        assert!((d + 4.0 / 248.0).abs() < 1e-6, "{d}");
        assert_eq!(
            send(&set, &mut src, &[0x96, 0x02, 0x7F]),
            [(Action::Hotcue { deck: 0, slot: 2 }, Trigger::Press)]
        );
        assert_eq!(
            send(&set, &mut src, &[0x97, 0x0A, 0x7F]),
            [(Action::DeleteHotcue { deck: 1, slot: 2 }, Trigger::Press)]
        );
        assert_eq!(
            send(&set, &mut src, &[0x96, 0x35, 0x7F]),
            [(
                Action::SampleCell {
                    deck: 2,
                    slot: 1,
                    cell: 1
                },
                Trigger::Press
            )]
        );
        let vinyl = |src: &mut crate::relative::Source| {
            let out = send(&set, src, &[0x91, 0x03, 0x7F]);
            send(&set, src, &[0x91, 0x03, 0x00]);
            out
        };
        let jog = |turntable| Action::JogMode { deck: 0, turntable };
        assert_eq!(vinyl(&mut src), [(jog(false), Trigger::Press)]);
        assert_eq!(vinyl(&mut src), [(jog(true), Trigger::Press)]);
    }

    #[test]
    fn inpulse_500_left_side_drives_deck_a_or_c() {
        let (set, mut src) = model("DJControl Inpulse 500");
        let play = |src: &mut crate::relative::Source| send(&set, src, &[0x91, 0x07, 0x7F]);
        assert_eq!(
            play(&mut src),
            [(Action::PlayPause { deck: 0 }, Trigger::Press)]
        );
        send(&set, &mut src, &[0x90, 0x16, 0x7F]);
        assert_eq!(
            play(&mut src),
            [(Action::PlayPause { deck: 2 }, Trigger::Press)]
        );
        let (a, d) = turns(&set, &mut src, &[0xB1, 0x0A, 0x01]);
        assert_eq!(a, Action::JogTurn { deck: 2 });
        assert!((d - 1.0 / 720.0).abs() < 1e-6, "{d}");
        assert_eq!(
            send(&set, &mut src, &[0x92, 0x07, 0x7F]),
            [(Action::PlayPause { deck: 1 }, Trigger::Press)]
        );
        send(&set, &mut src, &[0x90, 0x14, 0x7F]);
        assert_eq!(
            play(&mut src),
            [(Action::PlayPause { deck: 0 }, Trigger::Press)]
        );
    }

    #[test]
    fn mixtrack_pro_fx_shift_and_pad_modes_in_software() {
        let (set, mut src) = model("Mixtrack Pro FX");
        let (a, d) = turns(&set, &mut src, &[0xB1, 0x06, 0x02]);
        assert_eq!(a, Action::JogTurn { deck: 1 });
        assert!((d - 2.0 / 1024.0).abs() < 1e-6, "{d}");
        send(&set, &mut src, &[0x90, 0x20, 0x7F]);
        let (a, _) = turns(&set, &mut src, &[0xB1, 0x06, 0x7F]);
        assert_eq!(a, Action::JogSearch { deck: 1 });
        send(&set, &mut src, &[0x80, 0x20, 0x00]);
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x14, 0x7F]),
            [(Action::Hotcue { deck: 0, slot: 0 }, Trigger::Press)]
        );
        send(&set, &mut src, &[0x94, 0x0D, 0x7F]);
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x15, 0x7F]),
            [(
                Action::LoopSize {
                    deck: 0,
                    beats: 0.125
                },
                Trigger::Press
            )]
        );
        let start = set.port_messages[0].on_connect();
        assert_eq!(start.len(), 3);
        assert_eq!(start[1], [0xF0, 0x00, 0x20, 0x7F, 0x03, 0xF7]);
        let slot = Action::FxButton { unit: 1, button: 0 };
        assert_eq!(
            send(&set, &mut src, &[0xB9, 0x03, 0x02]),
            [(slot, Trigger::Set(1.0))]
        );
        assert_eq!(
            send(&set, &mut src, &[0xB9, 0x03, 0x00]),
            [(slot, Trigger::Set(0.0))]
        );
        assert_eq!(
            send(&set, &mut src, &[0x95, 0x15, 0x7F]),
            [(Action::Hotcue { deck: 1, slot: 1 }, Trigger::Press)]
        );
        let (set, mut src) = model("Mixtrack Platinum FX");
        send(&set, &mut src, &[0x96, 0x0B, 0x7F]);
        assert_eq!(
            send(&set, &mut src, &[0x96, 0x14, 0x7F]),
            [(
                Action::SampleCell {
                    deck: 2,
                    slot: 0,
                    cell: 0
                },
                Trigger::Press
            )]
        );
        assert_eq!(
            send(&set, &mut src, &[0x97, 0x14, 0x7F]),
            [(Action::Hotcue { deck: 3, slot: 0 }, Trigger::Press)]
        );
        let jog = |deck, turntable| (Action::JogMode { deck, turntable }, Trigger::Press);
        assert_eq!(send(&set, &mut src, &[0x92, 0x07, 0x7F]), [jog(2, false)]);
        assert_eq!(send(&set, &mut src, &[0x90, 0x07, 0x7F]), [jog(0, false)]);
        send(&set, &mut src, &[0x92, 0x08, 0x7F]);
        let assign = |unit| Action::FxAssign { channel: 2, unit };
        assert_eq!(
            send(&set, &mut src, &[0xB8, 0x03, 0x01]),
            [
                (assign(0), Trigger::Set(1.0)),
                (assign(1), Trigger::Set(1.0))
            ]
        );
    }

    #[test]
    fn mc7000_jogs_shift_and_pad_modes() {
        let (set, mut src) = model("DENON MC7000");
        let (a, d) = turns(&set, &mut src, &[0xB2, 0x06, 0x03]);
        assert_eq!(a, Action::JogTurn { deck: 2 });
        assert!((d - 3.0 / 894.0).abs() < 1e-6, "{d}");
        send(&set, &mut src, &[0x90, 0x32, 0x7F]);
        assert_eq!(
            turns(&set, &mut src, &[0xB2, 0x06, 0x01]).0,
            Action::JogSearch { deck: 2 }
        );
        assert_eq!(
            turns(&set, &mut src, &[0xB1, 0x06, 0x01]).0,
            Action::JogTurn { deck: 1 }
        );
        send(&set, &mut src, &[0x90, 0x32, 0x00]);
        send(&set, &mut src, &[0x94, 0x09, 0x7F]);
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x15, 0x7F]),
            [(
                Action::BeatJump {
                    deck: 0,
                    beats: 2.0
                },
                Trigger::Press
            )]
        );
        assert_eq!(
            send(&set, &mut src, &[0x94, 0x19, 0x7F]),
            [(
                Action::BeatJump {
                    deck: 0,
                    beats: -2.0
                },
                Trigger::Press
            )]
        );
        let jog = |deck| {
            (
                Action::JogMode {
                    deck,
                    turntable: false,
                },
                Trigger::Press,
            )
        };
        assert_eq!(send(&set, &mut src, &[0x92, 0x07, 0x7F]), [jog(2)]);
        assert_eq!(send(&set, &mut src, &[0x90, 0x07, 0x7F]), [jog(0)]);
        let side = Action::XfaderAssign { channel: 3 };
        assert_eq!(
            send(&set, &mut src, &[0x93, 0x1E, 0x01]),
            [(side, Trigger::Set(1.0))]
        );
        assert_eq!(
            send(&set, &mut src, &[0x93, 0x1E, 0x00]),
            [(side, Trigger::Set(0.5))]
        );
        assert_eq!(
            send(&set, &mut src, &[0x95, 0x15, 0x00]),
            [(Action::Hotcue { deck: 1, slot: 1 }, Trigger::Release)]
        );
    }

    #[test]
    fn flx4_leds_follow_the_decks() {
        let (set, _) = flx4();
        let mut f = crate::feedback::Feedback::default();
        f.decks[1].synced = true;
        f.decks[0].hotcues[3] = crate::feedback::HotcueKind::Cue;
        let mut sent = crate::output::Sender::default();
        let out = sent.update(
            &set.outputs,
            &f,
            &crate::modifier::Modifiers::default(),
            &[],
            false,
        );
        assert!(out.contains(&[0x91, 0x58, 0x7F]), "deck B SYNC lit");
        assert!(out.contains(&[0x90, 0x58, 0x00]), "deck A SYNC off");
        assert!(out.contains(&[0x97, 0x03, 0x7F]) && out.contains(&[0x98, 0x03, 0x7F]));
        assert!(out.contains(&[0x97, 0x04, 0x00]));
    }

    #[test]
    fn user_profiles_join_and_replace_the_shipped_ones() {
        let dir =
            std::env::temp_dir().join(format!("backspn-user-profiles-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut mine = for_port("DDJ-FLX4").unwrap();
        mine.notes = "mine".into();
        std::fs::write(dir.join("flx4.json"), serde_json::to_string(&mine).unwrap()).unwrap();
        let other = Profile {
            brand: "Acme".into(),
            model: "One".into(),
            ..mine.clone()
        };
        std::fs::write(dir.join("one.json"), serde_json::to_string(&other).unwrap()).unwrap();
        std::fs::write(dir.join("bad.json"), "{").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let (found, errors) = read_dir(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!((found.len(), errors.len()), (2, 1));
        let list = with_user(found);
        assert_eq!(list.len(), all().len() + 1);
        assert_eq!(list[0].brand, "Acme");
        assert!(
            list.iter()
                .any(|p| p.model == "DDJ-FLX4" && p.notes == "mine")
        );
        assert_eq!(read_dir(std::path::Path::new("/nonexistent")).0, Vec::new());
    }

    #[test]
    fn adding_a_profile_ties_it_to_its_ports() {
        let p = for_port("DDJ-FLX4").expect("FLX4");
        let mut set = MappingSet::default();
        set.add_profile(&p, "DDJ-FLX4", "DDJ-FLX4");
        set.add_profile(&p, "DDJ-FLX4", "DDJ-FLX4");
        assert_eq!(set.mappings.len(), p.mappings.len(), "added once");
        assert!(set.has_port("DDJ-FLX4"));
        assert_eq!(set.midi_inputs, ["DDJ-FLX4"]);
        assert_eq!(set.port_messages.len(), 1);
        assert_eq!(set.port_messages[0].port, "DDJ-FLX4");
        set.remove_ports("DDJ-FLX4", "DDJ-FLX4");
        assert!(set.mappings.is_empty() && set.outputs.is_empty());
    }
}
