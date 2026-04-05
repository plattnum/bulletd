use std::fs;
use std::io::{self, Write};

use bulletd_core::config::{
    Config, DisplayConfig, GeneralConfig, MigrationConfig, ThemeConfig, config_path,
    resolve_data_dir, serialize_config,
};
use color_eyre::eyre::{Result, bail};

/// Default theme — Tokyo Night inspired, same feel as wdttg.
fn default_theme() -> ThemeConfig {
    ThemeConfig {
        background: "#1a1b26".to_string(),
        foreground: "#c0caf5".to_string(),
        accent: "#7aa2f7".to_string(),
        success: "#9ece6a".to_string(),
        warning: "#e0af68".to_string(),
        error: "#f7768e".to_string(),
        muted: "#565f89".to_string(),
    }
}

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

    // Prompt for data directory
    let data_dir =
        prompt("Where should daily logs be stored? (e.g., ~/.local/share/bulletd/logs)")?;
    let data_dir = data_dir.trim().to_string();
    if data_dir.is_empty() {
        bail!("Data directory cannot be empty");
    }

    let config = Config {
        general: GeneralConfig {
            data_dir: data_dir.clone(),
            lookback_days: 7,
        },
        display: DisplayConfig {
            date_format: "%Y-%m-%d".to_string(),
            show_ids: false,
        },
        migration: MigrationConfig { stale_threshold: 3 },
        theme: default_theme(),
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
    println!("Theme and other settings can be customized in the config file.");

    Ok(())
}

fn prompt(question: &str) -> Result<String> {
    print!("{question}: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}
