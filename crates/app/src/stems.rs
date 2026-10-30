use std::path::{Path, PathBuf};
use std::sync::Arc;

use backspn_control::{LoadMode, StemPart, Trigger};
use backspn_engine::{
    CompressorSettings, StemControls, StemLimiterSettings, StemMaster, StemMasterSettings, Stems,
};
use backspn_formats::stem::{StemInfo, StemMetadata};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DefaultLoad {
    #[default]
    Stems,
    Track,
}

impl DefaultLoad {
    pub const ALL: [Self; 2] = [Self::Stems, Self::Track];

    pub fn name(self) -> &'static str {
        match self {
            Self::Stems => "Load as Stem",
            Self::Track => "Load as Track",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadAs {
    Primary,
    Alternative,
    Track,
    Stems,
}

impl From<LoadMode> for LoadAs {
    fn from(mode: LoadMode) -> Self {
        match mode {
            LoadMode::Alternative => Self::Alternative,
            LoadMode::Track => Self::Track,
            LoadMode::Stems => Self::Stems,
        }
    }
}

pub fn apply(controls: &mut StemControls, part: StemPart, trigger: Trigger) -> bool {
    let level = |current: f32| crate::mixer_view::level(current, trigger);
    let flip = |on: &mut bool| match trigger {
        Trigger::Press => {
            *on = !*on;
            true
        }
        Trigger::Set(v) => {
            *on = v >= 0.5;
            true
        }
        Trigger::Release | Trigger::Delta(_) | Trigger::Reset => false,
    };
    match part {
        StemPart::Volume => level(controls.volume)
            .map(|v| controls.volume = v)
            .is_some(),
        StemPart::Filter => level(controls.filter)
            .map(|v| controls.filter = v)
            .is_some(),
        StemPart::FilterEngaged => level(controls.filter)
            .map(|v| {
                controls.filter = v;
                if engages_filter(v) {
                    controls.filter_on = true;
                }
            })
            .is_some(),
        StemPart::FxAmount => level(controls.fx_send)
            .map(|v| controls.fx_send = v)
            .is_some(),
        StemPart::Mute => flip(&mut controls.mute),
        StemPart::FilterOn => flip(&mut controls.filter_on),
        StemPart::FxOn => flip(&mut controls.fx_on),
    }
}

/// How far from centre a filter knob moves before it switches the filter
/// on, so that the first reading of a knob at rest does not.
const FILTER_ENGAGE: f32 = 0.03;

fn engages_filter(amount: f32) -> bool {
    (amount - 0.5).abs() > FILTER_ENGAGE
}

pub fn is_stem_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_lowercase().ends_with(".stem.mp4"))
}

pub fn stems_to_play(
    path: &Path,
    associated: Option<&Path>,
    preference: DefaultLoad,
    how: LoadAs,
) -> Option<PathBuf> {
    let source = if is_stem_file(path) {
        path
    } else {
        associated?
    };
    let primary = preference == DefaultLoad::Stems;
    let stems = match how {
        LoadAs::Primary => primary,
        LoadAs::Alternative => !primary,
        LoadAs::Track => false,
        LoadAs::Stems => true,
    };
    stems.then(|| source.to_path_buf())
}

#[derive(Debug, Clone, PartialEq)]
pub struct StemDirs {
    pub own: PathBuf,
    pub theirs: Option<PathBuf>,
}

impl StemDirs {
    pub fn find(&self, hash: [u32; 4]) -> Option<PathBuf> {
        use backspn_library::storage::{own_stem_file, stem_file};
        std::iter::once(own_stem_file(&self.own, hash))
            .chain(
                std::iter::once(&self.own)
                    .chain(&self.theirs)
                    .map(|base| stem_file(base, hash)),
            )
            .find(|p| p.is_file())
    }

    pub fn target(&self, hash: [u32; 4]) -> PathBuf {
        backspn_library::storage::own_stem_file(&self.own, hash)
    }

    pub fn is_own(&self, file: &Path) -> bool {
        file.starts_with(&self.own)
    }
}

pub fn default_dir() -> PathBuf {
    crate::dirs::music().join("Stems")
}

pub fn default_foreign_dir() -> PathBuf {
    crate::dirs::home().join(backspn_formats::interop::STEMS_DIR)
}

pub fn mastering(meta: &StemMetadata) -> StemMasterSettings {
    let c = &meta.mastering_dsp.compressor;
    let l = &meta.mastering_dsp.limiter;
    StemMasterSettings {
        compressor: CompressorSettings {
            enabled: c.enabled,
            input_gain_db: c.input_gain,
            output_gain_db: c.output_gain,
            threshold_db: c.threshold,
            dry_wet: c.dry_wet,
            attack_s: c.attack,
            release_s: c.release,
            ratio: c.ratio,
            hp_cutoff: c.hp_cutoff,
        },
        limiter: StemLimiterSettings {
            enabled: l.enabled,
            threshold_db: l.threshold,
            ceiling_db: l.ceiling,
            release_s: l.release,
        },
    }
}

pub struct StemLoad {
    pub stems: Arc<Stems>,
    pub master: Box<StemMaster>,
    pub info: Vec<StemInfo>,
}

impl std::fmt::Debug for StemLoad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StemLoad")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

pub fn load(path: &Path, output_rate: u32) -> Result<StemLoad, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let (settings, info) = if backspn_formats::bstem::is_own_stem_file(path) {
        let header = backspn_formats::bstem::header(&bytes);
        (
            StemMasterSettings::default(),
            header.map(|h| h.stems).unwrap_or_default(),
        )
    } else {
        let meta = backspn_formats::stem::metadata(&bytes);
        (
            meta.as_ref().map(mastering).unwrap_or_default(),
            meta.map(|m| m.stems).unwrap_or_default(),
        )
    };
    drop(bytes);
    let tracks = backspn_decode::decode_stems(path).map_err(|e| e.to_string())?;
    let stems: [backspn_engine::Track; backspn_engine::STEMS] = tracks
        .try_into()
        .map_err(|t: Vec<_>| format!("{} stems instead of 4", t.len()))?;
    Ok(StemLoad {
        stems: Arc::new(Stems(stems)),
        master: Box::new(StemMaster::new(&settings, f64::from(output_rate))),
        info,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preference_and_the_action_pick_stems_or_track() {
        let stem = Path::new("/m/Artist - Title.STEM.mp4");
        let plain = Path::new("/m/Artist - Title.mp4");
        let plays = |path, pref, how| stems_to_play(path, None, pref, how).is_some();
        assert!(plays(stem, DefaultLoad::Stems, LoadAs::Primary));
        assert!(!plays(stem, DefaultLoad::Stems, LoadAs::Alternative));
        assert!(!plays(stem, DefaultLoad::Track, LoadAs::Primary));
        assert!(plays(stem, DefaultLoad::Track, LoadAs::Alternative));
        assert!(plays(stem, DefaultLoad::Track, LoadAs::Stems));
        assert!(!plays(stem, DefaultLoad::Stems, LoadAs::Track));
        assert!(
            !plays(plain, DefaultLoad::Stems, LoadAs::Stems),
            "not a stem file"
        );
        let generated = Path::new("/stems/001/ABC.stem.mp4");
        assert_eq!(
            stems_to_play(plain, Some(generated), DefaultLoad::Stems, LoadAs::Primary),
            Some(generated.to_path_buf())
        );
        assert_eq!(
            stems_to_play(plain, Some(generated), DefaultLoad::Stems, LoadAs::Track),
            None
        );
    }

    #[test]
    fn generated_stems_are_found_in_the_own_folder_first() {
        let root = std::env::temp_dir().join(format!("backspn-stemdirs-{}", std::process::id()));
        let dirs = StemDirs {
            own: root.join("own"),
            theirs: Some(root.join("theirs")),
        };
        let hash = [1, 2, 3, 4];
        assert_eq!(dirs.find(hash), None);
        let theirs = backspn_library::storage::stem_file(&root.join("theirs"), hash);
        std::fs::create_dir_all(theirs.parent().unwrap()).unwrap();
        std::fs::write(&theirs, b"x").unwrap();
        assert_eq!(dirs.find(hash), Some(theirs.clone()));
        assert!(!dirs.is_own(&theirs));
        let older = backspn_library::storage::stem_file(&root.join("own"), hash);
        std::fs::create_dir_all(older.parent().unwrap()).unwrap();
        std::fs::write(&older, b"x").unwrap();
        assert_eq!(dirs.find(hash), Some(older.clone()), "an older own file");
        let own = dirs.target(hash);
        assert!(backspn_formats::bstem::is_own_stem_file(&own));
        std::fs::write(&own, b"x").unwrap();
        assert_eq!(dirs.find(hash), Some(own.clone()));
        assert!(dirs.is_own(&own));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn mapped_controls_change_one_stem_control() {
        let mut c = StemControls::default();
        assert!(apply(&mut c, StemPart::Volume, Trigger::Set(0.25)));
        assert!((c.volume - 0.25).abs() < f32::EPSILON);
        assert!(apply(&mut c, StemPart::Mute, Trigger::Press));
        assert!(c.mute);
        assert!(!apply(&mut c, StemPart::Mute, Trigger::Release));
        assert!(c.mute, "a release does not flip back");
        assert!(apply(&mut c, StemPart::FxOn, Trigger::Press));
        assert!(
            !apply(&mut c, StemPart::Filter, Trigger::Press),
            "a knob ignores presses"
        );
        assert!(!c.fx_on, "on by default, the press switches it off");
        assert!(apply(&mut c, StemPart::FilterOn, Trigger::Set(1.0)));
        assert!(c.filter_on);
        assert!(apply(&mut c, StemPart::FilterOn, Trigger::Set(1.0)));
        assert!(c.filter_on, "a value switches on, it does not flip");
    }

    #[test]
    fn an_engaged_filter_switches_on_once_moved_off_centre() {
        let mut c = StemControls::default();
        assert!(apply(&mut c, StemPart::FilterEngaged, Trigger::Set(0.51)));
        assert!(!c.filter_on, "a knob at rest stays off");
        assert!(apply(&mut c, StemPart::FilterEngaged, Trigger::Set(0.8)));
        assert!(c.filter_on && (c.filter - 0.8).abs() < f32::EPSILON);
        assert!(apply(&mut c, StemPart::FilterEngaged, Trigger::Set(0.5)));
        assert!(c.filter_on, "back at centre it stays on");
        let mut c = StemControls::default();
        assert!(apply(&mut c, StemPart::Filter, Trigger::Set(0.8)));
        assert!(!c.filter_on, "the plain filter never switches on");
    }

    #[test]
    fn mastering_takes_the_file_values() {
        let json = r#"{"version":1,"stems":[],"mastering_dsp":{"compressor":{"enabled":true,"ratio":4,"threshold":-10,"attack":0.01,"release":0.5,"input_gain":3,"output_gain":-1,"hp_cutoff":120,"dry_wet":70},"limiter":{"enabled":false,"threshold":-2,"ceiling":-0.5,"release":0.05}}}"#;
        let meta: StemMetadata = serde_json::from_str(json).unwrap();
        let s = mastering(&meta);
        assert!(s.compressor.enabled && !s.limiter.enabled);
        assert!((s.compressor.ratio - 4.0).abs() < 1e-6);
        assert!((s.compressor.dry_wet - 70.0).abs() < 1e-6);
        assert!((s.limiter.ceiling_db + 0.5).abs() < 1e-6);
    }

    #[test]
    fn loads_a_factory_stem_file() {
        let Some(dir) = std::env::var_os("BACKSPN_STEM_FILES") else {
            return;
        };
        let dir = std::path::Path::new(&dir);
        let Some(file) = std::fs::read_dir(dir).ok().and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| is_stem_file(p))
        }) else {
            return;
        };
        let load = load(&file, 48_000).unwrap();
        assert_eq!(load.info.len(), 4);
        assert!(load.stems.0.iter().all(|t| t.frames() > 0));
    }
}
