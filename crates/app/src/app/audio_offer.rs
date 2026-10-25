use backspn_control::profile::Audio;

use crate::mixer_cards::MixerCard;
use eframe::egui::{self, Align2, Context, RichText, Vec2};

use crate::i18n::tr;
use crate::prefs::Monitor;
use crate::theme::WindowExt;
use crate::{App, Settings, theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Nothing,
    Ask,
    Activate,
}

fn decide(device: &str, settings: &Settings, devices: &[String]) -> Decision {
    if !devices.iter().any(|d| d == device) || settings.device.as_deref() == Some(device) {
        return Decision::Nothing;
    }
    match settings.audio_switch.get(device) {
        Some(true) => Decision::Activate,
        Some(false) => Decision::Nothing,
        None => Decision::Ask,
    }
}

fn activate(settings: &mut Settings, offer: &Offer) {
    match offer {
        Offer::Controller(audio) => {
            let audio = audio.clone();
            crate::mixer_cards::switch_device(settings, &audio.device.clone(), move |s| {
                s.buffer_frames = audio.buffer_frames.or(s.buffer_frames);
                s.monitor = Monitor::Channels(audio.monitor.0, audio.monitor.1);
                s.external_mixer.on = false;
            });
        }
        Offer::Mixer(card, device) => {
            crate::mixer_cards::switch_device(settings, device, |s| {
                crate::mixer_cards::apply_card(s, card);
            });
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Offer {
    Controller(Audio),
    Mixer(&'static MixerCard, String),
}

impl Offer {
    fn device(&self) -> &str {
        match self {
            Offer::Controller(audio) => &audio.device,
            Offer::Mixer(_, device) => device,
        }
    }

    fn name(&self) -> &str {
        match self {
            Offer::Controller(audio) => &audio.device,
            Offer::Mixer(card, _) => card.name,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AudioOffer {
    asking: Option<Offer>,
    dont_ask: bool,
    card_checked: Option<std::time::Instant>,
    mixers_checked: Option<std::time::Instant>,
    mixers_there: Vec<String>,
}

fn new_mixer_cards(devices: &[String], before: &[String]) -> Vec<Offer> {
    devices
        .iter()
        .filter(|d| !before.contains(d))
        .filter_map(|d| crate::mixer_cards::card_named(d).map(|c| Offer::Mixer(c, d.clone())))
        .collect()
}

const CARD_CHECK: std::time::Duration = std::time::Duration::from_secs(2);

impl App {
    pub(crate) fn offer_audio(&mut self, ctx: &Context) {
        if let Some(controls) = &mut self.controls
            && controls.native_card_pending()
            && self
                .audio_offer
                .card_checked
                .is_none_or(|t| t.elapsed() >= CARD_CHECK)
        {
            self.audio_offer.card_checked = Some(std::time::Instant::now());
            controls.offer_native_card(&backspn_engine::output_devices());
            ctx.request_repaint_after(CARD_CHECK);
        }
        if self.wizard.is_none()
            && let Some(audio) = self
                .controls
                .as_mut()
                .and_then(crate::controls::Controls::take_audio_offer)
        {
            self.consider(Offer::Controller(audio), &backspn_engine::output_devices());
        }
        if self.wizard.is_none()
            && self.audio_offer.asking.is_none()
            && self
                .audio_offer
                .mixers_checked
                .is_none_or(|t| t.elapsed() >= CARD_CHECK)
        {
            self.audio_offer.mixers_checked = Some(std::time::Instant::now());
            let devices = backspn_engine::output_devices();
            let offers = new_mixer_cards(&devices, &self.audio_offer.mixers_there);
            self.audio_offer.mixers_there.clone_from(&devices);
            for offer in offers {
                self.consider(offer, &devices);
            }
            ctx.request_repaint_after(CARD_CHECK);
        }
        let Some(audio) = self.audio_offer.asking.clone() else {
            return;
        };
        let t = theme::get(ctx);
        let (mut yes, mut no) = (false, false);
        let mut open = true;
        let mut dont_ask = self.audio_offer.dont_ask;
        egui::Window::new("Audio Device")
            .id(egui::Id::unique("backspn-audio-offer"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show_closable(ctx, &mut open, |ui| {
                ui.set_width(280.0);
                ui.label(format!("Switch audio to {}.", audio.name()));
                ui.add_space(6.0);
                crate::widgets::check(ui, &mut dont_ask, "Do not ask again.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    no = ui.button(tr("Cancel")).clicked();
                    yes = ui
                        .button(RichText::new(tr("Activate Device")).color(t.accent_ink))
                        .clicked();
                });
            });
        self.audio_offer.dont_ask = dont_ask;
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if yes || no || !open || escape {
            if dont_ask {
                self.settings
                    .audio_switch
                    .insert(audio.device().to_owned(), yes);
            }
            if yes {
                self.switch_audio_to(&audio);
            }
            self.audio_offer.asking = None;
            self.audio_offer.dont_ask = false;
        }
    }

    fn consider(&mut self, offer: Offer, devices: &[String]) {
        match decide(offer.device(), &self.settings, devices) {
            Decision::Activate => self.switch_audio_to(&offer),
            Decision::Ask if self.audio_offer.asking.is_none() => {
                self.audio_offer.asking = Some(offer);
                self.audio_offer.dont_ask = false;
            }
            Decision::Nothing | Decision::Ask => {}
        }
    }

    pub(crate) fn switch_audio(&mut self, audio: &Audio) {
        self.switch_audio_to(&Offer::Controller(audio.clone()));
    }

    fn switch_audio_to(&mut self, offer: &Offer) {
        activate(&mut self.settings, offer);
        self.change_audio();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_only_the_first_time_then_what_it_had() {
        let mut s = Settings {
            device: Some("Built-in".into()),
            buffer_frames: Some(256),
            ..Settings::default()
        };
        activate(&mut s, &Offer::Controller(flx4()));
        assert_eq!(s.buffer_frames, Some(512), "the first time, its defaults");
        s.buffer_frames = Some(128);
        s.inactive_devices
            .insert("Other".into(), crate::prefs::DeviceAudio::default());
        let other = Audio {
            device: "Other".into(),
            buffer_frames: None,
            ..flx4()
        };
        activate(&mut s, &Offer::Controller(other));
        activate(&mut s, &Offer::Controller(flx4()));
        assert_eq!(s.buffer_frames, Some(128));
        assert!(s.inactive_devices.contains_key("Other"));
    }

    #[test]
    fn a_mixer_card_is_offered_when_it_appears() {
        let devices = ["Built-in".to_owned(), "Xone:43C".to_owned()];
        let offers = new_mixer_cards(&devices, &[]);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].name(), "XONE 43C");
        assert!(
            new_mixer_cards(&devices, &devices).is_empty(),
            "already there"
        );
        let mut s = Settings::default();
        activate(&mut s, &offers[0]);
        assert_eq!(s.device.as_deref(), Some("Xone:43C"));
        assert!(s.external_mixer.on);
    }

    fn flx4() -> Audio {
        Audio {
            device: "DDJ-FLX4".into(),
            master: (0, 1),
            monitor: (2, 3),
            buffer_frames: Some(512),
        }
    }

    #[test]
    fn asks_once_unless_told_for_good() {
        let mut s = Settings::default();
        let there = ["DDJ-FLX4".to_owned()];
        assert_eq!(decide("DDJ-FLX4", &s, &[]), Decision::Nothing, "not there");
        assert_eq!(decide("DDJ-FLX4", &s, &there), Decision::Ask);
        s.audio_switch.insert("DDJ-FLX4".into(), true);
        assert_eq!(decide("DDJ-FLX4", &s, &there), Decision::Activate);
        s.audio_switch.insert("DDJ-FLX4".into(), false);
        assert_eq!(decide("DDJ-FLX4", &s, &there), Decision::Nothing);
        activate(&mut s, &Offer::Controller(flx4()));
        assert_eq!(s.device.as_deref(), Some("DDJ-FLX4"));
        assert_eq!(
            (s.buffer_frames, s.monitor),
            (Some(512), Monitor::Channels(2, 3))
        );
        s.audio_switch.clear();
        assert_eq!(decide("DDJ-FLX4", &s, &there), Decision::Nothing, "in use");
    }
}
