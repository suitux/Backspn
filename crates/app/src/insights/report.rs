use std::collections::BTreeMap;

use serde::Serialize;

use super::usage::Usage;

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema: u32,
    pub id: String,
    pub version: String,
    pub os: Os,
    pub locale: &'static str,
    pub audio: Option<Audio>,
    pub controllers: Vec<String>,
    pub library: Option<Library>,
    pub config: Config,
    pub usage: Usage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Os {
    pub platform: &'static str,
    pub arch: &'static str,
}

impl Os {
    pub fn current() -> Self {
        Self {
            platform: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Audio {
    pub sample_rate: u32,
    pub buffer_frames: Option<u32>,
    pub channels: u16,
    pub headphones: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub tracks: u64,
    pub analyzed: u64,
    pub playlists: u64,
    pub file_extensions: BTreeMap<String, u64>,
}

impl From<backspn_library::Stats> for Library {
    fn from(stats: backspn_library::Stats) -> Self {
        let mut file_extensions = BTreeMap::new();
        for (ext, n) in stats.extensions {
            let known = crate::AUDIO_EXTENSIONS.contains(&ext.as_str());
            *file_extensions
                .entry(if known { ext } else { "other".to_owned() })
                .or_default() += n;
        }
        Self {
            tracks: stats.tracks,
            analyzed: stats.analyzed,
            playlists: stats.playlists,
            file_extensions,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
    pub decks: usize,
    pub fx_units: usize,
    pub quantize: bool,
    pub snap: bool,
    pub auto_gain: bool,
    pub external_mixer: bool,
    pub link: bool,
    pub midi_clock: bool,
}

pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.truncate(32);
    out.trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_names_become_short_ids() {
        assert_eq!(
            slug("Native Instruments Kontrol S4 MK3"),
            "native-instruments-kontrol-s4-mk"
        );
        assert_eq!(slug("Pioneer DDJ-400"), "pioneer-ddj-400");
        assert_eq!(slug("  "), "");
    }

    fn sample() -> Report {
        let mut usage = Usage {
            sessions: 2,
            seconds_open: 5400,
            seconds_playing: 4200,
            tracks_loaded: 31,
            tracks_played: 24,
            mixes: 22,
            recordings: 1,
            seconds_recording: 3600,
            syncs: 18,
            hotcues: 40,
            loops: 6,
            ..Usage::default()
        };
        usage.inputs.add_n("controller", 900);
        usage.fx.add_n("Filter", 12);
        usage.imports.add("nml");
        Report {
            schema: SCHEMA,
            id: "6f1c2b9e-3d4a-4c8e-9b7a-2e5f1d0c3a4b".into(),
            version: "0.0.4".into(),
            os: Os {
                platform: "macos",
                arch: "aarch64",
            },
            locale: "es",
            audio: Some(Audio {
                sample_rate: 48_000,
                buffer_frames: Some(256),
                channels: 4,
                headphones: true,
            }),
            controllers: vec![slug("Kontrol S4 MK3")],
            library: Some(Library {
                tracks: 3200,
                analyzed: 3100,
                playlists: 14,
                file_extensions: [("flac".into(), 900), ("mp3".into(), 2300)].into(),
            }),
            config: Config {
                decks: 4,
                fx_units: 2,
                quantize: true,
                snap: true,
                auto_gain: true,
                external_mixer: false,
                link: false,
                midi_clock: false,
            },
            usage,
        }
    }

    #[test]
    fn the_server_fixture_matches_the_report() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../insights/test/fixtures/report.json");
        let json = serde_json::to_string_pretty(&sample()).unwrap() + "\n";
        if std::env::var_os("BACKSPN_BLESS").is_some() {
            std::fs::write(&path, &json).unwrap();
        }
        let fixture = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            fixture, json,
            "the report changed: update insights/src/schema.ts, then run with BACKSPN_BLESS=1"
        );
    }

    #[test]
    fn odd_extensions_are_lumped_together() {
        let stats = backspn_library::Stats {
            tracks: 3,
            analyzed: 1,
            playlists: 0,
            extensions: [("mp3".into(), 2), ("mysecretname".into(), 1)].into(),
        };
        let lib = Library::from(stats);
        assert_eq!(lib.file_extensions.get("mp3"), Some(&2));
        assert_eq!(lib.file_extensions.get("other"), Some(&1));
        assert_eq!(lib.file_extensions.len(), 2);
    }
}
