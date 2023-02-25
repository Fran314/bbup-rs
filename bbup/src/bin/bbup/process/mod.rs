use abst_fs::AbstPath;
use bbup_com::{BbupCom, JobType};
use fs_vcs::ExcludeList;
use ssh_tunnel::SshTunnel;
use tokio::net::TcpStream;

mod bijection;
use crate::model::{BackOps, ClientConfig, LinkConfig, LinkState, LinkType};

use anyhow::{Context, Result};

pub enum Operations {
    Pull,
    Sync,
}
pub async fn process(
    conf_dir: &AbstPath,
    link_root: &AbstPath,
    operation: Operations,
    options: BackOps,
) -> Result<()> {
    let client_config = ClientConfig::load(conf_dir)?;
    let link_config = LinkConfig::load(link_root)?;
    let exclude_list = ExcludeList::from(&link_config.exclude_list)?;

    if options.verbose {
        println!("Synchronizing endpoint: \"{}\"", link_config.endpoint);
    }

    let mut tunnel = SshTunnel::to(
        client_config.settings.local_port,
        client_config.settings.server_port,
        client_config.settings.host_name.clone(),
        client_config.settings.host_address.clone(),
    )?;

    if options.verbose {
        println!("ssh tunnel PID: {}", tunnel.pid());
    }

    tunnel.wait_for_ready()?;

    // Start connection
    let socket = TcpStream::connect(format!("127.0.0.1:{}", client_config.settings.local_port))
        .await
        .context("could not connect to server")?;
    let mut com = BbupCom::from(socket, options.progress);

    // Await green light to procede
    com.check_ok()
        .await
        .context("could not get green light from server to procede with conversation")?;

    com.send_struct(&link_config.endpoint).await?;
    com.check_ok()
        .await
        .context("could not get green light from server on validity of endpoint")?;

    match (link_config.link_type, operation) {
        (LinkType::Bijection, Operations::Pull) => {
            bijective_pull(link_root, &mut com, exclude_list, options).await?
        }
        (LinkType::Bijection, Operations::Sync) => {
            bijective_sync(link_root, &mut com, exclude_list, options).await?
        }
        (LinkType::Injection, Operations::Pull) => todo!(),
        (LinkType::Injection, Operations::Sync) => todo!(),
        (LinkType::BlockInjection, Operations::Pull) => todo!(),
        (LinkType::BlockInjection, Operations::Sync) => todo!(),
    }

    println!(
        "Endpoint correctly synchronized: \"{}\"",
        link_config.endpoint
    );
    Ok(())
}

async fn bijective_pull(
    link_root: &AbstPath,
    com: &mut BbupCom,
    exclude_list: ExcludeList,
    options: BackOps,
) -> Result<()> {
    let mut state = LinkState::load(link_root)?;

    com.send_struct(JobType::Pull).await?;
    bijection::pull(&mut state, com, link_root, &exclude_list, options.verbose).await?;

    com.send_struct(JobType::Quit).await?;
    Ok(())
}
async fn bijective_sync(
    link_root: &AbstPath,
    com: &mut BbupCom,
    exclude_list: ExcludeList,
    options: BackOps,
) -> Result<()> {
    let mut state = LinkState::load(link_root)?;

    com.send_struct(JobType::Pull).await?;
    bijection::pull(&mut state, com, link_root, &exclude_list, options.verbose).await?;

    com.send_struct(JobType::Push).await?;
    bijection::push(&mut state, com, link_root, &exclude_list, options.verbose).await?;

    com.send_struct(JobType::Quit).await?;
    Ok(())
}
