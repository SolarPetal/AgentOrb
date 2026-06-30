mod config;
mod daemon;
mod error;
mod event;
mod http;
mod prompt;
mod runner;
mod shell;
mod source;

use clap::{Parser, Subcommand};

use crate::{error::AppError, runner::run_wrapped_command};

#[derive(Debug, Parser)]
#[command(name = "agent_orb", version, about = "Agent Orb CLI wrapper")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run a target CLI through Agent Orb.
    Run {
        /// Command and arguments after `--`.
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let exit_code = match run_main().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("agent_orb: {err}");
            1
        }
    };

    std::process::exit(exit_code);
}

async fn run_main() -> Result<i32, AppError> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Run { command }) => run_wrapped_command(command).await,
        None => {
            println!("Agent Orb CLI. Try: agent_orb run -- echo hello");
            Ok(0)
        }
    }
}
