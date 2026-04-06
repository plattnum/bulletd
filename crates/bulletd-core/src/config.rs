use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// Application configuration, loaded from config.toml.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub general: GeneralConfig,
    pub display: DisplayConfig,
    pub migration: MigrationConfig,
    pub theme: ThemeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneralConfig {
    /// Where daily logs are stored (e.g., ~/.local/share/bulletd/logs)
    pub data_dir: String,
    /// Default number of days to look back for open tasks
    pub lookback_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisplayConfig {
    /// Date format for TUI display
    pub date_format: String,
    /// Whether the TUI displays bullet IDs
    pub show_ids: bool,
}

/// Icon set for TUI status display.
///
/// Purely visual — the on-disk markdown format always uses the
/// canonical emojis regardless of which icon set is active.
#[derive(Debug, Clone, PartialEq)]
pub struct IconsConfig {
    pub open: String,
    pub done: String,
    pub migrated: String,
    pub cancelled: String,
    pub backlogged: String,
}

impl IconsConfig {
    /// Minimal TUI-friendly icons (single-width Unicode).
    pub fn minimal() -> Self {
        Self {
            open: "○".to_string(),
            done: "✓".to_string(),
            migrated: "→".to_string(),
            cancelled: "✗".to_string(),
            backlogged: "▼".to_string(),
        }
    }

    /// Original emoji icons (matches the on-disk format).
    pub fn emoji() -> Self {
        Self {
            open: "📌".to_string(),
            done: "✅".to_string(),
            migrated: "➡️".to_string(),
            cancelled: "❌".to_string(),
            backlogged: "📥".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationConfig {
    /// Days before a task is flagged as stale during review
    pub stale_threshold: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeConfig {
    pub background: String,
    pub foreground: String,
    pub accent: String,
    pub success: String,
    pub warning: String,
    pub error: String,
    pub muted: String,
}

/// Resolve the config file path.
/// Uses $XDG_CONFIG_HOME/bulletd/config.toml if set,
/// otherwise ~/.config/bulletd/config.toml.
pub fn config_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("bulletd").join("config.toml")
    } else {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("bulletd")
            .join("config.toml")
    }
}

/// Load config from the default path.
pub fn load_config() -> crate::error::Result<Config> {
    let path = config_path();
    load_config_from(&path)
}

/// Load config from a specific path.
pub fn load_config_from(path: &Path) -> crate::error::Result<Config> {
    if !path.exists() {
        return Err(Error::ConfigNotFound {
            path: path.to_path_buf(),
        });
    }

    let content = fs::read_to_string(path).map_err(|source| Error::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;

    let config: Config = toml::from_str(&content).map_err(|source| Error::ConfigParse {
        path: path.to_path_buf(),
        source,
    })?;

    Ok(config)
}

/// Serialize a config to TOML string.
pub fn serialize_config(config: &Config) -> std::result::Result<String, String> {
    toml::to_string_pretty(config).map_err(|e| e.to_string())
}

/// Resolve the data directory path, expanding ~ to the home directory.
pub fn resolve_data_dir(data_dir: &str) -> PathBuf {
    if let Some(rest) = data_dir.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(data_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config_toml() -> &'static str {
        r##"[general]
data_dir = "~/.local/share/bulletd/logs"
lookback_days = 7

[display]
date_format = "%Y-%m-%d"
show_ids = false

[migration]
stale_threshold = 3

[theme]
background = "#1a1b26"
foreground = "#c0caf5"
accent = "#7aa2f7"
success = "#9ece6a"
warning = "#e0af68"
error = "#f7768e"
muted = "#565f89"
"##
    }

    #[test]
    fn parse_valid_config() {
        let config: Config = toml::from_str(sample_config_toml()).unwrap();

        assert_eq!(config.general.data_dir, "~/.local/share/bulletd/logs");
        assert_eq!(config.general.lookback_days, 7);
        assert_eq!(config.display.date_format, "%Y-%m-%d");
        assert!(!config.display.show_ids);
        assert_eq!(config.migration.stale_threshold, 3);
        assert_eq!(config.theme.background, "#1a1b26");
        assert_eq!(config.theme.foreground, "#c0caf5");
        assert_eq!(config.theme.accent, "#7aa2f7");
        assert_eq!(config.theme.success, "#9ece6a");
        assert_eq!(config.theme.warning, "#e0af68");
        assert_eq!(config.theme.error, "#f7768e");
        assert_eq!(config.theme.muted, "#565f89");
    }

    #[test]
    fn parse_missing_section() {
        let toml = r#"[general]
data_dir = "~/.local/share/bulletd/logs"
lookback_days = 7
"#;
        let result: Result<Config, _> = toml::from_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_invalid_type() {
        let toml = r##"[general]
data_dir = "~/.local/share/bulletd/logs"
lookback_days = "not a number"

[display]
date_format = "%Y-%m-%d"
show_ids = false

[migration]
stale_threshold = 3

[theme]
background = "#1a1b26"
foreground = "#c0caf5"
accent = "#7aa2f7"
success = "#9ece6a"
warning = "#e0af68"
error = "#f7768e"
muted = "#565f89"
"##;
        let result: Result<Config, _> = toml::from_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn round_trip_config() {
        let original: Config = toml::from_str(sample_config_toml()).unwrap();
        let serialized = toml::to_string_pretty(&original).unwrap();
        let reparsed: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(original, reparsed);
    }

    #[test]
    fn load_config_from_missing_file() {
        let result = load_config_from(Path::new("/nonexistent/config.toml"));
        assert!(matches!(result, Err(Error::ConfigNotFound { .. })));
    }

    #[test]
    fn load_config_from_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, sample_config_toml()).unwrap();

        let config = load_config_from(&path).unwrap();
        assert_eq!(config.general.lookback_days, 7);
    }

    #[test]
    fn load_config_from_invalid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "this is not valid toml [[[").unwrap();

        let result = load_config_from(&path);
        assert!(matches!(result, Err(Error::ConfigParse { .. })));
    }

    #[test]
    fn resolve_data_dir_with_tilde() {
        let resolved = resolve_data_dir("~/.local/share/bulletd/logs");
        // Should expand ~ to home directory
        assert!(!resolved.to_string_lossy().starts_with('~'));
        assert!(
            resolved
                .to_string_lossy()
                .ends_with(".local/share/bulletd/logs")
        );
    }

    #[test]
    fn resolve_data_dir_absolute() {
        let resolved = resolve_data_dir("/tmp/bulletd/logs");
        assert_eq!(resolved, PathBuf::from("/tmp/bulletd/logs"));
    }
}
