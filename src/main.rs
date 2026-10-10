use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "annotator",
    version,
    about = "Review agent-written Markdown in the browser"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Open a file for review and block until it is sent back (Done) or abandoned.
    Open {
        file: PathBuf,
        /// Print the summary as JSON instead of text.
        #[arg(long)]
        json: bool,
        /// Do not launch a browser tab.
        #[arg(long)]
        no_browser: bool,
    },
    /// Run the daemon (normally spawned automatically by `open`).
    Serve {
        /// Stay attached to the terminal instead of detaching.
        #[arg(long)]
        foreground: bool,
    },
    /// Show daemon and session status.
    Status,
    /// Stop the daemon.
    Stop,
    /// Print agent usage instructions (SKILL.md).
    Skill,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Open {
            file,
            json,
            no_browser,
        } => {
            let code = annotator::client::open(&file, json, no_browser)?;
            std::process::exit(code);
        }
        Cmd::Serve { foreground } => annotator::daemon::serve(foreground),
        Cmd::Status => annotator::client::status(),
        Cmd::Stop => annotator::client::stop(),
        Cmd::Skill => {
            println!("TODO: embed skill/SKILL.md");
            Ok(())
        }
    }
}
