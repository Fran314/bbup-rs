use abst_fs::AbstPath;
use bbup_com::{BbupCom, JobType};
use fs_vcs::ExcludeList;
use ssh_tunnel::SshTunnel;
use tokio::net::TcpStream;

mod bijection;
use crate::model::{BackOps, ClientConfig, LinkConfig, LinkState};

use anyhow::{bail, Context, Result};

pub async fn sync(home_dir: &AbstPath, cwd: &AbstPath, options: BackOps) -> Result<()> {
    let client_config = ClientConfig::load(home_dir)?;
    let link_config = LinkConfig::load(cwd)?;
    let exclude_list = ExcludeList::from(&link_config.exclude_list)?;

    if options.verbose {
        println!("Synchronizing link: [{}]", cwd);
    }

    let process = {
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

        let conversation_result: Result<()> = {
            // Await green light to procede
            com.check_ok()
                .await
                .context("could not get green light from server to procede with conversation")?;

            com.send_struct(&link_config.endpoint).await?;
            com.check_ok()
                .await
                .context("could not get green light from server on validity of endpoint")?;

            let mut state = LinkState::load(cwd)?;

            com.send_struct(JobType::Pull).await?;
            bijection::pull(&mut state, &mut com, cwd, &exclude_list, options.verbose).await?;

            com.send_struct(JobType::Push).await?;
            bijection::push(&mut state, &mut com, cwd, &exclude_list, options.verbose).await?;

            // Terminate conversation with server
            com.send_struct(JobType::Quit).await?;

            Ok(())
        };

        match conversation_result {
            Ok(()) => Ok(()),
            Err(error) => {
                if let Err(err) = com.send_error(1, "error propagated from client").await {
                    println!("Could not propagate error to server, because {err:#?}")
                }
                Err(error)
            }
        }
    };

    match process {
        Ok(()) => {
            if options.verbose {
                println!("Link correctly synchronized: [{}] ", cwd);
            }

            Ok(())
        }
        Err(err) => {
            bail!("Failed to sync link [{}]\n{}", cwd, err);
        }
    }
}
