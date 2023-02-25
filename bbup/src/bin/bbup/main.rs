mod model;
use model::*;
mod init;
mod process;
mod setup;
// mod sync;

use abst_fs as fs;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

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
    let cwd = match args.cwd {
        Some(val) => abst_fs::AbstPath::from(val),
        None => fs::cwd().context("could not resolve current working directory")?,
    };

    match args.cmd {
        SubCommand::Setup(options) => setup::setup(&home_dir, options),
        SubCommand::Init(options) => init::init(&cwd, options),
        SubCommand::Sync(options) => process::sync(&home_dir, &cwd, options).await,
        _ => Ok(()),
    }
}
