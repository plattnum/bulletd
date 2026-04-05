use bulletd_core::config::ThemeConfig;
use ratatui::style::Color;

/// Parsed theme colors for TUI rendering.
#[derive(Debug, Clone)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub muted: Color,
}

impl Theme {
    /// Load theme from config, falling back to defaults for any invalid colors.
    pub fn from_config(config: &ThemeConfig) -> Self {
        Self {
            background: parse_hex(&config.background).unwrap_or(Self::default().background),
            foreground: parse_hex(&config.foreground).unwrap_or(Self::default().foreground),
            accent: parse_hex(&config.accent).unwrap_or(Self::default().accent),
            success: parse_hex(&config.success).unwrap_or(Self::default().success),
            warning: parse_hex(&config.warning).unwrap_or(Self::default().warning),
            error: parse_hex(&config.error).unwrap_or(Self::default().error),
            muted: parse_hex(&config.muted).unwrap_or(Self::default().muted),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::Rgb(26, 27, 38),    // #1a1b26
            foreground: Color::Rgb(192, 202, 245), // #c0caf5
            accent: Color::Rgb(122, 162, 247),     // #7aa2f7
            success: Color::Rgb(158, 206, 106),    // #9ece6a
            warning: Color::Rgb(224, 175, 104),    // #e0af68
            error: Color::Rgb(247, 118, 142),      // #f7768e
            muted: Color::Rgb(86, 95, 137),        // #565f89
        }
    }
}

/// Parse a hex color string like "#1a1b26" into a ratatui Color::Rgb.
pub fn parse_hex(hex: &str) -> Option<Color> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_valid() {
        assert_eq!(parse_hex("#1a1b26"), Some(Color::Rgb(26, 27, 38)));
        assert_eq!(parse_hex("#ffffff"), Some(Color::Rgb(255, 255, 255)));
        assert_eq!(parse_hex("#000000"), Some(Color::Rgb(0, 0, 0)));
        assert_eq!(parse_hex("#FF0000"), Some(Color::Rgb(255, 0, 0)));
    }

    #[test]
    fn parse_hex_invalid() {
        assert_eq!(parse_hex("1a1b26"), None); // missing #
        assert_eq!(parse_hex("#1a1b2"), None); // too short
        assert_eq!(parse_hex("#1a1b26f"), None); // too long
        assert_eq!(parse_hex("#gggggg"), None); // invalid hex
        assert_eq!(parse_hex(""), None);
        assert_eq!(parse_hex("#"), None);
        assert_eq!(parse_hex("#αβγ"), None); // multi-byte UTF-8, 6 bytes but not 6 ASCII chars
    }

    #[test]
    fn theme_default_is_valid() {
        let theme = Theme::default();
        // Just verify it doesn't panic and produces Rgb colors
        assert!(matches!(theme.background, Color::Rgb(_, _, _)));
        assert!(matches!(theme.foreground, Color::Rgb(_, _, _)));
    }

    #[test]
    fn theme_from_config_with_valid_colors() {
        let config = ThemeConfig {
            background: "#ff0000".to_string(),
            foreground: "#00ff00".to_string(),
            accent: "#0000ff".to_string(),
            success: "#ffffff".to_string(),
            warning: "#000000".to_string(),
            error: "#123456".to_string(),
            muted: "#abcdef".to_string(),
        };
        let theme = Theme::from_config(&config);
        assert_eq!(theme.background, Color::Rgb(255, 0, 0));
        assert_eq!(theme.foreground, Color::Rgb(0, 255, 0));
    }

    #[test]
    fn theme_from_config_falls_back_on_invalid() {
        let config = ThemeConfig {
            background: "not a color".to_string(),
            foreground: "#00ff00".to_string(),
            accent: "#0000ff".to_string(),
            success: "#ffffff".to_string(),
            warning: "#000000".to_string(),
            error: "#123456".to_string(),
            muted: "#abcdef".to_string(),
        };
        let theme = Theme::from_config(&config);
        // Invalid background should fall back to default
        assert_eq!(theme.background, Theme::default().background);
        // Valid foreground should use config value
        assert_eq!(theme.foreground, Color::Rgb(0, 255, 0));
    }
}
