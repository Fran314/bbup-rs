use std::io::BufRead;

// TODO add custom error handling

pub struct SshTunnel {
    process: std::process::Child,
    ready: bool,
}

impl SshTunnel {
    pub fn to(
        local_port: u16,
        server_port: u16,
        host_user: String,
        host_address: String,
    ) -> std::io::Result<SshTunnel> {
        let ssh_tunnel_handle = std::process::Command::new("ssh")
            .arg("-tt")
            .arg("-L")
            .arg(format!("{}:localhost:{}", local_port, server_port,))
            .arg(format!("{}@{}", host_user, host_address))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()?;

        Ok(SshTunnel {
            process: ssh_tunnel_handle,
            ready: false,
        })
    }

    pub fn wait_for_ready(&mut self) -> std::io::Result<()> {
        if self.ready {
            return Ok(());
        }

        let mut f = std::io::BufReader::new(self.process.stdout.as_mut().unwrap());
        let mut buffer = String::new();
        f.read_line(&mut buffer)?;

        self.ready = true;

        Ok(())
    }

    pub fn pid(&self) -> u32 {
        self.process.id()
    }

    pub fn termiate(&mut self) {
        let _ = self.process.kill();
    }
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        self.termiate();
    }
}
