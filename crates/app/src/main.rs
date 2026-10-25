#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod activity;
mod analysis_queue;
mod app;
mod auto_cue;
mod auto_fade;
mod auto_gain;
mod browser;
mod columns;
mod controls;
mod covers;
mod cue_list;
mod deck_informer;
mod deck_view;
mod density;
mod dirs;
mod favorites;
mod fx_view;
mod grid_edit;
mod i18n;
mod icon;
mod jog;
mod key_shift;
mod key_view;
mod keys;
mod kits;
mod layouts;
#[cfg(target_os = "macos")]
mod menu;
mod midi_clock;
mod mixer_cards;
mod mixer_view;
mod modal;
mod moves;
#[cfg(target_os = "macos")]
mod music_api;
mod platter;
mod played;
mod prefs;
mod preview;
mod recorder;
mod repeat;
mod rescan_queue;
mod sample_capture;
mod sample_view;
mod screen;
mod search_combo;
mod selection;
mod set_folder;
mod slip_roll;
mod smartlist_editor;
mod status;
mod stem_jobs;
mod stems;
mod suggest_panel;
mod support;
mod tag_writer;
mod tap_tempo;
mod tempo_bend;
mod theme;
mod toast;
mod track_edit;
mod track_end;
mod track_filter;
mod track_store;
mod waveform;
mod widgets;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use backspn_engine::{Command, DECKS, Engine, EngineState, FX_UNITS};
use eframe::egui::{self, CentralPanel, Panel, Ui};

use app::{BROWSER_MIN, GAP, History};
use browser::Browser;
use controls::Controls;
use deck_view::DeckView;
use fx_view::FxView;
use jog::Jog;
use mixer_view::MixerView;
use prefs::{Prefs, Settings};

pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "m4a", "aac", "mp4", "flac", "wav", "aif", "aiff", "ogg",
];

fn main() -> eframe::Result {
    let previous = support::log::init();
    support::crash::install();
    let crash = support::crash::take_pending(&previous);
    let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Backspn")
        .with_inner_size([1500.0, 860.0])
        .with_min_inner_size([1000.0, 600.0]);
    if let Some(icon) = icon::app_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        persistence_path: Some(prefs::ui_state_path()),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Backspn",
        options,
        Box::new(move |cc| {
            let mut app = App::new();
            if let Some(pending) = crash {
                app.offer_crash_report(pending);
            }
            cc.egui_ctx.set_fonts(theme::fonts());
            theme::apply(&cc.egui_ctx, app.settings.theme.theme());
            theme::set_tooltips(&cc.egui_ctx, app.settings.show_tooltips);
            i18n::set(app.settings.language);
            toast::set_seconds(app.settings.toast_seconds);
            #[cfg(target_os = "macos")]
            match menu::AppMenu::install(&cc.egui_ctx) {
                Ok(m) => app.menu = Some(m),
                Err(e) => toast::error(format!("Cannot install the menu bar: {e}")),
            }
            #[cfg(not(target_os = "macos"))]
            let _ = cc;
            for (deck, path) in files.into_iter().take(DECKS).enumerate() {
                app.open_file(deck, path);
            }
            Ok(Box::new(app))
        }),
    );
    support::log::close();
    result
}

struct App {
    engine: Result<Engine, String>,
    decks: [DeckView; DECKS],
    mixer: MixerView,
    clock_bpm: f64,
    tap: tap_tempo::TapTempo,
    beat_taps: [tap_tempo::BeatTapIn; DECKS],
    epoch: Instant,
    outbox: Vec<Command>,
    browser: Browser,
    set_import: Option<app::SetImport>,
    fx: FxView,
    controls: Option<Controls>,
    #[cfg(target_os = "macos")]
    menu: Option<menu::AppMenu>,
    settings: Settings,
    last_saved: Option<Settings>,
    prefs: Prefs,
    bend_base: [Option<f64>; DECKS],
    auto_fade: Option<f32>,
    auto_gain_sent: [Option<f32>; DECKS],
    external_sent: Option<bool>,
    key_reset_held: [Option<Instant>; DECKS],
    bend_held: [Option<(f64, Instant, f64)>; DECKS],
    slip_rolls: [slip_roll::SlipRoll; DECKS],
    next_screen: Instant,
    jog: [Jog; DECKS],
    platters: [platter::Platter; DECKS],
    link: backspn_link::Link,
    link_tempo: f64,
    link_peers: usize,
    pad_pages: [app::PadPages; DECKS],
    pad_fx: [app::PadFxDeck; DECKS],
    loop_capture: Option<app::LoopCopyFor>,
    motor_decks: [bool; DECKS],
    playing: [bool; DECKS],
    watch: [Option<track_store::Watch>; DECKS],
    decks_bottom: f32,
    play_clock: played::PlayClock,
    browser_maximized: bool,
    focus: backspn_control::focus::Focus,
    browse_mode: app::BrowseMode,
    history: History,
    last_frame: Instant,
    status_synced: Instant,
    audio_missing: bool,
    audio_checked: Instant,
    recorder: recorder::Recorder,
    midi_clock: midi_clock::MidiClock,
    persist: bool,
    quit: app::Quit,
    quit_ask: Option<modal::Ask<()>>,
    audio_offer: app::AudioOffer,
    wizard: Option<app::Wizard>,
    crash: Option<support::crash::Pending>,
    crash_ask: Option<modal::Ask<app::CrashAnswer>>,
    update: support::update::Check,
}

impl App {
    fn new() -> Self {
        let first_run = !Settings::exists();
        let settings = Settings::load();
        let (engine, audio_missing) = app::start_engine(&settings.audio_config());
        let kits = kits::KitLibrary::open(kits::KitLibrary::default_dir());
        let mut browser = Browser::open(settings.music_folders.clone());
        browser.find_programs();
        if settings.import_folders_at_startup {
            browser.import_music_folders();
        }
        let mut app = Self::build(settings, engine, kits, browser, true);
        app.audio_missing = audio_missing;
        app.update = support::update::Check::start();
        if first_run {
            if let Err(e) = app.settings.save() {
                toast::error(format!("Cannot save the settings: {e}"));
            }
            app.start_wizard();
        }
        app
    }

    #[cfg(test)]
    fn offline() -> (Self, backspn_engine::Renderer) {
        let (engine, renderer) = Engine::offline(44_100);
        let mut app = Self::build(
            Settings::default(),
            Ok(engine),
            kits::KitLibrary::none(),
            Browser::in_memory(),
            false,
        );
        app.controls = Some(Controls::detached());
        (app, renderer)
    }

    #[allow(clippy::too_many_lines)]
    fn build(
        settings: Settings,
        engine: Result<Engine, String>,
        kits: kits::KitLibrary,
        mut browser: Browser,
        persist: bool,
    ) -> Self {
        let mut mixer = MixerView::new();
        let mut outbox = mixer.initial_commands();
        mixer.set_curve(settings.xfader_curve, &mut outbox);
        mixer.set_mixer_fx_slots(settings.mixer_fx_slots, &mut outbox);
        let sample_rate = engine.as_ref().map_or(44_100, |e| e.info().sample_rate);
        let mut fx = FxView::new(sample_rate, kits);
        fx.selection.clone_from(&settings.fx_selection);
        fx.restore = settings.fx_restore_params;
        fx.snapshots.clone_from(&settings.fx_snapshots);
        fx.kit_choice.clone_from(&settings.fx_pattern_kits);
        for u in 0..FX_UNITS {
            fx.set_routing(u, settings.fx_routing[u], &mut outbox);
            fx.set_mode(u, settings.fx_panel_mode[u], &mut outbox);
        }
        let mut app = Self {
            engine,
            decks: std::array::from_fn(|i| {
                let mut deck = DeckView::new(i);
                deck.set_zoom(settings.wave_zoom_s);
                deck
            }),
            outbox,
            set_import: None,
            mixer,
            clock_bpm: backspn_engine::DEFAULT_CLOCK_BPM,
            tap: tap_tempo::TapTempo::default(),
            beat_taps: std::array::from_fn(|_| tap_tempo::BeatTapIn::default()),
            epoch: Instant::now(),
            browser: {
                browser.columns.clone_from(&settings.browser_columns);
                browser.favorites.clone_from(&settings.favorites);
                browser.show_favorites = settings.show_favorites;
                browser.set_hidden_nodes(&settings.hidden_tree_nodes);
                browser.show_preview_player = settings.show_preview_player;
                browser.row_height = settings.row_height;
                browser.inplace_edit = settings.list_inplace_edit;
                browser.suggestions.criteria = settings.suggest_criteria;
                browser.set_library_xml(settings.library_xml.clone());
                browser.set_music_source(browser::music_source(
                    settings.music_use_xml,
                    settings.music_xml.clone(),
                ));
                browser
            },
            fx,
            controls: None,
            bend_base: [None; DECKS],
            bend_held: [None; DECKS],
            slip_rolls: [slip_roll::SlipRoll::default(); DECKS],
            key_reset_held: [None; DECKS],
            auto_gain_sent: [None; DECKS],
            external_sent: None,
            auto_fade: None,
            next_screen: Instant::now(),
            jog: [Jog::new(settings.jog_turntable); DECKS],
            platters: [platter::Platter::default(); DECKS],
            link: backspn_link::Link::new(backspn_engine::DEFAULT_CLOCK_BPM),
            link_tempo: 0.0,
            link_peers: 0,
            pad_pages: [app::PadPages::default(); DECKS],
            pad_fx: Default::default(),
            loop_capture: None,
            motor_decks: [false; DECKS],
            playing: [false; DECKS],
            watch: std::array::from_fn(|_| None),
            decks_bottom: 0.0,
            play_clock: played::PlayClock::default(),
            browser_maximized: false,
            focus: backspn_control::focus::Focus::default(),
            browse_mode: app::BrowseMode::List,
            history: History::default(),
            last_frame: Instant::now(),
            status_synced: Instant::now(),
            audio_missing: false,
            audio_checked: Instant::now(),
            #[cfg(target_os = "macos")]
            menu: None,
            last_saved: Some(settings.clone()),
            recorder: {
                let r = recorder::Recorder::default();
                r.set_gain(settings.recording_gain);
                r
            },
            midi_clock: midi_clock::MidiClock::default(),
            settings,
            prefs: Prefs::new(),
            persist,
            quit: app::Quit::Free,
            quit_ask: None,
            audio_offer: app::AudioOffer::default(),
            wizard: None,
            crash: None,
            crash_ask: None,
            update: support::update::Check::none(),
        };
        for (deck, &range) in app.decks.iter_mut().zip(&app.settings.deck_tempo_ranges) {
            deck.set_tempo_range(range);
        }
        for (deck, &size) in app.decks.iter_mut().zip(&app.settings.deck_sizes) {
            deck.set_size(size);
        }
        for (pages, &page) in app.pad_pages.iter_mut().zip(&app.settings.beat_jump_pages) {
            pages.beat_jump = page.clamp(1, 9);
        }
        let fx_commands = app.fx.initial_commands();
        app.outbox.extend(fx_commands);
        app.restore_status();
        app.send_deck_settings();
        app.send_mixer_settings();
        app.apply_midi_clock(true);
        app
    }

    fn flush(&mut self) {
        let Ok(engine) = &mut self.engine else {
            self.outbox.clear();
            return;
        };
        for command in self.outbox.drain(..) {
            if engine.controller().send(command).is_err() {
                log::warn!("command queue full, command dropped");
            }
        }
    }

    fn engine_state(&mut self) -> EngineState {
        match &mut self.engine {
            Ok(engine) => engine.controller().state(),
            Err(_) => EngineState::default(),
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.finish_recording();
        self.close_history();
        if self.persist {
            self.save_status();
            self.browser.backup();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }
}

impl App {
    fn browser_preferences(&mut self) {
        self.browser.set_analysis_defaults(
            self.settings.analysis_options().bpm_range,
            self.settings.auto_analyze_import,
        );
        self.browser.set_stem_dirs(crate::stems::StemDirs {
            own: self.settings.stems_dir.clone(),
            theirs: self.settings.foreign_stems_dir.clone(),
        });
    }

    #[allow(clippy::too_many_lines)]
    fn frame(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.settings.wave_style(&theme::get(&ctx)).set(&ctx);
        let mut events = Vec::new();
        for deck in 0..DECKS {
            events.extend(self.decks[deck].poll(&mut self.outbox));
            if let Some(path) = self.decks[deck].pending_open.take() {
                self.open_file(deck, path);
            }
            if let Some(track) = self.decks[deck].pending_track.take() {
                self.load_from_library(deck, track);
            }
            if std::mem::take(&mut self.decks[deck].pending_save_loop) {
                self.save_loop_from(sample_view::LoopDrag::Deck(deck));
            }
        }
        self.poll_sample_deck();
        self.handle_deck_events(events);
        self.handle_input(&ctx);
        self.flush();
        let mut loop_copy = None;
        if let Ok(engine) = &mut self.engine {
            engine.controller().collect_garbage();
            loop_copy = engine.controller().take_loop_copy();
        }
        if let Some(copy) = loop_copy {
            self.finish_loop_capture(&copy);
        }
        let state = self.engine_state();
        self.playing = std::array::from_fn(|d| state.decks[d].playing);
        for (deck, playing) in self.decks.iter_mut().zip(self.playing) {
            if !playing {
                deck.clear_notice(deck_informer::Key::DeckLocked);
            }
        }
        self.handle_controls(&ctx, &state);
        self.guard_quit(&ctx, &state);
        self.offer_audio(&ctx);
        self.sample_save_dialogs(&ctx);
        self.set_import_dialog(&ctx);
        self.poll_recorder();

        let t = theme::get(&ctx);
        let bar = egui::Frame::new()
            .fill(t.surface)
            .inner_margin(egui::Margin::symmetric(12, 6));
        Panel::top("top").frame(bar).show(ui, |ui| self.top_bar(ui));
        let status_height = Panel::bottom("status")
            .frame(bar.inner_margin(egui::Margin::symmetric(12, 3)))
            .show(ui, |ui| self.status_row(ui))
            .response
            .rect
            .height();
        let room = ui.available_rect_before_wrap();
        let mut browser_height = 0.0;
        if self.settings.show_browser {
            let playing = std::array::from_fn(|d| state.decks[d].playing);
            let browser = if self.browser_maximized {
                room.height()
            } else {
                (room.bottom() - self.decks_bottom - GAP).max(BROWSER_MIN)
            };
            browser_height = browser;
            let master = self.suggestion_master(&state);
            self.browser
                .set_loaded(self.decks.iter().filter_map(DeckView::track_id).collect());
            self.browser.set_sample_decks(std::array::from_fn(|d| {
                self.decks.get(d).is_some_and(|v| v.sample_deck.is_some())
            }));
            let keys = self.key_view(&state);
            for deck in &mut self.decks {
                deck.set_key_notation(keys.notation);
            }
            self.browser.set_key_view(keys);
            self.browser.set_preview_state(&state.preview);
            self.browser_preferences();
            let actions = Panel::bottom("browser")
                .frame(
                    egui::Frame::new()
                        .fill(t.surface)
                        .inner_margin(egui::Margin::symmetric(10, 8)),
                )
                .resizable(false)
                .exact_size(browser)
                .show(ui, |ui| {
                    self.browser
                        .ui(ui, (self.settings.deck_count, playing), master)
                })
                .inner;
            self.handle_browser_actions(actions);
        }
        let maximized = self.settings.show_browser && self.browser_maximized;
        CentralPanel::default()
            .frame(egui::Frame::new())
            .show(ui, |ui| {
                if maximized {
                    return;
                }
                let visible = (room.bottom() - browser_height - GAP - ui.cursor().top()).max(80.0);
                let clip = egui::Rect::from_min_size(
                    ui.cursor().min,
                    egui::vec2(ui.available_width(), visible),
                );
                ui.scope(|ui| {
                    ui.set_clip_rect(clip.intersect(ui.clip_rect()));
                    self.global_row(ui, &state);
                    app::seam(ui);
                    self.decks_and_mixer(ui, &state);
                    self.decks_bottom = ui.min_rect().bottom();
                });
            });
        self.background_work(ui.ctx(), &state);
        self.track_played_state(&state);
        self.preferences(&ctx);
        self.setup_wizard(&ctx);
        self.crash_prompt(&ctx);
        if let Some(message) = self.browser.take_message() {
            toast::info(message);
        }
        toast::show(&ctx, status_height);
        self.flush();
        self.flush();

        let busy =
            state.decks.iter().any(|d| d.playing) || self.decks.iter().any(DeckView::is_busy);
        if busy {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_audio_extensions() {
        assert!(is_audio(Path::new("a/b.FLAC")));
        assert!(!is_audio(Path::new("a/b.txt")));
        assert!(!is_audio(Path::new("noext")));
    }
}
