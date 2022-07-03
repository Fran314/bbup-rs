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

    if args.cmd == SubCommand::Setup {
        return setup::setup(home_dir);
    }

    // Load server state, necessary for conversation and
    //	"shared" between tasks (though only one can use it
    //	at a time and those who can't have it terminate)
    let state = ServerState::load(home_dir)?;
    let listener = TcpListener::bind(format!("127.0.0.1:{}", state.server_port)).await?;
    let state = Arc::new(Mutex::new(state));

    match args.cmd {
        SubCommand::Run { progress } => {
            // Start TCP server and spawn a task for each connection
            loop {
                let (socket, _) = listener.accept().await?;
                let state = state.clone();
                tokio::spawn(async move {
                    match process::process_connection(socket, state, progress).await {
                        Ok(()) => println!("connection processed correctly"),
                        Err(err) => println!("Error: {err:?}"),
                    }
                });
            }
        }
        _ => { /* already handled */ }
    }
    Ok(())
}
