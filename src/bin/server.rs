use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::{
    io::BufReader,
    net::{TcpListener, TcpStream},
};

use clap::Parser;

use std::path::PathBuf;

use bbup_rust::comunications as com;
use bbup_rust::fs;

struct ServerState {
    _commit_list: fs::CommitList,
}

#[derive(Parser, Debug)]
struct Args {
    /// Custom home directory for testing
    #[clap(short, long, value_parser)]
    dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let _home_dir = match args.dir {
        Some(val) => val,
        None => dirs::home_dir().expect("could not get home directory"),
    };

    let state = Arc::new(Mutex::new(ServerState {
        _commit_list: Vec::new(),
    }));

    let listener = TcpListener::bind("127.0.0.1:3000").await?;

    loop {
        let (socket, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            process(socket, state).await.unwrap();
        });
    }
}

async fn process(socket: TcpStream, state: Arc<Mutex<ServerState>>) -> std::io::Result<()> {
    let mut socket = BufReader::new(socket);
    let mut buffer = String::new();

    let _state = match state.try_lock() {
        Ok(val) => val,
        Err(_) => {
            com::asyncrw::write(
                &mut socket,
                com::Basic::new(1, "bbup-server, server occupied"),
            )
            .await?;
            return Ok(());
        }
    };

    com::asyncrw::write(
        &mut socket,
        com::Basic::new(0, "bbup-server, procede with last known commit"),
    )
    .await?;
    let read_val: com::LastCommit = com::asyncrw::read(&mut socket, &mut buffer).await?;
    println!("last known commit from client: {}", read_val.commit_id);

    Ok(())
}
