mod config;
mod daemon;
mod error;
mod event;
mod hook;
mod http;
mod launcher;
mod prompt;
mod runner;
mod shell;
mod source;

use clap::{Parser, Subcommand};

use crate::{
    error::AppError, hook::run_hook, launcher::launch_adapter, runner::run_wrapped_command,
};

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
    /// Start the desktop orb, run an adapter CLI, then stop session-local runtime processes.
    Launch {
        /// Adapter executable to run, for example `codex` or `claude`.
        #[arg(long)]
        adapter: String,
        /// Arguments forwarded to the adapter after `--`.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Internal: consume a Claude Code hook event on stdin and report status.
    ///
    /// Registered in the adapter's settings.json by `npx agent_orb`. Reads the
    /// hook JSON from stdin, maps it to an orb status, and posts it to the local
    /// daemon. Always exits 0 so it can never disrupt the host CLI.
    Hook {
        /// Adapter name hint, for example `claude`. Defaults to `claude`.
        #[arg(long)]
        adapter: Option<String>,
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
        Some(Commands::Launch { adapter, args }) => launch_adapter(adapter, args).await,
        Some(Commands::Hook { adapter }) => run_hook(adapter).await,
        None => {
            println!(
                "Agent Orb CLI. Try: agent_orb launch --adapter claude -- or agent_orb run -- echo hello"
            );
            Ok(0)
        }
    }
}
