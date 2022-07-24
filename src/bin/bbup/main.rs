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
        SubCommand::Setup => return setup::setup(home_dir),
        SubCommand::Init => return init::init(cwd),
        SubCommand::Sync { verbose, progress } 
		// | SubCommand::OtherTypeOfSync when I'll have one
		//	such as SubCommand::Pull
		=> {
            let global_config_path = home_dir
                .join(".config")
                .join("bbup-client")
                .join("config.yaml");
            if !global_config_path.exists() {
                anyhow::bail!("Bbup client isn't setup. Try using 'bbup setup'")
            }
            let global_config: ClientConfig = fs::load(&global_config_path)?;

            // Parse Link configs
            let local_config_path = cwd.join(".bbup").join("config.yaml");
            if !local_config_path.exists() {
                anyhow::bail!(
                    "Current directory [{:?}] isn't initialized as a backup source",
                    cwd
                )
            }
            let local_config: LinkConfig = fs::load(&local_config_path)?;

            let exclude_list = ExcludeList::from(&local_config.exclude_list)?;

            let connection = Connection {
                local_port: global_config.settings.local_port,
                server_port: global_config.settings.server_port,
                host_name: global_config.settings.host_name.clone(),
                host_address: global_config.settings.host_address.clone(),
            };

            let flags = Flags { verbose, progress };
            let config = ProcessConfig {
                link_root: cwd.clone(),
                exclude_list,
                endpoint: local_config.endpoint,
                connection,
                flags,
            };

            return sync::process_link(config).await;
        }
    }
}
