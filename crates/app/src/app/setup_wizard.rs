use eframe::egui::{self, Align2, Context, RichText, Vec2};

use crate::App;
use crate::i18n::tr;
use crate::search_combo::{self, Entry};
use crate::theme::{self, WindowExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Welcome,
    Device,
    Audio,
    Calibrate,
    Test,
    Music,
    Theme,
}

impl Step {
    const ALL: [Self; 7] = [
        Self::Welcome,
        Self::Device,
        Self::Audio,
        Self::Calibrate,
        Self::Test,
        Self::Music,
        Self::Theme,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome to Backspn",
            Self::Device => "Controller or mixer",
            Self::Audio => "Audio outputs",
            Self::Calibrate => "Calibrate the knobs",
            Self::Test => "Try the controls",
            Self::Music => "Your music",
            Self::Theme => "Look",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Choice {
    Nothing,
    Native(&'static str),
    Midi(String),
}

fn detected(native: &[(&'static str, bool)], midi: &[(String, bool)]) -> Choice {
    if let Some((name, _)) = native.iter().find(|(_, on)| *on) {
        return Choice::Native(name);
    }
    if let Some((name, _)) = midi.iter().find(|(_, on)| *on) {
        return Choice::Midi(name.clone());
    }
    Choice::Nothing
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Seen {
    source: String,
    control: String,
    value: f32,
    pressed: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ControlTest {
    seen: Vec<Seen>,
}

impl ControlTest {
    const SHOWN: usize = 8;

    fn record(&mut self, source: &str, event: &backspn_control::InputEvent) {
        if source == crate::controls::KEYBOARD {
            return;
        }
        let control = event.input.to_string();
        self.seen
            .retain(|s| !(s.source == source && s.control == control));
        self.seen.insert(
            0,
            Seen {
                source: source.to_owned(),
                control,
                value: event.value,
                pressed: event.pressed,
            },
        );
    }

    fn count(&self) -> usize {
        self.seen.len()
    }
}

fn add_new(
    chosen: &mut Vec<std::path::PathBuf>,
    existing: &[std::path::PathBuf],
    picked: Vec<std::path::PathBuf>,
) {
    for folder in picked {
        if !chosen.contains(&folder) && !existing.contains(&folder) {
            chosen.push(folder);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Wizard {
    step: usize,
    device: Option<Choice>,
    sound_cards: Option<Vec<String>>,
    test: ControlTest,
    music: Vec<std::path::PathBuf>,
}

impl Wizard {
    fn current(&self) -> Step {
        Step::ALL[self.step.min(Step::ALL.len() - 1)]
    }

    fn is_last(&self) -> bool {
        self.step + 1 >= Step::ALL.len()
    }

    fn next(&mut self, skip: impl Fn(Step) -> bool) -> bool {
        let Some(i) = (self.step + 1..Step::ALL.len()).find(|&i| !skip(Step::ALL[i])) else {
            return false;
        };
        self.step = i;
        true
    }

    fn back(&mut self, skip: impl Fn(Step) -> bool) {
        if let Some(i) = (0..self.step).rev().find(|&i| !skip(Step::ALL[i])) {
            self.step = i;
        }
    }

    pub(crate) fn testing(&self) -> bool {
        self.current() == Step::Test
    }

    pub(crate) fn record(&mut self, source: &str, event: &backspn_control::InputEvent) {
        self.test.record(source, event);
    }
}

enum Move {
    Back,
    Next,
    Close,
}

impl App {
    pub(crate) fn start_wizard(&mut self) {
        self.wizard = Some(Wizard::default());
    }

    pub(crate) fn setup_wizard(&mut self, ctx: &Context) {
        let Some(wizard) = self.wizard.clone() else {
            return;
        };
        let step = wizard.current();
        let t = theme::get(ctx);
        let mut open = true;
        let mut action = None;
        let mut device = None;
        let mut sound_cards = None;
        let mut music = wizard.music.clone();
        egui::Window::new(tr("Setup Assistant"))
            .id(egui::Id::unique("backspn-setup-wizard"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show_closable(ctx, &mut open, |ui| {
                ui.set_width(460.0);
                ui.heading(tr(step.title()));
                ui.add_space(6.0);
                match step {
                    Step::Welcome => welcome(ui),
                    Step::Device => device = self.device_step(ui, wizard.device.clone()),
                    Step::Audio => {
                        let cards = wizard
                            .sound_cards
                            .clone()
                            .unwrap_or_else(backspn_engine::output_devices);
                        self.audio_step(ui, &cards);
                        sound_cards = Some(cards);
                    }
                    Step::Calibrate => self.calibrate_step(ui),
                    Step::Test => test_step(ui, &wizard.test),
                    Step::Music => self.music_step(ui, &mut music),
                    Step::Theme => self.theme_step(ui),
                }
                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if wizard.step > 0 && ui.button(tr("Back")).clicked() {
                        action = Some(Move::Back);
                    }
                    if ui
                        .button(tr("Skip setup"))
                        .on_hover_text(tr("Closes the assistant; it is in the Backspn menu"))
                        .clicked()
                    {
                        action = Some(Move::Close);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if wizard.is_last() { "Finish" } else { "Next" };
                        if ui
                            .button(RichText::new(tr(label)).color(t.accent_ink))
                            .clicked()
                        {
                            action = Some(Move::Next);
                        }
                    });
                });
            });
        if !open {
            action = Some(Move::Close);
        }
        if let Some(wizard) = &mut self.wizard {
            if device.is_some() {
                wizard.device.clone_from(&device);
            }
            if wizard.sound_cards.is_none() {
                wizard.sound_cards = sound_cards;
            }
            wizard.music = music;
        }
        if let Some(action) = &action {
            self.leave_step(step, action, ctx);
        }
        let can_calibrate = self
            .controls
            .as_ref()
            .is_some_and(crate::controls::Controls::can_calibrate);
        let skip = |s: Step| s == Step::Calibrate && !can_calibrate;
        let Some(wizard) = &mut self.wizard else {
            return;
        };
        match action {
            Some(Move::Back) => wizard.back(skip),
            Some(Move::Next) => {
                if !wizard.next(skip) {
                    self.wizard = None;
                }
            }
            Some(Move::Close) => self.wizard = None,
            None => {}
        }
    }

    fn leave_step(&mut self, step: Step, action: &Move, ctx: &Context) {
        if matches!(action, Move::Next)
            && step == Step::Device
            && let Some(Choice::Midi(name)) = self.wizard_device()
            && let Some(controls) = &mut self.controls
        {
            controls.use_model(&name, ctx);
        }
        let Some(controls) = &mut self.controls else {
            return;
        };
        match step {
            Step::Audio if matches!(action, Move::Next | Move::Close) => {
                controls.take_audio_offer();
            }
            Step::Calibrate => controls.cancel_calibration(),
            _ => {}
        }
        if matches!(action, Move::Next) && step == Step::Music {
            self.add_music_folders();
        }
    }

    fn add_music_folders(&mut self) {
        let chosen = self
            .wizard
            .as_mut()
            .map(|w| std::mem::take(&mut w.music))
            .unwrap_or_default();
        let mut added = false;
        for folder in chosen {
            if let Some(crate::browser::BrowserAction::FoldersChanged(list)) =
                self.browser.add_folder(folder)
            {
                self.settings.music_folders = list;
                added = true;
            }
        }
        if added {
            self.settings.show_browser = true;
            self.browser.set_tag_writing(
                self.settings.tags_serialization == 2,
                backspn_formats::nml::KeyNotation::from_value(self.settings.key_notation_exported),
            );
            self.browser.import_music_folders();
            self.count(|u| u.imports.add("folders"));
        }
    }

    fn wizard_device(&self) -> Option<Choice> {
        let picked = self.wizard.as_ref()?.device.clone();
        let controls = self.controls.as_ref()?;
        Some(picked.unwrap_or_else(|| detected(&controls.native_models(), &controls.midi_models())))
    }

    fn device_step(&mut self, ui: &mut egui::Ui, picked: Option<Choice>) -> Option<Choice> {
        let controls = self.controls.as_ref()?;
        let native = controls.native_models();
        let midi = controls.midi_models();
        let chosen = picked.unwrap_or_else(|| detected(&native, &midi));
        let plugged = tr("(plugged in)");
        let label = |name: &str, on: bool| {
            if on {
                format!("{name} {plugged}")
            } else {
                name.to_owned()
            }
        };
        let mut entries = vec![Entry::item(
            Choice::Nothing,
            tr("No controller (mouse and keyboard)"),
            chosen == Choice::Nothing,
        )];
        entries.push(Entry::Separator);
        entries.push(Entry::Heading(tr("Kontrol (USB)").to_owned()));
        for &(name, on) in &native {
            let choice = Choice::Native(name);
            let selected = chosen == choice;
            entries.push(Entry::item(choice, label(name, on), selected));
        }
        entries.push(Entry::Separator);
        entries.push(Entry::Heading(tr("MIDI").to_owned()));
        for (name, on) in &midi {
            let choice = Choice::Midi(name.clone());
            let selected = chosen == choice;
            entries.push(Entry::item(choice, label(name, *on), selected));
        }
        let title = match &chosen {
            Choice::Nothing => tr("No controller (mouse and keyboard)").to_owned(),
            Choice::Native(name) => (*name).to_owned(),
            Choice::Midi(name) => name.clone(),
        };
        let mut pick = None;
        ui.label(tr("What will you play with?"));
        ui.horizontal(|ui| {
            pick = search_combo::show(ui, "wizard-device", title, 320.0, &entries).1;
        });
        let note = match &chosen {
            Choice::Nothing => tr(
                "Backspn works with the mouse and keyboard. A controller can be added later in Preferences ▸ Controller Manager.",
            ),
            Choice::Native(name) if native.iter().any(|(n, on)| n == name && *on) => {
                tr("Plugged in and set up with its mappings.")
            }
            Choice::Native(_) => tr("Plug it in by USB: it sets itself up with its mappings."),
            Choice::Midi(name) if controls.has_model(name) => {
                tr("Set up with its mappings; it works once plugged in.")
            }
            Choice::Midi(_) => {
                tr("Its mappings are loaded when you click Next; it works once plugged in.")
            }
        };
        ui.label(RichText::new(note).weak());
        pick
    }
}

impl App {
    fn audio_step(&mut self, ui: &mut egui::Ui, cards: &[String]) {
        let offer = self
            .controls
            .as_ref()
            .and_then(crate::controls::Controls::audio_offer)
            .filter(|o| cards.contains(&o.device))
            .filter(|o| self.settings.device.as_deref() != Some(&o.device))
            .cloned();
        if let Some(offer) = offer {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} {}",
                    tr("Your controller has its own sound card:"),
                    offer.device
                ));
                if ui.button(tr("Use it")).clicked() {
                    if let Some(controls) = &mut self.controls {
                        controls.take_audio_offer();
                    }
                    self.switch_audio(&offer);
                }
            });
            ui.add_space(6.0);
        }
        let info = self.engine.as_ref().ok().map(|e| e.info().clone());
        let mut restart = false;
        egui::Grid::new("wizard-audio")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(tr("Sound card"));
                restart |= crate::prefs::device_combo(ui, &mut self.settings, cards);
                ui.end_row();
                ui.label(tr("Master"));
                ui.label(tr("Outputs 1/2"));
                ui.end_row();
                ui.label(tr("Headphones"));
                let channels = info.as_ref().map_or(2, |i| i.channels);
                restart |= crate::prefs::monitor_combo(ui, &mut self.settings, channels);
                ui.end_row();
            });
        let now = match info.as_ref().map(|i| i.monitor) {
            None => tr("No sound card is open.").to_owned(),
            Some(Some((l, r))) => {
                format!("{} {}/{}.", tr("Now: headphones on outputs"), l + 1, r + 1)
            }
            Some(None) => tr("Now: no headphone output; the cue goes nowhere.").to_owned(),
        };
        ui.label(RichText::new(now).weak());
        ui.label(
            RichText::new(tr(
                "More in Preferences ▸ Audio Setup and Output Routing (buffer, sample rate, mono).",
            ))
            .weak(),
        );
        if restart {
            self.change_audio();
        }
    }
}

impl App {
    fn music_step(&mut self, ui: &mut egui::Ui, chosen: &mut Vec<std::path::PathBuf>) {
        ui.label(tr(
            "Choose the folders with your music: they show under FOLDERS and their tracks are imported into the collection when you click Next.",
        ));
        ui.add_space(6.0);
        for folder in &self.settings.music_folders {
            ui.label(RichText::new(format!("📁 {}", folder.display())).monospace());
        }
        let mut remove = None;
        for (i, folder) in chosen.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("📁 {}", folder.display())).monospace());
                if ui.small_button(tr("Remove")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            chosen.remove(i);
        }
        if self.settings.music_folders.is_empty() && chosen.is_empty() {
            ui.label(RichText::new(tr("No folder yet.")).weak());
        }
        ui.add_space(4.0);
        if ui.button(tr("Add folders…")).clicked()
            && let Some(folders) = rfd::FileDialog::new()
                .set_directory(crate::dirs::user_music())
                .pick_folders()
        {
            add_new(chosen, &self.settings.music_folders, folders);
        }
        ui.add_space(8.0);
        crate::prefs::tags_serialization(ui, &mut self.settings);
        ui.label(
            RichText::new(tr(
                "Writing tags puts edits, ratings and the analysis (BPM, key) into the music files themselves.",
            ))
            .weak(),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(tr(
                "More folders, or none, in Preferences ▸ File Management. Skipping this step imports nothing.",
            ))
            .weak(),
        );
    }

    fn calibrate_step(&mut self, ui: &mut egui::Ui) {
        ui.label(tr("Its firmware calibrates the knobs, a group at a time."));
        ui.add_space(6.0);
        if let Some(controls) = &mut self.controls {
            controls.calibration_section(ui, None);
        }
    }

    fn theme_step(&mut self, ui: &mut egui::Ui) {
        ui.label(tr("Choose the colours of the app."));
        ui.add_space(6.0);
        let changed = ui
            .horizontal(|ui| {
                ui.label(tr("Theme"));
                crate::prefs::theme_combo(ui, &mut self.settings)
            })
            .inner;
        if changed {
            theme::apply(ui.ctx(), self.settings.theme.theme());
        }
        ui.label(
            RichText::new(tr(
                "Also in the View menu and Preferences ▸ Global Settings.",
            ))
            .weak(),
        );
        ui.add_space(12.0);
        self.layout_step(ui);
    }

    fn layout_step(&mut self, ui: &mut egui::Ui) {
        ui.label(tr("Choose the decks on screen."));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(tr("Decks:"));
            crate::widgets::radio(ui, &mut self.settings.deck_count, 2, "2 (A, B)");
            crate::widgets::radio(ui, &mut self.settings.deck_count, 4, "4 (A–D)");
        });
        let mut picked = [None; backspn_engine::DECKS];
        egui::Grid::new("wizard-decks")
            .num_columns(3)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.label("");
                ui.label(tr("Deck size:"));
                ui.label(tr("Deck type:"));
                ui.end_row();
                for d in 0..self.settings.deck_count {
                    ui.label(char::from(b'A' + d as u8).to_string());
                    let mut size = self.decks[d].size();
                    crate::prefs::deck_size_combo(ui, d, &mut size);
                    if size != self.decks[d].size() {
                        self.decks[d].set_size(size);
                        self.settings.deck_sizes[d] = size;
                    }
                    picked[d] = crate::prefs::deck_type_combo(ui, d, &self.settings);
                    ui.end_row();
                }
            });
        self.apply_deck_types(picked);
        ui.label(RichText::new(tr("Also in Preferences ▸ Decks Layout.")).weak());
    }
}

fn test_step(ui: &mut egui::Ui, test: &ControlTest) {
    ui.label(tr(
        "Move faders and knobs and press buttons: each control shows here. Nothing happens in the app meanwhile.",
    ));
    ui.add_space(6.0);
    if test.seen.is_empty() {
        ui.label(RichText::new(tr("Waiting for a control…")).weak());
    }
    egui::Grid::new("wizard-test")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for s in test.seen.iter().take(ControlTest::SHOWN) {
                ui.label(RichText::new(&s.control).monospace());
                match s.pressed {
                    Some(true) => ui.label(tr("pressed")),
                    Some(false) => ui.label(tr("released")),
                    None => ui.add(
                        egui::ProgressBar::new(s.value.clamp(0.0, 1.0))
                            .desired_width(160.0)
                            .text(format!("{:.0} %", s.value * 100.0)),
                    ),
                };
                ui.end_row();
            }
        });
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "{} {}",
            test.count(),
            tr("controls tried. One that never shows needs mapping in Preferences ▸ Controller Manager.")
        ))
        .weak(),
    );
}

fn welcome(ui: &mut egui::Ui) {
    ui.label(tr(
        "This assistant sets up your controller or mixer. Every step can be skipped and changed later in Preferences.",
    ));
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr(
            "Run it again any time from the Backspn menu or Preferences ▸ Global Settings.",
        ))
        .weak(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goes_through_the_steps_and_ends_after_the_last() {
        let all = |_| false;
        let mut w = Wizard::default();
        assert_eq!(w.current(), Step::Welcome);
        w.back(all);
        assert_eq!(w.current(), Step::Welcome, "nothing before the first");
        assert!(w.next(all));
        assert_eq!(w.current(), Step::Device);
        assert!(w.next(all));
        assert_eq!(w.current(), Step::Audio);
        assert!(w.next(all));
        assert_eq!(w.current(), Step::Calibrate);
        assert!(w.next(all));
        assert_eq!(w.current(), Step::Test);
        assert!(w.testing());
        assert!(w.next(all));
        assert_eq!(w.current(), Step::Music);
        assert!(w.next(all));
        assert_eq!(w.current(), Step::Theme);
        assert!(w.is_last());
        assert!(!w.next(all), "the last one ends the assistant");
        w.back(all);
        assert_eq!(w.current(), Step::Music);
    }

    #[test]
    fn the_calibration_page_is_left_out_without_a_controller_for_it() {
        let skip = |s| s == Step::Calibrate;
        let mut w = Wizard::default();
        w.next(skip);
        w.next(skip);
        assert_eq!(w.current(), Step::Audio);
        w.next(skip);
        assert_eq!(w.current(), Step::Test);
        w.back(skip);
        assert_eq!(w.current(), Step::Audio);
    }

    fn event(input: &str, value: f32, pressed: Option<bool>) -> backspn_control::InputEvent {
        backspn_control::InputEvent {
            input: backspn_control::Input::Hid(input.to_owned()),
            value,
            raw: 0,
            pressed,
        }
    }

    #[test]
    fn folders_are_added_once() {
        use std::path::PathBuf;
        let existing = [PathBuf::from("/m/a")];
        let mut chosen = vec![PathBuf::from("/m/b")];
        add_new(
            &mut chosen,
            &existing,
            vec!["/m/a".into(), "/m/b".into(), "/m/c".into()],
        );
        assert_eq!(chosen, [PathBuf::from("/m/b"), PathBuf::from("/m/c")]);
    }

    #[test]
    fn each_control_shows_once_the_latest_first() {
        let mut t = ControlTest::default();
        t.record("s4", &event("Left.Play", 1.0, Some(true)));
        t.record("s4", &event("Mixer.Crossfader", 0.3, None));
        t.record("s4", &event("Left.Play", 0.0, Some(false)));
        assert_eq!(t.count(), 2);
        assert_eq!(t.seen[0].control, "HID Left.Play");
        assert_eq!(t.seen[0].pressed, Some(false));
        t.record(crate::controls::KEYBOARD, &event("Space", 1.0, Some(true)));
        assert_eq!(t.count(), 2, "keys are left out");
    }

    #[test]
    fn the_list_starts_on_the_device_plugged_in() {
        let native = [("Kontrol S4 MK3", false), ("Kontrol Z1", true)];
        let midi = [("Pioneer DJ DDJ-SB2".to_owned(), true)];
        assert_eq!(detected(&native, &midi), Choice::Native("Kontrol Z1"));
        let native = [("Kontrol S4 MK3", false)];
        assert_eq!(
            detected(&native, &midi),
            Choice::Midi("Pioneer DJ DDJ-SB2".to_owned()),
            "a MIDI model when no native one is there"
        );
        let midi = [("Pioneer DJ DDJ-SB2".to_owned(), false)];
        assert_eq!(detected(&native, &midi), Choice::Nothing);
    }
}
