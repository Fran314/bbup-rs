use std::{path::PathBuf, sync::Arc};

mod smodel;
use smodel::*;
mod process;
mod setup;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::{net::TcpListener, sync::Mutex};

#[derive(Subcommand, Debug, PartialEq)]
enum SubCommand {
    /// Run the daemon
    Run {
        /// Increase verbosity
        #[clap(short, long, value_parser)]
        verbose: bool,

        /// Show progress during file transfer
        #[clap(short, long)]
        progress: bool,
    },
    /// Initialize bbup client
    Setup,
}

#[derive(Parser, Debug)]
struct Args {
    /// Custom home directory for testing
    #[clap(long, value_parser)]
    home_dir: Option<PathBuf>,

    #[clap(subcommand)]
    cmd: SubCommand,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    let home_dir = match args.home_dir {
        Some(val) => Some(val),
        None => dirs::home_dir(),
    }
    .context("could not resolve home_dir path")?;

    match args.cmd {
        SubCommand::Setup => return setup::setup(home_dir),
        SubCommand::Run { verbose, progress } => {
            // Load server state, necessary for conversation and
            //	"shared" between tasks (though only one can use it
            //	at a time and those who can't have it terminate)
            let state = ServerState::load(home_dir)?;

            // Start TCP server
            let listener = TcpListener::bind(format!("127.0.0.1:{}", state.server_port)).await?;

            // Transform state into an ArcMutex of its origina
            //	value to pass it around
            let state = Arc::new(Mutex::new(state));

            // Spawn a task for each connection
            loop {
                let (socket, _) = listener.accept().await?;
                let state = state.clone();
                tokio::spawn(async move {
                    match process::process_connection(socket, state, progress).await {
                        Ok(()) => {
                            if verbose {
                                println!("connection processed correctly")
                            }
                        }
                        Err(err) => println!("Error: {err:?}"),
                    }
                });
            }
        }
    }
}
