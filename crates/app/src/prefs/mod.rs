use std::time::Duration;

use backspn_engine::{DECKS, EffectKind, StreamInfo, output_devices};
use eframe::egui::{self, ComboBox, RichText, ScrollArea, Ui};

use crate::controls::Controls;
use crate::i18n::tr;
use crate::kits::KitLibrary;
use crate::status::DeckType;
use crate::theme::WindowExt;

mod effects;
mod kits;
mod layouts;
mod pages;
mod screens;
mod search;
mod settings;

use pages::{
    analyze_options, auto_fade, auto_gain, channel_filter, crossfader_curve, eq_model,
    external_mixing_mode, external_sync, file_management_page, global_settings, headroom,
    input_routing, limiter, loading, loop_recorder, mix_recorder, mixer_fx_slots, output_routing,
    sample_decks, transport, waveform_settings,
};
pub use pages::{device_combo, monitor_combo, tags_serialization, theme_combo};
pub(crate) use settings::settings_path;
pub use settings::{
    DeviceAudio, DeviceRouting, InputRouting, Monitor, RecordingSource, Settings, keep_backup,
    stem_model_path, ui_state_path,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    AudioSetup,
    OutputRouting,
    InputRouting,
    ExternalSync,
    TimecodeSetup,
    Transport,
    DecksLayout,
    TrackDecks,
    Loading,
    SampleDecks,
    Mixer,
    Effects,
    DrumMachine,
    GlobalSettings,
    MixRecorder,
    LoopRecorder,
    Streaming,
    Broadcasting,
    BrowserDetails,
    FileManagement,
    AnalyzeOptions,
    ControllerManager,
    KontrolS4Mk3,
}

impl Page {
    const ALL: [Self; 23] = [
        Self::AudioSetup,
        Self::OutputRouting,
        Self::InputRouting,
        Self::ExternalSync,
        Self::TimecodeSetup,
        Self::Transport,
        Self::DecksLayout,
        Self::TrackDecks,
        Self::Loading,
        Self::SampleDecks,
        Self::Mixer,
        Self::Effects,
        Self::DrumMachine,
        Self::GlobalSettings,
        Self::MixRecorder,
        Self::LoopRecorder,
        Self::Streaming,
        Self::Broadcasting,
        Self::BrowserDetails,
        Self::FileManagement,
        Self::AnalyzeOptions,
        Self::ControllerManager,
        Self::KontrolS4Mk3,
    ];

    fn shown(s4_page: bool) -> impl Iterator<Item = Self> {
        Self::ALL
            .into_iter()
            .filter(move |p| s4_page || *p != Self::KontrolS4Mk3)
    }

    fn title(self) -> &'static str {
        tr(match self {
            Self::AudioSetup => "Audio Setup",
            Self::OutputRouting => "Output Routing",
            Self::InputRouting => "Input Routing",
            Self::ExternalSync => "External Sync",
            Self::TimecodeSetup => "Timecode Setup",
            Self::Transport => "Transport",
            Self::DecksLayout => "Decks Layout",
            Self::TrackDecks => "Track Decks",
            Self::Loading => "Loading",
            Self::SampleDecks => "Sample Decks",
            Self::Mixer => "Mixer",
            Self::Effects => "Effects",
            Self::DrumMachine => "Drum Machine",
            Self::GlobalSettings => "Global Settings",
            Self::MixRecorder => "Mix Recorder",
            Self::LoopRecorder => "Loop Recorder",
            Self::Streaming => "Streaming",
            Self::Broadcasting => "Broadcasting",
            Self::BrowserDetails => "Browser Details",
            Self::FileManagement => "File Management",
            Self::AnalyzeOptions => "Analyze Options",
            Self::ControllerManager => "Controller Manager",
            Self::KontrolS4Mk3 => "Kontrol S4 MK3",
        })
    }

    fn pending(self) -> Option<&'static str> {
        Some(match self {
            Self::TimecodeSetup => "Timecode control (DVS) is not available yet.",
            Self::Streaming => "Streaming services are not available yet.",
            Self::Broadcasting => "Broadcasting is not available yet.",
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changes {
    pub settings: bool,
    pub restart_audio: bool,
    pub deck_types: [Option<DeckType>; DECKS],
    pub load_layout: Option<usize>,
    pub global: Option<Global>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Global {
    SetupWizard,
    ReportProblem,
    ShowLogFile,
    ResetSettings,
}

pub struct Prefs {
    pub open: bool,
    page: Page,
    devices: Vec<String>,
    fx_marks: (Option<EffectKind>, Option<usize>),
    search: search::Search,
    search_box: egui::Id,
    s4_page: bool,
    screen_previews: screens::Previews,
    kit: kits::KitEditor,
    layouts: layouts::LayoutEditor,
    pub deck_setup: crate::layouts::DeckSetup,
    pub preview: Option<backspn_engine::Track>,
    pub output_latency: Duration,
    buffer_drag: Option<u32>,
}

impl Prefs {
    pub fn new() -> Self {
        Self {
            open: false,
            page: Page::AudioSetup,
            devices: Vec::new(),
            fx_marks: (None, None),
            search: search::Search::default(),
            search_box: egui::Id::NULL,
            s4_page: false,
            screen_previews: screens::Previews::default(),
            kit: kits::KitEditor::default(),
            layouts: layouts::LayoutEditor::default(),
            deck_setup: crate::layouts::DeckSetup::default(),
            preview: None,
            output_latency: Duration::ZERO,
            buffer_drag: None,
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            self.devices = output_devices();
            self.search.index = None;
        }
    }

    pub fn open_layouts(&mut self) {
        if !self.open {
            self.toggle();
        }
        self.page = Page::DecksLayout;
        self.layouts.scroll_to = true;
    }

    pub fn window(
        &mut self,
        ctx: &egui::Context,
        settings: &mut Settings,
        audio: Option<&StreamInfo>,
        controls: &mut Controls,
        kits: &mut KitLibrary,
    ) -> Changes {
        let mut changes = Changes::default();
        let mut open = self.open;
        let s4_page = controls.selected_native() == Some(backspn_control::s4mk3::NAME);
        if s4_page != self.s4_page {
            self.s4_page = s4_page;
            self.search.index = None;
            if !s4_page && self.page == Page::KontrolS4Mk3 {
                self.page = Page::ControllerManager;
            }
        }
        egui::Window::new("Preferences")
            .default_width(820.0)
            .default_height(560.0)
            .collapsible(false)
            .show_closable(ctx, &mut open, |ui| {
                if !self.search.query.trim().is_empty() && self.search.index.is_none() {
                    self.search.index = Some(self.index(ui, ctx, settings, audio, controls, kits));
                }
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(170.0);
                        self.search_box = ui
                            .add(
                                egui::TextEdit::singleline(&mut self.search.query)
                                    .id_salt("prefs-search")
                                    .hint_text("🔍 Search settings")
                                    .desired_width(f32::INFINITY),
                            )
                            .id;
                        ui.add_space(4.0);
                        ScrollArea::vertical().id_salt("pages").show(ui, |ui| {
                            if self.search.query.trim().is_empty() {
                                self.page_list(ui);
                            } else {
                                self.results(ui);
                            }
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.heading(self.page.title());
                        ui.add_space(6.0);
                        self.search.begin(ui);
                        ScrollArea::vertical()
                            .id_salt("page")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                changes = self
                                    .page_ui(self.page, ui, ctx, settings, audio, controls, kits);
                            });
                    });
                });
            });
        if open != self.open {
            self.open = open;
        }
        changes
    }

    fn page_list(&mut self, ui: &mut Ui) {
        for page in Page::shown(self.s4_page) {
            let text = if page.pending().is_some() {
                RichText::new(page.title()).weak()
            } else {
                RichText::new(page.title())
            };
            ui.selectable_value(&mut self.page, page, text);
        }
    }

    fn results(&mut self, ui: &mut Ui) {
        let results = self.search.results(self.s4_page);
        if results.is_empty() {
            ui.label(RichText::new(tr("No setting found.")).weak());
        }
        for (page, title) in results {
            let text = if title == page.title() {
                RichText::new(&title).strong()
            } else {
                RichText::new(format!("{title}\n{}", page.title()))
            };
            if ui.selectable_label(self.page == page, text).clicked() {
                self.page = page;
                search::Search::target(ui, title);
            }
        }
    }

    fn index(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        settings: &Settings,
        audio: Option<&StreamInfo>,
        controls: &mut Controls,
        kits: &mut KitLibrary,
    ) -> Vec<(Page, String)> {
        let mut hidden = ui.new_child(egui::UiBuilder::new().id_salt("prefs-index").invisible());
        let mut index = Vec::new();
        for page in Page::shown(self.s4_page) {
            if page.pending().is_some() || page == Page::ControllerManager {
                continue;
            }
            let mut copy = settings.clone();
            let titles = search::Search::collect(&mut hidden, |ui| {
                ui.push_id(page.title(), |ui| {
                    self.page_ui(page, ui, ctx, &mut copy, audio, controls, kits);
                });
            });
            index.extend(titles.into_iter().map(|t| (page, t)));
        }
        index
    }

    #[allow(clippy::too_many_arguments)]
    fn page_ui(
        &mut self,
        page: Page,
        ui: &mut Ui,
        ctx: &egui::Context,
        s: &mut Settings,
        audio: Option<&StreamInfo>,
        controls: &mut Controls,
        kits: &mut KitLibrary,
    ) -> Changes {
        if let Some(reason) = page.pending() {
            ui.label(RichText::new(tr(reason)).weak());
            return Changes::default();
        }
        let before = s.clone();
        let mut restart = false;
        let mut deck_types = [None; DECKS];
        let mut load_layout = None;
        let mut global = None;
        match page {
            Page::AudioSetup => restart = self.audio_setup(ui, s, audio),
            Page::OutputRouting => restart = output_routing(ui, s, audio),
            Page::InputRouting => input_routing(ui, s, audio),
            Page::Transport => transport(
                ui,
                s,
                controls.native_connected(backspn_control::s4mk3::NAME),
            ),
            Page::DecksLayout => {
                ui.horizontal(|ui| {
                    search::title(ui, "Decks:");
                    search::radio(ui, &mut s.deck_count, 2, "2 (A, B)");
                    search::radio(ui, &mut s.deck_count, 4, "4 (A–D)");
                });
                deck_sizes(ui, s);
                deck_types = deck_type_row(ui, s);
                search::check(ui, &mut s.show_fx, "Show FX units");
                ui.horizontal(|ui| {
                    search::title(ui, "Global section left:");
                    search::radio(ui, &mut s.global_left_loop_recorder, false, "FX Unit 1");
                    search::radio(ui, &mut s.global_left_loop_recorder, true, "Loop Recorder");
                });
                search::check(ui, &mut s.show_browser, "Show browser");
                ui.add_space(10.0);
                waveform_settings(ui, s);
                ui.add_space(14.0);
                load_layout = self.layouts.ui(ui, s, &self.deck_setup);
            }
            Page::TrackDecks => {
                search::check(
                    ui,
                    &mut s.snap,
                    "Snap: cue, hotcue and loop points snap to beats",
                );
                search::check(ui, &mut s.quantize, "Quantize: jumps keep the beat phase");
                ui.add_space(4.0);
                ui.label(
                    RichText::new(tr("Also in the block between the FX units (SNAP, QUANT)."))
                        .weak(),
                );
            }
            Page::Loading => loading(ui, s),
            Page::SampleDecks => sample_decks(ui, s),
            Page::Mixer => {
                crossfader_curve(ui, s);
                ui.add_space(8.0);
                auto_fade(ui, s);
                ui.add_space(8.0);
                mixer_fx_slots(ui, s);
                ui.add_space(8.0);
                channel_filter(ui, s);
                ui.add_space(8.0);
                eq_model(ui, s);
                ui.add_space(8.0);
                auto_gain(ui, s);
                ui.add_space(8.0);
                headroom(ui, s);
                ui.add_space(8.0);
                limiter(ui, s);
                ui.add_space(8.0);
                search::title(ui, "Limiter: in the block between the FX units.");
                ui.add_space(8.0);
                external_mixing_mode(ui, s);
                ui.add_space(8.0);
                ui.label(tr(
                    "Headphone volume and cue mix: mappable to keyboard, MIDI or controllers.",
                ));
            }
            Page::Effects => self.effects(ui, s),
            Page::MixRecorder => mix_recorder(ui, s),
            Page::LoopRecorder => loop_recorder(ui, s),
            Page::ExternalSync => external_sync(ui, s),
            Page::DrumMachine => self.drum_machine(ui, kits),
            Page::GlobalSettings => global = global_settings(ui, s),
            Page::BrowserDetails => Self::browser_details(ui, s),
            Page::FileManagement => file_management_page(ui, s),
            Page::AnalyzeOptions => analyze_options(ui, s),
            Page::ControllerManager => controls.contents(ui, ctx),
            Page::KontrolS4Mk3 => kontrol_s4mk3(ui, s, controls, &mut self.screen_previews),
            _ => {}
        }
        Changes {
            settings: *s != before,
            restart_audio: restart,
            deck_types,
            load_layout,
            global,
        }
    }

    fn audio_setup(&mut self, ui: &mut Ui, s: &mut Settings, audio: Option<&StreamInfo>) -> bool {
        let mut restart = false;
        egui::Grid::new("audio")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                search::title(ui, "Audio Device");
                restart |= device_combo(ui, s, &self.devices);
                ui.end_row();
                let Some(info) = audio else {
                    return;
                };

                search::title(ui, "Sample Rate");
                ComboBox::from_id_salt("sample rate")
                    .selected_text(format!("{} Hz", info.sample_rate))
                    .show_ui(ui, |ui| {
                        for &rate in &info.sample_rates {
                            if ui
                                .selectable_label(rate == info.sample_rate, format!("{rate} Hz"))
                                .clicked()
                                && rate != info.sample_rate
                            {
                                s.sample_rate = Some(rate);
                                restart = true;
                            }
                        }
                    });
                ui.end_row();

                search::title(ui, "Buffer Size");
                ui.horizontal(|ui| restart |= self.buffer_slider(ui, s, info));
                ui.end_row();

                let buffer = info.buffer_frames.unwrap_or(0);
                let [processing, output, overall] =
                    latency_readout(buffer, self.output_latency, info.sample_rate);
                for (title, ms) in [
                    ("Processing", processing),
                    ("Output", output),
                    ("Overall", overall),
                ] {
                    search::title(ui, title);
                    ui.monospace(format!("{ms:5.1} ms"));
                    ui.end_row();
                }

                search::title(ui, "Outputs");
                ui.label(format!("{} channels on {}", info.channels, info.device));
                ui.end_row();
            });
        ui.label(
            RichText::new(crate::i18n::tr(
                "A new device, buffer or outputs keep the decks playing; a new sample rate \
                 unloads them.",
            ))
            .weak(),
        );
        restart
    }

    fn buffer_slider(&mut self, ui: &mut Ui, s: &mut Settings, info: &StreamInfo) -> bool {
        let current = info
            .buffer_frames
            .unwrap_or(backspn_engine::DEFAULT_BUFFER_FRAMES);
        let Some((lo, hi)) = info.buffer_range.and_then(buffer_steps) else {
            ui.label(format!("{current} samples"));
            return false;
        };
        let mut value = self.buffer_drag.unwrap_or(current).clamp(lo, hi);
        ui.spacing_mut().slider_width = 260.0;
        let r = ui.add(
            egui::Slider::new(&mut value, lo..=hi)
                .step_by(f64::from(BUFFER_STEP))
                .show_value(false),
        );
        ui.label(format!("{value} samples"));
        if r.changed() {
            self.buffer_drag = Some(value);
        }
        if r.dragged() || self.buffer_drag.is_none() {
            return false;
        }
        self.buffer_drag = None;
        if value == current {
            return false;
        }
        s.buffer_frames = Some(value);
        true
    }
}

const BUFFER_STEP: u32 = 64;

fn buffer_steps((min, max): (u32, u32)) -> Option<(u32, u32)> {
    let lo = min.max(1).div_ceil(BUFFER_STEP) * BUFFER_STEP;
    let hi = max / BUFFER_STEP * BUFFER_STEP;
    (lo <= hi).then_some((lo, hi))
}

fn latency_readout(buffer: u32, measured: Duration, rate: u32) -> [f64; 3] {
    let rate = f64::from(rate.max(1));
    let ms = |frames: f64| (frames * 1000.0 / rate * 10.0).round() / 10.0;
    let buffer = f64::from(buffer);
    let device = (measured.as_secs_f64() * rate).round() - buffer;
    let (processing, output) = (ms(buffer), ms(device.max(0.0)));
    [processing, output, processing + output]
}

fn kontrol_s4mk3(
    ui: &mut Ui,
    s: &mut Settings,
    controls: &mut Controls,
    previews: &mut screens::Previews,
) {
    controls.device_section(ui, backspn_control::s4mk3::NAME);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        search::title(ui, "Jog mode at start-up:");
        search::radio(ui, &mut s.jog_turntable, false, "Jog");
        search::radio(ui, &mut s.jog_turntable, true, "TT (turntable)");
    });
    ui.horizontal(|ui| {
        search::title(ui, "Tempo fader:");
        search::radio(ui, &mut s.tempo_fader_relative, true, "Relative");
        search::radio(ui, &mut s.tempo_fader_relative, false, "Absolute");
    })
    .response
    .on_hover_text(crate::i18n::tr(
        "Relative: moving the fader changes the tempo by that much, wherever it is. \
         Absolute: the fader position is the tempo. SHIFT + SYNC locks the tempo at 0 %.",
    ));
    ui.horizontal(|ui| {
        search::title(ui, "Turntable base speed:");
        search::radio(ui, &mut s.platter_45, false, "33.3 RPM");
        search::radio(ui, &mut s.platter_45, true, "45 RPM");
    })
    .response
    .on_hover_text(tr(
        "How fast the platter turns in TT mode while the deck plays",
    ));
    ui.label(
        RichText::new(crate::i18n::tr(
            "In TT mode the motor turns the platter and the track follows it: touch to \
             scratch, SHIFT + PLAY lets it run down. The tempo fader moves platter and track \
             ±8 %; SYNC does not apply.",
        ))
        .weak(),
    );
    ui.label(RichText::new(tr("Jog sensitivity: Transport page.")).weak());
    ui.add_space(6.0);
    search::heading(ui, "Calibration");
    controls.calibration_section(ui);
    ui.add_space(6.0);
    screens::screens(ui, s, previews);
}

fn deck_sizes(ui: &mut Ui, s: &mut Settings) {
    ui.horizontal(|ui| {
        search::title(ui, "Deck size:");
        for (d, size) in s.deck_sizes.iter_mut().enumerate() {
            ui.label(deck_letter(d));
            deck_size_combo(ui, d, size);
        }
    });
}

fn deck_type_row(ui: &mut Ui, s: &Settings) -> [Option<DeckType>; DECKS] {
    let mut picked = [None; DECKS];
    ui.horizontal(|ui| {
        search::title(ui, "Deck type:");
        for d in 0..DECKS {
            ui.label(deck_letter(d));
            picked[d] = deck_type_combo(ui, d, s);
        }
    });
    picked
}

fn deck_letter(deck: usize) -> String {
    char::from(b'A' + deck as u8).to_string()
}

pub(crate) fn deck_size_combo(ui: &mut Ui, deck: usize, size: &mut crate::deck_view::DeckSize) {
    use crate::deck_view::DeckSize;
    const SIZES: [(DeckSize, &str); 6] = [
        (DeckSize::Micro, "Micro"),
        (DeckSize::Essential, "Essential"),
        (DeckSize::Full, "Full"),
        (DeckSize::Advanced, "Advanced"),
        (DeckSize::ParallelSlim, "Parallel Slim"),
        (DeckSize::ParallelFull, "Parallel Full"),
    ];
    let name = SIZES.iter().find(|(s, _)| s == size).map_or("", |(_, n)| n);
    egui::ComboBox::from_id_salt(("deck size", deck))
        .width(110.0)
        .selected_text(name)
        .show_ui(ui, |ui| {
            for (value, label) in SIZES {
                ui.selectable_value(size, value, label);
            }
        });
}

pub(crate) fn deck_type_combo(ui: &mut Ui, deck: usize, s: &Settings) -> Option<DeckType> {
    let status = &s.status.decks[deck];
    let current = DeckType::of(status.deck_kind, status.live);
    let mut chosen = current;
    egui::ComboBox::from_id_salt(("deck type", deck))
        .width(110.0)
        .selected_text(tr(chosen.label()))
        .show_ui(ui, |ui| {
            for value in DeckType::ALL {
                ui.selectable_value(&mut chosen, value, tr(value.label()));
            }
        });
    (chosen != current).then_some(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_s4_page_is_in_the_tree_only_while_its_controller_is_chosen() {
        assert!(Page::shown(true).any(|p| p == Page::KontrolS4Mk3));
        assert!(Page::shown(false).all(|p| p != Page::KontrolS4Mk3));
        assert_eq!(Page::shown(false).count(), Page::ALL.len() - 1);
    }

    #[test]
    fn buffer_slider_moves_in_steps_of_64_inside_the_device_range() {
        assert_eq!(buffer_steps((14, 4096)), Some((64, 4096)));
        assert_eq!(buffer_steps((100, 1000)), Some((128, 960)));
        assert_eq!(buffer_steps((65, 100)), None);
    }

    #[test]
    fn latency_is_the_buffer_plus_what_the_device_adds() {
        let measured = Duration::from_secs_f64(688.0 / 44_100.0);
        let [p, o, all] = latency_readout(512, measured, 44_100);
        assert!((p - 11.6).abs() < 1e-9, "{p}");
        assert!((o - 4.0).abs() < 1e-9, "{o}");
        assert!((all - 15.6).abs() < 1e-9, "{all}");
        assert_eq!(
            latency_readout(256, Duration::ZERO, 48_000),
            [5.3, 0.0, 5.3]
        );
    }

    #[test]
    fn the_search_box_keeps_focus_when_the_first_letter_indexes_the_pages() {
        let ctx = egui::Context::default();
        let mut prefs = Prefs::new();
        prefs.open = true;
        let mut settings = Settings::default();
        let mut controls = Controls::detached();
        let mut kits = KitLibrary::none();
        let mut frame = |prefs: &mut Prefs, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 800.0),
                )),
                events,
                ..egui::RawInput::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                prefs.window(ui.ctx(), &mut settings, None, &mut controls, &mut kits);
            });
            output.textures_delta.clear();
        };
        frame(&mut prefs, Vec::new());
        ctx.memory_mut(|m| m.request_focus(prefs.search_box));
        frame(&mut prefs, Vec::new());
        for letter in ["m", "i", "x"] {
            frame(&mut prefs, vec![egui::Event::Text(letter.into())]);
            frame(&mut prefs, Vec::new());
        }
        assert_eq!(prefs.search.query, "mix", "every letter reached the box");
        assert!(prefs.search.index.is_some(), "the pages were indexed");
    }
}
