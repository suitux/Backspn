use backspn_control::{Action, LoadMode, LoopRecorderPart, MoveMode, MoveSize, StemPart};
use eframe::egui::Ui;

use crate::i18n::tr;

struct Category {
    name: &'static str,
    actions: Vec<Action>,
}

fn deck_common(d: usize) -> Vec<Action> {
    let mut v = vec![
        Action::PlayPause { deck: d },
        Action::Cue { deck: d },
        Action::Cup { deck: d },
        Action::Sync { deck: d },
        Action::TempoSync { deck: d },
        Action::PhaseSync { deck: d },
        Action::Master { deck: d },
        Action::Keylock { deck: d },
        Action::Slip { deck: d },
        Action::Reverse { deck: d },
        Action::Tempo { deck: d },
        Action::TempoRange { deck: d },
        Action::TempoRangeCycle { deck: d },
        Action::TempoBend {
            deck: d,
            direction: -1.0,
        },
        Action::TempoBend {
            deck: d,
            direction: 1.0,
        },
        Action::BeatTap { deck: d },
        Action::DeckTick { deck: d },
        Action::CopyPhase { deck: d },
        Action::DeckSize { deck: d },
        Action::DeckFocus { deck: d },
        Action::DeckType { deck: d },
        Action::JogTouch { deck: d },
        Action::JogTurn { deck: d },
        Action::JogMode {
            deck: d,
            turntable: true,
        },
        Action::JogMode {
            deck: d,
            turntable: false,
        },
        Action::MotorOff { deck: d },
        Action::FaderStart {
            deck: d,
            play: true,
        },
        Action::FaderStart {
            deck: d,
            play: false,
        },
        Action::LoadSelected { deck: d },
        Action::LoadSelectedAs {
            deck: d,
            mode: LoadMode::Alternative,
        },
        Action::LoadSelectedAs {
            deck: d,
            mode: LoadMode::Track,
        },
        Action::LoadSelectedAs {
            deck: d,
            mode: LoadMode::Stems,
        },
        Action::LoadNext { deck: d },
        Action::LoadPrevious { deck: d },
        Action::LoadLastRecording { deck: d },
    ];
    v.extend(
        (0..4)
            .filter(|&f| f != d)
            .map(|from| Action::DuplicateDeck { deck: d, from }),
    );
    v
}

fn cue_loops(d: usize) -> Vec<Action> {
    let mut v = vec![
        Action::LoopIn { deck: d },
        Action::LoopOut { deck: d },
        Action::LoopActive { deck: d },
        Action::FastLoop { deck: d },
        Action::LoopSize {
            deck: d,
            beats: 4.0,
        },
        Action::FastLoopSize {
            deck: d,
            beats: 4.0,
        },
        Action::LoopMove { deck: d },
        Action::BeatJump {
            deck: d,
            beats: -4.0,
        },
        Action::BeatJump {
            deck: d,
            beats: 4.0,
        },
        Action::Move {
            deck: d,
            direction: -1.0,
        },
        Action::Move {
            deck: d,
            direction: 1.0,
        },
    ];
    v.extend(MoveMode::ALL.map(|mode| Action::MoveMode { deck: d, mode }));
    v.extend(
        (0..=12)
            .filter_map(MoveSize::new)
            .map(|size| Action::MoveSize { deck: d, size }),
    );
    v.extend([
        Action::CueStore { deck: d },
        Action::CueSetStore { deck: d },
        Action::CueDelete { deck: d },
        Action::CueJump {
            deck: d,
            direction: -1.0,
        },
        Action::CueJump {
            deck: d,
            direction: 1.0,
        },
        Action::CueMove { deck: d },
    ]);
    v.extend((0..6).map(|kind| Action::CueType { deck: d, kind }));
    v.extend((0..8).map(|slot| Action::MapHotcue { deck: d, slot }));
    v.extend((0..8).map(|slot| Action::Hotcue { deck: d, slot }));
    v.extend((0..8).map(|slot| Action::DeleteHotcue { deck: d, slot }));
    v.push(Action::Freeze { deck: d });
    v.extend((0..backspn_engine::SLICES).map(|slot| Action::Slice { deck: d, slot }));
    v.extend((0..8).map(|pad| Action::BeatJumpPad { deck: d, pad }));
    for direction in [-1.0, 1.0] {
        for far in [false, true] {
            v.push(Action::BeatJumpPage {
                deck: d,
                direction,
                far,
            });
        }
    }
    v
}

fn track_deck(d: usize) -> Vec<Action> {
    let mut v = vec![
        Action::KeyAdjust { deck: d },
        Action::KeyFine { deck: d },
        Action::KeySemitone {
            deck: d,
            direction: -1.0,
        },
        Action::KeySemitone {
            deck: d,
            direction: 1.0,
        },
        Action::KeyReset { deck: d },
    ];
    v.extend((0..8).map(|pad| Action::KeyShiftPad { deck: d, pad }));
    v.extend((0..8).map(|pad| Action::KeyboardPad { deck: d, pad }));
    v.push(Action::KeyboardDelete { deck: d });
    for direction in [-1.0, 1.0] {
        v.push(Action::KeyShiftPage { deck: d, direction });
        v.push(Action::KeyboardPage { deck: d, direction });
    }
    for bank in 0..2 {
        v.extend((0..16).map(|pad| Action::PadFx { deck: d, bank, pad }));
    }
    for direction in [-1.0, 1.0] {
        v.push(Action::PadFxBeat { deck: d, direction });
    }
    v
}

fn stem_deck(d: usize) -> Vec<Action> {
    (0..backspn_engine::STEMS)
        .flat_map(|stem| {
            StemPart::ALL.map(|part| Action::Stem {
                deck: d,
                stem,
                part,
            })
        })
        .collect()
}

fn sample_deck(d: usize) -> Vec<Action> {
    use backspn_control::{SamplePart, SampleSlotPart};
    let mut v: Vec<Action> = SamplePart::ALL
        .map(|part| Action::SampleDeck { deck: d, part })
        .to_vec();
    v.extend((0..4).flat_map(|slot| {
        SampleSlotPart::ALL.map(|part| Action::SampleSlot {
            deck: d,
            slot,
            part,
        })
    }));
    v.extend((0..4).flat_map(|slot| {
        (0..4).map(move |cell| Action::SampleCell {
            deck: d,
            slot,
            cell,
        })
    }));
    v
}

fn mixer(c: usize) -> Vec<Action> {
    let mut v = vec![
        Action::Volume { channel: c },
        Action::Gain { channel: c },
        Action::Balance { channel: c },
        Action::Mute { channel: c },
        Action::Eq {
            channel: c,
            band: 2,
        },
        Action::Eq {
            channel: c,
            band: 1,
        },
        Action::Eq {
            channel: c,
            band: 0,
        },
        Action::Eq {
            channel: c,
            band: 3,
        },
        Action::Filter { channel: c },
        Action::MonitorCue { channel: c },
        Action::FxAssign {
            channel: c,
            unit: 0,
        },
        Action::FxAssign {
            channel: c,
            unit: 1,
        },
        Action::XfaderAssign { channel: c },
    ];
    v.extend([false, true].map(|right| Action::XfaderSide { channel: c, right }));
    v.extend([
        Action::Crossfader,
        Action::XfaderCurve,
        Action::AutoFade { direction: -1.0 },
        Action::AutoFade { direction: 1.0 },
        Action::MasterVolume,
        Action::MasterBalance,
        Action::CueVolume,
        Action::CueMix,
    ]);
    v
}

fn fx_unit(u: usize) -> Vec<Action> {
    let mut v = vec![Action::FxOn { unit: u }, Action::FxDryWet { unit: u }];
    v.extend((0..3).map(|knob| Action::FxKnob { unit: u, knob }));
    v.extend((0..3).map(|button| Action::FxButton { unit: u, button }));
    v.extend((0..3).map(|slot| Action::FxSelect { unit: u, slot }));
    v.extend([
        Action::FxSnapshotStore { unit: u },
        Action::FxSnapshotLoad { unit: u },
        Action::FxMode { unit: u },
        Action::FxLfoReset,
    ]);
    v
}

fn drum_machine(u: usize) -> Vec<Action> {
    let mut v: Vec<Action> = (0..16)
        .map(|step| Action::PatternStep { unit: u, step })
        .collect();
    v.extend([
        Action::PatternReset { unit: u },
        Action::PatternClear { unit: u },
        Action::PatternKit { unit: u },
        Action::PatternRoll { unit: u },
    ]);
    v
}

fn master_clock() -> Vec<Action> {
    vec![
        Action::MasterTap,
        Action::MasterTick,
        Action::ClockSend,
        Action::ClockResync,
        Action::ClockExternal,
        Action::ClockLink,
        Action::LinkDownbeat,
    ]
}

fn loop_recorder() -> Vec<Action> {
    [
        LoopRecorderPart::Record,
        LoopRecorderPart::Play,
        LoopRecorderPart::Size,
        LoopRecorderPart::DryWet,
        LoopRecorderPart::Delete,
        LoopRecorderPart::Source,
        LoopRecorderPart::Undo,
        LoopRecorderPart::Position,
        LoopRecorderPart::State,
    ]
    .map(|part| Action::LoopRecorder { part })
    .to_vec()
}

fn categories() -> Vec<Category> {
    vec![
        Category {
            name: "Deck Common",
            actions: deck_common(0),
        },
        Category {
            name: "Cue/Loops",
            actions: cue_loops(0),
        },
        Category {
            name: "Track Deck",
            actions: track_deck(0),
        },
        Category {
            name: "Stem Deck",
            actions: stem_deck(0),
        },
        Category {
            name: "Sample Deck",
            actions: sample_deck(0),
        },
        Category {
            name: "Mixer",
            actions: mixer(0),
        },
        Category {
            name: "FX Unit",
            actions: fx_unit(0),
        },
        Category {
            name: "Drum Machine",
            actions: drum_machine(0),
        },
        Category {
            name: "Master Clock",
            actions: master_clock(),
        },
        Category {
            name: "Loop Recorder",
            actions: loop_recorder(),
        },
        Category {
            name: "Audio Recorder",
            actions: vec![Action::RecordStop, Action::RecordCut, Action::RecordGain],
        },
        Category {
            name: "Browser",
            actions: vec![
                Action::BrowserUpDown,
                Action::BrowserFavoriteStep,
                Action::PreviewLoad,
                Action::PreviewSeek,
            ],
        },
        Category {
            name: "Modifier",
            actions: (0..8).map(|index| Action::Modifier { index }).collect(),
        },
        Category {
            name: "Global",
            actions: vec![Action::Snap, Action::Quantize, Action::FocusToggle],
        },
    ]
}

/// Every command a mapping can take, on deck A, channel 1 and unit 1.
pub(crate) fn all() -> Vec<Action> {
    categories().into_iter().flat_map(|c| c.actions).collect()
}

pub(super) fn menu(ui: &mut Ui, label: &str, keep: impl Fn(Action) -> bool) -> Option<Action> {
    let mut chosen = None;
    ui.menu_button(tr(label), |ui| {
        for category in categories() {
            let actions: Vec<Action> = category.actions.into_iter().filter(|&a| keep(a)).collect();
            if actions.is_empty() {
                continue;
            }
            ui.menu_button(tr(category.name), |ui| {
                for action in actions {
                    if ui.button(super::bare_name(action)).clicked() {
                        chosen = Some(action);
                        ui.close();
                    }
                }
            });
        }
    });
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_offered_once() {
        let all = all();
        for (i, a) in all.iter().enumerate() {
            assert!(!all[..i].contains(a), "{} is offered twice", a.label());
        }
        assert!(all.contains(&Action::PlayPause { deck: 0 }));
        assert!(all.contains(&Action::Modifier { index: 7 }));
        assert!(all.contains(&Action::Crossfader));
    }

    #[test]
    fn deck_commands_come_on_deck_a() {
        for mut a in all() {
            let label = a.label();
            let deck = a
                .deck_mut()
                .map(|d| *d)
                .or_else(|| a.channel_mut().map(|c| *c));
            if let Some(d) = deck {
                assert_eq!(d, 0, "{label}");
            }
        }
    }
}
