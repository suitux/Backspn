use backspn_engine::loop_recorder::{Command as LoopRecorderCommand, Source};
use backspn_engine::{
    Command, DECKS, DeckCommand, Engine, FX_UNITS, KeylockRange, MixerCommand, SyncCommand,
};
use eframe::egui;

use crate::deck_view::DeckView;
#[cfg(target_os = "macos")]
use crate::menu;
use crate::prefs::Settings;
use crate::status::DeckType;
use crate::{App, theme};

impl App {
    fn sync_browser_settings(&mut self) {
        self.settings.suggest_criteria = self.browser.suggestions.criteria;
        if self.settings.favorites != self.browser.favorites {
            self.settings.favorites.clone_from(&self.browser.favorites);
        }
        if self.settings.browser_columns != self.browser.columns {
            self.settings
                .browser_columns
                .clone_from(&self.browser.columns);
        }
        self.browser.show_favorites = self.settings.show_favorites;
        self.browser
            .set_hidden_nodes(&self.settings.hidden_tree_nodes);
        self.browser.show_preview_player = self.settings.show_preview_player;
        self.browser.row_height = self.settings.row_height;
        self.browser.inplace_edit = self.settings.list_inplace_edit;
        self.browser.set_tag_writing(
            self.settings.tags_serialization == 2,
            backspn_formats::nml::KeyNotation::from_value(self.settings.key_notation_exported),
        );
    }

    fn sync_fx_settings(&mut self) {
        self.settings.fx_panel_mode = self.fx.modes();
        self.settings.fx_pattern_kits = self.fx.kit_ids();
        if self.settings.fx_snapshots != self.fx.snapshots {
            self.settings.fx_snapshots.clone_from(&self.fx.snapshots);
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn preferences(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)) {
            self.prefs.toggle();
        }
        self.settings.deck_tempo_ranges = std::array::from_fn(|d| self.decks[d].tempo_range());
        self.settings.deck_sizes = std::array::from_fn(|d| self.decks[d].size());
        self.settings.beat_jump_pages = std::array::from_fn(|d| self.pad_pages[d].beat_jump);
        self.settings.xfader_curve = self.mixer.curve();
        self.sync_fx_settings();
        self.sync_browser_settings();
        let before = self.settings.clone();
        #[cfg(target_os = "macos")]
        if let Some(menu) = &self.menu {
            for command in menu.commands() {
                match command {
                    menu::MenuCommand::Preferences => self.prefs.toggle(),
                    menu::MenuCommand::SetupWizard => {
                        self.wizard = Some(crate::app::Wizard::default());
                    }
                    menu::MenuCommand::ImportNml => self.browser.import_nml(),
                    menu::MenuCommand::ImportLibraryXml => self.browser.import_library_xml(),
                    menu::MenuCommand::ImportCollection => self.browser.import_collection(),
                    menu::MenuCommand::ExportCollection => self.browser.export_collection(),
                    menu::MenuCommand::ToggleBrowser => {
                        self.settings.show_browser = !self.settings.show_browser;
                    }
                    menu::MenuCommand::ToggleFx => self.settings.show_fx = !self.settings.show_fx,
                    menu::MenuCommand::Decks(count) => self.settings.deck_count = count,
                    menu::MenuCommand::Theme(id) => self.settings.theme = id,
                    menu::MenuCommand::ReportProblem => Self::report_problem(),
                    menu::MenuCommand::ShowLogFile => Self::show_log_file(),
                }
            }
            menu.sync(
                self.settings.show_browser,
                self.settings.show_fx,
                self.settings.deck_count,
                self.settings.theme,
            );
        }
        let mut restart = false;
        if self.prefs.open {
            self.prefs.deck_setup = self.deck_setup();
        }
        if self.prefs.open {
            self.prefs.insights_status = Some(self.insights_status());
        }
        if self.prefs.open
            && let Some(controls) = &mut self.controls
        {
            let info = self.engine.as_ref().ok().map(|e| e.info().clone());
            if let Ok(engine) = &mut self.engine {
                self.prefs.output_latency = engine.controller().stats().output_latency;
            }
            let changes = self.prefs.window(
                ctx,
                &mut self.settings,
                info.as_ref(),
                controls,
                &mut self.fx.kits,
            );
            restart = changes.restart_audio;
            self.apply_deck_types(changes.deck_types);
            if let Some(i) = changes.load_layout {
                self.apply_layout(i);
            }
            match changes.global {
                Some(crate::prefs::Global::SetupWizard) => self.start_wizard(),
                Some(crate::prefs::Global::ReportProblem) => Self::report_problem(),
                Some(crate::prefs::Global::ShowLogFile) => Self::show_log_file(),
                Some(crate::prefs::Global::ResetSettings) => self.ask_reset(),
                Some(crate::prefs::Global::InsightsPreview) => self.preview_insights(),
                Some(crate::prefs::Global::InsightsSend) => self.send_insights(),
                None => {}
            }
            self.apply_kit_edits();
        }
        let s = self.settings.clone();
        if (s.music_use_xml, &s.music_xml) != (before.music_use_xml, &before.music_xml) {
            self.browser.set_music_source(crate::browser::music_source(
                s.music_use_xml,
                s.music_xml.clone(),
            ));
        }
        if (s.xfader_curve - before.xfader_curve).abs() > f32::EPSILON {
            self.mixer.set_curve(s.xfader_curve, &mut self.outbox);
        }
        self.mixer
            .set_mixer_fx_slots(s.mixer_fx_slots, &mut self.outbox);
        for u in 0..FX_UNITS {
            self.fx.set_routing(u, s.fx_routing[u], &mut self.outbox);
            self.fx.set_mode(u, s.fx_panel_mode[u], &mut self.outbox);
        }
        if s.browser_columns != before.browser_columns {
            self.browser.columns.clone_from(&s.browser_columns);
        }
        if s.fx_selection != before.fx_selection {
            self.fx.selection.clone_from(&s.fx_selection);
        }
        self.fx.restore = s.fx_restore_params;
        self.apply_deck_views(&before, &s);
        if s.jog_turntable != before.jog_turntable {
            for jog in &mut self.jog {
                jog.turntable = s.jog_turntable;
            }
        }
        if deck_settings_changed(&before, &s) {
            self.send_deck_settings();
        }
        if (s.loop_recorder_decay - before.loop_recorder_decay).abs() > f32::EPSILON
            || (s.loop_recorder_latency_ms - before.loop_recorder_latency_ms).abs() > f32::EPSILON
        {
            self.send_loop_recorder_settings();
        }
        if s.input_routing != before.input_routing
            || s.recording_source != before.recording_source
            || (s.microphone_gain - before.microphone_gain).abs() > f32::EPSILON
        {
            self.send_inputs();
        }
        if sample_settings_changed(&before, &s) {
            self.send_sample_settings();
        }
        if mixer_settings_changed(&before, &s) {
            self.send_mixer_settings();
        }
        if midi_clock_changed(&before, &s) {
            self.apply_midi_clock(false);
        }
        if s.music_folders != before.music_folders {
            self.browser.set_folders(s.music_folders.clone());
        }
        if s.language != before.language {
            crate::i18n::set(s.language);
            #[cfg(target_os = "macos")]
            if let Some(menu) = &self.menu {
                menu.retitle();
            }
        }
        crate::toast::set_seconds(s.toast_seconds);
        if s.show_tooltips != before.show_tooltips {
            theme::set_tooltips(ctx, s.show_tooltips);
        }
        if s.theme != before.theme {
            theme::apply(ctx, s.theme.theme());
        }
        if (s.wave_zoom_s - before.wave_zoom_s).abs() > f64::EPSILON {
            for deck in &mut self.decks {
                deck.set_zoom(s.wave_zoom_s);
            }
        }
        if restart {
            self.change_audio();
        }
        if self.persist && s != self.saved_settings() {
            if let Err(e) = s.save() {
                crate::toast::error(format!("Cannot save the settings: {e}"));
            }
            self.last_saved = Some(s);
        }
    }

    fn apply_kit_edits(&mut self) {
        if let Some(track) = self.prefs.preview.take() {
            self.outbox.extend(
                [
                    DeckCommand::Load(std::sync::Arc::new(track)),
                    DeckCommand::Play,
                ]
                .map(Command::Preview),
            );
        }
        self.fx.refresh_kits(&mut self.outbox);
    }

    fn saved_settings(&self) -> Settings {
        self.last_saved.clone().unwrap_or_else(Settings::load)
    }

    pub(crate) fn watch_audio(&mut self, ctx: &eframe::egui::Context) {
        let waiting = match &self.engine {
            Ok(engine) => engine.lost(),
            Err(_) => self.audio_missing,
        };
        if !waiting {
            return;
        }
        ctx.request_repaint_after(AUDIO_CHECK);
        if self.audio_checked.elapsed() < AUDIO_CHECK {
            return;
        }
        self.audio_checked = std::time::Instant::now();
        let config = self.settings.audio_config();
        if !backspn_engine::device_present(
            config.device.as_deref(),
            &backspn_engine::output_devices(),
        ) {
            return;
        }
        if let Ok(engine) = &mut self.engine {
            match engine.reopen(&config) {
                Ok(()) => return,
                Err(e) => log::warn!("audio device back, cannot reopen it: {e}"),
            }
        }
        self.restart_audio();
    }

    pub(crate) fn change_audio(&mut self) {
        let config = self.settings.audio_config();
        if let Ok(engine) = &mut self.engine
            && config
                .sample_rate
                .is_none_or(|r| r == engine.info().sample_rate)
        {
            match engine.reopen(&config) {
                Ok(()) => return,
                Err(e) => log::warn!("cannot move the audio to the new device: {e}"),
            }
        }
        self.restart_audio();
    }

    fn restart_audio(&mut self) {
        self.finish_recording();
        self.engine = Err("restarting".into());
        (self.engine, self.audio_missing) = start_engine(&self.settings.audio_config());
        self.decks = std::array::from_fn(DeckView::new);
        for deck in &mut self.decks {
            deck.set_zoom(self.settings.wave_zoom_s);
        }
        for (deck, &range) in self.decks.iter_mut().zip(&self.settings.deck_tempo_ranges) {
            deck.set_tempo_range(range);
        }
        for (deck, &size) in self.decks.iter_mut().zip(&self.settings.deck_sizes) {
            deck.set_size(size);
        }
        self.watch = std::array::from_fn(|_| None);
        self.bend_base = [None; DECKS];
        self.bend_held = [None; DECKS];
        self.auto_gain_sent = [None; DECKS];
        self.external_sent = None;
        self.pad_fx = Default::default();
        self.outbox = self.mixer.all_commands();
        self.outbox.extend(self.fx.all_commands());
        self.send_deck_settings();
        self.send_mixer_settings();
        self.apply_midi_clock(true);
        self.send_inputs();
        self.send_loop_recorder_settings();
    }

    pub(crate) fn apply_midi_clock(&mut self, fresh: bool) {
        let s = &self.settings;
        self.midi_clock.configure(
            s.midi_clock_send,
            s.midi_clock_output.as_deref(),
            s.midi_clock_offset_ms,
            fresh,
            &mut self.outbox,
        );
    }

    pub(crate) fn send_mixer_settings(&mut self) {
        self.mixer.four_band = self.settings.eq_model == backspn_engine::EqModel::FourBand;
        let (headroom_db, on_meters) = self.settings.headroom();
        self.mixer.meter_gain = crate::mixer_view::channel_meter_gain(headroom_db, on_meters);
        let external = self.settings.external_mixer;
        self.mixer.set_lock(external.lock(), &mut self.outbox);
        self.outbox.push(Command::Outputs(external.outputs()));
        if self.external_sent != Some(external.on) {
            self.external_sent = Some(external.on);
            let source = if external.on {
                Source::Ext
            } else {
                Source::Main
            };
            self.outbox
                .push(Command::LoopRecorder(LoopRecorderCommand::Source(source)));
        }
        let s = &self.settings;
        self.outbox.extend(
            [
                MixerCommand::FilterModel(s.filter_model),
                MixerCommand::FilterResonance(s.filter_resonance),
                MixerCommand::MasterMono(s.master_mono),
                MixerCommand::MonitorMono(s.monitor_mono),
                MixerCommand::Headroom(headroom_db),
                MixerCommand::EqModel(s.eq_model),
                MixerCommand::LimiterModel(s.limiter_model),
            ]
            .map(Command::Mixer),
        );
    }

    pub(crate) fn apply_deck_types(&mut self, picked: [Option<DeckType>; DECKS]) {
        if picked.iter().all(Option::is_none) {
            return;
        }
        let state = self.engine_state();
        for (d, deck_type) in picked.into_iter().enumerate() {
            if let Some(deck_type) = deck_type {
                self.set_deck_type(d, deck_type, &state);
            }
        }
        self.settings.status = self.current_status(&state);
    }

    fn apply_deck_views(&mut self, before: &Settings, s: &Settings) {
        if s.deck_sizes != before.deck_sizes {
            for (deck, &size) in self.decks.iter_mut().zip(&s.deck_sizes) {
                deck.set_size(size);
            }
        }
        if (s.tempo_range - before.tempo_range).abs() > f32::EPSILON {
            for deck in &mut self.decks {
                deck.set_tempo_range(s.tempo_range);
            }
        }
    }

    pub(crate) fn send_deck_settings(&mut self) {
        self.outbox.extend([
            Command::Sync(SyncCommand::SetAutoMaster(self.settings.auto_master)),
            Command::Sync(SyncCommand::SetOnAirOnly(self.settings.on_air_master)),
            Command::Sync(SyncCommand::SetSampleDecksLead(
                self.settings.sample_decks_master,
            )),
        ]);
        let (snap, quantize) = (self.settings.snap, self.settings.quantize);
        let range = if self.settings.keylock_scratch_range {
            KeylockRange::Scratch
        } else {
            KeylockRange::Normal
        };
        for deck in 0..DECKS {
            self.outbox
                .push(Command::Deck(deck, DeckCommand::SetSnap(snap)));
            self.outbox
                .push(Command::Deck(deck, DeckCommand::SetQuantize(quantize)));
            self.outbox
                .push(Command::Deck(deck, DeckCommand::SetKeylockRange(range)));
            self.outbox.push(Command::Deck(
                deck,
                DeckCommand::SetSlipPause(self.settings.slip_pause),
            ));
        }
    }
}

fn sample_settings_changed(before: &Settings, now: &Settings) -> bool {
    now.sample_auto_play != before.sample_auto_play
        || now.sample_one_shot_ignores_punch != before.sample_one_shot_ignores_punch
        || now.sample_one_shot_ignores_quantize != before.sample_one_shot_ignores_quantize
}

fn deck_settings_changed(before: &Settings, now: &Settings) -> bool {
    now.snap != before.snap
        || now.quantize != before.quantize
        || now.keylock_scratch_range != before.keylock_scratch_range
        || now.slip_pause != before.slip_pause
        || now.auto_master != before.auto_master
        || now.on_air_master != before.on_air_master
        || now.sample_decks_master != before.sample_decks_master
}

fn midi_clock_changed(before: &Settings, now: &Settings) -> bool {
    now.midi_clock_send != before.midi_clock_send
        || now.midi_clock_output != before.midi_clock_output
}

fn mixer_settings_changed(before: &Settings, s: &Settings) -> bool {
    s.filter_model != before.filter_model
        || (s.filter_resonance - before.filter_resonance).abs() > f32::EPSILON
        || s.master_mono != before.master_mono
        || s.monitor_mono != before.monitor_mono
        || (s.headroom_db - before.headroom_db).abs() > f32::EPSILON
        || s.headroom_on_meters != before.headroom_on_meters
        || s.external_mixer != before.external_mixer
        || s.eq_model != before.eq_model
        || s.limiter_model != before.limiter_model
}

const AUDIO_CHECK: std::time::Duration = std::time::Duration::from_secs(2);

pub(crate) fn start_engine(config: &backspn_engine::Config) -> (Result<Engine, String>, bool) {
    match Engine::start(config) {
        Ok(engine) => (Ok(engine), false),
        Err(e) => {
            log::error!("audio engine failed to start: {e}");
            (Err(e.to_string()), e.device_missing())
        }
    }
}
