use super::{protocol, ProcessConfig, ProcessState};

use tokio::net::TcpStream;

use bbup::com::{BbupCom, JobType};
use ssh_tunnel::SshTunnel;

use anyhow::{bail, Context, Result};

pub async fn process_link(config: ProcessConfig) -> Result<()> {
    if config.flags.verbose {
        println!("Synchronizing link: [{}]", config.link_root);
    }

    let process = {
        let mut tunnel = SshTunnel::to(
            config.connection.local_port,
            config.connection.server_port,
            config.connection.host_name.clone(),
            config.connection.host_address.clone(),
        )?;

        if config.flags.verbose {
            println!("ssh tunnel PID: {}", tunnel.pid());
        }

        tunnel.wait_for_ready()?;

        // Start connection
        let socket = TcpStream::connect(format!("127.0.0.1:{}", config.connection.local_port))
            .await
            .context("could not connect to server")?;
        let mut com = BbupCom::from(socket, config.flags.progress);

        let conversation_result: Result<()> = {
            // Await green light to procede
            com.check_ok()
                .await
                .context("could not get green light from server to procede with conversation")?;

            com.send_struct(&config.endpoint).await?;

            let mut state = ProcessState::load(&config.link_root)?;

            {
                // GET DELTA
                protocol::get_local_delta(&config, &mut state)?;
            }

            {
                // PULL
                com.send_struct(JobType::Pull).await?;
                protocol::pull_update_delta(&config, &mut state, &mut com).await?;
                // protocol::check_for_conflicts(&mut state).await?;
                // protocol::download_update(&config, &mut state, &mut com).await?;
                // protocol::apply_update(&config, &mut state).await?;
                protocol::apply_update_or_get_conflicts(&config, &mut state, &mut com).await?;
            }

            {
                // PUSH
                com.send_struct(JobType::Push).await?;
                protocol::upload_changes(&config, &mut state, &mut com).await?;
            }

            // Terminate conversation with server
            com.send_struct(JobType::Quit).await?;

            Ok(())
        };

        match conversation_result {
            Ok(()) => Ok(()),
            Err(error) => {
                if let Err(err) = com.send_error(1, "error propagated from client").await {
                    println!("Could not propagate error to server, because {:#?}", err)
                }
                Err(error)
            }
        }
    };

    match process {
        Ok(()) => {
            if config.flags.verbose {
                println!("Link correctly synchronized: [{}] ", config.link_root);
            }

            Ok(())
        }
        Err(err) => {
            bail!("Failed to sync link [{}]\n{}", config.link_root, err);
        }
    }
}
