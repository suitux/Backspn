use crate::prefs::{DeviceAudio, DeviceRouting, InputRouting, Monitor, Settings};

type Pair = Option<(u16, u16)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixerCard {
    pub name: &'static str,
    pub devices: &'static [&'static str],
    pub buffer_frames: u32,
    pub sample_rate: u32,
    pub decks: [Pair; 4],
    pub fx_return: Pair,
    pub monitor: Pair,
    pub inputs: [Pair; 4],
    pub send: Pair,
}

#[allow(clippy::unnecessary_wraps)]
const fn p(l: u16) -> Pair {
    Some((l, l + 1))
}

const fn four(
    name: &'static str,
    devices: &'static [&'static str],
    (buffer_frames, sample_rate): (u32, u32),
    [a, b, c, d]: [u16; 4],
) -> MixerCard {
    let decks = [p(a), p(b), p(c), p(d)];
    MixerCard {
        name,
        devices,
        buffer_frames,
        sample_rate,
        decks,
        fx_return: None,
        monitor: p(2),
        inputs: decks,
        send: None,
    }
}

const fn two(
    name: &'static str,
    devices: &'static [&'static str],
    (buffer_frames, sample_rate): (u32, u32),
) -> MixerCard {
    let decks = [p(0), p(2), None, None];
    MixerCard {
        name,
        devices,
        buffer_frames,
        sample_rate,
        decks,
        fx_return: None,
        monitor: p(2),
        inputs: decks,
        send: None,
    }
}

pub const CARDS: &[MixerCard] = &[
    four(
        "Pioneer DJM-900",
        &["DJM-900nexus"],
        (512, 44_100),
        [0, 6, 2, 4],
    ),
    four(
        "Pioneer DJM-900NXS2",
        &["DJM-900NXS2"],
        (512, 44_100),
        [2, 4, 0, 6],
    ),
    four(
        "Pioneer DJM-750MK2",
        &["DJM-750MK2"],
        (256, 48_000),
        [2, 4, 0, 6],
    ),
    two("Pioneer DJM-250MK2", &["DJM-250MK2"], (256, 48_000)),
    two("Pioneer DJM-450", &["DJM-450"], (256, 48_000)),
    four(
        "Pioneer DJM-TOUR1",
        &["DJM-TOUR1"],
        (256, 48_000),
        [2, 4, 0, 6],
    ),
    four("Pioneer DJM-750", &["DJM-750"], (512, 44_100), [2, 4, 0, 6]),
    four(
        "Pioneer DJM-850",
        &["PIONEER DJM-850"],
        (512, 44_100),
        [0, 6, 2, 4],
    ),
    MixerCard {
        name: "Pioneer DJM-T1",
        devices: &["PIONEER DJM-T1"],
        buffer_frames: 512,
        sample_rate: 48_000,
        decks: [p(0), p(2), p(4), p(4)],
        fx_return: None,
        monitor: None,
        inputs: [p(0), p(2), None, None],
        send: p(4),
    },
    four("Pioneer DJM-V10", &["DJM-V10"], (512, 48_000), [0, 2, 4, 6]),
    four("Pioneer DJM-A9", &["DJM-A9"], (512, 41_000), [0, 2, 4, 6]),
    MixerCard {
        inputs: [None; 4],
        monitor: None,
        ..four(
            "American Audio VMS4",
            &["VMS4 AUDIO MIDI Device"],
            (512, 44_100),
            [2, 4, 0, 6],
        )
    },
    two("Rane MP2014", &["Rane MP2014"], (512, 44_100)),
    MixerCard {
        send: p(8),
        ..four("Rane MP2015", &["Rane MP2015"], (512, 44_100), [0, 2, 4, 6])
    },
    four("XONE PX5", &["XONE:PX5"], (512, 44_100), [2, 4, 0, 6]),
    two("XONE 23C", &["Xone:23C"], (256, 48_000)),
    four("XONE 43C", &["Xone:43C"], (256, 48_000), [2, 4, 0, 6]),
    four(
        "XONE DB2",
        &["Allen&Heath Xone:DB2"],
        (256, 48_000),
        [2, 4, 0, 6],
    ),
    four(
        "XONE DB4",
        &["Allen&Heath Xone:DB4"],
        (256, 48_000),
        [2, 4, 0, 6],
    ),
    MixerCard {
        fx_return: p(4),
        send: p(4),
        ..two("Reloop FLUX", &["Reloop FLUX"], (512, 48_000))
    },
];

pub fn card_named(device: &str) -> Option<&'static MixerCard> {
    CARDS.iter().find(|c| c.devices.contains(&device))
}

fn current(s: &Settings) -> DeviceAudio {
    let x = &s.external_mixer;
    DeviceAudio {
        buffer_frames: s.buffer_frames,
        monitor: s.monitor,
        routing: Some(DeviceRouting {
            sample_rate: s.sample_rate,
            external: x.on,
            decks: x.decks,
            preview: x.preview,
            fx_return: x.fx_return,
            inputs: s.input_routing,
        }),
    }
}

fn restore(s: &mut Settings, kept: DeviceAudio) {
    s.buffer_frames = kept.buffer_frames;
    s.monitor = kept.monitor;
    if let Some(r) = kept.routing {
        s.sample_rate = r.sample_rate;
        let x = &mut s.external_mixer;
        x.on = r.external;
        x.decks = r.decks;
        x.preview = r.preview;
        x.fx_return = r.fx_return;
        s.input_routing = r.inputs;
    }
}

pub fn apply_card(s: &mut Settings, card: &MixerCard) {
    s.buffer_frames = Some(card.buffer_frames);
    s.sample_rate = Some(card.sample_rate);
    s.monitor = card
        .monitor
        .map_or(Monitor::None, |(l, r)| Monitor::Channels(l, r));
    let x = &mut s.external_mixer;
    x.on = true;
    x.decks = card.decks;
    x.preview = None;
    x.fx_return = card.fx_return;
    s.input_routing = InputRouting {
        decks: card.inputs,
        microphone: None,
        send: card.send,
    };
}

pub fn switch_device(s: &mut Settings, device: &str, first_time: impl FnOnce(&mut Settings)) {
    if s.device.as_deref() == Some(device) {
        return;
    }
    if let Some(old) = s.device.clone() {
        let left = current(s);
        s.inactive_devices.insert(old, left);
    }
    s.device = Some(device.to_owned());
    match s.inactive_devices.remove(device) {
        Some(kept) => restore(s, kept),
        None => first_time(s),
    }
}

fn apply_generic(s: &mut Settings) {
    s.buffer_frames = Some(512);
    s.sample_rate = Some(44_100);
    s.monitor = Monitor::Auto;
    let x = &mut s.external_mixer;
    x.on = false;
    x.decks = [p(0), None, None, None];
    x.preview = None;
    x.fx_return = None;
    s.input_routing = InputRouting {
        decks: [p(0), None, None, None],
        microphone: None,
        send: None,
    };
}

pub fn first_time(device: &str) -> impl FnOnce(&mut Settings) {
    let card = card_named(device);
    move |s| match card {
        Some(card) => apply_card(s, card),
        None => apply_generic(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_mixer_gets_its_channels_and_the_rest_unassigned() {
        let mut s = Settings::default();
        s.input_routing.microphone = Some((4, 5));
        s.external_mixer.preview = Some((8, 9));
        switch_device(&mut s, "Xone:43C", first_time("Xone:43C"));
        let x = &s.external_mixer;
        assert!(x.on);
        assert_eq!(
            x.decks,
            [Some((2, 3)), Some((4, 5)), Some((0, 1)), Some((6, 7))]
        );
        assert_eq!((x.preview, x.fx_return), (None, None));
        assert_eq!(s.input_routing.decks, x.decks);
        assert_eq!(s.input_routing.microphone, None);
        assert_eq!((s.buffer_frames, s.sample_rate), (Some(256), Some(48_000)));
        assert_eq!(s.monitor, Monitor::Channels(2, 3));
    }

    #[test]
    fn each_device_gets_back_what_it_had() {
        let mut s = Settings {
            device: Some("Built-in".into()),
            buffer_frames: Some(128),
            ..Settings::default()
        };
        switch_device(&mut s, "DJM-V10", first_time("DJM-V10"));
        s.external_mixer.preview = Some((8, 9));
        switch_device(&mut s, "Built-in", first_time("Built-in"));
        assert!(!s.external_mixer.on);
        assert_eq!(s.buffer_frames, Some(128));
        switch_device(&mut s, "DJM-V10", first_time("DJM-V10"));
        assert!(s.external_mixer.on);
        assert_eq!(s.external_mixer.preview, Some((8, 9)));
    }

    #[test]
    fn an_unknown_device_gets_the_generic_configuration_the_first_time() {
        let mut s = Settings::default();
        s.external_mixer.on = true;
        s.monitor = Monitor::Channels(2, 3);
        s.input_routing.microphone = Some((4, 5));
        switch_device(&mut s, "Some Card", first_time("Some Card"));
        assert!(!s.external_mixer.on);
        assert_eq!((s.buffer_frames, s.sample_rate), (Some(512), Some(44_100)));
        assert_eq!(s.monitor, Monitor::Auto);
        assert_eq!(s.external_mixer.decks, [Some((0, 1)), None, None, None]);
        assert_eq!(s.input_routing.decks, [Some((0, 1)), None, None, None]);
        assert_eq!(s.input_routing.microphone, None);
    }

    #[test]
    fn cards_are_matched_by_their_exact_system_name() {
        assert_eq!(card_named("Xone:23C").map(|c| c.name), Some("XONE 23C"));
        assert!(card_named("xone:23c").is_none());
        assert!(CARDS.iter().all(|c| c.decks.iter().any(Option::is_some)));
    }
}
