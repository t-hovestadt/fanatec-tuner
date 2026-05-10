use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub profiles: ProfilesConfig,
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
