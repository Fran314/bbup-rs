mod model;
use model::*;
mod init;
mod process;
mod setup;
// mod sync;

use abst_fs as fs;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use process::Operations;

#[derive(Subcommand, Debug, PartialEq)]
enum SubCommand {
    /// Pull updates from server and push local updates
    Sync(BackOps),

    /// Pull updates from server
    Pull(BackOps),

    /// Initialize link
    Init(InitOps),

    /// Initialize bbup client
    Setup(SetupOps),
}

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(subcommand)]
    cmd: SubCommand,

    /// Set alternative config dir path (default will be set to ~/.config/bbup)
    #[clap(short, long)]
    conf_dir: Option<String>,

    /// Set hardcoded link root (default will be current working directory)
    #[clap(short, long)]
    link_root: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    let conf_dir = match args.conf_dir {
        Some(val) => abst_fs::AbstPath::from(val),
        None => fs::home_dir()
            .context("could not resolve home_dir path")?
            .add_last(".config")
            .add_last("bbup"),
    };
    let link_root = match args.link_root {
        Some(val) => abst_fs::AbstPath::from(val),
        None => fs::cwd().context("could not resolve current working directory")?,
    };

    match args.cmd {
        SubCommand::Setup(options) => setup::setup(&conf_dir, options),
        SubCommand::Init(options) => init::init(&link_root, options),
        SubCommand::Sync(options) => {
            process::process(&conf_dir, &link_root, Operations::Sync, options).await
        }
        SubCommand::Pull(options) => {
            process::process(&conf_dir, &link_root, Operations::Pull, options).await
        }
    }
}
