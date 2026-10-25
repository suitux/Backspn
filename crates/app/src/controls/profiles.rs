use backspn_control::midi;
use backspn_control::profile::{Audio, Profile};
use eframe::egui;

use super::Controls;

fn output_for(profile: &Profile, input: &str, outputs: &[String]) -> String {
    if outputs.iter().any(|o| o == input) {
        return input.to_owned();
    }
    profile
        .ports
        .iter()
        .find(|p| outputs.contains(p))
        .or(profile.ports.first())
        .cloned()
        .unwrap_or_else(|| input.to_owned())
}

pub(super) fn input_for(
    profile: &Profile,
    inputs: &[String],
    mapped: impl Fn(&str) -> bool,
) -> String {
    profile
        .ports
        .iter()
        .find(|p| inputs.contains(p) && !mapped(p))
        .or(profile.ports.first())
        .cloned()
        .unwrap_or_default()
}

fn unmapped_known(
    profiles: &[Profile],
    inputs: &[String],
    mapped: impl Fn(&str) -> bool,
) -> Vec<(String, Profile)> {
    inputs
        .iter()
        .filter(|p| !mapped(p))
        .filter_map(|p| {
            let profile = profiles.iter().find(|profile| profile.owns(p))?;
            Some((p.clone(), profile.clone()))
        })
        .collect()
}

pub(super) fn user_dir() -> std::path::PathBuf {
    super::mappings_path()
        .parent()
        .map(|d| d.join("Profiles"))
        .unwrap_or_default()
}

impl Controls {
    pub(super) fn add_profile(&mut self, profile: &Profile, input: &str, ctx: &egui::Context) {
        let output = output_for(profile, input, &midi::output_ports());
        self.ports.retain(|p| p.name != input);
        self.set.add_profile(profile, input, &output);
        if midi::input_ports().iter().any(|p| p == input) {
            self.connect(input, ctx);
        }
        if let Some(i) = self.leds.iter().position(|l| l.port.name == output) {
            self.leds.remove(i);
        }
        if midi::output_ports().contains(&output) {
            self.connect_output(&output);
        }
        self.message = Some(format!("Added {}", profile.name()));
        self.audio_offer.clone_from(&profile.audio);
        self.save();
    }

    pub(super) fn recognize(&mut self, inputs: &[String], ctx: &egui::Context) {
        for (input, profile) in unmapped_known(&self.profiles, inputs, |p| self.set.has_port(p)) {
            self.add_profile(&profile, &input, ctx);
        }
    }

    pub fn native_card_pending(&mut self) -> bool {
        let devices = &self.devices;
        self.cards_offered
            .retain(|name| devices.iter().any(|d| d.name() == *name && d.connected()));
        self.devices.iter().any(|d| {
            d.connected() && d.audio().is_some() && !self.cards_offered.contains(&d.name())
        })
    }

    pub fn offer_native_card(&mut self, cards: &[String]) {
        if self.audio_offer.is_some() {
            return;
        }
        for d in &self.devices {
            if !d.connected() || self.cards_offered.contains(&d.name()) {
                continue;
            }
            let Some(mut audio) = d.audio() else {
                continue;
            };
            if let Some(card) = backspn_control::profile::card_named(&audio.device, cards) {
                audio.device = card;
                self.cards_offered.push(d.name());
                self.audio_offer = Some(audio);
                return;
            }
        }
    }

    pub fn audio_offer(&self) -> Option<&Audio> {
        self.audio_offer.as_ref()
    }

    pub fn take_audio_offer(&mut self) -> Option<Audio> {
        self.audio_offer.take()
    }

    pub fn native_models(&self) -> Vec<(&'static str, bool)> {
        backspn_control::controller::models()
            .iter()
            .map(|m| (m.name, self.native_connected(m.name)))
            .collect()
    }

    pub fn midi_models(&self) -> Vec<(String, bool)> {
        let inputs = midi::input_ports();
        self.profiles
            .iter()
            .map(|p| (p.name(), inputs.iter().any(|i| p.owns(i))))
            .collect()
    }

    pub fn has_model(&self, name: &str) -> bool {
        self.set.devices.iter().any(|d| d.name == name)
    }

    pub fn use_model(&mut self, name: &str, ctx: &egui::Context) {
        if self.has_model(name) {
            return;
        }
        let Some(profile) = self.profiles.iter().find(|p| p.name() == name).cloned() else {
            return;
        };
        let input = input_for(&profile, &midi::input_ports(), |port| {
            self.set.has_port(port)
        });
        self.add_profile(&profile, &input, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use backspn_control::profile;

    fn flx4() -> Profile {
        profile::for_port("DDJ-FLX4").unwrap()
    }

    #[test]
    fn known_inputs_without_mappings_get_their_profile() {
        let inputs = vec!["DDJ-FLX4".to_owned(), "Other".to_owned()];
        let all = profile::all();
        let found = unmapped_known(&all, &inputs, |_| false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "DDJ-FLX4");
        assert_eq!(unmapped_known(&all, &inputs, |p| p == "DDJ-FLX4").len(), 0);
    }

    #[test]
    fn ports_pair_by_name() {
        let p = flx4();
        assert_eq!(output_for(&p, "DDJ-FLX4", &["DDJ-FLX4".into()]), "DDJ-FLX4");
        assert_eq!(output_for(&p, "DDJ-FLX4", &[]), "DDJ-FLX4", "opens later");
        assert_eq!(input_for(&p, &[], |_| false), "DDJ-FLX4");
    }
}
