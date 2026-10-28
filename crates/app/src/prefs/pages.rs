use std::path::PathBuf;

use backspn_engine::{EqModel, FilterModel, LimiterModel, MixerFx, StreamInfo};
use eframe::egui::{self, ComboBox, RichText, Ui};

use crate::columns::{BrowserColumn, ColumnLayout};
use crate::deck_view::TEMPO_RANGES;
use crate::i18n::tr;
use crate::theme::ThemeId;
use crate::waveform::{WaveMode, WavePalette};

use super::search;
use super::settings::settings_path;
use super::{Monitor, Prefs, RecordingSource, Settings};

pub(super) fn input_routing(ui: &mut Ui, s: &mut Settings, audio: Option<&StreamInfo>) {
    let inputs = audio.map_or(0, |a| a.inputs);
    if inputs == 0 {
        ui.label(RichText::new(tr("The audio device has no inputs.")).weak());
    }
    let row = |ui: &mut Ui, id: &str, title: &str, routing: &mut Option<(u16, u16)>| {
        ui.horizontal(|ui| {
            search::title(ui, title);
            for (side, name) in [(0, "L"), (1, "R")] {
                let current = routing.map(|(l, r)| if side == 0 { l } else { r });
                let label = |c: Option<u16>| {
                    c.map_or_else(
                        || "None".to_owned(),
                        |c| format!("{}: Input {}", c + 1, c + 1),
                    )
                };
                ComboBox::from_id_salt((id, side))
                    .width(110.0)
                    .selected_text(format!("{name} {}", label(current)))
                    .show_ui(ui, |ui| {
                        for c in std::iter::once(None).chain((0..inputs).map(Some)) {
                            if ui.selectable_label(current == c, label(c)).clicked() {
                                *routing = match (c, *routing) {
                                    (None, _) => None,
                                    (Some(c), Some((l, r))) => {
                                        Some(if side == 0 { (c, r) } else { (l, c) })
                                    }
                                    (Some(c), None) => Some((c, c)),
                                };
                            }
                        }
                    });
            }
        });
    };
    for d in 0..s.input_routing.decks.len() {
        let title = format!("Input Deck {}", backspn_control::deck_letter(d));
        row(ui, &title.clone(), &title, &mut s.input_routing.decks[d]);
    }
    row(
        ui,
        "mic",
        "Input Microphone",
        &mut s.input_routing.microphone,
    );
    row(ui, "send", "Input FX Send (Ext)", &mut s.input_routing.send);
    ui.horizontal(|ui| {
        search::title(ui, "Microphone gain");
        ui.add(egui::Slider::new(&mut s.microphone_gain, 0.0..=2.0));
    });
    ui.label(
        RichText::new(crate::i18n::tr(
            "LIVE in a deck's header plays its input through the channel (gain, EQ, FX, fader). \
             The microphone goes straight into the master.",
        ))
        .weak(),
    );
}

pub fn device_combo(ui: &mut Ui, s: &mut Settings, devices: &[String]) -> bool {
    let current = s.device.clone().unwrap_or_else(|| "System default".into());
    let mut entries = vec![crate::search_combo::Entry::item(
        None,
        tr("System default"),
        s.device.is_none(),
    )];
    for name in devices {
        let on = s.device.as_deref() == Some(name.as_str());
        entries.push(crate::search_combo::Entry::item(
            Some(name.clone()),
            name.clone(),
            on,
        ));
    }
    if let (_, Some(device)) = crate::search_combo::show(ui, "device", current, 320.0, &entries)
        && device != s.device
    {
        match device {
            Some(name) => {
                crate::mixer_cards::switch_device(s, &name, crate::mixer_cards::first_time(&name));
            }
            None => s.device = None,
        }
        return true;
    }
    false
}

pub fn monitor_combo(ui: &mut Ui, s: &mut Settings, channels: u16) -> bool {
    let label = |m: Monitor| match m {
        Monitor::Auto => "Auto (3/4 if available)".to_owned(),
        Monitor::None => "None".to_owned(),
        Monitor::Channels(l, r) => format!("{}/{}", l + 1, r + 1),
    };
    let mut changed = false;
    ComboBox::from_id_salt("monitor")
        .width(200.0)
        .selected_text(label(s.monitor))
        .show_ui(ui, |ui| {
            let mut options = vec![Monitor::Auto, Monitor::None];
            options.extend(
                (2..channels.saturating_sub(1))
                    .step_by(2)
                    .map(|l| Monitor::Channels(l, l + 1)),
            );
            for m in options {
                changed |= ui.selectable_value(&mut s.monitor, m, label(m)).changed();
            }
        });
    changed
}

fn pair_label(pair: Option<(u16, u16)>) -> String {
    pair.map_or_else(
        || "None".to_owned(),
        |(l, r)| format!("{}/{}", l + 1, r + 1),
    )
}

fn output_pairs(channels: u16) -> Vec<Option<(u16, u16)>> {
    std::iter::once(None)
        .chain(
            (0..channels.saturating_sub(1))
                .step_by(2)
                .map(|l| Some((l, l + 1))),
        )
        .collect()
}

fn external_outputs(ui: &mut Ui, s: &mut Settings, channels: u16) {
    let x = &mut s.external_mixer;
    let mut rows: Vec<(String, &mut Option<(u16, u16)>)> = x
        .decks
        .iter_mut()
        .enumerate()
        .map(|(d, pair)| {
            (
                format!("Output Deck {}", backspn_control::deck_letter(d)),
                pair,
            )
        })
        .collect();
    rows.push(("Output Preview".to_owned(), &mut x.preview));
    rows.push(("Output FX Return".to_owned(), &mut x.fx_return));
    egui::Grid::new("external outputs")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            for (title, pair) in rows {
                search::title(ui, &title);
                ComboBox::from_id_salt(&title)
                    .width(120.0)
                    .selected_text(pair_label(*pair))
                    .show_ui(ui, |ui| {
                        for option in output_pairs(channels) {
                            ui.selectable_value(pair, option, pair_label(option));
                        }
                    });
                ui.end_row();
            }
        });
    ui.label(
        RichText::new(tr(
            "Each deck leaves on its own outputs to a channel of the external mixer. \
             Decks on the same pair are summed. Send effects take Input FX Send (Ext) \
             and come back on Output FX Return.",
        ))
        .weak(),
    );
}

pub(super) fn output_routing(ui: &mut Ui, s: &mut Settings, audio: Option<&StreamInfo>) -> bool {
    let channels = audio.map_or(2, |a| a.channels);
    let mut restart = false;
    ui.horizontal(|ui| {
        search::title(ui, "Mixing Mode");
        search::radio(ui, &mut s.external_mixer.on, false, "Internal");
        search::radio(ui, &mut s.external_mixer.on, true, "External");
    })
    .response
    .on_hover_text(tr(
        "When internal is selected the audio signals are mixed with the internal mixer. \
         When external is selected the audio signals are sent to the audio interface \
         connected to the external mixer.",
    ));
    ui.add_space(6.0);
    if s.external_mixer.on {
        external_outputs(ui, s, channels);
        return false;
    }
    search::title(ui, "Output master: channels 1/2.");
    crate::widgets::check(ui, &mut s.master_mono, "Master in mono");
    crate::widgets::check(ui, &mut s.monitor_mono, "Monitor (headphones) in mono");
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        search::title(ui, "Output monitor (headphones):");
        restart |= monitor_combo(ui, s, channels);
    });
    if let Some(info) = audio {
        ui.label(
            RichText::new(match info.monitor {
                Some((l, r)) => format!("Now: headphones on outputs {}/{}.", l + 1, r + 1),
                None => "Now: no headphone output (cue goes nowhere).".to_owned(),
            })
            .weak(),
        );
    }
    restart
}

pub(super) fn bpm_range_name(mode: u8) -> String {
    if mode == 9 {
        return "Automatic".to_owned();
    }
    let r = backspn_analysis::BpmRange::from_mode(mode);
    format!("{:.0} - {:.0} BPM", r.min, r.max)
}

pub(super) fn analyze_options(ui: &mut Ui, s: &mut Settings) {
    search::check(
        ui,
        &mut s.auto_analyze_import,
        "Analyze new imported tracks",
    )
    .on_hover_text(tr(
        "Tracks added to the collection are analysed in the background",
    ));
    search::check(
        ui,
        &mut s.auto_analyze_on_load,
        "Analyze new tracks when loading into deck",
    )
    .on_hover_text(tr("A track loaded without tempo, key or gain gets them"));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        search::title(ui, "BPM range");
        ComboBox::from_id_salt("bpm-range")
            .selected_text(bpm_range_name(s.bpm_range_mode))
            .show_ui(ui, |ui| {
                for mode in [9, 0, 1, 2, 3, 4, 5, 6, 7, 8] {
                    ui.selectable_value(&mut s.bpm_range_mode, mode, bpm_range_name(mode));
                }
            });
    });
}

#[allow(clippy::too_many_lines)]
pub(super) fn transport(ui: &mut Ui, s: &mut Settings, s4: bool) {
    egui::Grid::new("transport")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            search::title(ui, "Default tempo range");
            ComboBox::from_id_salt("range")
                .selected_text(format!("±{:.0}%", s.tempo_range * 100.0))
                .show_ui(ui, |ui| {
                    for r in TEMPO_RANGES {
                        ui.selectable_value(&mut s.tempo_range, r, format!("±{:.0}%", r * 100.0));
                    }
                });
            ui.end_row();
            search::title(ui, "Tempo bend sensitivity");
            ui.add(egui::Slider::new(&mut s.bend_sensitivity, 0.0..=100.0).suffix("%"));
            ui.end_row();
            search::title(ui, "Track end warning");
            ui.add(egui::Slider::new(&mut s.end_warning_s, 0.0..=120.0).suffix(" s"))
                .on_hover_text(tr("When the time left starts flashing red"));
            ui.end_row();
            search::title(ui, "Keylock range");
            ComboBox::from_id_salt("keylock range")
                .selected_text(if s.keylock_scratch_range {
                    "Scratch"
                } else {
                    "Normal"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut s.keylock_scratch_range,
                        false,
                        tr("Normal (up to 2×)"),
                    );
                    ui.selectable_value(
                        &mut s.keylock_scratch_range,
                        true,
                        tr("Scratch (0.7–1.5×)"),
                    );
                });
            ui.end_row();
            search::title(ui, "Slip on pause");
            crate::widgets::check(
                ui,
                &mut s.slip_pause,
                "With SLIP, the track runs on while paused",
            )
            .on_hover_text(tr(
                "Play then resumes where the track would be (slip pause)",
            ));
            ui.end_row();
            search::title(ui, "Auto Master Mode");
            crate::widgets::check(
                ui,
                &mut s.auto_master,
                "When the tempo master stops, a playing deck takes over",
            )
            .on_hover_text(tr(
                "The deck closest in tempo to the master clock, then in phase (the AUTO button)",
            ));
            ui.end_row();
            search::title(ui, "Tempo master");
            crate::widgets::check(
                ui,
                &mut s.on_air_master,
                "Only on air decks can be tempo master",
            )
            .on_hover_text(tr(
                "Decks whose channel fader or the crossfader shuts them cannot lead",
            ));
            ui.end_row();
            ui.label("");
            crate::widgets::check(
                ui,
                &mut s.sample_decks_master,
                "Sample decks can be tempo master",
            )
            .on_hover_text(tr(
                "A playing sample deck with samples loaded can lead the clock",
            ));
            ui.end_row();
            search::title(ui, "Progressive tempo bend");
            crate::widgets::check(ui, &mut s.bend_progressive, "Bend grows while held");
            ui.end_row();
            search::title(ui, "Scratch: audio per jog turn");
            ui.add(egui::Slider::new(&mut s.jog_seconds_per_turn, 0.5..=4.0).suffix(" s"));
            ui.end_row();
            search::title(ui, "Jog nudge sensitivity");
            ui.add(
                egui::Slider::new(&mut s.jog_nudge_sensitivity, 0.0..=1.0)
                    .custom_formatter(|v, _| format!("×{:.2}", crate::jog::bend_factor(v))),
            )
            .on_hover_text(crate::i18n::tr(
                "How far the track moves for each turn when nudging a playing deck \
                 (turning the ring, or the platter in jog mode)",
            ));
            ui.end_row();
            search::title(ui, "Jog spin sensitivity");
            ui.add(
                egui::Slider::new(&mut s.jog_sensitivity, 0.0..=1.0)
                    .custom_formatter(|v, _| format!("×{:.2}", crate::jog::bend_factor(v))),
            )
            .on_hover_text(crate::i18n::tr(
                "How far the track moves for each turn of a jog wheel let go while \
                 turning (×1.00 follows the wheel)",
            ));
            ui.end_row();
            if s4 {
                search::title(ui, "Jogwheel tension (S4 MK3)");
                ui.add(egui::Slider::new(&mut s.jog_tension, 0..=255))
                    .on_hover_text(crate::i18n::tr(
                        "Turning resistance the jog wheel motors give in jog mode; \
                         low values let a spun wheel run on longer",
                    ));
                ui.end_row();
            }
            search::title(ui, "Played-state delay");
            ui.add(egui::Slider::new(&mut s.played_state_delay, 0.0..=120.0).suffix(" s"))
                .on_hover_text(tr(
                    "How long a track must be heard (playing, channel fader up) to count as played",
                ));
            ui.end_row();
        });
    ui.add_space(4.0);
    ui.label(RichText::new(tr("1.8 s per turn is a 33⅓ rpm record.")).weak());
}

pub(super) fn key_notation_name(value: u8) -> &'static str {
    match value {
        0 => "Musical (all sharps)",
        1 => "Musical Key",
        _ => "Open Key",
    }
}

pub(super) fn auto_fade(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "Auto crossfade");
    ui.add(egui::Slider::new(&mut s.auto_fade_s, 0.0..=30.0).suffix(" s"))
        .on_hover_text(tr("Time the crossfader takes from one side to the other"));
}

const EQ_MODELS: [(EqModel, &str, &str); 7] = [
    (
        EqModel::WideIsolator,
        "Isolator 284 Hz / 4.65 kHz",
        "Three bands, full kill at the bottom, +6 dB at the top",
    ),
    (
        EqModel::Isolator,
        "Isolator 250 Hz / 2.5 kHz",
        "The app's earlier isolator",
    ),
    (
        EqModel::Classic,
        "Classic",
        "Smooth three-band EQ: −24 dB at the bottom, +12 dB at the top",
    ),
    (
        EqModel::ThreeCentre,
        "64 Hz / 1.9 kHz / 12.75 kHz",
        "Three bands: −24 dB to +12 dB at each centre",
    ),
    (
        EqModel::SweepingShelves,
        "Sweeping shelves",
        "Low and high shelves whose corners move with the knob, 1.2 kHz mid",
    ),
    (
        EqModel::WideBands,
        "Wide 70 Hz / 1 kHz",
        "Very wide low and mid bands, −27 dB to +6 dB; moving high shelf",
    ),
    (
        EqModel::FourBand,
        "4-band",
        "Low and high shelves and two mids (350 Hz, 1.85 kHz) whose \
         frequency and width follow the knob; adds MID LO",
    ),
];

fn eq_model_name(model: EqModel) -> &'static str {
    EQ_MODELS
        .iter()
        .find(|(m, _, _)| *m == model)
        .map_or("", |(_, name, _)| name)
}

pub(super) fn eq_model(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "EQ");
    ComboBox::from_id_salt("eq-model")
        .selected_text(eq_model_name(s.eq_model))
        .show_ui(ui, |ui| {
            for (model, name, hover) in EQ_MODELS {
                ui.selectable_value(&mut s.eq_model, model, name)
                    .on_hover_text(hover);
            }
        });
}

pub(super) fn headroom(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "Headroom");
    ui.add(
        egui::Slider::new(&mut s.headroom_db, -12.0..=0.0)
            .step_by(1.0)
            .suffix(" dB"),
    )
    .on_hover_text(tr("Master level before the limiter"));
    search::check(
        ui,
        &mut s.headroom_on_meters,
        "Apply Headroom to Channel Meters",
    )
    .on_hover_text(tr(
        "When selected the current headroom setting is reflected by the channel meters in the mixer channels.",
    ));
}

pub(super) fn limiter(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "Limiter");
    ui.horizontal(|ui| {
        search::radio(
            ui,
            &mut s.limiter_model,
            LimiterModel::Transparent,
            "Transparent",
        )
        .on_hover_text(tr(
            "Look-ahead limiter (about 3 ms) with a smooth gain curve, 35 ms release",
        ));
        search::radio(ui, &mut s.limiter_model, LimiterModel::Classic, "Classic")
            .on_hover_text(tr("Short look-ahead (0.7 ms), 200 ms release"));
        search::radio(ui, &mut s.limiter_model, LimiterModel::Standard, "Standard").on_hover_text(
            tr("The app's earlier limiter: 1.5 ms look-ahead, 100 ms release"),
        );
    });
}

pub(super) fn external_mixing_mode(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "External Mixing Mode");
    let x = &mut s.external_mixer;
    ui.horizontal(|ui| {
        search::title(ui, "Headroom");
        ui.add(
            egui::Slider::new(&mut x.headroom_db, -12.0..=0.0)
                .step_by(1.0)
                .suffix(" dB"),
        );
    });
    search::check(
        ui,
        &mut x.headroom_on_meters,
        "Apply Headroom to Channel Meters",
    );
    search::check(ui, &mut x.auto_gain, "Enable Autogain");
    search::check(
        ui,
        &mut x.master_volume_control,
        "Enable Master Volume Control",
    )
    .on_hover_text(tr("MAIN sets the level of every deck output"));
    search::check(ui, &mut x.eq_gain_control, "Enable EQ + GAIN");
    search::check(ui, &mut x.fx_control, "Enable Filter / Mixer FX");
    ui.label(
        RichText::new(tr(
            "Used when Output Routing's Mixing Mode is External. Channel faders, \
             balance and the crossfader are left to the external mixer.",
        ))
        .weak(),
    );
}

pub(super) fn auto_gain(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "Auto-gain");
    crate::widgets::check(
        ui,
        &mut s.auto_gain,
        "Set each track's gain from its loudness",
    )
    .on_hover_text(tr(
        "Loud tracks are turned down, quiet ones up (±12 dB), on top of GAIN",
    ));
}

pub(super) fn channel_filter(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "Channel filter");
    ui.horizontal(|ui| {
        search::radio(ui, &mut s.filter_model, FilterModel::Standard, "Standard")
            .on_hover_text(tr("Smooth sweep, no resonance peak"));
        search::radio(ui, &mut s.filter_model, FilterModel::Ladder, "Ladder").on_hover_text(tr(
            "Four-pole ladder: low-pass 98 Hz–15 kHz, high-pass 46 Hz–5.3 kHz",
        ));
        search::radio(ui, &mut s.filter_model, FilterModel::Club, "Club").on_hover_text(
            crate::i18n::tr(
                "Resonant sweep modelled on club mixers: cubic cut-off law, \
                 low-pass 100 Hz–25 kHz, high-pass 20 Hz–8.2 kHz",
            ),
        );
    });
    ui.add_enabled_ui(s.filter_model == FilterModel::Club, |ui| {
        ui.add(
            egui::Slider::new(&mut s.filter_resonance, 0.0..=1.0)
                .show_value(false)
                .text("Resonance"),
        )
        .on_hover_text(tr("Left: no peak · centre: default · right: strong peak"));
    });
}

pub(super) fn mixer_fx_slots(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "Mixer FX");
    egui::Grid::new("mixer-fx-slots")
        .num_columns(2)
        .show(ui, |ui| {
            for (n, slot) in s.mixer_fx_slots.iter_mut().enumerate() {
                search::title(ui, &format!("Mixer FX slot {}", n + 1));
                egui::ComboBox::from_id_salt(("mixer-fx-slot", n))
                    .selected_text(slot.name())
                    .show_ui(ui, |ui| {
                        for fx in MixerFx::ALL {
                            ui.selectable_value(slot, fx, fx.name());
                        }
                    });
                ui.end_row();
            }
        });
    ui.label(
        RichText::new(crate::i18n::tr(
            "The channel knob drives the filter or the chosen mixer FX; the channel's FX \
             button switches it.",
        ))
        .weak(),
    );
}

pub(super) fn crossfader_curve(ui: &mut Ui, s: &mut Settings) {
    search::check(ui, &mut s.show_crossfader, "Show Crossfader")
        .on_hover_text(tr("Hidden, it still works from controllers and mappings."));
    search::heading(ui, "Crossfader curve");
    ui.horizontal(|ui| {
        for (value, name) in [(0.0, "Smooth"), (0.5, "Additive"), (1.0, "Sharp cut")] {
            if ui
                .selectable_label((s.xfader_curve - value).abs() < 1e-3, name)
                .clicked()
            {
                s.xfader_curve = value;
            }
        }
    });
    ui.add(
        egui::Slider::new(&mut s.xfader_curve, 0.0..=1.0)
            .show_value(false)
            .text("Fine"),
    );
    ui.label(
        RichText::new(crate::i18n::tr(
            "Smooth: long blends without a dip. Sharp cut: both sides full until the very \
             end, for scratching.",
        ))
        .weak(),
    );
}

pub(super) fn waveform_settings(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "Waveform");
    egui::Grid::new("waveform")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            search::title(ui, "Colours");
            ui.horizontal(|ui| {
                for mode in WaveMode::ALL {
                    search::radio(ui, &mut s.wave_mode, mode, mode.name());
                }
            });
            ui.end_row();
            search::title(ui, "Palette");
            palette_select(ui, s);
            ui.end_row();
            if s.wave_palette == WavePalette::Custom {
                search::title(ui, "Low / mid / high");
                ui.horizontal(|ui| {
                    for c in &mut s.wave_custom {
                        ui.color_edit_button_srgb(c);
                    }
                });
                ui.end_row();
            }
            search::title(ui, "Default zoom");
            ui.add(
                egui::Slider::new(
                    &mut s.wave_zoom_s,
                    crate::deck_view::MIN_ZOOM_S..=crate::deck_view::MAX_ZOOM_S,
                )
                .logarithmic(true)
                .custom_formatter(|v, _| format!("±{v:.1} s")),
            );
            ui.end_row();
        });
    if s.wave_palette != WavePalette::Custom
        && ui
            .button(tr("Customise this palette"))
            .on_hover_text(tr("Copies its colours to Custom, to edit them"))
            .clicked()
    {
        let colors = s
            .wave_palette
            .colors(&crate::theme::get(ui.ctx()), s.wave_custom);
        s.wave_custom = colors.map(|c| [c.r(), c.g(), c.b()]);
        s.wave_palette = WavePalette::Custom;
    }
    ui.label(
        RichText::new(tr(
            "Zoom each deck with the mouse wheel or the − / + buttons on its waveform.",
        ))
        .weak(),
    );
}

pub(super) fn palette_select(ui: &mut Ui, s: &mut Settings) {
    let theme = crate::theme::get(ui.ctx());
    let swatch = |ui: &mut Ui, palette: WavePalette| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(54.0, 16.0), egui::Sense::hover());
        paint_swatch(ui.painter(), rect, palette.colors(&theme, s.wave_custom));
    };
    ui.horizontal(|ui| {
        swatch(ui, s.wave_palette);
        let mut chosen = s.wave_palette;
        ComboBox::from_id_salt("palette")
            .selected_text(s.wave_palette.name())
            .width(130.0)
            .show_ui(ui, |ui| {
                for palette in WavePalette::ALL {
                    ui.horizontal(|ui| {
                        swatch(ui, palette);
                        ui.selectable_value(&mut chosen, palette, palette.name());
                    });
                }
            });
        s.wave_palette = chosen;
    });
}

fn theme_swatch(ui: &mut Ui, id: ThemeId) {
    const SQUARE: f32 = 8.0;
    const GAP: f32 = 3.0;
    let t = id.theme();
    let colors = [
        t.accent, t.decks[0], t.decks[1], t.decks[2], t.decks[3], t.invert,
    ];
    #[allow(clippy::cast_precision_loss)]
    let width = colors.len() as f32 * (SQUARE + GAP) + GAP;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 16.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, t.surface);
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, t.border_strong),
        egui::StrokeKind::Inside,
    );
    let mut x = rect.left() + GAP;
    for c in colors {
        let square = egui::Rect::from_min_size(
            egui::pos2(x, rect.center().y - SQUARE / 2.0),
            egui::vec2(SQUARE, SQUARE),
        );
        painter.rect_filled(square, 0.0, c);
        x += SQUARE + GAP;
    }
}

pub(super) fn paint_swatch(
    painter: &egui::Painter,
    rect: egui::Rect,
    [low, mid, high]: [egui::Color32; 3],
) {
    let mid_y = rect.center().y;
    let half = rect.height() / 2.0;
    let mut x = rect.left();
    let mut i = 0.0f32;
    while x < rect.right() {
        let beat = (i * 0.21).sin().abs();
        let levels = [
            0.35 + 0.6 * beat.powi(6),
            0.3 + 0.25 * (i * 0.9).sin().abs(),
            0.18 + 0.12 * (i * 2.3).cos().abs(),
        ];
        for (level, c) in levels.into_iter().zip([low, mid, high]) {
            painter.line_segment(
                [
                    egui::pos2(x, mid_y - level * half),
                    egui::pos2(x, mid_y + level * half),
                ],
                egui::Stroke::new(1.0, c),
            );
        }
        x += 1.0;
        i += 1.0;
    }
}

pub(super) fn loading(ui: &mut Ui, s: &mut Settings) {
    search::check(
        ui,
        &mut s.load_only_stopped,
        "Loading only into stopped deck",
    );
    ui.label(
        RichText::new(crate::i18n::tr(
            "Prevents loading a track into a playing deck (browser, drag and drop, \
                 controllers and files).",
        ))
        .weak(),
    );
    ui.add_space(6.0);
    search::check(
        ui,
        &mut s.duplicate_same_track,
        "Duplicate deck when loading same track",
    );
    ui.label(
        RichText::new(crate::i18n::tr(
            "Loading the track another deck has continues from its position, \
                 tempo and play state.",
        ))
        .weak(),
    );
    ui.add_space(6.0);
    search::check(ui, &mut s.cue_to_marker, "Cue to load marker");
    ui.label(
        RichText::new(tr(
            "Starts at the stored cue point instead of the track start.",
        ))
        .weak(),
    );
    ui.add_space(6.0);
    auto_cue(ui, s);
    ui.add_space(6.0);
    search::title(ui, "Stem Loading");
    ui.horizontal(|ui| {
        for mode in crate::stems::DefaultLoad::ALL {
            search::radio(ui, &mut s.stems_default_load, mode, mode.name());
        }
    });
    ui.label(
        RichText::new(crate::i18n::tr(
            "What loading a stem file does. The other way is the loading alternative \
             (a mapping or the browser's Load menu).",
        ))
        .weak(),
    );
}

pub(super) fn sample_decks(ui: &mut Ui, s: &mut Settings) {
    search::title(ui, "Behaviors");
    search::check(
        ui,
        &mut s.sample_auto_play,
        "Auto Enable Deck Play on Sample Trigger",
    );
    ui.label(
        RichText::new(tr(
            "The sample deck starts when a sample is triggered, even if it was stopped.",
        ))
        .weak(),
    );
    search::check(
        ui,
        &mut s.sample_one_shot_ignores_punch,
        "One-Shot Samples Ignore Punch Mode",
    );
    ui.label(
        RichText::new(crate::i18n::tr(
            "A one-shot always starts from its beginning; without it, it goes on from \
             where the previous sample was.",
        ))
        .weak(),
    );
    search::check(
        ui,
        &mut s.sample_one_shot_ignores_quantize,
        "One-Shot Samples Ignore Quantize Mode",
    );
    ui.label(
        RichText::new(tr(
            "A one-shot plays at once instead of on the next quantize point.",
        ))
        .weak(),
    );
    ui.add_space(8.0);
    search::title(ui, "Loading");
    search::check(
        ui,
        &mut s.sample_auto_gain,
        "Set Auto-Gain When Loading Samples",
    );
    ui.label(RichText::new(tr("The sample's stored auto-gain becomes the cell's gain.")).weak());
    ui.add_space(8.0);
    search::title(ui, "Saving");
    search::check(
        ui,
        &mut s.sample_autosave_sets,
        "Auto-Save Edited Sample Sets",
    );
    ui.label(
        RichText::new(crate::i18n::tr(
            "An edited set is saved when the deck leaves it; otherwise its changes are lost.",
        ))
        .weak(),
    );
    ui.add_space(8.0);
    search::title(ui, "Samples Folder");
    ui.horizontal(|ui| {
        ui.label(RichText::new(s.samples_dir.display().to_string()).monospace());
        if ui.button(tr("Choose…")).clicked()
            && let Some(dir) = rfd::FileDialog::new()
                .set_directory(&s.samples_dir)
                .pick_folder()
        {
            s.samples_dir = dir;
        }
    });
    ui.label(RichText::new(tr("Where captured samples are written.")).weak());
    ui.add_space(8.0);
    search::title(ui, "Sample Sets Folder");
    ui.horizontal(|ui| {
        ui.label(RichText::new(s.sets_dir.display().to_string()).monospace());
        if ui.button(tr("Choose…")).clicked()
            && let Some(dir) = rfd::FileDialog::new()
                .set_directory(&s.sets_dir)
                .pick_folder()
        {
            s.sets_dir = dir;
        }
    });
    ui.label(RichText::new(tr("Where imported sample sets are kept.")).weak());
    ui.add_space(8.0);
    ui.label(
        RichText::new(crate::i18n::tr(
            "A deck becomes a sample deck with the SAMPLE button in its header; the slot \
             volume, filter and FX controls are the stems' (Slot Volume, Slot Filter…).",
        ))
        .weak(),
    );
}

pub fn theme_combo(ui: &mut Ui, s: &mut Settings) -> bool {
    theme_swatch(ui, s.theme);
    let mut chosen = s.theme;
    ComboBox::from_id_salt("theme")
        .selected_text(s.theme.name())
        .width(130.0)
        .show_ui(ui, |ui| {
            for id in ThemeId::ALL {
                ui.horizontal(|ui| {
                    theme_swatch(ui, id);
                    ui.selectable_value(&mut chosen, id, id.name());
                });
            }
        });
    let changed = chosen != s.theme;
    s.theme = chosen;
    changed
}

pub(super) fn global_settings(
    ui: &mut Ui,
    s: &mut Settings,
    insights: Option<&(String, bool)>,
) -> Option<super::Global> {
    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .button(tr("Setup Assistant…"))
            .on_hover_text(tr("Sets up the controller or mixer again, step by step"))
            .clicked()
        {
            action = Some(super::Global::SetupWizard);
        }
        if ui
            .button(tr("Report a Problem…"))
            .on_hover_text(tr(
                "Opens a new GitHub issue in your browser with the version, the system and the log",
            ))
            .clicked()
        {
            action = Some(super::Global::ReportProblem);
        }
        if ui
            .button(tr("Show Log File"))
            .on_hover_text(tr("Shows the log file in the file manager"))
            .clicked()
        {
            action = Some(super::Global::ShowLogFile);
        }
        if ui
            .button(tr("Reset All Settings…"))
            .on_hover_text(tr(
                "Deletes all of Backspn's configuration and restarts it as on first launch; \
                 the music library stays",
            ))
            .clicked()
        {
            action = Some(super::Global::ResetSettings);
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        search::title(ui, "Theme");
        theme_combo(ui, s);
    });
    search::check(ui, &mut s.show_tooltips, "Show Tooltips")
        .on_hover_text(tr("Enables or disables the tooltips"));
    ui.horizontal(|ui| {
        search::title(ui, "Language");
        ComboBox::from_id_salt("language")
            .selected_text(s.language.name())
            .show_ui(ui, |ui| {
                for l in crate::i18n::Language::ALL {
                    ui.selectable_value(&mut s.language, l, l.name());
                }
            });
    })
    .response
    .on_hover_text(tr("The language of the app"));
    search::heading(ui, "Notifications");
    let kinds = ["Info", "Warning", "Error"];
    for (seconds, kind) in s.toast_seconds.iter_mut().zip(kinds) {
        ui.horizontal(|ui| {
            search::title(ui, kind);
            ui.add(
                egui::Slider::new(seconds, 1.0..=30.0)
                    .step_by(1.0)
                    .suffix(" s"),
            );
        })
        .response
        .on_hover_text(tr("How long these notices stay at the bottom right"));
    }
    if let Some((status, editable)) = insights {
        action = insights_settings(ui, s, status, *editable).or(action);
    }
    action
}

fn insights_settings(
    ui: &mut Ui,
    s: &mut Settings,
    status: &str,
    editable: bool,
) -> Option<super::Global> {
    let mut action = None;
    search::heading(ui, "Anonymous Usage Statistics");
    if ui
        .add_enabled_ui(editable, |ui| {
            search::check(
                ui,
                &mut s.insights_enabled,
                "Send anonymous usage statistics",
            )
        })
        .inner
        .on_hover_text(tr(
            "Once a day: version, system, audio settings, controller models, library size and \
             counts such as mixes made. Never track names, file paths or device names.",
        ))
        .changed()
    {
        s.insights_notice_seen = true;
    }
    ui.label(RichText::new(status).weak());
    ui.horizontal(|ui| {
        if ui
            .button(tr("Show What Is Sent…"))
            .on_hover_text(tr("Shows the exact report Backspn would send now"))
            .clicked()
        {
            action = Some(super::Global::InsightsPreview);
        }
        if ui
            .add_enabled(
                editable && s.insights_enabled,
                egui::Button::new(tr("Send Now")),
            )
            .clicked()
        {
            s.insights_notice_seen = true;
            action = Some(super::Global::InsightsSend);
        }
        ui.hyperlink_to(tr("Public statistics"), crate::insights::PAGE);
    });
    action
}

pub fn tags_serialization(ui: &mut Ui, s: &mut Settings) {
    ui.horizontal(|ui| {
        search::title(ui, "Tags");
        let write = s.tags_serialization == 2;
        ComboBox::from_id_salt("tags-serialization")
            .selected_text(tr(if write {
                "Write all tags to files"
            } else {
                "Do not write any tags to files"
            }))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut s.tags_serialization,
                    0,
                    tr("Do not write any tags to files"),
                )
                .on_hover_text(tr("The music files are never changed"));
                ui.selectable_value(&mut s.tags_serialization, 2, tr("Write all tags to files"))
                    .on_hover_text(tr(
                        "Edits, ratings and analysis results are written into the files' tags",
                    ));
            });
    });
}

pub(super) fn file_management(ui: &mut Ui) {
    let library = std::env::var_os("BACKSPN_LIBRARY")
        .map_or_else(|| crate::dirs::data().join("library.sqlite"), PathBuf::from);
    egui::Grid::new("files")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            search::title(ui, "Library");
            ui.label(RichText::new(library.display().to_string()).monospace());
            ui.end_row();
            search::title(ui, "Settings");
            ui.label(RichText::new(settings_path().display().to_string()).monospace());
            ui.end_row();
        });
}

const PER_ROW: usize = 3;

impl Prefs {
    fn library_nodes(ui: &mut Ui, s: &mut Settings) {
        search::heading(ui, "Library nodes");
        ui.horizontal_wrapped(|ui| {
            for node in crate::browser::TreeNode::ALL {
                let mut shown = !s.hidden_tree_nodes.contains(&node);
                if search::check(ui, &mut shown, node.name()).changed() {
                    s.hidden_tree_nodes.retain(|n| *n != node);
                    if !shown {
                        s.hidden_tree_nodes.push(node);
                    }
                }
            }
        });
        ui.label(
            RichText::new(crate::i18n::tr(
                "Untick a node to hide it from LIBRARY; nothing is deleted. Collection always shows.",
            ))
            .weak(),
        );
    }

    pub(super) fn browser_details(ui: &mut Ui, s: &mut Settings) {
        search::heading(ui, "Columns");
        let columns: Vec<BrowserColumn> = s.browser_columns.all().to_vec();
        egui::Grid::new("columns")
            .num_columns(PER_ROW)
            .spacing([24.0, 4.0])
            .show(ui, |ui| {
                for (i, column) in columns.iter().enumerate() {
                    let mut shown = s.browser_columns.is_shown(*column);
                    if search::check(ui, &mut shown, column.name()).changed() {
                        s.browser_columns.set_shown(*column, shown);
                    }
                    if i % PER_ROW == PER_ROW - 1 {
                        ui.end_row();
                    }
                }
            });
        ui.horizontal(|ui| {
            if ui.button(tr("Reset columns")).clicked() {
                s.browser_columns = ColumnLayout::default();
            }
            ui.label(
                RichText::new(crate::i18n::tr(
                    "Shown left to right in this order; drag a header to move a column. \
                     Also: right-click a column header.",
                ))
                .weak(),
            );
        });
        ui.add_space(10.0);
        Self::library_nodes(ui, s);
        ui.add_space(10.0);
        egui::Grid::new("keys")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                search::title(ui, "Key displayed in the app");
                ComboBox::from_id_salt("key-notation")
                    .selected_text(key_notation_name(s.key_notation))
                    .show_ui(ui, |ui| {
                        for v in [2, 1, 0] {
                            ui.selectable_value(&mut s.key_notation, v, key_notation_name(v));
                        }
                    });
                ui.end_row();
                search::title(ui, "Key written to file tags").on_hover_text(tr(
                    "The notation written into the files, for other applications",
                ));
                ComboBox::from_id_salt("key-notation-exported")
                    .selected_text(key_notation_name(s.key_notation_exported))
                    .show_ui(ui, |ui| {
                        for v in [2, 1, 0] {
                            ui.selectable_value(
                                &mut s.key_notation_exported,
                                v,
                                key_notation_name(v),
                            );
                        }
                    });
                ui.end_row();
                search::title(ui, "Key highlighting");
                ComboBox::from_id_salt("key-highlight")
                    .selected_text(s.key_highlight.name())
                    .show_ui(ui, |ui| {
                        for h in crate::key_view::Highlight::ALL {
                            ui.selectable_value(&mut s.key_highlight, h, h.name());
                        }
                    });
                ui.end_row();
                search::title(ui, "List row height");
                ui.add(egui::Slider::new(&mut s.row_height, 18.0..=48.0).suffix(" pt"));
                ui.end_row();
                search::title(ui, "Allow editing in the list");
                search::check(ui, &mut s.list_inplace_edit, "");
                ui.end_row();
                search::title(ui, "Show playlist favorites");
                search::check(ui, &mut s.show_favorites, "");
                ui.end_row();
                search::title(ui, "Show Preview Player");
                search::check(ui, &mut s.show_preview_player, "");
                ui.end_row();
                search::title(ui, "Key reference");
                ui.horizontal(|ui| {
                    search::radio(ui, &mut s.key_reference_focus, false, "Master Deck");
                    search::radio(ui, &mut s.key_reference_focus, true, "Deck Focus");
                })
                .response
                .on_hover_text(tr("That deck must have key lock on"));
                ui.end_row();
            });
    }
}

pub(super) fn file_management_page(ui: &mut Ui, s: &mut Settings) {
    file_management(ui);
    search::check(
        ui,
        &mut s.import_folders_at_startup,
        "Import music folders at startup",
    )
    .on_hover_text(tr(
        "New tracks in the music folders join the collection when the app starts",
    ));
    tags_serialization(ui, s);
    ui.add_space(8.0);
    stem_folders(ui, s);
    ui.add_space(8.0);
    search::heading(ui, "Music folders");
    if s.music_folders.is_empty() {
        ui.label(RichText::new(tr("None yet: use + next to FOLDERS in the browser.")).weak());
    }
    let mut remove = None;
    for (i, folder) in s.music_folders.iter().enumerate() {
        ui.horizontal(|ui| {
            if ui
                .small_button("✖")
                .on_hover_text(tr("Remove from the list"))
                .clicked()
            {
                remove = Some(i);
            }
            ui.label(RichText::new(folder.display().to_string()).monospace());
        });
    }
    if let Some(i) = remove {
        s.music_folders.remove(i);
    }
    ui.add_space(8.0);
    music_library(ui, s);
}

fn music_library(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "Music library");
    if cfg!(target_os = "macos") {
        search::check(ui, &mut s.music_use_xml, "Use legacy XML music library").on_hover_text(tr(
            "Reads the XML file the music player exports instead of its library",
        ));
    }
    let current = s
        .music_xml
        .clone()
        .unwrap_or_else(crate::browser::default_music_xml);
    ui.horizontal(|ui| {
        search::title(ui, "Library XML");
        if ui.button(tr("Choose…")).clicked()
            && let Some(file) = rfd::FileDialog::new()
                .add_filter("XML", &["xml"])
                .set_directory(current.parent().unwrap_or(&current))
                .pick_file()
        {
            s.music_xml = Some(file);
        }
        ui.label(RichText::new(current.display().to_string()).monospace());
    })
    .response
    .on_hover_text(tr("The music player's exported library file"));
}

fn stem_folders(ui: &mut Ui, s: &mut Settings) {
    let pick = |current: &PathBuf| rfd::FileDialog::new().set_directory(current).pick_folder();
    egui::Grid::new("stem-folders")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            search::title(ui, "Stem Files Location");
            ui.horizontal(|ui| {
                ui.label(RichText::new(s.stems_dir.display().to_string()).monospace());
                if ui.button(tr("Choose…")).clicked()
                    && let Some(dir) = pick(&s.stems_dir)
                {
                    s.stems_dir = dir;
                }
            });
            ui.end_row();
            search::title(ui, "Also read stems from");
            ui.horizontal(|ui| {
                let mut on = s.foreign_stems_dir.is_some();
                if crate::widgets::check(ui, &mut on, "").changed() {
                    s.foreign_stems_dir = on.then(crate::stems::default_foreign_dir);
                }
                if let Some(dir) = &mut s.foreign_stems_dir {
                    ui.label(RichText::new(dir.display().to_string()).monospace());
                    if ui.button(tr("Choose…")).clicked()
                        && let Some(picked) = pick(dir)
                    {
                        *dir = picked;
                    }
                }
            });
            ui.end_row();
        });
    ui.label(
        RichText::new(crate::i18n::tr(
            "Generate Stems writes each track's stems in the first folder. The second \
             holds stems made by another DJ application: they load like yours, but are \
             never changed or deleted.",
        ))
        .weak(),
    );
}

pub(super) fn loop_recorder(ui: &mut Ui, s: &mut Settings) {
    ui.horizontal(|ui| {
        search::title(ui, "Loop Decay");
        ui.add(egui::Slider::new(&mut s.loop_recorder_decay, 0.0..=100.0).suffix(" %"));
    })
    .response
    .on_hover_text(tr("How much of the loop fades out on each overdub pass"));
    ui.horizontal(|ui| {
        search::title(ui, "Rec Latency");
        ui.add(egui::Slider::new(&mut s.loop_recorder_latency_ms, 0.0..=100.0).suffix(" ms"));
    })
    .response
    .on_hover_text(tr(
        "With the EXT source: how late its input arrives, so it is recorded on the beat",
    ));
    ui.label(
        RichText::new(crate::i18n::tr(
            "The recorder's panel shows in the global section's left side \
             (Decks Layout ▸ Global section left).",
        ))
        .weak(),
    );
}

const RECORDING_SOURCES: [RecordingSource; 6] = [
    RecordingSource::Internal,
    RecordingSource::Deck(0),
    RecordingSource::Deck(1),
    RecordingSource::Deck(2),
    RecordingSource::Deck(3),
    RecordingSource::Send,
];

fn recording_source_name(source: RecordingSource) -> &'static str {
    match source {
        RecordingSource::Internal => "Internal",
        RecordingSource::Deck(0) => "Input Deck A",
        RecordingSource::Deck(1) => "Input Deck B",
        RecordingSource::Deck(2) => "Input Deck C",
        RecordingSource::Deck(_) => "Input Deck D",
        RecordingSource::Send => "Input FX Send (Ext)",
    }
}

pub(super) fn mix_recorder(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "Source");
    ui.horizontal(|ui| {
        search::title(ui, "Mix Recorder Source:");
        ComboBox::from_id_salt("recording-source")
            .selected_text(tr(recording_source_name(s.recording_source)))
            .show_ui(ui, |ui| {
                for source in RECORDING_SOURCES {
                    ui.selectable_value(
                        &mut s.recording_source,
                        source,
                        tr(recording_source_name(source)),
                    );
                }
            });
    });
    if s.recording_source == RecordingSource::Internal && s.external_mixer.on {
        ui.label(
            RichText::new(tr(
                "With an external mixer the master is not mixed here: choose the input the mixer's record output comes in on.",
            ))
            .weak(),
        );
    }
    ui.add_space(10.0);
    search::heading(ui, "File");
    egui::Grid::new("recorder-file")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            search::title(ui, "Directory");
            ui.horizontal(|ui| {
                ui.label(RichText::new(s.recordings_dir.display().to_string()).monospace());
                if ui.button(tr("Choose…")).clicked()
                    && let Some(dir) = rfd::FileDialog::new()
                        .set_directory(&s.recordings_dir)
                        .pick_folder()
                {
                    s.recordings_dir = dir;
                }
            });
            ui.end_row();
            search::title(ui, "Prefix");
            ui.add(
                egui::TextEdit::singleline(&mut s.recording_prefix)
                    .hint_text("Recording")
                    .desired_width(200.0),
            );
            ui.end_row();
            search::title(ui, "Format");
            ui.horizontal(|ui| {
                for f in crate::recorder::Format::ALL {
                    search::radio(ui, &mut s.recording_format, f, f.name());
                }
            });
            ui.end_row();
            search::title(ui, "Split File at Size");
            ui.add(
                egui::DragValue::new(&mut s.recording_split_mb)
                    .range(1..=4000)
                    .suffix(" MB"),
            );
            ui.end_row();
        });

    ui.label(
        RichText::new(crate::i18n::tr(
            "WAV at the audio device's rate, named <prefix>_<date>_<time>.wav. 24-bit \
             keeps the full quality; 16-bit (dithered) is the smallest; 32-bit float \
             never clips. Recordings join the collection.",
        ))
        .weak(),
    );
}

pub(super) fn external_sync(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "External Clock Source");
    ui.horizontal(|ui| {
        search::radio(ui, &mut s.clock_source_link, true, "Link");
        search::radio(ui, &mut s.clock_source_link, false, "External (MIDI clock)");
    })
    .response
    .on_hover_text(crate::i18n::tr(
        "Link: the master clock panel shows LINK, to share tempo and phase with other apps on \
         the network. External: it shows EXT, to follow a MIDI clock.",
    ));
    ui.add_space(8.0);
    search::heading(ui, "MIDI Clock Settings");
    search::check(ui, &mut s.midi_clock_send, "Enable MIDI Clock").on_hover_text(tr(
        "Extends the master clock panel with START, SYNC and the clock offset.",
    ));
    ui.add_enabled_ui(s.midi_clock_send, |ui| {
        ui.horizontal(|ui| {
            search::title(ui, "MIDI Clock Output");
            let mut entries = vec![crate::search_combo::Entry::item(
                None,
                tr("None"),
                s.midi_clock_output.is_none(),
            )];
            for port in backspn_control::midi::output_ports() {
                let on = s.midi_clock_output.as_deref() == Some(port.as_str());
                entries.push(crate::search_combo::Entry::item(
                    Some(port.clone()),
                    port,
                    on,
                ));
            }
            let current = s.midi_clock_output.clone().unwrap_or_else(|| "None".into());
            if let (_, Some(port)) =
                crate::search_combo::show(ui, "midi-clock-output", current, 220.0, &entries)
            {
                s.midi_clock_output = port;
            }
        });
    });
    ui.label(
        RichText::new(crate::i18n::tr(
            "24 ticks a beat at the master clock's tempo. START sends Start on the next \
             beat; SYNC stops and starts the devices again.",
        ))
        .weak(),
    );
    ui.add_space(8.0);
    ui.label(
        RichText::new(crate::i18n::tr(
            "Receiving MIDI clock: EXT in the master clock panel follows the clock from any \
             Controller Manager In-Port.",
        ))
        .weak(),
    );
}

pub(super) fn ai_control(ui: &mut Ui, s: &mut Settings) {
    search::heading(ui, "AI Control");
    search::check(ui, &mut s.remote_control, "Allow AI control").on_hover_text(tr(
        "An AI assistant on this computer can plan a set from your description and play it, \
         mixing track to track on its own.",
    ));
    ui.add_space(8.0);
    ui.label(
        RichText::new(tr(
            "Track titles, artists and tags from your library are shared with the assistant \
             you connect. Taking over a deck or a fader pauses the set.",
        ))
        .weak(),
    );
    ui.add_space(8.0);
    ui.label(
        RichText::new(tr(
            "To connect an assistant, add the backspn-mcp program to it as an MCP server.",
        ))
        .weak(),
    );
}

fn auto_cue(ui: &mut Ui, s: &mut Settings) {
    ui.horizontal(|ui| {
        search::title(ui, "Auto Cue:");
        for (d, on) in s.auto_cue.iter_mut().enumerate() {
            crate::widgets::check(ui, on, backspn_control::deck_letter(d).to_string());
        }
    });
    ui.horizontal(|ui| {
        search::title(ui, "Auto Cue level:");
        egui::ComboBox::from_id_salt("auto-cue-level")
            .selected_text(format!("{} dB", s.auto_cue_level_db))
            .show_ui(ui, |ui| {
                for db in crate::auto_cue::LEVELS_DB {
                    ui.selectable_value(&mut s.auto_cue_level_db, db, format!("{db} dB"));
                }
            });
    });
    ui.label(
        RichText::new(tr(
            "A loaded track is cued where its sound starts, skipping the silence before it. \
             A stored load marker comes first. Lower levels count quieter sound as the start.",
        ))
        .weak(),
    );
}
