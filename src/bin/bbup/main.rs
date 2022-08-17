mod cmodel;
use cmodel::*;
mod init;
mod protocol;
mod setup;
mod sync;

use bbup_rust::{fs, model::ExcludeList};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Subcommand, Debug, PartialEq)]
enum SubCommand {
    /// Pull updates from server and push local updates
    Sync {
        /// Increase verbosity
        #[clap(short, long, value_parser)]
        verbose: bool,

        /// Show progress during file transfer
        #[clap(short, long, value_parser)]
        progress: bool,
    },
    /// Initialize link
    Init,
    /// Initialize bbup client
    Setup,
}

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(subcommand)]
    cmd: SubCommand,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    let home_dir = fs::home_dir().context("could not resolve home_dir path")?;
    let cwd = fs::cwd().context("could not resolve current working directory")?;

    match args.cmd {
        SubCommand::Setup => setup::setup(&home_dir),
        SubCommand::Init => init::init(&cwd),
        SubCommand::Sync { verbose, progress } 
		// | SubCommand::OtherTypeOfSync when I'll have one
		//	such as SubCommand::Pull
		=> {
			let client_config = ClientConfig::load(&home_dir)?;
			let link_config = LinkConfig::load(&cwd)?;

            let connection = Connection {
                local_port: client_config.settings.local_port,
                server_port: client_config.settings.server_port,
                host_name: client_config.settings.host_name.clone(),
                host_address: client_config.settings.host_address.clone(),
            };
            let flags = Flags { verbose, progress };
            let config = ProcessConfig {
                link_root: cwd.clone(),
                exclude_list: ExcludeList::from(&link_config.exclude_list)?,
                endpoint: link_config.endpoint,
                connection,
                flags,
            };

            sync::process_link(config).await
        }
    }
}
