use std::sync::Arc;

use abst_fs as fs;

mod model;
use model::*;
mod process;
mod setup;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::{net::TcpListener, sync::Mutex};

#[derive(Subcommand, Debug, PartialEq)]
enum SubCommand {
    #[clap(version)]
    /// Run the daemon
    Run {
        /// Increase verbosity
        #[clap(short, long, value_parser)]
        verbose: bool,

        /// Show progress during file transfer
        #[clap(short, long)]
        progress: bool,
    },
    #[clap(version)]
    /// Initialize bbup client
    Setup,
}

#[derive(Parser, Debug)]
#[clap(version, name = "bbup-server")]
struct Args {
    #[clap(subcommand)]
    cmd: SubCommand,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    let home_dir = fs::home_dir().context("could not resolve home_dir path")?;

    match args.cmd {
        SubCommand::Setup => setup::setup(home_dir),
        SubCommand::Run { verbose, progress } => {
            let server_config = ServerConfig::load(&home_dir)?;
            let archive_root = home_dir.append(&server_config.archive_root);

            let archive_state =
                ArchiveState::load(&archive_root).context("failed to load aarchive's state")?;
            let archive_config = ArchiveConfig { archive_root };
            let state = Arc::new(Mutex::new(archive_state));

            // Start TCP server
            let listener =
                TcpListener::bind(format!("127.0.0.1:{}", server_config.server_port)).await?;

            // Spawn a task for each connection
            loop {
                let (socket, _) = listener.accept().await?;
                let state = state.clone();
                let config = archive_config.clone();
                tokio::spawn(async move {
                    let result = process::process_connection(config, socket, state, progress).await;
                    match result {
                        Ok(()) => {
                            if verbose {
                                println!("connection processed correctly")
                            }
                        }
                        Err(err) => println!("Error: {err}"),
                    }
                });
            }
        }
    }
}
