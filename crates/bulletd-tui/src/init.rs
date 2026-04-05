use std::fs;
use std::io::{self, Write};

use bulletd_core::config::{
    Config, DisplayConfig, GeneralConfig, MigrationConfig, ThemeConfig, config_path,
    resolve_data_dir, serialize_config,
};
use color_eyre::eyre::{Result, bail};

/// Run the interactive init wizard.
pub fn run_init() -> Result<()> {
    println!("bulletd init — interactive setup\n");

    let cfg_path = config_path();

    // Check if config already exists
    if cfg_path.exists() {
        println!("Config file already exists at: {}", cfg_path.display());
        let overwrite = prompt("Overwrite? (yes/no)")?;
        if overwrite.trim().to_lowercase() != "yes" {
            println!("Aborted.");
            return Ok(());
        }
        println!();
    }

    // Prompt for each setting
    let data_dir =
        prompt("Where should daily logs be stored? (e.g., ~/.local/share/bulletd/logs)")?;
    let data_dir = data_dir.trim().to_string();
    if data_dir.is_empty() {
        bail!("Data directory cannot be empty");
    }

    let lookback_days =
        prompt_u32("How many days back should the open tasks view scan? (e.g., 7)")?;
    let stale_threshold = prompt_u32(
        "After how many days should a task be flagged as stale during review? (e.g., 3)",
    )?;

    let show_ids_input = prompt("Show bullet IDs in the TUI? (yes/no)")?;
    let show_ids = show_ids_input.trim().to_lowercase() == "yes";

    println!("\nTheme colors (hex values, e.g., #1a1b26):");
    let background = prompt_color("  Background")?;
    let foreground = prompt_color("  Foreground")?;
    let accent = prompt_color("  Accent")?;
    let success = prompt_color("  Success")?;
    let warning = prompt_color("  Warning")?;
    let error = prompt_color("  Error")?;
    let muted = prompt_color("  Muted")?;

    let config = Config {
        general: GeneralConfig {
            data_dir: data_dir.clone(),
            lookback_days,
        },
        display: DisplayConfig {
            date_format: "%Y-%m-%d".to_string(),
            show_ids,
        },
        migration: MigrationConfig { stale_threshold },
        theme: ThemeConfig {
            background,
            foreground,
            accent,
            success,
            warning,
            error,
            muted,
        },
    };

    // Create data directory
    let resolved_data_dir = resolve_data_dir(&data_dir);
    if !resolved_data_dir.exists() {
        fs::create_dir_all(&resolved_data_dir)?;
        println!("\nCreated data directory: {}", resolved_data_dir.display());
    }

    // Write config file
    let config_toml = serialize_config(&config)
        .map_err(|e| color_eyre::eyre::eyre!("failed to serialize config: {e}"))?;

    if let Some(parent) = cfg_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }

    fs::write(&cfg_path, &config_toml)?;
    println!("Config written to: {}", cfg_path.display());
    println!("\nbulletd is ready. Run `bulletd` to start the TUI.");

    Ok(())
}

fn prompt(question: &str) -> Result<String> {
    print!("{question}: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn prompt_u32(question: &str) -> Result<u32> {
    loop {
        let input = prompt(question)?;
        match input.parse::<u32>() {
            Ok(n) => return Ok(n),
            Err(_) => println!("  Please enter a valid number."),
        }
    }
}

fn prompt_color(label: &str) -> Result<String> {
    loop {
        let input = prompt(&format!("{label} (e.g., #1a1b26)"))?;
        let trimmed = input.trim();
        if is_valid_hex_color(trimmed) {
            return Ok(trimmed.to_string());
        }
        println!("  Please enter a valid hex color (e.g., #1a1b26).");
    }
}

fn is_valid_hex_color(s: &str) -> bool {
    if let Some(hex) = s.strip_prefix('#') {
        hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_hex_colors() {
        assert!(is_valid_hex_color("#1a1b26"));
        assert!(is_valid_hex_color("#FFFFFF"));
        assert!(is_valid_hex_color("#000000"));
        assert!(is_valid_hex_color("#c0caf5"));
    }

    #[test]
    fn invalid_hex_colors() {
        assert!(!is_valid_hex_color("1a1b26")); // missing #
        assert!(!is_valid_hex_color("#1a1b2")); // too short
        assert!(!is_valid_hex_color("#1a1b26f")); // too long
        assert!(!is_valid_hex_color("#gggggg")); // invalid hex
        assert!(!is_valid_hex_color("")); // empty
        assert!(!is_valid_hex_color("#")); // just hash
    }
}
