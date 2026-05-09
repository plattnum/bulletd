use std::fs;
use std::path::{Path, PathBuf};

use chrono::Weekday;
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
    /// Weekdays that count as work days for the `m` migrate action.
    /// Defaults to Mon–Fri when absent. Must be non-empty.
    #[serde(
        default = "default_work_days",
        deserialize_with = "deserialize_work_days",
        serialize_with = "serialize_work_days"
    )]
    pub work_days: Vec<Weekday>,
}

pub fn default_work_days() -> Vec<Weekday> {
    use Weekday::*;
    vec![Mon, Tue, Wed, Thu, Fri]
}

fn parse_weekday(s: &str) -> std::result::Result<Weekday, String> {
    use Weekday::*;
    match s.trim().to_lowercase().as_str() {
        "mon" | "monday" => Ok(Mon),
        "tue" | "tuesday" => Ok(Tue),
        "wed" | "wednesday" => Ok(Wed),
        "thu" | "thursday" => Ok(Thu),
        "fri" | "friday" => Ok(Fri),
        "sat" | "saturday" => Ok(Sat),
        "sun" | "sunday" => Ok(Sun),
        other => Err(format!("unknown weekday: {other:?}")),
    }
}

fn weekday_short(w: Weekday) -> &'static str {
    use Weekday::*;
    match w {
        Mon => "Mon",
        Tue => "Tue",
        Wed => "Wed",
        Thu => "Thu",
        Fri => "Fri",
        Sat => "Sat",
        Sun => "Sun",
    }
}

fn deserialize_work_days<'de, D>(deserializer: D) -> std::result::Result<Vec<Weekday>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let strings: Vec<String> = Vec::deserialize(deserializer)?;
    let mut out = Vec::with_capacity(strings.len());
    for s in strings {
        let w = parse_weekday(&s).map_err(serde::de::Error::custom)?;
        if !out.contains(&w) {
            out.push(w);
        }
    }
    if out.is_empty() {
        return Err(serde::de::Error::custom("work_days must not be empty"));
    }
    Ok(out)
}

fn serialize_work_days<S>(value: &[Weekday], serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = serializer.serialize_seq(Some(value.len()))?;
    for w in value {
        seq.serialize_element(weekday_short(*w))?;
    }
    seq.end()
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
work_days = ["Mon", "Tue", "Wed", "Thu", "Fri"]

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
        assert_eq!(config.migration.work_days, default_work_days());
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

    // --- work_days field tests --------------------------------------------------

    fn config_toml_with_migration(migration_block: &str) -> String {
        format!(
            r##"[general]
data_dir = "~/.local/share/bulletd/logs"
lookback_days = 7

[display]
date_format = "%Y-%m-%d"
show_ids = false

[migration]
{migration_block}

[theme]
background = "#1a1b26"
foreground = "#c0caf5"
accent = "#7aa2f7"
success = "#9ece6a"
warning = "#e0af68"
error = "#f7768e"
muted = "#565f89"
"##
        )
    }

    #[test]
    fn work_days_default_applied_when_absent() {
        let toml = config_toml_with_migration("stale_threshold = 3");
        let config: Config = toml::from_str(&toml).unwrap();
        assert_eq!(config.migration.work_days, default_work_days());
    }

    #[test]
    fn work_days_empty_list_rejected() {
        let toml = config_toml_with_migration("stale_threshold = 3\nwork_days = []");
        let result: Result<Config, _> = toml::from_str(&toml);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("work_days must not be empty"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn work_days_case_insensitive_abbreviations() {
        use Weekday::*;
        let toml = config_toml_with_migration(
            "stale_threshold = 3\nwork_days = [\"mon\", \"TUE\", \"Wed\"]",
        );
        let config: Config = toml::from_str(&toml).unwrap();
        assert_eq!(config.migration.work_days, vec![Mon, Tue, Wed]);
    }

    #[test]
    fn work_days_full_names_accepted() {
        use Weekday::*;
        let toml = config_toml_with_migration(
            "stale_threshold = 3\nwork_days = [\"Monday\", \"Friday\", \"Saturday\"]",
        );
        let config: Config = toml::from_str(&toml).unwrap();
        assert_eq!(config.migration.work_days, vec![Mon, Fri, Sat]);
    }

    #[test]
    fn work_days_unknown_name_rejected() {
        let toml = config_toml_with_migration("stale_threshold = 3\nwork_days = [\"Funday\"]");
        let result: Result<Config, _> = toml::from_str(&toml);
        assert!(result.is_err());
    }

    #[test]
    fn work_days_duplicates_deduplicated() {
        use Weekday::*;
        let toml = config_toml_with_migration(
            "stale_threshold = 3\nwork_days = [\"Mon\", \"Mon\", \"Tue\"]",
        );
        let config: Config = toml::from_str(&toml).unwrap();
        assert_eq!(config.migration.work_days, vec![Mon, Tue]);
    }

    #[test]
    fn work_days_serialize_uses_canonical_short_form() {
        use Weekday::*;
        let mut config: Config = toml::from_str(sample_config_toml()).unwrap();
        config.migration.work_days = vec![Mon, Thu, Fri];
        let serialized = toml::to_string_pretty(&config).unwrap();
        for token in ["\"Mon\"", "\"Thu\"", "\"Fri\""] {
            assert!(
                serialized.contains(token),
                "missing {token} in serialized output:\n{serialized}"
            );
        }
        // No long-name leakage.
        assert!(!serialized.contains("\"Monday\""));
        assert!(!serialized.contains("\"Thursday\""));
    }

    #[test]
    fn work_days_round_trip_irregular() {
        use Weekday::*;
        let mut original: Config = toml::from_str(sample_config_toml()).unwrap();
        original.migration.work_days = vec![Mon, Thu, Fri];
        let serialized = toml::to_string_pretty(&original).unwrap();
        let reparsed: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(reparsed.migration.work_days, vec![Mon, Thu, Fri]);
    }
}
