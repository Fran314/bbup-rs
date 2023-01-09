mod model;
use model::*;
mod init;
mod protocol;
mod setup;
mod sync;

use abst_fs as fs;

use fs_vcs::ExcludeList;

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
    Init {
        /// Set endpoint
        #[clap(short, long)]
        endpoint: Option<String>,

        /// Set exclude list to empty
        #[clap(short, long)]
        no_exclude_list: bool,
    },
    /// Initialize bbup client
    Setup {
        /// Set port for client
        #[clap(short, long, value_parser)]
        local_port: Option<u16>,

        /// Set port for server
        #[clap(short, long, value_parser)]
        server_port: Option<u16>,

        /// Set server username
        #[clap(short = 'n', long, value_parser)]
        host_name: Option<String>,

        /// Set server address
        #[clap(short = 'a', long, value_parser)]
        host_address: Option<String>,
    },
}

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(subcommand)]
    cmd: SubCommand,

    /// Set fake home directory
    #[clap(short = 'H', long)]
    home_dir: Option<String>,

    /// Set fake current working directory
    #[clap(short = 'C', long)]
    cwd: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    let home_dir = match args.home_dir {
        Some(val) => abst_fs::AbstPath::from(val),
        None => fs::home_dir().context("could not resolve home_dir path")?,
    };
    //dbg!(&abst_fs::AbstPath::from(args.home_dir.unwrap()));
    //let home_dir = fs::home_dir().context("could not resolve home_dir path")?;
    //dbg!(&home_dir);
    let cwd = match args.cwd {
        Some(val) => abst_fs::AbstPath::from(val),
        None => fs::cwd().context("could not resolve current working directory")?,
    };

    match args.cmd {
        SubCommand::Setup { local_port, server_port, host_name, host_address } => setup::setup(&home_dir, local_port, server_port, host_name, host_address),
        SubCommand::Init { endpoint, no_exclude_list } => init::init(&cwd, endpoint, no_exclude_list),
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
