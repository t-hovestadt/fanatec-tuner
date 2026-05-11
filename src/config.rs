use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize, Default)]
pub struct XmlConfig {
    /// Additional XML car lists directory. Checked after profiles/xml/ and the
    /// Fanatec App default install path.
    pub path: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct FanatecAppConfig {
    /// Path to the Fanatec App data folder (auto-detected if absent).
    /// Default: C:\Users\Public\Fanatec\OneFanatec
    pub path: Option<String>,
}

impl FanatecAppConfig {
    /// Return the resolved OneFanatec root, or None if not found.
    pub fn resolve(&self) -> Option<std::path::PathBuf> {
        if let Some(ref p) = self.path {
            let pb = std::path::PathBuf::from(p);
            if pb.exists() {
                return Some(pb);
            }
        }
        #[cfg(windows)]
        {
            let default = std::path::PathBuf::from(r"C:\Users\Public\Fanatec\OneFanatec");
            if default.exists() {
                return Some(default);
            }
        }
        None
    }
}

#[derive(Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub profiles: ProfilesConfig,
    #[serde(default)]
    pub monitor: MonitorConfig,
    #[serde(default)]
    pub xml: XmlConfig,
    #[serde(default)]
    pub fanatec_app: FanatecAppConfig,
}

#[derive(Deserialize, Default)]
pub struct MonitorConfig {
    /// Seconds between car-detection polls (default: 3)
    pub scan_interval: Option<u64>,
}

impl MonitorConfig {
    pub fn scan_interval_secs(&self) -> u64 {
        self.scan_interval.unwrap_or(3)
    }
}

#[derive(Deserialize, Default)]
pub struct ProfilesConfig {
    /// Directory containing .pws profile files.
    pub path: Option<String>,
    /// Base type string used to filter profiles (e.g. "CS DD+"). Reserved for Phase 3.
    #[allow(dead_code)]
    pub base: Option<String>,
}

/// Loads `fanatec-tuner.toml` from `config_path`.
/// Returns `Config::default()` if the file does not exist.
pub fn load(config_path: &Path) -> Result<Config, String> {
    if !config_path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(config_path)
        .map_err(|e| format!("cannot read {}: {}", config_path.display(), e))?;
    toml::from_str(&text).map_err(|e| format!("cannot parse {}: {}", config_path.display(), e))
}
