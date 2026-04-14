use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// Named theme (e.g. "gruvbox-dark", "catppuccin-mocha").
    pub theme: Option<String>,
    /// Per-color overrides applied on top of the selected theme.
    #[serde(default)]
    pub colors: ColorOverrides,
}

#[derive(Debug, Default, Deserialize)]
pub struct ColorOverrides {
    pub active:       Option<String>,
    pub inactive:     Option<String>,
    pub meta:         Option<String>,
    pub preview_text: Option<String>,
    pub status_msg:   Option<String>,
    pub hint:         Option<String>,
    pub danger:       Option<String>,
}

impl Config {
    /// Load from `~/.config/ccm/config.toml`, silently returning defaults on
    /// any error (missing file, parse failure, etc.).
    pub fn load() -> Self {
        config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config").join("ccm").join("config.toml"))
}
