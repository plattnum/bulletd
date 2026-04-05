mod app;
mod init;
mod theme;

use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;

use bulletd_core::config::load_config;

#[derive(Parser)]
#[command(
    name = "bulletd",
    version,
    about = "Structured bullet logging for software engineers"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Launch the MCP server over stdio
    Serve,
    /// Initialize bulletd configuration
    Init,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Serve) => {
            eprintln!("bulletd MCP server — not yet implemented");
        }
        Some(Command::Init) => {
            init::run_init()?;
        }
        None => {
            app::install_panic_handler();
            let config = match load_config() {
                Ok(c) => c,
                Err(bulletd_core::Error::ConfigNotFound { .. }) => {
                    eprintln!("No config found. Run `bulletd init` to set up bulletd.");
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            };
            let mut tui_app = app::App::new(&config);
            tui_app.run()?;
        }
    }

    Ok(())
}
