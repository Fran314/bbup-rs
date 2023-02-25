use std::sync::Arc;

use abst_fs as fs;

mod model;
use model::*;
mod create;
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
    Setup {
        /// Set server port
        #[clap(short, long)]
        server_port: Option<u16>,

        /// Set archive root
        #[clap(short, long)]
        archive_root: Option<String>,
    },

    #[clap(version)]
    /// Create archive endpoint
    Create {
        /// Name of the endpoint
        #[clap(short, long)]
        endpoint: Option<String>,
    },
}

#[derive(Parser, Debug)]
#[clap(version, name = "bbup-server")]
struct Args {
    #[clap(subcommand)]
    cmd: SubCommand,

    /// Set alternative config dir path (default will be set to ~/.config/bbup-server)
    #[clap(short, long)]
    conf_dir: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    let conf_dir = match args.conf_dir {
        Some(val) => fs::AbstPath::from(val),
        None => fs::home_dir()
            .context("could not resolve home_dir path")?
            .add_last(".config")
            .add_last("bbup-server"),
    };

    match args.cmd {
        SubCommand::Setup {
            server_port,
            archive_root,
        } => setup::setup(conf_dir, server_port, archive_root),
        SubCommand::Create { endpoint } => {
            let server_config = ServerConfig::load(&conf_dir)?;
            create::create(&server_config.archive_root, endpoint)
        }
        SubCommand::Run { verbose, progress } => {
            let server_config = ServerConfig::load(&conf_dir)?;
            // let archive_root = home_dir.append(&server_config.archive_root);

            let archive_state = Archive::load(&server_config.archive_root)
                .context("failed to load archive's state")?;
            // let archive_config = ArchiveConfig { archive_root };
            let state = Arc::new(Mutex::new(archive_state));

            // Start TCP server
            let listener =
                TcpListener::bind(format!("127.0.0.1:{}", server_config.server_port)).await?;

            // Spawn a task for each connection
            loop {
                let (socket, _) = listener.accept().await?;
                let state = state.clone();
                let archive_root = server_config.archive_root.clone();
                // let config = archive_config.clone();
                tokio::spawn(async move {
                    let result =
                        process::process_connection(&archive_root, socket, state, progress).await;
                    match result {
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
