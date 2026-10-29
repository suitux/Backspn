use std::sync::LazyLock;

use super::{analog, button, encoder};
use crate::feedback::{Feedback, SampleSlotFeedback};
use crate::modifier::Condition;
use crate::s4mk3::{Analog, Button, Decoder, Encoder, Layout, Leds, Shifted, colour};
use crate::{Action, Input, Interaction, Mapping, SamplePart, SampleSlotPart, StemPart};

pub const NAME: &str = "Kontrol F1";
pub const SOURCE: &str = "F1";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x1120;

const LED_LEN: usize = 80;
const LIT: u8 = 0x7F;
const DIM: u8 = 0x10;
const SLOTS: usize = 4;
const ROWS: usize = 4;

/// Two digits, the right one first.
const DISPLAY: usize = 0;
const PAD_BASE: usize = 24;
const STOP_LEDS: [usize; SLOTS] = [78, 76, 74, 72];
const STOP_MASKS: [u8; SLOTS] = [0x80, 0x40, 0x20, 0x10];

const SHIFT: usize = 1;
const BROWSE: usize = 2;

fn b(name: &str, byte: usize, mask: u8, led: usize) -> Button {
    button(name, byte, mask, Some(led), None)
}

fn pad_name(pad: usize) -> String {
    format!("Grid.{}", pad + 1)
}

fn pad_bit(pad: usize) -> (usize, u8) {
    (0x01 + pad / 8, 0x80 >> (pad % 8))
}

fn buttons() -> Vec<Button> {
    let mut v = vec![
        b("Browse", 0x03, 0x08, 16),
        b("Size", 0x03, 0x10, 17),
        b("Type", 0x03, 0x20, 18),
        b("Reverse", 0x03, 0x40, 19),
        b("Shift", 0x03, 0x80, 20),
        b("Capture", 0x04, 0x02, 21),
        b("Quant", 0x04, 0x04, 22),
        b("Sync", 0x04, 0x08, 23),
        button("Browse.Push", 0x03, 0x04, None, None),
    ];
    for slot in 0..SLOTS {
        v.push(b(
            &format!("Slot {}.Stop", slot + 1),
            0x04,
            STOP_MASKS[slot],
            STOP_LEDS[slot],
        ));
    }
    for pad in 0..SLOTS * ROWS {
        let (byte, mask) = pad_bit(pad);
        v.push(button(pad_name(pad), byte, mask, None, None));
    }
    v
}

const ANALOGS: &[Analog] = &[
    analog("Slot 1.Filter", 0x06),
    analog("Slot 2.Filter", 0x08),
    analog("Slot 3.Filter", 0x0A),
    analog("Slot 4.Filter", 0x0C),
    analog("Slot 1.Volume", 0x0E),
    analog("Slot 2.Volume", 0x10),
    analog("Slot 3.Volume", 0x12),
    analog("Slot 4.Volume", 0x14),
];

const ENCODERS: &[Encoder] = &[encoder("Browse.Turn", 0x05, false)];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 22)];

fn on_focus(name: &str, action: Action) -> Mapping {
    let mut m = Mapping::new(
        Input::Hid(name.to_owned()),
        action,
        action.default_interaction(),
    );
    m.focus = true;
    m
}

fn when(mut m: Mapping, modifier: usize, value: u8) -> Mapping {
    m.gate.conditions.push(Condition { modifier, value });
    m
}

fn held(name: &str, index: usize) -> Mapping {
    let mut m = super::map(name, Action::Modifier { index }, Interaction::Hold);
    m.gate.value = Some(1);
    m
}

fn default_mappings() -> Vec<Mapping> {
    let sample = |part| Action::SampleDeck { deck: 0, part };
    let stem = |stem, part| Action::Stem {
        deck: 0,
        stem,
        part,
    };
    let mut v = vec![
        held("Shift", SHIFT),
        held("Browse", BROWSE),
        on_focus("Browse", sample(SamplePart::CellLoadModifier)),
        when(
            on_focus("Capture", sample(SamplePart::CellCaptureModifier)),
            SHIFT,
            0,
        ),
        when(
            on_focus("Capture", sample(SamplePart::CellDeleteModifier)),
            SHIFT,
            1,
        ),
        on_focus("Reverse", sample(SamplePart::CellReverseModifier)),
        on_focus("Quant", sample(SamplePart::QuantizeOn)),
        on_focus("Sync", Action::Sync { deck: 0 }),
        when(on_focus("Browse.Turn", sample(SamplePart::Page)), BROWSE, 0),
        when(
            super::map("Browse.Turn", Action::BrowserUpDown, Interaction::Relative),
            BROWSE,
            1,
        ),
        when(
            on_focus("Browse.Push", sample(SamplePart::PlayAll)),
            BROWSE,
            0,
        ),
        when(
            on_focus("Browse.Push", sample(SamplePart::LoadSet)),
            BROWSE,
            1,
        ),
    ];
    for slot in 0..SLOTS {
        let n = slot + 1;
        v.push(on_focus(
            &format!("Slot {n}.Volume"),
            stem(slot, StemPart::Volume),
        ));
        v.push(on_focus(
            &format!("Slot {n}.Filter"),
            stem(slot, StemPart::Filter),
        ));
        let stop = format!("Slot {n}.Stop");
        v.push(when(on_focus(&stop, stem(slot, StemPart::Mute)), SHIFT, 0));
        v.push(when(
            on_focus(
                &stop,
                Action::SampleSlot {
                    deck: 0,
                    slot,
                    part: SampleSlotPart::Secondary,
                },
            ),
            SHIFT,
            1,
        ));
    }
    for pad in 0..SLOTS * ROWS {
        v.push(on_focus(
            &pad_name(pad),
            Action::SampleCell {
                deck: 0,
                slot: pad % SLOTS,
                cell: pad / SLOTS,
            },
        ));
    }
    v
}

fn shifted(_name: &str) -> Shifted {
    None
}

fn scaled(rgb: [u8; 3], level: u8) -> [u8; 3] {
    rgb.map(|c| u8::try_from(u16::from(c) * u16::from(level) / 255).unwrap_or(level))
}

fn pad_colour(slot: &SampleSlotFeedback, cell: usize, fallback: [u8; 3]) -> [u8; 3] {
    let colour = slot.selected_colour.unwrap_or(fallback);
    match slot.cells.get(cell).copied().unwrap_or(0) {
        0 => [0; 3],
        2 => scaled(colour, LIT),
        3 => scaled(colour, LIT / 2),
        _ => scaled(colour, DIM),
    }
}

fn write_pad(leds: &mut Leds, pad: usize, [r, g, b]: [u8; 3]) {
    let base = PAD_BASE + 3 * pad;
    if let Some(bytes) = leds.0.get_mut(base..base + 3) {
        bytes.copy_from_slice(&[b, r, g]);
    }
}

fn set(leds: &mut Leds, decoder: &Decoder, name: &str, level: u8) {
    if let Some(i) = decoder.led(name)
        && let Some(byte) = leds.0.get_mut(i)
    {
        *byte = level;
    }
}

fn deck_leds(feedback: &Feedback, decoder: &Decoder, _layer: [usize; 2], leds: &mut Leds) {
    let focus = feedback.focus.min(feedback.decks.len() - 1);
    let deck = &feedback.decks[focus];
    set(leds, decoder, "Sync", if deck.synced { LIT } else { DIM });
    for name in ["Browse", "Capture", "Reverse", "Shift"] {
        set(leds, decoder, name, DIM);
    }
    let Some(sample) = deck.sample_deck else {
        set(leds, decoder, "Quant", 0);
        return;
    };
    set(
        leds,
        decoder,
        "Quant",
        if sample.quantize_on { LIT } else { DIM },
    );
    let fallback = colour::rgb(if focus >= 2 {
        colour::ORANGE
    } else {
        colour::BLUE
    });
    let [tens, units] = super::loop_display::number_digits(sample.page.saturating_add(1));
    super::loop_display::write_digits(&mut leds.0, DISPLAY, [units, tens], LIT);
    let first = usize::from(sample.page) * ROWS;
    for (slot, s) in sample.slots.iter().enumerate() {
        for row in 0..ROWS {
            write_pad(
                leds,
                row * SLOTS + slot,
                pad_colour(s, first + row, fallback),
            );
        }
        let level = match (s.state, s.muted) {
            (0, _) => 0,
            (_, true) => DIM,
            (2, false) => LIT,
            _ => LIT / 2,
        };
        let led = STOP_LEDS[slot];
        leds.0[led] = level;
        leds.0[led + 1] = level;
    }
}

pub static LAYOUT: LazyLock<Layout> = LazyLock::new(|| Layout {
    name: NAME,
    source: SOURCE,
    vendor_id: VENDOR_ID,
    product_id: PRODUCT_ID,
    buttons: buttons(),
    switches: &[],
    analogs: ANALOGS,
    encoders: ENCODERS,
    jogs: &[],
    jog_steps_per_turn: 1.0,
    analog_report: 0x01,
    jog_report: 0x03,
    encoder_report: 0x01,
    led_reports: &[(0x80, LED_LEN)],
    meter_report: None,
    initial_reports: INITIAL_REPORTS,
    motors: false,
    sound_card: None,
    calibration: &[],
    stored_calibration: None,
    rings: false,
    screens: false,
    tempo_leds: None,
    wheel_leds: None,
    strip_leds: None,
    meters: None,
    meter_leds: None,
    deck_leds: Some(deck_leds),
    default_mappings,
    shifted,
    upgrade: None,
    jog_button_toggles_touch: false,
    samples_button_stems: false,
    samples_button_toggles: false,
    fx_select_toggles: false,
    mx2_transport: false,
    shared_shift: false,
    connect_reports: &[],
});

pub fn start(
    events: crate::controller::EventSender,
    wake: crate::controller::Waker,
) -> Result<Box<dyn crate::controller::Controller>, String> {
    crate::s4mk3::driver::start_with(&LAYOUT, events, wake)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::SampleFeedback;

    fn find(name: &str) -> Vec<Mapping> {
        default_mappings()
            .into_iter()
            .filter(|m| m.input == Input::Hid(name.into()))
            .collect()
    }

    #[test]
    fn the_grid_plays_four_cells_of_each_slot() {
        let first = find("Grid.1");
        assert_eq!(
            first[0].action,
            Action::SampleCell {
                deck: 0,
                slot: 0,
                cell: 0
            }
        );
        assert!(first[0].focus, "follows the focused deck");
        let last = find("Grid.16");
        assert_eq!(
            last[0].action,
            Action::SampleCell {
                deck: 0,
                slot: 3,
                cell: 3
            }
        );
        let second_row = find("Grid.5");
        assert_eq!(
            second_row[0].action,
            Action::SampleCell {
                deck: 0,
                slot: 0,
                cell: 1
            }
        );
    }

    #[test]
    fn grid_bits_run_right_to_left_in_two_bytes() {
        assert_eq!(pad_bit(0), (0x01, 0x80));
        assert_eq!(pad_bit(7), (0x01, 0x01));
        assert_eq!(pad_bit(8), (0x02, 0x80));
        assert_eq!(pad_bit(15), (0x02, 0x01));
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 22];
        r[0] = 0x01;
        assert!(d.decode(&r).iter().all(|e| e.pressed.is_none()));
        r[0x02] = 0x01;
        let pressed: Vec<_> = d
            .decode(&r)
            .into_iter()
            .filter(|e| e.pressed == Some(true))
            .collect();
        assert_eq!(pressed.len(), 1);
        assert_eq!(pressed[0].input, Input::Hid("Grid.16".into()));
    }

    #[test]
    fn knobs_and_faders_drive_the_slots() {
        let volume = find("Slot 3.Volume");
        assert_eq!(
            volume[0].action,
            Action::Stem {
                deck: 0,
                stem: 2,
                part: StemPart::Volume
            }
        );
        let filter = find("Slot 1.Filter");
        assert_eq!(
            filter[0].action,
            Action::Stem {
                deck: 0,
                stem: 0,
                part: StemPart::Filter
            }
        );
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 22];
        r[0] = 0x01;
        r[0x14] = 0xFF;
        r[0x15] = 0x0F;
        let events = d.decode(&r);
        let v = events
            .iter()
            .find(|e| e.input == Input::Hid("Slot 4.Volume".into()))
            .expect("fader 4");
        assert!((v.value - 1.0).abs() < 1e-6);
    }

    #[test]
    fn stop_mutes_and_shift_stop_stops() {
        let stop = find("Slot 2.Stop");
        let plain = stop
            .iter()
            .find(|m| {
                m.gate.conditions
                    == [Condition {
                        modifier: SHIFT,
                        value: 0,
                    }]
            })
            .expect("plain");
        assert_eq!(
            plain.action,
            Action::Stem {
                deck: 0,
                stem: 1,
                part: StemPart::Mute
            }
        );
        let shifted = stop
            .iter()
            .find(|m| {
                m.gate.conditions
                    == [Condition {
                        modifier: SHIFT,
                        value: 1,
                    }]
            })
            .expect("shifted");
        assert_eq!(
            shifted.action,
            Action::SampleSlot {
                deck: 0,
                slot: 1,
                part: SampleSlotPart::Secondary
            }
        );
    }

    #[test]
    fn browse_held_turns_the_encoder_into_the_list() {
        let turn = find("Browse.Turn");
        assert_eq!(turn.len(), 2);
        let browsing = turn
            .iter()
            .find(|m| {
                m.gate.conditions
                    == [Condition {
                        modifier: BROWSE,
                        value: 1,
                    }]
            })
            .expect("browsing");
        assert_eq!(browsing.action, Action::BrowserUpDown);
        assert!(find("Browse").iter().any(|m| m.action
            == Action::SampleDeck {
                deck: 0,
                part: SamplePart::CellLoadModifier
            }));
    }

    #[test]
    fn pads_show_the_page_in_blue_red_green_order() {
        let mut f = Feedback {
            focus: 2,
            ..Feedback::default()
        };
        let mut r = SampleFeedback {
            page: 1,
            ..SampleFeedback::default()
        };
        r.slots[1].cells[4] = 2;
        r.slots[1].selected_colour = Some([255, 0, 0]);
        r.slots[1].state = 2;
        r.slots[0].cells[0] = 2;
        f.decks[2].sample_deck = Some(r);
        let d = Decoder::new(&LAYOUT);
        let mut leds = Leds::for_layout(&LAYOUT);
        deck_leds(&f, &d, [0, 1], &mut leds);
        let pad = PAD_BASE + 3;
        assert_eq!(
            &leds.0[pad..pad + 3],
            &[0, LIT, 0],
            "red lands in the second byte"
        );
        assert_eq!(&leds.0[PAD_BASE..PAD_BASE + 3], &[0; 3], "page 0 is hidden");
        assert_eq!(leds.0[STOP_LEDS[1]], LIT);
        assert_eq!(leds.0[STOP_LEDS[1] + 1], LIT);
        assert_eq!(leds.0[STOP_LEDS[0]], 0, "empty slot");
        let [tens, units] = super::super::loop_display::number_digits(2);
        assert_eq!(tens, 0);
        let lit: Vec<usize> = (0..8).filter(|bit| units & (1 << bit) != 0).collect();
        assert!(
            lit.iter().all(|&bit| leds.0[DISPLAY + bit] == LIT),
            "page 2 on the right digit"
        );
        assert!(leds.0[DISPLAY + 8..DISPLAY + 16].iter().all(|&b| b == 0));
    }

    #[test]
    fn leds_fit_the_report() {
        assert!(PAD_BASE + 3 * SLOTS * ROWS <= STOP_LEDS[3]);
        assert!(STOP_LEDS[0] + 1 < LED_LEN);
        crate::s4mk3::tests::check_layout(&LAYOUT);
    }
}
