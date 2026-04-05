mod init;

use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;

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
            eprintln!("bulletd TUI — not yet implemented");
        }
    }

    Ok(())
}
