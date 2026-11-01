use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;

pub const FILE: &str = "backspn-manifest.json";
pub const SIGNATURE: &str = "backspn-manifest.json.minisig";

const KEY: &str = include_str!("backspn.pub");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    App,
    Nsis,
    AppImage,
    Deb,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Nsis => "nsis",
            Self::AppImage => "appimage",
            Self::Deb => "deb",
        }
    }

    fn platform(self) -> &'static str {
        match self {
            Self::App => "apple",
            Self::Nsis => "windows",
            Self::AppImage | Self::Deb => "linux",
        }
    }
}

pub fn arch(kind: Kind) -> &'static str {
    if kind == Kind::App {
        "universal"
    } else {
        std::env::consts::ARCH
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

impl Manifest {
    pub fn asset(&self, kind: Kind, arch: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| {
            a.kind == kind.name()
                && a.platform == kind.platform()
                && a.arch == arch
                && plain_name(&a.name)
                && a.sha256.len() == 64
        })
    }
}

fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

pub fn read(json: &str, signature: &str, version: &str) -> Result<Manifest, String> {
    read_with(KEY, json, signature, version)
}

fn read_with(key: &str, json: &str, signature: &str, version: &str) -> Result<Manifest, String> {
    let key = PublicKey::decode(key).map_err(|_| "this build has no update key".to_owned())?;
    let signature = Signature::decode(signature).map_err(|e| format!("bad signature: {e}"))?;
    key.verify(json.as_bytes(), &signature, false)
        .map_err(|e| format!("the update is not signed by Backspn: {e}"))?;
    let manifest: Manifest = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if manifest.version != version {
        return Err(format!(
            "the update describes {} instead of {version}",
            manifest.version
        ));
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = include_str!("fixtures/test-key.pub");
    const JSON: &str = include_str!("fixtures/manifest.json");
    const SIG: &str = include_str!("fixtures/manifest.json.minisig");

    #[test]
    fn only_a_signed_manifest_of_that_version_is_read() {
        let m = read_with(TEST_KEY, JSON, SIG, "0.1.0").unwrap();
        assert_eq!(m.assets.len(), 5);
        assert!(read_with(TEST_KEY, JSON, SIG, "0.1.1").is_err(), "replayed");
        let tampered = JSON.replace("\"size\": 3", "\"size\": 4");
        assert!(read_with(TEST_KEY, &tampered, SIG, "0.1.0").is_err());
        assert!(read_with("", JSON, SIG, "0.1.0").is_err(), "no key");
        assert!(read_with(TEST_KEY, JSON, "", "0.1.0").is_err());
    }

    #[test]
    fn the_asset_matches_the_platform_kind_and_arch() {
        let m: Manifest = serde_json::from_str(JSON).unwrap();
        let name = |kind, arch| m.asset(kind, arch).map(|a| a.name.as_str());
        assert_eq!(
            name(Kind::App, "universal"),
            Some("Backspn_0.1.0_apple_universal.app.tar.gz")
        );
        assert_eq!(
            name(Kind::Nsis, "x86_64"),
            Some("Backspn_0.1.0_windows_x86_64.exe")
        );
        assert_eq!(
            name(Kind::Deb, "x86_64"),
            Some("Backspn_0.1.0_linux_x86_64.deb")
        );
        assert_eq!(name(Kind::AppImage, "aarch64"), None);
        assert_eq!(arch(Kind::App), "universal");
    }

    #[test]
    fn names_stay_inside_the_download_folder() {
        assert!(plain_name("Backspn_0.1.0_linux_x86_64.AppImage"));
        for bad in ["", "../escape.deb", "a/b", "a\\b", ".hidden", "x y"] {
            assert!(!plain_name(bad), "{bad}");
        }
    }

    #[test]
    fn missing_fields_read_as_empty() {
        let m: Manifest = serde_json::from_str(r#"{"assets":[{"kind":"deb"}]}"#).unwrap();
        assert_eq!(m.version, "");
        assert_eq!(m.assets[0].size, 0);
        assert_eq!(m.asset(Kind::Deb, "x86_64"), None);
    }
}
