use std::path::PathBuf;

use backspn_engine::{
    Config, DECKS, EffectKind, EqModel, FX_UNITS, FilterModel, FxMode, FxRouting, LimiterModel,
    MIXER_FX_SLOTS, MixerFx,
};
use serde::{Deserialize, Serialize};

use crate::columns::ColumnLayout;
use crate::theme::ThemeId;
use crate::waveform::{WaveMode, WavePalette, WaveStyle};

pub(crate) fn settings_path() -> PathBuf {
    if let Some(path) = std::env::var_os("BACKSPN_SETTINGS") {
        return PathBuf::from(path);
    }
    crate::dirs::data().join("settings.json")
}

pub fn stem_model_path() -> PathBuf {
    settings_path()
        .with_file_name("models")
        .join(backspn_separate::MODEL_FILE)
}

pub fn ui_state_path() -> PathBuf {
    settings_path().with_file_name("ui-state.ron")
}

fn default_recordings_dir() -> PathBuf {
    crate::dirs::music().join("Recordings")
}

pub fn keep_backup(path: &std::path::Path) {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    if let Err(e) = std::fs::copy(path, &backup) {
        log::warn!("cannot back up {}: {e}", path.display());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InputRouting {
    pub decks: [Option<(u16, u16)>; DECKS],
    pub microphone: Option<(u16, u16)>,
    #[serde(default)]
    pub send: Option<(u16, u16)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ExternalMixer {
    pub on: bool,
    pub decks: [Option<(u16, u16)>; DECKS],
    pub preview: Option<(u16, u16)>,
    pub fx_return: Option<(u16, u16)>,
    pub headroom_db: f32,
    pub headroom_on_meters: bool,
    pub auto_gain: bool,
    pub master_volume_control: bool,
    pub eq_gain_control: bool,
    pub fx_control: bool,
}

impl Default for ExternalMixer {
    fn default() -> Self {
        Self {
            on: false,
            decks: [None; DECKS],
            preview: None,
            fx_return: None,
            headroom_db: 0.0,
            headroom_on_meters: false,
            auto_gain: true,
            master_volume_control: false,
            eq_gain_control: false,
            fx_control: false,
        }
    }
}

impl ExternalMixer {
    pub fn outputs(&self) -> backspn_engine::Outputs {
        if !self.on {
            return backspn_engine::Outputs::Internal;
        }
        backspn_engine::Outputs::External(backspn_engine::ExternalOutputs {
            decks: self.decks,
            preview: self.preview,
            fx_return: self.fx_return,
        })
    }

    pub fn lock(&self) -> crate::mixer_view::MixerLock {
        crate::mixer_view::MixerLock {
            channel: self.on,
            eq_gain: self.on && !self.eq_gain_control,
            master: self.on && !self.master_volume_control,
            fx: self.on && !self.fx_control,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DeviceAudio {
    pub buffer_frames: Option<u32>,
    pub monitor: Monitor,
    #[serde(default)]
    pub routing: Option<DeviceRouting>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceRouting {
    pub sample_rate: Option<u32>,
    pub external: bool,
    pub decks: [Option<(u16, u16)>; DECKS],
    pub preview: Option<(u16, u16)>,
    pub fx_return: Option<(u16, u16)>,
    pub inputs: InputRouting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RecordingSource {
    #[default]
    Internal,
    Deck(usize),
    Send,
}

impl RecordingSource {
    pub fn engine(self, inputs: &InputRouting) -> backspn_engine::RecordSource {
        use backspn_engine::RecordSource::{Input, Master};
        match self {
            Self::Internal => Master,
            Self::Deck(d) => Input(inputs.decks.get(d).copied().flatten()),
            Self::Send => Input(inputs.send),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Monitor {
    #[default]
    Auto,
    None,
    Channels(u16, u16),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub device: Option<String>,
    pub buffer_frames: Option<u32>,
    pub sample_rate: Option<u32>,
    pub monitor: Monitor,
    pub master_mono: bool,
    pub monitor_mono: bool,
    pub eq_model: EqModel,
    pub headroom_db: f32,
    pub headroom_on_meters: bool,
    pub limiter_model: LimiterModel,
    pub tempo_range: f32,
    pub deck_tempo_ranges: [f32; DECKS],
    pub deck_sizes: [crate::deck_view::DeckSize; DECKS],
    pub beat_jump_pages: [u8; DECKS],
    pub screen_layout: crate::screen::Layout,
    pub screen_idle_logo: bool,
    pub screen_quit_logo: bool,
    pub screen_logo: Option<PathBuf>,
    pub input_routing: InputRouting,
    pub external_mixer: ExternalMixer,
    pub microphone_gain: f32,
    pub status: crate::status::Status,
    pub bend_sensitivity: f64,
    pub bend_progressive: bool,
    pub keylock_scratch_range: bool,
    #[serde(alias = "flux_pause")]
    pub slip_pause: bool,
    pub auto_master: bool,
    pub clock_source_link: bool,
    pub on_air_master: bool,
    pub sample_decks_master: bool,
    pub auto_gain: bool,
    pub end_warning_s: f64,
    pub auto_fade_s: f64,
    pub jog_seconds_per_turn: f64,
    pub jog_sensitivity: f64,
    pub jog_nudge_sensitivity: f64,
    pub jog_tension: u8,
    pub mixer_fx_slots: [MixerFx; 4],
    pub jog_turntable: bool,
    pub tempo_fader_relative: bool,
    pub platter_45: bool,
    pub played_state_delay: f64,
    pub deck_count: usize,
    pub show_fx: bool,
    pub global_left_loop_recorder: bool,
    pub loop_recorder_decay: f32,
    pub loop_recorder_latency_ms: f32,
    pub xfader_curve: f32,
    pub show_crossfader: bool,
    pub filter_model: FilterModel,
    pub filter_resonance: f32,
    pub fx_units: usize,
    pub fx_routing: [FxRouting; FX_UNITS],
    pub fx_panel_mode: [FxMode; FX_UNITS],
    pub fx_pattern_kits: [String; FX_UNITS],
    pub recording_source: RecordingSource,
    pub recordings_dir: PathBuf,
    pub recording_prefix: String,
    pub recording_split_mb: u32,
    pub recording_gain: f32,
    pub recording_format: crate::recorder::Format,
    pub midi_clock_send: bool,
    pub midi_clock_output: Option<String>,
    pub audio_switch: std::collections::BTreeMap<String, bool>,
    pub inactive_devices: std::collections::BTreeMap<String, DeviceAudio>,
    pub midi_clock_offset_ms: i32,
    pub stems_default_load: crate::stems::DefaultLoad,
    #[serde(alias = "remix_auto_play")]
    pub sample_auto_play: bool,
    #[serde(alias = "remix_one_shot_ignores_punch")]
    pub sample_one_shot_ignores_punch: bool,
    #[serde(alias = "remix_one_shot_ignores_quantize")]
    pub sample_one_shot_ignores_quantize: bool,
    #[serde(alias = "remix_auto_gain")]
    pub sample_auto_gain: bool,
    #[serde(alias = "remix_autosave_sets")]
    pub sample_autosave_sets: bool,
    pub samples_dir: PathBuf,
    pub sets_dir: PathBuf,
    pub stems_dir: PathBuf,
    #[serde(alias = "reference_stems_dir")]
    pub foreign_stems_dir: Option<PathBuf>,
    pub fx_restore_params: bool,
    #[serde(default)]
    pub fx_snapshots: std::collections::HashMap<EffectKind, crate::fx_view::Snapshot>,
    pub fx_selection: Vec<EffectKind>,
    #[serde(default = "first_catalogue")]
    pub fx_catalogue: Vec<EffectKind>,
    pub show_browser: bool,
    pub layouts: Vec<crate::layouts::SavedLayout>,
    pub browser_columns: ColumnLayout,
    pub suggest_criteria: backspn_library::suggest::Criteria,
    /// Lets an AI plan and play sets through the local socket.
    pub remote_control: bool,
    pub snap: bool,
    pub quantize: bool,
    pub load_only_stopped: bool,
    pub duplicate_same_track: bool,
    pub cue_to_marker: bool,
    pub auto_cue: [bool; DECKS],
    pub auto_cue_level_db: i32,
    pub music_folders: Vec<PathBuf>,
    pub tags_serialization: u8,
    pub auto_analyze_import: bool,
    pub auto_analyze_on_load: bool,
    pub bpm_range_mode: u8,
    pub key_notation: u8,
    pub key_notation_exported: u8,
    pub key_highlight: crate::key_view::Highlight,
    pub key_reference_focus: bool,
    pub row_height: f32,
    pub list_inplace_edit: bool,
    pub library_xml: Option<PathBuf>,
    pub music_use_xml: bool,
    pub music_xml: Option<PathBuf>,
    pub import_folders_at_startup: bool,
    pub factory_kits_installed: bool,
    pub hidden_tree_nodes: Vec<crate::browser::TreeNode>,
    pub favorites: Vec<String>,
    pub show_favorites: bool,
    pub show_preview_player: bool,
    pub show_tooltips: bool,
    pub language: crate::i18n::Language,
    pub toast_seconds: [f32; 3],
    pub dismissed_version: Option<String>,
    pub insights_enabled: bool,
    pub insights_notice_seen: bool,
    pub theme: ThemeId,
    pub wave_mode: WaveMode,
    pub wave_palette: WavePalette,
    pub wave_custom: [[u8; 3]; 3],
    pub wave_zoom_s: f64,
}

impl Default for Settings {
    #[allow(clippy::too_many_lines)]
    fn default() -> Self {
        Self {
            device: None,
            buffer_frames: None,
            sample_rate: None,
            monitor: Monitor::Auto,
            master_mono: false,
            monitor_mono: false,
            eq_model: EqModel::WideIsolator,
            headroom_db: backspn_engine::DEFAULT_HEADROOM_DB,
            headroom_on_meters: false,
            limiter_model: LimiterModel::Transparent,
            tempo_range: 0.08,
            deck_tempo_ranges: [0.08; DECKS],
            deck_sizes: [crate::deck_view::DeckSize::Full; DECKS],
            beat_jump_pages: [backspn_control::pads::BEAT_JUMP_PAGE_START; DECKS],
            screen_layout: crate::screen::Layout::Standard,
            screen_idle_logo: true,
            screen_quit_logo: true,
            screen_logo: None,
            input_routing: InputRouting::default(),
            external_mixer: ExternalMixer::default(),
            microphone_gain: 1.0,
            status: crate::status::Status::default(),
            bend_sensitivity: 100.0,
            bend_progressive: false,
            keylock_scratch_range: true,
            slip_pause: false,
            auto_master: true,
            clock_source_link: true,
            on_air_master: false,
            sample_decks_master: true,
            auto_gain: true,
            end_warning_s: crate::track_end::DEFAULT_WARNING_S,
            auto_fade_s: crate::auto_fade::DEFAULT_TIME_S,
            jog_seconds_per_turn: 1.8,
            jog_sensitivity: 0.5,
            jog_nudge_sensitivity: 0.5,
            jog_tension: 50,
            mixer_fx_slots: MIXER_FX_SLOTS,
            jog_turntable: false,
            tempo_fader_relative: true,
            platter_45: false,
            played_state_delay: 10.0,
            deck_count: 2,
            show_fx: true,
            global_left_loop_recorder: false,
            loop_recorder_decay: 0.0,
            loop_recorder_latency_ms: 0.0,
            xfader_curve: 0.0,
            show_crossfader: true,
            filter_model: FilterModel::Ladder,
            filter_resonance: 0.5,
            fx_units: 2,
            fx_routing: [FxRouting::Insert; FX_UNITS],
            fx_panel_mode: [FxMode::Group; FX_UNITS],
            fx_pattern_kits: std::array::from_fn(|_| String::new()),
            recordings_dir: default_recordings_dir(),
            recording_prefix: String::new(),
            recording_split_mb: 650,
            recording_gain: 0.5,
            recording_source: RecordingSource::default(),
            recording_format: crate::recorder::Format::default(),
            midi_clock_send: false,
            midi_clock_output: None,
            audio_switch: std::collections::BTreeMap::new(),
            inactive_devices: std::collections::BTreeMap::new(),
            midi_clock_offset_ms: 0,
            stems_default_load: crate::stems::DefaultLoad::default(),
            sample_auto_play: true,
            sample_one_shot_ignores_punch: true,
            sample_one_shot_ignores_quantize: true,
            sample_auto_gain: false,
            sample_autosave_sets: false,
            samples_dir: crate::dirs::music().join("Samples"),
            sets_dir: crate::dirs::music().join("Sample Sets"),
            stems_dir: crate::stems::default_dir(),
            foreign_stems_dir: Some(crate::stems::default_foreign_dir()),
            fx_restore_params: false,
            fx_snapshots: std::collections::HashMap::new(),
            fx_selection: default_fx_selection(),
            fx_catalogue: default_fx_selection(),
            show_browser: true,
            layouts: Vec::new(),
            browser_columns: ColumnLayout::default(),
            suggest_criteria: backspn_library::suggest::Criteria::default(),
            remote_control: false,
            snap: true,
            quantize: true,
            load_only_stopped: true,
            duplicate_same_track: true,
            cue_to_marker: true,
            auto_cue: [true; DECKS],
            auto_cue_level_db: crate::auto_cue::DEFAULT_LEVEL_DB,
            music_folders: Vec::new(),
            tags_serialization: 2,
            auto_analyze_import: true,
            auto_analyze_on_load: true,
            bpm_range_mode: 9,
            key_notation: 2,
            key_notation_exported: 2,
            key_highlight: crate::key_view::Highlight::default(),
            key_reference_focus: false,
            row_height: 24.0,
            list_inplace_edit: true,
            library_xml: None,
            music_use_xml: false,
            music_xml: None,
            import_folders_at_startup: true,
            factory_kits_installed: false,
            hidden_tree_nodes: Vec::new(),
            favorites: crate::favorites::defaults(),
            show_favorites: true,
            show_preview_player: true,
            show_tooltips: true,
            language: crate::i18n::Language::default(),
            toast_seconds: crate::toast::DEFAULT_SECONDS,
            dismissed_version: None,
            insights_enabled: true,
            insights_notice_seen: false,
            theme: ThemeId::default(),
            wave_mode: WaveMode::default(),
            wave_palette: WavePalette::default(),
            wave_custom: [[0x2D, 0x6C, 0xFF], [0xFF, 0x9A, 0x2E], [0xF4, 0xF4, 0xF4]],
            wave_zoom_s: crate::deck_view::DEFAULT_ZOOM_S,
        }
    }
}

fn first_catalogue() -> Vec<EffectKind> {
    use EffectKind::{BeatChopper, Delay, Filter, Flanger, Gater, LoFi, Phaser, Reverb, Ringmod};
    vec![
        Delay,
        Reverb,
        Filter,
        Flanger,
        Phaser,
        Gater,
        BeatChopper,
        LoFi,
        Ringmod,
    ]
}

fn add_new_effects(selection: &mut Vec<EffectKind>, known: &[EffectKind]) -> Vec<EffectKind> {
    let all = default_fx_selection();
    for &kind in &all {
        if !known.contains(&kind) && !selection.contains(&kind) {
            selection.push(kind);
        }
    }
    all
}

pub fn default_fx_selection() -> Vec<EffectKind> {
    EffectKind::ALL
        .into_iter()
        .filter(|&k| k != EffectKind::None)
        .collect()
}

impl Settings {
    pub fn analysis_options(&self) -> backspn_analysis::Options {
        backspn_analysis::Options {
            bpm_range: backspn_analysis::BpmRange::from_mode(self.bpm_range_mode),
            ..backspn_analysis::Options::default()
        }
    }

    pub fn load_analysis(&self) -> Option<backspn_analysis::Options> {
        self.auto_analyze_on_load.then(|| self.analysis_options())
    }

    pub fn wave_style(&self, theme: &crate::theme::Theme) -> WaveStyle {
        WaveStyle {
            mode: self.wave_mode,
            colors: self.wave_palette.colors(theme, self.wave_custom),
            direct: crate::density::is_direct(self.wave_palette),
        }
    }

    pub fn exists() -> bool {
        settings_path().exists()
    }

    pub fn load() -> Self {
        let Ok(text) = std::fs::read_to_string(settings_path()) else {
            return Self::default();
        };
        let (mut settings, rejected) = Self::from_json(&text);
        if !rejected.is_empty() {
            log::warn!("settings: ignoring unreadable values of {rejected:?}");
            keep_backup(&settings_path());
        }
        settings.fx_catalogue = add_new_effects(&mut settings.fx_selection, &settings.fx_catalogue);
        settings
    }

    pub fn from_json(text: &str) -> (Self, Vec<String>) {
        if let Ok(settings) = serde_json::from_str(text) {
            return (settings, Vec::new());
        }
        let Ok(serde_json::Value::Object(fields)) = serde_json::from_str(text) else {
            return (Self::default(), vec!["(whole file)".to_owned()]);
        };
        let mut accepted = serde_json::Map::new();
        let mut rejected = Vec::new();
        for (key, value) in fields {
            let mut trial = accepted.clone();
            trial.insert(key.clone(), value);
            if serde_json::from_value::<Self>(serde_json::Value::Object(trial.clone())).is_ok() {
                accepted = trial;
            } else {
                rejected.push(key);
            }
        }
        let settings =
            serde_json::from_value(serde_json::Value::Object(accepted)).unwrap_or_default();
        (settings, rejected)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = settings_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    pub fn headroom(&self) -> (f32, bool) {
        let x = &self.external_mixer;
        if x.on {
            (x.headroom_db, x.headroom_on_meters)
        } else {
            (self.headroom_db, self.headroom_on_meters)
        }
    }

    pub fn auto_gain_on(&self) -> bool {
        if self.external_mixer.on {
            self.external_mixer.auto_gain
        } else {
            self.auto_gain
        }
    }

    pub fn audio_config(&self) -> Config {
        Config {
            device: self.device.clone(),
            buffer_frames: self.buffer_frames,
            sample_rate: self.sample_rate,
            monitor: match self.monitor {
                Monitor::Auto => None,
                Monitor::None => Some((u16::MAX, u16::MAX)),
                Monitor::Channels(l, r) => Some((l, r)),
            },
        }
    }
}

/// Whether going from `before` to `after` needs the audio device opened
/// again.
pub fn needs_audio_restart(before: &Settings, after: &Settings) -> bool {
    before.device != after.device
        || before.buffer_frames != after.buffer_frames
        || before.sample_rate != after.sample_rate
        || before.monitor != after.monitor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_audio_device_changes_restart_the_audio() {
        let before = Settings::default();
        let deck_count = Settings {
            deck_count: 4,
            ..before.clone()
        };
        assert!(!needs_audio_restart(&before, &deck_count));
        let buffer = Settings {
            buffer_frames: Some(128),
            ..before.clone()
        };
        assert!(needs_audio_restart(&before, &buffer));
    }

    #[test]
    fn each_mixing_mode_keeps_its_headroom_and_auto_gain() {
        let mut s = Settings::default();
        assert_eq!(s.headroom(), (backspn_engine::DEFAULT_HEADROOM_DB, false));
        assert_eq!(
            s.external_mixer.outputs(),
            backspn_engine::Outputs::Internal
        );
        s.external_mixer.on = true;
        s.auto_gain = false;
        assert_eq!(s.headroom(), (0.0, false));
        assert!(s.auto_gain_on());
        s.external_mixer.decks[1] = Some((2, 3));
        let backspn_engine::Outputs::External(o) = s.external_mixer.outputs() else {
            panic!("external");
        };
        assert_eq!(o.decks[1], Some((2, 3)));
    }

    #[test]
    fn the_external_mixer_locks_what_its_switches_leave_off() {
        let mut x = ExternalMixer::default();
        assert_eq!(x.lock(), crate::mixer_view::MixerLock::default());
        x.on = true;
        x.eq_gain_control = true;
        let lock = x.lock();
        assert!(lock.channel && lock.master && lock.fx && !lock.eq_gain);
    }

    #[test]
    fn effects_new_to_the_catalogue_join_the_selection_once() {
        use EffectKind::{Delay, Echo, ModDecay, Reverb};
        let (mut s, _) = Settings::from_json(r#"{"fx_selection": ["Reverb", "Delay"]}"#);
        s.fx_catalogue = add_new_effects(&mut s.fx_selection, &s.fx_catalogue);
        let new: Vec<EffectKind> = default_fx_selection()
            .into_iter()
            .filter(|k| !first_catalogue().contains(k))
            .collect();
        assert_eq!(s.fx_selection[..2], [Reverb, Delay]);
        assert_eq!(s.fx_selection[2..], new[..]);
        assert_eq!(new.first(), Some(&Echo));
        assert_eq!(new.last(), Some(&ModDecay));
        s.fx_selection.truncate(2);
        let known = s.fx_catalogue.clone();
        add_new_effects(&mut s.fx_selection, &known);
        assert_eq!(s.fx_selection, [Reverb, Delay]);
    }

    #[test]
    fn settings_saved_with_the_old_field_names_load() {
        let s: Settings =
            serde_json::from_str(r#"{"flux_pause": true, "remix_auto_gain": true}"#).unwrap();
        assert!(s.slip_pause && s.sample_auto_gain);
    }

    #[test]
    fn settings_round_trip_and_old_files_load() {
        let s = Settings {
            device: Some("Kontrol S4 MK3".into()),
            monitor: Monitor::Channels(2, 3),
            ..Settings::default()
        };
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&text).unwrap(), s);
        let old: Settings = serde_json::from_str(r#"{"deck_count": 4}"#).unwrap();
        assert_eq!(old.deck_count, 4);
        assert_eq!(old.tempo_range, Settings::default().tempo_range);
        assert!(old.load_only_stopped, "protection is on unless turned off");
    }

    #[test]
    fn hidden_tree_nodes_round_trip_and_default_to_none() {
        use crate::browser::TreeNode;
        let old: Settings = serde_json::from_str(r#"{"deck_count": 2}"#).unwrap();
        assert!(old.hidden_tree_nodes.is_empty(), "everything shows");
        let s = Settings {
            hidden_tree_nodes: vec![TreeNode::Music, TreeNode::DjSoftware],
            ..Settings::default()
        };
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&text).unwrap(), s);
    }

    #[test]
    fn one_unreadable_value_keeps_the_rest() {
        let (s, rejected) =
            Settings::from_json(r#"{"deck_count": 4, "tempo_range": "wide", "snap": false}"#);
        assert_eq!(rejected, ["tempo_range"]);
        assert_eq!(s.deck_count, 4);
        assert!(!s.snap);
        assert_eq!(s.tempo_range, Settings::default().tempo_range);
        let (s, rejected) = Settings::from_json("not json");
        assert_eq!(s, Settings::default());
        assert_eq!(rejected.len(), 1);
        let (_, rejected) = Settings::from_json(r#"{"deck_count": 2}"#);
        assert_eq!(rejected, Vec::<String>::new());
    }

    #[test]
    fn monitor_maps_to_engine_config() {
        let mut s = Settings::default();
        assert_eq!(s.audio_config().monitor, None);
        s.monitor = Monitor::None;
        assert_eq!(s.audio_config().monitor, Some((u16::MAX, u16::MAX)));
        s.monitor = Monitor::Channels(4, 5);
        assert_eq!(s.audio_config().monitor, Some((4, 5)));
    }
}
